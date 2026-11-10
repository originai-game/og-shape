//! A bevelled edge that tapers to a tip at both ends
//! (`docs/research.md` §7.8): a lone piece of a longer crease (one
//! segment of a circle's rim, a piece of a boolean's cut edge) whose
//! neighbours along the crease are not bevelled.
//!
//! Each end is a tip at the vertex itself (§7.4's choice (b), slide zero),
//! so nothing at the ends moves and no old face tilts. The width grows
//! inside the edge instead: on each face the offset line leaves the tip
//! at 45° to the edge, runs at the offset, and returns at 45°; a short
//! piece (shorter than twice the largest offset) peaks at its middle at
//! half its length. The strip gets a profile at each station where the
//! width stops or starts growing, and fans to the tips. Each station's two
//! points lie in the plane of the edge's triangle on that face, square to
//! the edge, so the faces keep their planes; they are added to the
//! triangle's outline, which is rebuilt (with its planar face, where the
//! point lies outside the triangle) as any rebuilt corner is. Blender's
//! notes cover neither the taper's shape nor where it sits: both are ours.

use std::collections::{BTreeMap, BTreeSet};

use shape_core::DVec3;
use shape_mesh::{EdgeKey, Solid};

use super::{Build, Spec, bevel_runs, tri_normal};
use crate::BevelError;

/// The profiles of a lone piece's stations, `lo` to `hi`, each from face
/// `a` to face `b`; and the points to put on the two face triangles, as
/// ((triangle, the corner they follow), points).
pub(super) type Taper = (Vec<Vec<u32>>, [((usize, u32), Vec<u32>); 2]);

impl Build<'_> {
    pub(super) fn taper(&mut self, key: EdgeKey) -> Result<Taper, BevelError> {
        let s = self.s;
        let (lo, hi) = key;
        let (pl, ph) = (s.positions[lo as usize], s.positions[hi as usize]);
        let ta = self.topo.dir[&(lo, hi)];
        let tb = self.topo.dir[&(hi, lo)];
        let len = (ph - pl).length();
        let e = (ph - pl) / len;
        // Into each face, square to the edge, in its triangle's plane.
        let into_a = tri_normal(s, ta).cross(e).normalize_or_zero();
        let into_b = e.cross(tri_normal(s, tb)).normalize_or_zero();
        let off = self.edges[&key].off;
        let largest = off.iter().copied().fold(0.0f64, f64::max);
        if !(len > 0.0 && into_a != DVec3::ZERO && into_b != DVec3::ZERO) {
            return Err(BevelError::TooLarge(pl));
        }
        let at: Vec<f64> = if len > 2.0 * largest {
            vec![largest, len - largest]
        } else {
            vec![len / 2.0]
        };
        let mut profiles = Vec::with_capacity(at.len());
        let (mut on_a, mut on_b) = (Vec::new(), Vec::new());
        for &x in &at {
            let t = x / len;
            let k = (x.min(len - x) / largest).min(1.0);
            let (da, db) = (
                off[0] + (off[2] - off[0]) * t,
                off[1] + (off[3] - off[1]) * t,
            );
            let c = pl + e * x;
            let a = self.push(c + into_a * (da * k));
            let b = self.push(c + into_b * (db * k));
            profiles.push(self.chain(a, b, c));
            on_a.push(a);
            on_b.push(b);
        }
        // Face `a`'s triangle runs lo → hi, face `b`'s hi → lo.
        on_b.reverse();
        Ok((profiles, [((ta, lo), on_a), ((tb, hi), on_b)]))
    }
}

/// The selected edges that taper to a tip at both ends: each end has one
/// other real edge, unselected.
fn lone_pieces(s: &Solid, edges: &[EdgeKey]) -> Vec<EdgeKey> {
    let picked: BTreeSet<EdgeKey> = edges.iter().copied().collect();
    let mut at: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for e in s.real_edges() {
        let p = usize::from(picked.contains(&e));
        for v in [e.0, e.1] {
            let n = at.entry(v).or_default();
            n.0 += 1;
            n.1 += p;
        }
    }
    let lone = |v: u32| at.get(&v) == Some(&(2, 1));
    edges
        .iter()
        .copied()
        .filter(|&(a, b)| lone(a) && lone(b))
        .collect()
}

/// Where a bevel did not fit: the error naming the first lone piece, when
/// the selection holds one and the bevel fits without them (so the pieces'
/// tapers are what does not fit); `None` otherwise.
pub(super) fn lone_piece(s: &Solid, spec: &Spec) -> Option<BevelError> {
    let lone = lone_pieces(s, &spec.edges);
    let &(a, b) = lone.first()?;
    let rest: Vec<EdgeKey> = spec
        .edges
        .iter()
        .copied()
        .filter(|e| !lone.contains(e))
        .collect();
    if !rest.is_empty()
        && bevel_runs(
            s,
            &Spec {
                edges: rest,
                ..spec.clone()
            },
        )
        .is_err()
    {
        return None;
    }
    let mut names: Vec<String> = s
        .edge_tags
        .get(&(a, b))
        .into_iter()
        .flatten()
        .map(|&t| s.names.name(t).to_string())
        .collect();
    names.sort();
    Some(BevelError::LonePiece(
        s.positions[a as usize],
        s.positions[b as usize],
        names,
    ))
}

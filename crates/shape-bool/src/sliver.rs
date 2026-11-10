//! Zero-area triangles whose three corners are distinct positions on one
//! line (`docs/research.md` §15.4).
//!
//! A boolean makes them where two crossings that the exact predicates
//! tell apart lie less than an ulp from each other: one operand's edge
//! passes a rounding error off the other's, and each crosses the other's
//! face. Their positions can round onto one line with a third point of
//! the cut. Rounding a solid's positions (a move by a distance that is not
//! exact) can put a needle a rounding error wide onto a line the same way.
//!
//! Such a sliver's middle corner `m` lies strictly between the other two,
//! `a` and `b`, so the triangle across its long edge, `(a, b, c)`, is
//! split at `m` into `(a, m, c)` and `(m, b, c)`, which tile it exactly,
//! and the sliver is dropped. Every directed edge stays balanced, so a
//! closed surface stays closed. A half that lands on another triangle's
//! three corners facing the other way (a needle beside the sliver) is
//! dropped with it: the pair bounds no volume. No tolerance is involved:
//! only positions that lie on one line, by `orient2d` in each axis plane,
//! are joined.
//!
//! Entry: [`drop_slivers`] on a solid; the boolean calls [`flip`] on its
//! output.

use glam::DVec3;
use robust::{Coord, orient2d};
use shape_mesh::Solid;
use std::collections::BTreeMap;

/// Whether three positions lie on one line, exactly: their signed area
/// is zero in each axis plane.
fn on_line(p: [DVec3; 3]) -> bool {
    let c = |x: f64, y: f64| Coord { x, y };
    let [a, b, d] = p;
    orient2d(c(a.x, a.y), c(b.x, b.y), c(d.x, d.y)) == 0.0
        && orient2d(c(a.y, a.z), c(b.y, b.z), c(d.y, d.z)) == 0.0
        && orient2d(c(a.z, a.x), c(b.z, b.x), c(d.z, d.x)) == 0.0
}

/// The triangles left once each sliver of `tris` is flipped away, each
/// with the index of the input triangle it is (or is a half of). `w`
/// gives each point's id after welding by exact position; slivers are
/// those with three distinct ids on one line. A sliver whose long edge is
/// not used once each way, or whose neighbour across it is itself on one
/// line, is kept. Triangles are taken in order, halves after the input.
pub(crate) fn flip(points: &[DVec3], w: &[u32], tris: &[[u32; 3]]) -> Vec<([u32; 3], usize)> {
    let pos = |t: [u32; 3]| t.map(|v| points[v as usize]);
    let wid = |t: [u32; 3]| t.map(|v| w[v as usize]);
    let distinct = |t: [u32; 3]| t[0] != t[1] && t[1] != t[2] && t[0] != t[2];
    let mut out: Vec<([u32; 3], usize)> = tris.iter().copied().zip(0..).collect();
    let mut dropped = vec![false; out.len()];
    let mut halves: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < out.len() {
        let t = out[i].0;
        if dropped[i] || !distinct(wid(t)) || !on_line(pos(t)) {
            i += 1;
            continue;
        }
        // The middle corner, by the first axis along which the corners
        // differ: distinct points on one line are in order along it.
        let p = pos(t);
        let axis = (0..3)
            .find(|&j| p[0][j] != p[1][j] || p[1][j] != p[2][j])
            .unwrap_or(0);
        let k = (0..3)
            .find(|&k| {
                let (x, y, z) = (p[k][axis], p[(k + 1) % 3][axis], p[(k + 2) % 3][axis]);
                (y < x && x < z) || (z < x && x < y)
            })
            .unwrap_or(0);
        let (a, m, b) = (t[(k + 2) % 3], t[k], t[(k + 1) % 3]);
        let (wa, wb) = (w[a as usize], w[b as usize]);
        // The sliver runs b to a; its neighbour a to b, the only one.
        let mut fwd: Vec<(usize, usize)> = Vec::new();
        let mut back = 0;
        for (j, (u, _)) in out.iter().enumerate() {
            if dropped[j] {
                continue;
            }
            let wu = wid(*u);
            for r in 0..3 {
                match (wu[r], wu[(r + 1) % 3]) {
                    e if e == (wa, wb) => fwd.push((j, r)),
                    e if e == (wb, wa) => back += 1,
                    _ => {}
                }
            }
        }
        if let ([(j, r)], 1) = (fwd.as_slice(), back)
            && !on_line(pos(out[*j].0))
        {
            let (u, src) = out[*j];
            let (a2, b2, c) = (u[*r], u[(*r + 1) % 3], u[(*r + 2) % 3]);
            out[*j] = ([a2, m, c], src);
            out.push(([m, b2, c], src));
            dropped.push(false);
            dropped[i] = true;
            halves.extend([*j, out.len() - 1]);
        }
        i += 1;
    }
    if halves.is_empty() {
        return out;
    }
    // A half facing the other way on another triangle's corners: a fin.
    let key = |t: [u32; 3]| {
        let t = wid(t);
        let r = (0..3).min_by_key(|&k| t[k]).unwrap_or(0);
        [t[r], t[(r + 1) % 3], t[(r + 2) % 3]]
    };
    let mut by: BTreeMap<[u32; 3], Vec<usize>> = BTreeMap::new();
    for (j, (u, _)) in out.iter().enumerate() {
        if !dropped[j] {
            by.entry(key(*u)).or_default().push(j);
        }
    }
    for h in halves {
        if dropped[h] {
            continue;
        }
        let [x, y, z] = key(out[h].0);
        let twin = by
            .get(&key([z, y, x]))
            .and_then(|v| v.iter().copied().find(|&j| j != h && !dropped[j]));
        if let Some(j) = twin {
            dropped[h] = true;
            dropped[j] = true;
        }
    }
    out.into_iter()
        .zip(dropped)
        .filter(|(_, d)| !d)
        .map(|(x, _)| x)
        .collect()
}

/// Removes `s`'s slivers (the module's rule): each half keeps its
/// triangle's surface and face names, and a name on a split edge goes to
/// both its pieces. Returns whether anything changed; the solid is
/// untouched when nothing did. Vertices are not renumbered, so call
/// [`Solid::canonicalize`] after if order matters.
pub fn drop_slivers(s: &mut Solid) -> bool {
    let any = s.triangles.iter().any(|t| {
        let p = t.map(|v| s.positions[v as usize]);
        p[0] != p[1] && p[1] != p[2] && p[0] != p[2] && on_line(p)
    });
    if !any {
        return false;
    }
    let mut at: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    let w: Vec<u32> = s
        .positions
        .iter()
        .map(|p| {
            let n = at.len() as u32;
            *at.entry(p.to_array().map(|x| (x + 0.0).to_bits()))
                .or_insert(n)
        })
        .collect();
    let out = flip(&s.positions, &w, &s.triangles);
    let changed = out.len() != s.triangles.len()
        || out
            .iter()
            .enumerate()
            .any(|(i, &(t, src))| src != i || t != s.triangles[i]);
    if !changed {
        return false;
    }
    // Edges split at a sliver's middle corner: (a, b) became (a, m) and
    // (m, b), seen as an input edge missing from the output whose two
    // pieces are.
    let edge = |a: u32, b: u32| (a.min(b), a.max(b));
    let mut present: BTreeMap<(u32, u32), ()> = BTreeMap::new();
    for (t, _) in &out {
        for k in 0..3 {
            present.insert(edge(t[k], t[(k + 1) % 3]), ());
        }
    }
    // (the edge, its pieces)
    type Split = ((u32, u32), Vec<(u32, u32)>);
    let mut moved: Vec<Split> = Vec::new();
    for &(a, b) in s.edge_tags.keys() {
        if present.contains_key(&(a, b)) {
            continue;
        }
        let pieces: Vec<(u32, u32)> = out
            .iter()
            .flat_map(|(t, _)| t.iter().copied())
            .filter(|&m| {
                m != a
                    && m != b
                    && present.contains_key(&edge(a, m))
                    && present.contains_key(&edge(m, b))
                    && on_line([a, m, b].map(|v| s.positions[v as usize]))
            })
            .take(1)
            .flat_map(|m| [edge(a, m), edge(m, b)])
            .collect();
        moved.push(((a, b), pieces));
    }
    for (from, to) in moved {
        if let Some(tags) = s.edge_tags.remove(&from) {
            for e in to {
                let entry = s.edge_tags.entry(e).or_default();
                entry.extend(&tags);
                entry.sort_unstable();
                entry.dedup();
            }
        }
    }
    s.surfaces = out.iter().map(|&(_, src)| s.surfaces[src]).collect();
    s.face_tags = out
        .iter()
        .map(|(_, src)| s.face_tags[*src].clone())
        .collect();
    s.triangles = out.into_iter().map(|(t, _)| t).collect();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tetrahedron whose face `(a, b, d)` is split at the middle `m` of
    /// edge `ab` and closed by the sliver `(a, b, m)`: the sliver goes,
    /// the bottom `(a, c, b)` is split at `m`, and the name on edge `ab` lands on
    /// both its halves.
    #[test]
    fn a_sliver_is_flipped_into_its_neighbour() {
        let mut s = Solid::default();
        let [a, b, c, d, m] = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(2.0, 0.0, 0.0),
            DVec3::new(1.0, 2.0, 0.0),
            DVec3::new(1.0, 1.0, 2.0),
            DVec3::new(1.0, 0.0, 0.0),
        ]
        .map(|p| s.push_vertex(p));
        s.push_triangle([a, c, b], 0, &["bottom"]);
        s.push_triangle([a, m, d], 1, &["front"]);
        s.push_triangle([m, b, d], 1, &["front"]);
        s.push_triangle([b, c, d], 2, &[]);
        s.push_triangle([c, a, d], 3, &[]);
        s.push_triangle([a, b, m], 1, &["front"]);
        let tag = s.names.intern("rim");
        s.edge_tags.insert((a.min(b), a.max(b)), vec![tag]);
        let before = s.measure();
        assert!(before.closed && before.degenerate_triangles == 1);
        assert!(drop_slivers(&mut s));
        let after = s.measure();
        assert!(after.closed && after.degenerate_triangles == 0, "{after:?}");
        assert_eq!(s.triangles.len(), 6);
        assert_eq!(after.volume, before.volume);
        assert_eq!(s.edge_tags.len(), 2);
        assert!(!drop_slivers(&mut s));
    }
}

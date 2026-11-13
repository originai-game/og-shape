//! Morph targets (blend shapes) on a [`crate::Solid`] (`docs/design.md`
//! §4, `docs/research.md` §43): named sets of per-vertex offsets, each
//! with a weight, that move the rest shape before a skin poses it.
//!
//! Entry: [`Morphs`]. A solid's `morphs` follow its vertices as its
//! skin's weights do: a new vertex has no offset until it is given one,
//! and vertices welded into one take the mean of their offsets.
//! [`carried`] gives a solid made from others (cut, joined, bevelled)
//! their targets.

use std::collections::BTreeMap;

use shape_core::DVec3;
use shape_core::order::cmp_dvec3;

use crate::Solid;

/// One target: where every vertex goes at weight 1, as an offset from
/// its rest position.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Morph {
    /// The target's name (unique in its solid).
    pub name: String,
    /// The weight the shape rests at (glTF's `weights`).
    pub weight: f64,
    /// Per vertex of the solid, its offset at weight 1, in the solid's
    /// units.
    pub deltas: Vec<DVec3>,
}

/// A solid's morph targets, in the file's order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Morphs {
    /// The targets.
    pub targets: Vec<Morph>,
}

/// Why a solid's morph targets are not valid.
#[derive(Clone, Debug, PartialEq)]
pub enum MorphError {
    /// Two targets share a name.
    DuplicateTarget(String),
    /// A target's offsets do not match the vertices one to one.
    VertexCount {
        /// The target.
        target: String,
        /// Its offsets.
        deltas: usize,
        /// The solid's vertices.
        vertices: usize,
    },
    /// A target has an offset or a weight that is not finite.
    NotFinite(String),
}

impl core::fmt::Display for MorphError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MorphError::DuplicateTarget(n) => write!(f, "two morph targets are named {n:?}"),
            MorphError::VertexCount {
                target,
                deltas,
                vertices,
            } => write!(
                f,
                "morph target {target:?} has {deltas} offsets for {vertices} vertices"
            ),
            MorphError::NotFinite(n) => {
                write!(
                    f,
                    "morph target {n:?} has an offset or weight that is not finite"
                )
            }
        }
    }
}

impl Morphs {
    /// Checks the targets against a solid of `vertices` vertices.
    pub fn check(&self, vertices: usize) -> Result<(), MorphError> {
        for (i, t) in self.targets.iter().enumerate() {
            if self.targets[..i].iter().any(|o| o.name == t.name) {
                return Err(MorphError::DuplicateTarget(t.name.clone()));
            }
            if t.deltas.len() != vertices {
                return Err(MorphError::VertexCount {
                    target: t.name.clone(),
                    deltas: t.deltas.len(),
                    vertices,
                });
            }
            if !t.weight.is_finite() || t.deltas.iter().any(|d| !d.is_finite()) {
                return Err(MorphError::NotFinite(t.name.clone()));
            }
        }
        Ok(())
    }

    /// Every target given `vertices` offsets: new ones zero.
    pub fn resize(&mut self, vertices: usize) {
        for t in &mut self.targets {
            t.deltas.resize(vertices, DVec3::ZERO);
        }
    }

    /// The targets' rest weights.
    pub fn weights(&self) -> Vec<f64> {
        self.targets.iter().map(|t| t.weight).collect()
    }

    /// Vertex `to` given vertex `from`'s offsets (a copy of it).
    pub fn copy(&mut self, from: usize, to: usize) {
        for t in &mut self.targets {
            if let Some(d) = t.deltas.get(from).copied()
                && let Some(slot) = t.deltas.get_mut(to)
            {
                *slot = d;
            }
        }
    }

    /// Vertex `to` given the weighted sum of other vertices' offsets
    /// (`items`: vertex and share).
    pub fn blend(&mut self, to: usize, items: &[(usize, f64)]) {
        for t in &mut self.targets {
            let mut sum = DVec3::ZERO;
            for &(v, share) in items {
                sum += t.deltas.get(v).copied().unwrap_or(DVec3::ZERO) * share;
            }
            if let Some(slot) = t.deltas.get_mut(to) {
                *slot = sum;
            }
        }
    }

    /// The targets on new vertex indices (`remap[old]`, or `u32::MAX`
    /// for a vertex that is gone): vertices merged into one take the
    /// mean of their offsets, sorted first so their order does not
    /// matter; a new vertex no old one maps to has none.
    pub fn remapped(&self, remap: &[u32], vertices: usize) -> Morphs {
        let mut groups: Vec<Vec<u32>> = vec![Vec::new(); vertices];
        for (old, &new) in remap.iter().enumerate() {
            if let Some(g) = groups.get_mut(new as usize) {
                g.push(old as u32);
            }
        }
        Morphs {
            targets: self
                .targets
                .iter()
                .map(|t| Morph {
                    name: t.name.clone(),
                    weight: t.weight,
                    deltas: groups
                        .iter()
                        .map(|g| {
                            let mut copies: Vec<DVec3> = g
                                .iter()
                                .map(|&o| t.deltas.get(o as usize).copied().unwrap_or(DVec3::ZERO))
                                .collect();
                            weld(&mut copies)
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// How many groups of vertices `remap` merges whose offsets differ
    /// in some target: a place the mean changes the shape (lips that
    /// touch at rest and part in a target).
    pub fn torn(&self, remap: &[u32], vertices: usize) -> usize {
        let mut first: Vec<u32> = vec![u32::MAX; vertices];
        let mut torn = vec![false; vertices];
        for (old, &new) in remap.iter().enumerate() {
            let Some(slot) = first.get_mut(new as usize) else {
                continue;
            };
            if *slot == u32::MAX {
                *slot = old as u32;
            } else if self.targets.iter().any(|t| {
                t.deltas
                    .get(old)
                    .is_some_and(|d| cmp_dvec3(*d, t.deltas[*slot as usize]).is_ne())
            }) {
                torn[new as usize] = true;
            }
        }
        torn.iter().filter(|t| **t).count()
    }

    /// `positions` moved by the targets at `weights` (one a target; a
    /// missing one is the target's rest weight).
    pub fn applied(&self, positions: &[DVec3], weights: &[f64]) -> Vec<DVec3> {
        let mut out = positions.to_vec();
        for (k, t) in self.targets.iter().enumerate() {
            let w = weights.get(k).copied().unwrap_or(t.weight);
            if w == 0.0 {
                continue;
            }
            for (p, d) in out.iter_mut().zip(&t.deltas) {
                *p += *d * w;
            }
        }
        out
    }
}

/// The offset of vertices welded into one: their mean, summed in sorted
/// order.
fn weld(copies: &mut [DVec3]) -> DVec3 {
    let Some(first) = copies.first().copied() else {
        return DVec3::ZERO;
    };
    if copies.iter().all(|c| cmp_dvec3(*c, first).is_eq()) {
        return first;
    }
    copies.sort_by(|a, b| cmp_dvec3(*a, *b));
    let mut sum = DVec3::ZERO;
    for c in copies.iter() {
        sum += *c;
    }
    sum / copies.len() as f64
}

/// The closest point of triangle `t` to `p`, as weights of its corners.
fn closest(p: DVec3, t: [DVec3; 3]) -> [f64; 3] {
    let (ab, ac, ap) = (t[1] - t[0], t[2] - t[0], p - t[0]);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let bp = p - t[1];
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return [0.0, 1.0, 0.0];
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = if d1 - d3 != 0.0 { d1 / (d1 - d3) } else { 0.0 };
        return [1.0 - v, v, 0.0];
    }
    let cp = p - t[2];
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return [0.0, 0.0, 1.0];
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = if d2 - d6 != 0.0 { d2 / (d2 - d6) } else { 0.0 };
        return [1.0 - w, 0.0, w];
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let sum = (d4 - d3) + (d5 - d6);
        let w = if sum != 0.0 { (d4 - d3) / sum } else { 0.0 };
        return [0.0, 1.0 - w, w];
    }
    let sum = va + vb + vc;
    if sum == 0.0 {
        return [1.0, 0.0, 0.0];
    }
    [va / sum, vb / sum, vc / sum]
}

/// The morph targets of `sources` carried onto `onto`, a solid made
/// from them (cut, joined, bevelled): each of its vertices takes, for
/// each target, the offset of the nearest point of the sources'
/// surfaces, blended from that triangle's corners. A vertex that was a
/// source's own keeps its offsets exactly; one a cut made on a face
/// moves as the face does there; one on a source without a target does
/// not move for it. Targets are those of every source, by name, in the
/// order met. `None` when no source has any.
pub fn carried(sources: &[&Solid], onto: &Solid) -> Option<Morphs> {
    let mut names: Vec<(String, f64)> = Vec::new();
    for s in sources {
        for t in s.morphs.iter().flat_map(|m| &m.targets) {
            if !names.iter().any(|(n, _)| *n == t.name) {
                names.push((t.name.clone(), t.weight));
            }
        }
    }
    if names.is_empty() {
        return None;
    }
    // Each source's targets, by the result's order (a missing one: none).
    let slots: Vec<Vec<Option<usize>>> = sources
        .iter()
        .map(|s| {
            names
                .iter()
                .map(|(n, _)| {
                    s.morphs
                        .as_ref()
                        .and_then(|m| m.targets.iter().position(|t| t.name == *n))
                })
                .collect()
        })
        .collect();
    // A grid of the sources' triangles.
    let mut edge_sum = 0.0;
    let mut count = 0usize;
    for s in sources {
        for t in &s.triangles {
            let [a, b, c] = t.map(|v| s.positions[v as usize]);
            edge_sum += (b - a).length() + (c - b).length() + (a - c).length();
            count += 3;
        }
    }
    let cell = if count > 0 && edge_sum > 0.0 {
        edge_sum / count as f64 * 2.0
    } else {
        1.0
    };
    let key = |p: DVec3| {
        (
            (p.x / cell).floor() as i64,
            (p.y / cell).floor() as i64,
            (p.z / cell).floor() as i64,
        )
    };
    let mut grid: BTreeMap<(i64, i64, i64), Vec<(u32, u32)>> = BTreeMap::new();
    let (mut low, mut high) = (
        (i64::MAX, i64::MAX, i64::MAX),
        (i64::MIN, i64::MIN, i64::MIN),
    );
    for (si, s) in sources.iter().enumerate() {
        for (ti, t) in s.triangles.iter().enumerate() {
            let [a, b, c] = t.map(|v| s.positions[v as usize]);
            let (lo, hi) = (key(a.min(b).min(c)), key(a.max(b).max(c)));
            for x in lo.0..=hi.0 {
                for y in lo.1..=hi.1 {
                    for z in lo.2..=hi.2 {
                        grid.entry((x, y, z))
                            .or_default()
                            .push((si as u32, ti as u32));
                    }
                }
            }
            low = (low.0.min(lo.0), low.1.min(lo.1), low.2.min(lo.2));
            high = (high.0.max(hi.0), high.1.max(hi.1), high.2.max(hi.2));
        }
    }
    let reach = if count > 0 {
        (high.0 - low.0).max(high.1 - low.1).max(high.2 - low.2) + 2
    } else {
        0
    };
    let mut targets: Vec<Morph> = names
        .into_iter()
        .map(|(name, weight)| Morph {
            name,
            weight,
            deltas: vec![DVec3::ZERO; onto.positions.len()],
        })
        .collect();
    for (v, &p) in onto.positions.iter().enumerate() {
        let at = key(p);
        // The nearest triangle: rings of cells outward until none
        // further out can be nearer.
        let mut best: Option<(f64, u32, u32, [f64; 3])> = None;
        let start = (at.0.clamp(low.0, high.0) - at.0)
            .abs()
            .max((at.1.clamp(low.1, high.1) - at.1).abs())
            .max((at.2.clamp(low.2, high.2) - at.2).abs());
        let mut ring = start;
        while ring <= start + reach {
            if let Some((d, ..)) = best
                && d <= ((ring - 1).max(0) as f64 * cell) * ((ring - 1).max(0) as f64 * cell)
            {
                break;
            }
            for dx in -ring..=ring {
                for dy in -ring..=ring {
                    for dz in -ring..=ring {
                        if dx.abs().max(dy.abs()).max(dz.abs()) != ring {
                            continue;
                        }
                        let cell_key = (at.0 + dx, at.1 + dy, at.2 + dz);
                        for &(si, ti) in grid.get(&cell_key).into_iter().flatten() {
                            let s = sources[si as usize];
                            let t = s.triangles[ti as usize].map(|i| s.positions[i as usize]);
                            let w = closest(p, t);
                            let q = t[0] * w[0] + t[1] * w[1] + t[2] * w[2];
                            let d = (q - p).length_squared();
                            let nearer = match best {
                                None => true,
                                Some((b, bs, bt, _)) => d < b || (d == b && (si, ti) < (bs, bt)),
                            };
                            if nearer {
                                best = Some((d, si, ti, w));
                            }
                        }
                    }
                }
            }
            ring += 1;
        }
        let Some((_, si, ti, w)) = best else { continue };
        let s = sources[si as usize];
        let corners = s.triangles[ti as usize];
        for (k, target) in targets.iter_mut().enumerate() {
            let Some(slot) = slots[si as usize][k] else {
                continue;
            };
            let Some(deltas) = s.morphs.as_ref().map(|m| &m.targets[slot].deltas) else {
                continue;
            };
            target.deltas[v] = deltas[corners[0] as usize] * w[0]
                + deltas[corners[1] as usize] * w[1]
                + deltas[corners[2] as usize] * w[2];
        }
    }
    Some(Morphs { targets })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Solid;

    /// A square of two triangles, built as a file has it: the diagonal's
    /// two vertices twice. Target `lift` raises one corner; `tear` moves
    /// the two copies of a diagonal vertex apart.
    fn square() -> Solid {
        let mut s = Solid::default();
        let p = [DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0), DVec3::Y];
        for k in [0, 1, 2, 0, 2, 3] {
            s.push_vertex(p[k]);
        }
        s.push_triangle([0, 1, 2], 0, &["a"]);
        s.push_triangle([3, 4, 5], 0, &["a"]);
        let mut lift = vec![DVec3::ZERO; 6];
        lift[1] = DVec3::Z;
        let mut tear = vec![DVec3::ZERO; 6];
        tear[2] = DVec3::Z;
        tear[4] = -DVec3::Z;
        s.morphs = Some(Morphs {
            targets: vec![
                Morph {
                    name: "lift".into(),
                    weight: 0.5,
                    deltas: lift,
                },
                Morph {
                    name: "tear".into(),
                    weight: 0.0,
                    deltas: tear,
                },
            ],
        });
        s
    }

    #[test]
    fn targets_follow_their_vertices_through_a_weld_a_map_and_a_join() {
        let mut s = square();
        let plain = {
            let mut p = square();
            p.morphs = None;
            p.canonicalize().unwrap();
            p
        };
        // Welding: the copies that agree keep their offset; the two that
        // part take the mean, and `torn` counts the place.
        let order = shape_core::order::sorted_order(&s.positions);
        let mut remap = vec![0u32; 6];
        let mut groups = 0u32;
        for (k, &v) in order.iter().enumerate() {
            if k > 0 && s.positions[order[k - 1] as usize] != s.positions[v as usize] {
                groups += 1;
            }
            remap[v as usize] = groups;
        }
        assert_eq!(s.morphs.as_ref().unwrap().torn(&remap, 4), 1);
        s.canonicalize().unwrap();
        assert_eq!(s.positions, plain.positions);
        let m = s.morphs.clone().unwrap();
        m.check(4).unwrap();
        let at = |p: DVec3| s.positions.iter().position(|q| *q == p).unwrap();
        assert_eq!(m.targets[0].deltas[at(DVec3::X)], DVec3::Z);
        assert_eq!(
            m.targets[1].deltas[at(DVec3::new(1.0, 1.0, 0.0))],
            DVec3::ZERO
        );
        // The hash is the plain solid's only without targets.
        let hash = |s: &Solid| {
            let mut h = shape_core::OutputHash::new();
            s.hash_into(&mut h).unwrap();
            h.finish_hex()
        };
        assert_ne!(hash(&s), hash(&plain));
        // At its rest weights the lifted corner stands half way up; at
        // weights given, where they put it.
        assert_eq!(
            s.morphed(&[]).positions[at(DVec3::X)],
            DVec3::new(1.0, 0.0, 0.5)
        );
        let up = s.morphed(&[2.0, 0.0]);
        assert_eq!(up.positions[at(DVec3::X)], DVec3::new(1.0, 0.0, 2.0));
        assert!(up.morphs.is_none());
        // A map scales and turns an offset and does not shift it.
        let mut big = s.clone();
        big.map_positions(|p| p * 3.0 + DVec3::new(7.0, 0.0, 0.0), false);
        let d = &big.morphs.as_ref().unwrap().targets[0].deltas;
        assert_eq!(d[at(DVec3::X)], DVec3::new(0.0, 0.0, 3.0));
        // Joined with a solid that has another target, each side's
        // targets move its own vertices only.
        let mut other = plain.clone();
        other.map_positions(|p| p + DVec3::new(5.0, 0.0, 0.0), false);
        other.morphs = Some(Morphs {
            targets: vec![Morph {
                name: "sink".into(),
                weight: 0.0,
                deltas: vec![-DVec3::Z; 4],
            }],
        });
        let mut both = s.clone();
        both.append(&other);
        let m = both.morphs.as_ref().unwrap();
        m.check(8).unwrap();
        let names: Vec<&str> = m.targets.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["lift", "tear", "sink"]);
        assert!(m.targets[2].deltas[..4].iter().all(|d| *d == DVec3::ZERO));
        assert!(m.targets[0].deltas[4..].iter().all(|d| *d == DVec3::ZERO));
    }
}

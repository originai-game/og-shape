//! Morph targets (blend shapes) on a [`crate::Solid`] (`docs/design.md`
//! §4, `docs/research.md` §43): named sets of per-vertex offsets, each
//! with a weight, that move the rest shape before a skin poses it.
//!
//! Entry: [`Morphs`]. A solid's `morphs` follow its vertices as its
//! skin's weights do: a new vertex has no offset until it is given one,
//! and vertices welded into one take the mean of their offsets.

use shape_core::DVec3;
use shape_core::order::cmp_dvec3;

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

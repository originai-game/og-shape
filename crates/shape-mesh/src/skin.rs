//! A solid's skin: a skeleton of named bones and up to four bone weights
//! per vertex (`docs/design.md` §4, `docs/plan.md` phase 5b).
//!
//! A skin is optional on a [`crate::Solid`]. A solid without one is the
//! same as before skins existed: same bytes, same hash.
//!
//! Weights are plain data, made to be interpolated: [`Weights::blend`]
//! sums weighted copies per bone, keeps the four largest and normalizes.
//! Welding (in [`crate::Solid::canonicalize`]) and, later, decimation and
//! repair carry weights through it.

use core::cmp::Ordering;
use core::fmt;
use std::collections::BTreeMap;

use shape_core::{DVec3, Frame};

/// A 4×4 matrix, column-major (glTF's order): `m[4 * column + row]`.
pub type Mat4 = [f64; 16];

/// The 4×4 matrix of a frame: local coordinates to world.
pub fn frame_matrix(f: &Frame) -> Mat4 {
    [
        f.x.x, f.x.y, f.x.z, 0.0, //
        f.y.x, f.y.y, f.y.z, 0.0, //
        f.z.x, f.z.y, f.z.z, 0.0, //
        f.origin.x, f.origin.y, f.origin.z, 1.0,
    ]
}

/// The 4×4 matrix of a frame's inverse: world coordinates to local. Exact
/// for an orthonormal frame (the transpose, not a general inverse).
pub fn frame_inverse_matrix(f: &Frame) -> Mat4 {
    let t = DVec3::new(-f.x.dot(f.origin), -f.y.dot(f.origin), -f.z.dot(f.origin));
    [
        f.x.x, f.y.x, f.z.x, 0.0, //
        f.x.y, f.y.y, f.z.y, 0.0, //
        f.x.z, f.y.z, f.z.z, 0.0, //
        t.x, t.y, t.z, 1.0,
    ]
}

/// One bone of a skeleton.
#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    /// Unique in the skeleton: the glTF node's name.
    pub name: String,
    /// The parent's index in [`Skin::bones`], which comes before this
    /// bone; `None` for a root.
    pub parent: Option<u32>,
    /// The bone's frame in the solid's coordinates at rest (the bind
    /// pose): its origin is the pivot it turns about.
    pub rest: Frame,
    /// Solid coordinates to the bone's at bind time (glTF's inverse bind
    /// matrix, kernel units). For a bone built from its rest frame it is
    /// the rest frame's inverse; an imported bone keeps the file's.
    pub inverse_bind: Mat4,
}

impl Bone {
    /// A bone whose inverse bind matrix is its rest frame's inverse.
    pub fn new(name: &str, parent: Option<u32>, rest: Frame) -> Bone {
        Bone {
            name: name.to_string(),
            parent,
            rest,
            inverse_bind: frame_inverse_matrix(&rest),
        }
    }
}

/// A vertex's bone weights: up to four `(bone, weight)` pairs.
///
/// Canonical form, which every constructor here returns: the used pairs
/// first, by weight high to low, then by bone index; the unused ones are
/// `(0, 0.0)`; the weights sum to 1. The zero value has no weights at
/// all, which [`Skin::check`] reports.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Weights {
    /// Bone indices into [`Skin::bones`].
    pub bones: [u32; 4],
    /// The weights, each in `0..=1`.
    pub weights: [f64; 4],
}

impl Weights {
    /// Fully weighted to one bone.
    pub fn single(bone: u32) -> Weights {
        Weights {
            bones: [bone, 0, 0, 0],
            weights: [1.0, 0.0, 0.0, 0.0],
        }
    }

    /// The used `(bone, weight)` pairs.
    pub fn pairs(&self) -> impl Iterator<Item = (u32, f64)> + '_ {
        self.bones
            .iter()
            .zip(&self.weights)
            .filter(|(_, w)| **w > 0.0)
            .map(|(&b, &w)| (b, w))
    }

    /// Weights from any `(bone, raw weight)` list: equal bones summed,
    /// weights not above zero (or NaN) dropped, the four largest kept (ties
    /// to the lower bone), normalized to sum 1. An empty result is the
    /// zero value.
    pub fn from_raw(raw: impl IntoIterator<Item = (u32, f64)>) -> Weights {
        let mut sum: BTreeMap<u32, f64> = BTreeMap::new();
        for (b, w) in raw {
            if w > 0.0 {
                *sum.entry(b).or_insert(0.0) += w;
            }
        }
        let mut list: Vec<(u32, f64)> = sum.into_iter().collect();
        list.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        list.truncate(4);
        let total: f64 = list.iter().map(|p| p.1).sum();
        let mut out = Weights::default();
        if total > 0.0 && total.is_finite() {
            for (k, (b, w)) in list.into_iter().enumerate() {
                out.bones[k] = b;
                out.weights[k] = w / total;
            }
        }
        out
    }

    /// The interpolation of weights: `Σ factor · weights` per bone, then
    /// [`Weights::from_raw`]. Weights without any pair contribute nothing.
    /// The result depends on the order of `items` only in the last bits;
    /// callers that need order independence sort first (see
    /// [`Weights::cmp_total`]).
    pub fn blend(items: &[(Weights, f64)]) -> Weights {
        Weights::from_raw(
            items
                .iter()
                .flat_map(|(w, f)| w.pairs().map(move |(b, x)| (b, x * f))),
        )
    }

    /// A total order on weights (bones, then the weights' bits), for
    /// sorting before a blend.
    pub fn cmp_total(&self, other: &Weights) -> Ordering {
        self.bones.cmp(&other.bones).then_with(|| {
            self.weights
                .iter()
                .zip(&other.weights)
                .map(|(a, b)| a.total_cmp(b))
                .find(|o| o.is_ne())
                .unwrap_or(Ordering::Equal)
        })
    }
}

/// A skeleton and per-vertex weights.
///
/// `weights[i]` belongs to the solid's vertex `i`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skin {
    /// The bones, parents before children.
    pub bones: Vec<Bone>,
    /// One entry per vertex of the solid.
    pub weights: Vec<Weights>,
}

/// Why a skin is not valid.
#[derive(Clone, Debug, PartialEq)]
pub enum SkinError {
    /// The skeleton has no bones.
    NoBones,
    /// Two bones share a name.
    DuplicateBone(String),
    /// A bone's parent is not a bone declared before it.
    BadParent(String),
    /// A bone's rest frame or inverse bind matrix is not finite, or its
    /// rest frame is not orthonormal and right-handed.
    BadFrame(String),
    /// The weights do not match the vertices one to one.
    VertexCount {
        /// Weight entries.
        weights: usize,
        /// Vertices of the solid.
        vertices: usize,
    },
    /// A vertex has no weight (e.g. it was added by an operation that
    /// does not carry skins).
    Unweighted(u32),
    /// A vertex's weights name a missing bone, are negative or not
    /// finite, or do not sum to 1.
    BadWeights(u32),
}

impl fmt::Display for SkinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SkinError::NoBones => f.write_str("the skin has no bones"),
            SkinError::DuplicateBone(n) => write!(f, "two bones are named {n:?}"),
            SkinError::BadParent(n) => write!(
                f,
                "bone {n:?}: its parent must be a bone declared before it"
            ),
            SkinError::BadFrame(n) => write!(
                f,
                "bone {n:?}: its rest frame must be finite, orthonormal and right-handed, and its inverse bind matrix finite"
            ),
            SkinError::VertexCount { weights, vertices } => write!(
                f,
                "the skin has {weights} weight entries for {vertices} vertices"
            ),
            SkinError::Unweighted(v) => write!(f, "vertex {v} has no bone weight"),
            SkinError::BadWeights(v) => write!(
                f,
                "vertex {v}: its weights must name existing bones, lie in 0..=1 and sum to 1"
            ),
        }
    }
}

impl std::error::Error for SkinError {}

/// Weights within this of 1 in sum count as normalized.
const SUM_TOLERANCE: f64 = 1e-9;

impl Skin {
    /// Checks the skin against a solid of `vertices` vertices: bones
    /// unique, parents first, frames rigid; one normalized weight entry
    /// per vertex.
    pub fn check(&self, vertices: usize) -> Result<(), SkinError> {
        if self.bones.is_empty() {
            return Err(SkinError::NoBones);
        }
        let mut seen: BTreeMap<&str, ()> = BTreeMap::new();
        for (i, b) in self.bones.iter().enumerate() {
            if seen.insert(b.name.as_str(), ()).is_some() {
                return Err(SkinError::DuplicateBone(b.name.clone()));
            }
            if b.parent.is_some_and(|p| p as usize >= i) {
                return Err(SkinError::BadParent(b.name.clone()));
            }
            let f = &b.rest;
            let finite = [f.origin, f.x, f.y, f.z].iter().all(|v| v.is_finite())
                && b.inverse_bind.iter().all(|v| v.is_finite());
            let rigid = (f.x.length() - 1.0).abs() < 1e-9
                && (f.y.length() - 1.0).abs() < 1e-9
                && (f.z.length() - 1.0).abs() < 1e-9
                && f.x.dot(f.y).abs() < 1e-9
                && f.x.cross(f.y).dot(f.z) > 0.999_999;
            if !finite || !rigid {
                return Err(SkinError::BadFrame(b.name.clone()));
            }
        }
        if self.weights.len() != vertices {
            return Err(SkinError::VertexCount {
                weights: self.weights.len(),
                vertices,
            });
        }
        let n = self.bones.len() as u32;
        for (v, w) in self.weights.iter().enumerate() {
            let v = v as u32;
            if w.weights.iter().all(|&x| x == 0.0) {
                return Err(SkinError::Unweighted(v));
            }
            let ok = w.bones.iter().all(|&b| b < n)
                && w.weights.iter().all(|&x| (0.0..=1.0).contains(&x));
            let sum: f64 = w.weights.iter().sum();
            if !ok || (sum - 1.0).abs() > SUM_TOLERANCE {
                return Err(SkinError::BadWeights(v));
            }
        }
        Ok(())
    }

    /// The index of the bone named `name`.
    pub fn bone(&self, name: &str) -> Option<u32> {
        self.bones
            .iter()
            .position(|b| b.name == name)
            .map(|i| i as u32)
    }

    /// The weights of the vertices welded into one, in any order: the
    /// mean of those that carry weights (sorted first, so the result does
    /// not depend on their order). With none, the zero value.
    pub fn weld(copies: &mut [Weights]) -> Weights {
        if let Some(first) = copies.first()
            && copies.iter().all(|w| w == first)
        {
            return *first;
        }
        copies.sort_by(|a, b| a.cmp_total(b));
        let items: Vec<(Weights, f64)> = copies.iter().map(|w| (*w, 1.0)).collect();
        Weights::blend(&items)
    }
}

/// A motion of a skin over time, as it was read from a file: at each key
/// time, every bone's skinning matrix (solid coordinates at rest to
/// solid coordinates then; what `Σ weight · matrix` moves a vertex by).
///
/// The file's curves are sampled at their own key times when it is read,
/// so whatever drives a bone there (a chain of nodes, an easing curve) is
/// in the samples, and between two keys the matrices run straight. The
/// matrices need not be rigid: a bone the file scales is scaled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkinClip {
    /// The clip's name in the file.
    pub name: String,
    /// The key times, in seconds, rising.
    pub times: Vec<f64>,
    /// Per key, per bone (in the skin's order): the skinning matrix.
    pub poses: Vec<Vec<Mat4>>,
    /// Per key, per morph target (in the solid's order): its weight.
    /// Empty when the clip does not drive the targets.
    pub morphs: Vec<Vec<f64>>,
}

/// `a · b`, column-major.
pub fn mat_mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[4 * c + r] = (0..4).map(|k| a[4 * k + r] * b[4 * c + k]).sum();
        }
    }
    out
}

/// The inverse of an affine matrix (its last row 0, 0, 0, 1), if its
/// linear part is not singular.
pub fn affine_inverse(m: &Mat4) -> Option<Mat4> {
    let a = |r: usize, c: usize| m[4 * c + r];
    let cof =
        |r0: usize, r1: usize, c0: usize, c1: usize| a(r0, c0) * a(r1, c1) - a(r0, c1) * a(r1, c0);
    let det = a(0, 0) * cof(1, 2, 1, 2) - a(0, 1) * cof(1, 2, 0, 2) + a(0, 2) * cof(1, 2, 0, 1);
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    // The adjugate over the determinant.
    let inv = [
        [cof(1, 2, 1, 2), -cof(0, 2, 1, 2), cof(0, 1, 1, 2)],
        [-cof(1, 2, 0, 2), cof(0, 2, 0, 2), -cof(0, 1, 0, 2)],
        [cof(1, 2, 0, 1), -cof(0, 2, 0, 1), cof(0, 1, 0, 1)],
    ];
    let mut out = [0.0; 16];
    for r in 0..3 {
        for c in 0..3 {
            out[4 * c + r] = inv[r][c] / det;
        }
    }
    for r in 0..3 {
        out[12 + r] = -(0..3).map(|c| out[4 * c + r] * m[12 + c]).sum::<f64>();
    }
    out[15] = 1.0;
    Some(out)
}

impl SkinClip {
    /// The clip of the same skin once its solid has been moved by the
    /// affine `m` (a placement: a turn, a scale, a shift): each matrix
    /// `m · S · m⁻¹`. `None` if `m` is singular.
    pub fn moved(&self, m: &Mat4) -> Option<SkinClip> {
        let inverse = affine_inverse(m)?;
        Some(SkinClip {
            name: self.name.clone(),
            times: self.times.clone(),
            morphs: self.morphs.clone(),
            poses: self
                .poses
                .iter()
                .map(|pose| {
                    pose.iter()
                        .map(|s| mat_mul(&mat_mul(m, s), &inverse))
                        .collect()
                })
                .collect(),
        })
    }

    /// The last key's time; 0 for a clip without keys.
    pub fn duration(&self) -> f64 {
        self.times.last().copied().unwrap_or(0.0)
    }

    /// Every morph target's weight at `time`, as [`SkinClip::at`] takes
    /// the matrices. `None` for a clip that does not drive the targets.
    pub fn weights_at(&self, time: f64) -> Option<Vec<f64>> {
        let last = self.times.len().min(self.morphs.len()).checked_sub(1)?;
        let k = self.times[..=last].partition_point(|&t| t <= time);
        if k == 0 {
            return self.morphs.first().cloned();
        }
        if k > last {
            return self.morphs.get(last).cloned();
        }
        let (t0, t1) = (self.times[k - 1], self.times[k]);
        let f = if t1 > t0 {
            (time - t0) / (t1 - t0)
        } else {
            0.0
        };
        let (a, b) = (&self.morphs[k - 1], &self.morphs[k]);
        Some(a.iter().zip(b).map(|(x, y)| x + (y - x) * f).collect())
    }

    /// Every bone's skinning matrix at `time`: the first key's before it,
    /// the last key's after it, between two keys their matrices mixed
    /// element by element. `None` for a clip without keys.
    pub fn at(&self, time: f64) -> Option<Vec<Mat4>> {
        let last = self.times.len().checked_sub(1)?;
        let k = self.times.partition_point(|&t| t <= time);
        if k == 0 {
            return self.poses.first().cloned();
        }
        if k > last {
            return self.poses.get(last).cloned();
        }
        let (t0, t1) = (self.times[k - 1], self.times[k]);
        let f = if t1 > t0 {
            (time - t0) / (t1 - t0)
        } else {
            0.0
        };
        let (a, b) = (self.poses.get(k - 1)?, self.poses.get(k)?);
        Some(
            a.iter()
                .zip(b)
                .map(|(x, y)| {
                    let mut m = [0.0; 16];
                    for i in 0..16 {
                        m[i] = x[i] + (y[i] - x[i]) * f;
                    }
                    m
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_raw_keeps_four_largest_and_normalizes() {
        let w = Weights::from_raw([(3, 1.0), (1, 2.0), (5, 0.5), (2, 2.0), (7, 0.25), (1, 0.0)]);
        assert_eq!(w.bones, [1, 2, 3, 5]);
        let sum: f64 = w.weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-15);
        assert_eq!(w.weights[0], w.weights[1]);
        assert_eq!(Weights::from_raw([(0, 0.0)]), Weights::default());
    }

    #[test]
    fn weld_is_the_order_independent_mean() {
        let a = Weights::single(0);
        let b = Weights::from_raw([(0, 1.0), (1, 3.0)]);
        let ab = Skin::weld(&mut [a, b, Weights::default()]);
        let ba = Skin::weld(&mut [Weights::default(), b, a]);
        assert_eq!(ab, ba);
        assert_eq!(ab.bones[..2], [0, 1]);
        assert!((ab.weights[0] - 0.625).abs() < 1e-15);
        assert!((ab.weights[1] - 0.375).abs() < 1e-15);
    }

    #[test]
    fn check_reports_unweighted_vertices_and_bad_parents() {
        let mut s = Skin {
            bones: vec![Bone::new("root", None, Frame::IDENTITY)],
            weights: vec![Weights::single(0), Weights::default()],
        };
        assert_eq!(s.check(2), Err(SkinError::Unweighted(1)));
        assert_eq!(
            s.check(3),
            Err(SkinError::VertexCount {
                weights: 2,
                vertices: 3
            })
        );
        s.weights[1] = Weights::single(0);
        assert_eq!(s.check(2), Ok(()));
        s.bones.push(Bone::new("arm", Some(1), Frame::IDENTITY));
        assert_eq!(s.check(2), Err(SkinError::BadParent("arm".into())));
    }

    #[test]
    fn inverse_bind_undoes_the_rest_frame() {
        let f = Frame::from_normal(
            DVec3::new(1.0, 2.0, 3.0),
            DVec3::new(0.0, 1.0, 1.0),
            DVec3::X,
        )
        .unwrap();
        let (m, inv) = (frame_matrix(&f), frame_inverse_matrix(&f));
        let p = DVec3::new(-4.0, 0.5, 7.0);
        let apply = |m: &Mat4, p: DVec3| {
            DVec3::new(
                m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12],
                m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13],
                m[2] * p.x + m[6] * p.y + m[10] * p.z + m[14],
            )
        };
        assert!((apply(&inv, apply(&m, p)) - p).length() < 1e-12);
    }
}

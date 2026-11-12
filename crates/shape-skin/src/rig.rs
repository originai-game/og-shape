//! Bones from named regions, weights across seams, posing and measures.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use shape_core::{DVec3, Frame, trig};
use shape_mesh::skin::{Mat4, frame_inverse_matrix, frame_matrix};
use shape_mesh::{Bone, EdgeKey, Skin, SkinError, Solid, Weights, edge_key};

use crate::mat::{apply, apply_vector, mul, turn};
use crate::refine::{adjacency, distances, split_edges};

/// One bone of a [`Rig`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RigBone {
    /// Unique in the rig: the glTF joint node's name.
    pub name: String,
    /// The bone it hangs on, declared before it; empty for a root.
    pub parent: String,
    /// The pivot at rest, in the solid's coordinates: the bone turns about
    /// its origin, and a [`Turn`]'s axis is in its axes.
    pub pivot: Frame,
    /// The face names this bone moves: a face belongs to the bone if it
    /// carries a name `r` or `r.*` for one of these. Empty for a bone that
    /// only carries others (e.g. a hip above both legs).
    pub regions: Vec<String>,
}

/// How to skin a solid: its bones and how their weights blend.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rig {
    /// The bones, parents before children.
    pub bones: Vec<RigBone>,
    /// Width of the band across each seam over which the weights go from
    /// one bone to the other, in millimetres along the surface. 0 makes
    /// hard seams (a seam vertex is shared half and half).
    pub blend: f64,
    /// Edges longer than this that reach into a blend band are split
    /// until none is; 0 for `blend / 4`, infinity for no splitting.
    pub max_edge: f64,
}

/// Why a rig, a pose or a measure could not be honoured.
#[derive(Clone, Debug, PartialEq)]
pub enum RigError {
    /// A value was NaN or infinite where it must be finite (named).
    NotFinite(String),
    /// A size was negative (named).
    Negative(&'static str),
    /// The rig has no bones.
    NoBones,
    /// Two bones share a name.
    DuplicateBone(String),
    /// A bone hangs on a bone that is not declared before it.
    UnknownParent {
        /// The bone.
        bone: String,
        /// The parent it names.
        parent: String,
    },
    /// A bone's pivot is not orthonormal and right-handed.
    BadPivot(String),
    /// Region names no face carries; then every face name there is.
    NoSuchRegion(Vec<String>, Vec<String>),
    /// A face belongs to two bones: its names, then the bones.
    Ambiguous(Vec<String>, Vec<String>),
    /// Faces belong to no bone: the names of the first such face.
    Unassigned(Vec<String>),
    /// The solid already has a skin.
    AlreadySkinned,
    /// The solid has no skin to pose or measure.
    NotSkinned,
    /// A pose names a bone the skin lacks; then the bones there are.
    NoSuchBone(String, Vec<String>),
    /// A turn's axis has no length (its bone named).
    NoAxis(String),
    /// The skin itself is not valid.
    Skin(SkinError),
}

impl From<SkinError> for RigError {
    fn from(e: SkinError) -> Self {
        RigError::Skin(e)
    }
}

impl fmt::Display for RigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RigError::NotFinite(w) => write!(f, "{w} must be finite"),
            RigError::Negative(w) => write!(f, "{w} must not be negative"),
            RigError::NoBones => f.write_str("the rig has no bones"),
            RigError::DuplicateBone(n) => write!(f, "two bones are named {n:?}"),
            RigError::UnknownParent { bone, parent } => write!(
                f,
                "bone {bone:?} hangs on {parent:?}, which is not a bone declared before it"
            ),
            RigError::BadPivot(n) => {
                write!(
                    f,
                    "bone {n:?}: its pivot must be orthonormal and right-handed"
                )
            }
            RigError::NoSuchRegion(m, a) => write!(
                f,
                "no face is named {}; names: {}",
                m.join(", "),
                a.join(", ")
            ),
            RigError::Ambiguous(names, bones) => write!(
                f,
                "a face named {} belongs to the bones {}; give each region to one bone",
                names.join(", "),
                bones.join(", ")
            ),
            RigError::Unassigned(names) => write!(
                f,
                "faces named {} belong to no bone; add a region for them",
                names.join(", ")
            ),
            RigError::AlreadySkinned => f.write_str("the solid already has a skin"),
            RigError::NotSkinned => f.write_str("the solid has no skin"),
            RigError::NoSuchBone(n, a) => {
                write!(f, "no bone is named {n:?}; bones: {}", a.join(", "))
            }
            RigError::NoAxis(n) => write!(f, "a turn of bone {n:?} has an axis of zero length"),
            RigError::Skin(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for RigError {}

fn rigid(f: &Frame) -> bool {
    [f.origin, f.x, f.y, f.z].iter().all(|v| v.is_finite())
        && (f.x.length() - 1.0).abs() < 1e-9
        && (f.y.length() - 1.0).abs() < 1e-9
        && (f.z.length() - 1.0).abs() < 1e-9
        && f.x.dot(f.y).abs() < 1e-9
        && f.x.cross(f.y).dot(f.z) > 0.999_999
}

/// The skeleton of a rig, checked.
fn bones(rig: &Rig) -> Result<Vec<Bone>, RigError> {
    if rig.bones.is_empty() {
        return Err(RigError::NoBones);
    }
    let mut index: BTreeMap<&str, u32> = BTreeMap::new();
    let mut out = Vec::with_capacity(rig.bones.len());
    for b in &rig.bones {
        if index.contains_key(b.name.as_str()) {
            return Err(RigError::DuplicateBone(b.name.clone()));
        }
        let parent = if b.parent.is_empty() {
            None
        } else {
            Some(
                *index
                    .get(b.parent.as_str())
                    .ok_or_else(|| RigError::UnknownParent {
                        bone: b.name.clone(),
                        parent: b.parent.clone(),
                    })?,
            )
        };
        if !rigid(&b.pivot) {
            return Err(RigError::BadPivot(b.name.clone()));
        }
        index.insert(b.name.as_str(), out.len() as u32);
        out.push(Bone::new(&b.name, parent, b.pivot));
    }
    Ok(out)
}

/// Each triangle's bone.
fn owners(s: &Solid, rig: &Rig) -> Result<Vec<u32>, RigError> {
    let matches = |tag: &str, region: &str| {
        tag == region
            || (tag.len() > region.len()
                && tag.starts_with(region)
                && tag.as_bytes()[region.len()] == b'.')
    };
    // Every region must name some face.
    let all: BTreeSet<&str> = s
        .face_tags
        .iter()
        .flatten()
        .map(|&t| s.names.name(t))
        .collect();
    let missing: Vec<String> = rig
        .bones
        .iter()
        .flat_map(|b| &b.regions)
        .filter(|r| !all.iter().any(|t| matches(t, r)))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(RigError::NoSuchRegion(
            missing,
            all.iter().map(|t| t.to_string()).collect(),
        ));
    }
    let mut out = Vec::with_capacity(s.triangles.len());
    for tags in &s.face_tags {
        let names: Vec<&str> = tags.iter().map(|&t| s.names.name(t)).collect();
        let found: Vec<u32> = rig
            .bones
            .iter()
            .enumerate()
            .filter(|(_, b)| {
                b.regions
                    .iter()
                    .any(|r| names.iter().any(|t| matches(t, r)))
            })
            .map(|(i, _)| i as u32)
            .collect();
        let mut sorted: Vec<String> = names.iter().map(|t| t.to_string()).collect();
        sorted.sort();
        match found.as_slice() {
            [b] => out.push(*b),
            [] => return Err(RigError::Unassigned(sorted)),
            _ => {
                return Err(RigError::Ambiguous(
                    sorted,
                    found
                        .iter()
                        .map(|&b| rig.bones[b as usize].name.clone())
                        .collect(),
                ));
            }
        }
    }
    Ok(out)
}

/// Per vertex, the bones of the triangles around it (sorted).
fn vertex_bones(s: &Solid, owner: &[u32]) -> Vec<Vec<u32>> {
    let mut vb: Vec<Vec<u32>> = vec![Vec::new(); s.positions.len()];
    for (t, &b) in s.triangles.iter().zip(owner) {
        for &v in t {
            vb[v as usize].push(b);
        }
    }
    for l in &mut vb {
        l.sort_unstable();
        l.dedup();
    }
    vb
}

/// `1 − smoothstep(t)` for `t` in `0..=1`: 1 at 0, 0 at 1, flat at both.
fn falloff(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - t * t * (3.0 - 2.0 * t)
}

/// The solid with a skin built from the rig (see the crate docs for the
/// rule), in canonical order. A refined band adds vertices.
pub fn skin(s: &Solid, rig: &Rig) -> Result<Solid, RigError> {
    if s.skin.is_some() {
        return Err(RigError::AlreadySkinned);
    }
    if !rig.blend.is_finite() {
        return Err(RigError::NotFinite("blend".into()));
    }
    if rig.blend < 0.0 {
        return Err(RigError::Negative("blend"));
    }
    if rig.max_edge.is_nan() {
        return Err(RigError::NotFinite("max_edge".into()));
    }
    if rig.max_edge < 0.0 {
        return Err(RigError::Negative("max_edge"));
    }
    if s.positions.iter().any(|p| !p.is_finite()) {
        return Err(RigError::NotFinite("a position".into()));
    }
    let skeleton = bones(rig)?;
    let half = rig.blend / 2.0;
    let max_edge = if rig.max_edge == 0.0 {
        rig.blend / 4.0
    } else {
        rig.max_edge
    };
    let mut out = s.clone();
    let mut owner = owners(&out, rig)?;
    // Split long edges that reach into a band, until none does (each round
    // halves them; 32 rounds is far beyond any real mesh).
    if half > 0.0 && max_edge.is_finite() {
        for _ in 0..32 {
            let vb = vertex_bones(&out, &owner);
            let seam: Vec<u32> = (0..vb.len() as u32)
                .filter(|&v| vb[v as usize].len() > 1)
                .collect();
            let adj = adjacency(&out);
            let d = distances(&adj, &seam, half);
            let mut long: BTreeSet<EdgeKey> = BTreeSet::new();
            for t in &out.triangles {
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    let len = (out.positions[a as usize] - out.positions[b as usize]).length();
                    if len > max_edge && d[a as usize].min(d[b as usize]) < half {
                        long.insert(edge_key(a, b));
                    }
                }
            }
            if long.is_empty() {
                break;
            }
            out = split_edges(&out, &long);
            owner = owners(&out, rig)?;
        }
    }
    // Per bone, the surface distance from its faces, within the band.
    let vb = vertex_bones(&out, &owner);
    let adj = adjacency(&out);
    let mut raw: Vec<Vec<(u32, f64)>> = vec![Vec::new(); out.positions.len()];
    for b in 0..skeleton.len() as u32 {
        let sources: Vec<u32> = (0..vb.len() as u32)
            .filter(|&v| vb[v as usize].binary_search(&b).is_ok())
            .collect();
        if sources.is_empty() {
            continue;
        }
        if half > 0.0 {
            let d = distances(&adj, &sources, half);
            for (v, &dv) in d.iter().enumerate() {
                if dv < half {
                    raw[v].push((b, falloff(dv / half)));
                }
            }
        } else {
            for &v in &sources {
                raw[v as usize].push((b, 1.0));
            }
        }
    }
    let weights: Vec<Weights> = raw.into_iter().map(Weights::from_raw).collect();
    out.skin = Some(Skin {
        bones: skeleton,
        weights,
    });
    out.canonicalize()
        .map_err(|_| RigError::NotFinite("a position".into()))?;
    Ok(out)
}

/// A turn of one bone about its pivot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Turn {
    /// The bone's name.
    pub bone: String,
    /// The axis, in the bone's rest axes; zero is its +Z.
    pub axis: DVec3,
    /// Radians, right-handed about the axis.
    pub angle: f64,
}

/// A pose: bone turns, applied in order (several on one bone compose).
/// Bones not named stay at rest relative to their parent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pose {
    /// The turns.
    pub turns: Vec<Turn>,
}

/// Each bone's skinning matrix for the pose: solid coordinates at rest to
/// solid coordinates posed (`posed world · inverse bind`). At rest every
/// matrix of a bone built from its rest frame is the identity (to
/// rounding).
pub fn skin_matrices(skin: &Skin, pose: &Pose) -> Result<Vec<Mat4>, RigError> {
    let n = skin.bones.len();
    let mut motion: Vec<Mat4> = vec![frame_matrix(&Frame::IDENTITY); n];
    for t in &pose.turns {
        let Some(b) = skin.bone(&t.bone) else {
            return Err(RigError::NoSuchBone(
                t.bone.clone(),
                skin.bones.iter().map(|b| b.name.clone()).collect(),
            ));
        };
        if !t.axis.is_finite() || !t.angle.is_finite() {
            return Err(RigError::NotFinite(format!("a turn of bone {:?}", t.bone)));
        }
        let axis = if t.axis == DVec3::ZERO {
            DVec3::Z
        } else {
            t.axis
        };
        let len = axis.length();
        if len == 0.0 || !len.is_finite() {
            return Err(RigError::NoAxis(t.bone.clone()));
        }
        motion[b as usize] = mul(&motion[b as usize], &turn(axis / len, t.angle));
    }
    let mut world: Vec<Mat4> = Vec::with_capacity(n);
    for (i, b) in skin.bones.iter().enumerate() {
        // Rest relative to the parent, then the bone's own motion.
        let local = mul(&frame_matrix(&b.rest), &motion[i]);
        let w = match b.parent {
            Some(p) => {
                let p = p as usize;
                if p >= i {
                    return Err(SkinError::BadParent(b.name.clone()).into());
                }
                let rel = mul(&frame_inverse_matrix(&skin.bones[p].rest), &local);
                mul(&world[p], &rel)
            }
            None => local,
        };
        world.push(w);
    }
    Ok(world
        .iter()
        .zip(&skin.bones)
        .map(|(w, b)| mul(w, &b.inverse_bind))
        .collect())
}

/// The posed mesh: every vertex moved by `Σ weight · skinning matrix`
/// (linear blend skinning). Vertex `i` of the result is vertex `i` of
/// `s`, and triangles, surfaces and names are kept, so the result can be
/// previewed and measured against the rest mesh. It is a snapshot: it
/// carries no skin.
pub fn pose(s: &Solid, pose: &Pose) -> Result<Solid, RigError> {
    let skin = s.skin.as_ref().ok_or(RigError::NotSkinned)?;
    skin.check(s.positions.len())?;
    let m = skin_matrices(skin, pose)?;
    // Morph targets first, at their rest weights, then the bones.
    let mut out = s.morphed(&[]);
    out.skin = None;
    for (p, w) in out.positions.iter_mut().zip(&skin.weights) {
        let rest = *p;
        let mut q = DVec3::ZERO;
        for (b, x) in w.pairs() {
            q += apply(&m[b as usize], rest) * x;
        }
        *p = q;
    }
    Ok(out)
}

/// The mesh moved by skinning matrices given outright, one a bone (a
/// file's clip at a time, `SkinClip::at`), as [`pose`] moves it by a
/// pose's: a snapshot without a skin.
pub fn pose_by(s: &Solid, matrices: &[Mat4]) -> Result<Solid, RigError> {
    let skin = s.skin.as_ref().ok_or(RigError::NotSkinned)?;
    skin.check(s.positions.len())?;
    if matrices.len() != skin.bones.len() {
        return Err(RigError::NotFinite(format!(
            "{} matrices for {} bones",
            matrices.len(),
            skin.bones.len()
        )));
    }
    // Morph targets first, at their rest weights, then the bones.
    let mut out = s.morphed(&[]);
    out.skin = None;
    for (p, w) in out.positions.iter_mut().zip(&skin.weights) {
        let rest = *p;
        let mut q = DVec3::ZERO;
        for (b, x) in w.pairs() {
            q += apply(&matrices[b as usize], rest) * x;
        }
        *p = q;
    }
    Ok(out)
}

/// How a pose deforms the triangles (`docs/design.md` §7: measurements,
/// no thresholds).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PoseMeasures {
    /// Triangles measured.
    pub triangles: usize,
    /// The posed mesh is closed (every edge used once each way).
    pub closed: bool,
    /// The posed mesh's non-manifold edges.
    pub non_manifold_edges: usize,
    /// The largest angle, radians, between a posed triangle's normal and
    /// its rest normal moved by the skin (each corner's blend of bone
    /// matrices, averaged). Above π/2 the triangle is inverted.
    pub max_turn: f64,
    /// Triangles turned more than π/2 from where the skin moves their
    /// normal.
    pub inverted: usize,
    /// The inverted triangles' total area at rest.
    pub inverted_area: f64,
    /// The best shape among the inverted triangles: twice the area over
    /// the longest edge squared (√3/2 for an equilateral triangle, near 0
    /// for a sliver); 0 when none is inverted.
    pub inverted_quality: f64,
    /// Posed triangles of zero area.
    pub collapsed: usize,
    /// The smallest and largest ratio of posed to rest area.
    pub min_area_ratio: f64,
    /// See `min_area_ratio`.
    pub max_area_ratio: f64,
    /// The posed signed volume over the rest one.
    pub volume_ratio: f64,
}

/// Measures how `pose` deforms the skinned solid `s`.
pub fn measure_pose(s: &Solid, p: &Pose) -> Result<PoseMeasures, RigError> {
    let posed = pose(s, p)?;
    let skin = s.skin.as_ref().ok_or(RigError::NotSkinned)?;
    let m = skin_matrices(skin, p)?;
    let rest_m = s.measure();
    let posed_m = posed.measure();
    let mut out = PoseMeasures {
        triangles: s.triangles.len(),
        closed: posed_m.closed,
        non_manifold_edges: posed_m.non_manifold_edges,
        min_area_ratio: f64::INFINITY,
        max_area_ratio: 0.0,
        volume_ratio: if rest_m.volume != 0.0 {
            posed_m.volume / rest_m.volume
        } else {
            0.0
        },
        ..Default::default()
    };
    for t in &s.triangles {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        let [pa, pb, pc] = t.map(|v| posed.positions[v as usize]);
        let n0 = (b - a).cross(c - a);
        let n1 = (pb - pa).cross(pc - pa);
        let (l0, l1) = (n0.length(), n1.length());
        if l1 == 0.0 {
            out.collapsed += 1;
        }
        if l0 > 0.0 {
            let r = l1 / l0;
            out.min_area_ratio = out.min_area_ratio.min(r);
            out.max_area_ratio = out.max_area_ratio.max(r);
        }
        if l0 == 0.0 || l1 == 0.0 {
            continue;
        }
        // Where the skin moves the normal: each corner's blend of bone
        // matrices, averaged.
        let mut expected = DVec3::ZERO;
        for &v in t {
            for (bone, w) in skin.weights[v as usize].pairs() {
                expected += apply_vector(&m[bone as usize], n0 / l0) * w;
            }
        }
        if expected.length() == 0.0 {
            continue;
        }
        let cos = (expected.dot(n1 / l1) / expected.length()).clamp(-1.0, 1.0);
        let angle = trig::acos(cos);
        out.max_turn = out.max_turn.max(angle);
        if angle > trig::PI / 2.0 {
            out.inverted += 1;
            out.inverted_area += l0 / 2.0;
            let longest = [(b - a).length(), (c - b).length(), (a - c).length()]
                .into_iter()
                .fold(0.0, f64::max);
            out.inverted_quality = out.inverted_quality.max(l0 / (longest * longest));
        }
    }
    if out.min_area_ratio == f64::INFINITY {
        out.min_area_ratio = 0.0;
    }
    Ok(out)
}

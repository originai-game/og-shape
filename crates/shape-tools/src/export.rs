//! GLB, OBJ and STL from a [`Scene`] of named solids.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use shape_core::{DVec3, Frame, trig};
use shape_mesh::Solid;
use shape_mesh::skin::Mat4;

use crate::image::Texture;
use crate::json::{Json, f32n, nums};
use crate::surface::{METRES, face_normal, split, y_up};

/// How a [`Material`]'s `color` and `emissive` are given (three.js's
/// colour spaces).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorSpace {
    /// sRGB components, as a person picks them (three.js
    /// `SRGBColorSpace`); written to glTF converted to linear.
    #[default]
    Srgb,
    /// Linear components, as glTF's factors are (three.js
    /// `LinearSRGBColorSpace`); written as they are. An import gives this,
    /// so a file's factors go out unchanged.
    Linear,
}

/// What a part is made of: glTF's metallic-roughness model with
/// three.js's `MeshStandardMaterial` names. The zero value is three.js's
/// default material (white, roughness 1, not metal, opaque).
///
/// Every map is sampled at the solid's [`Solid::uvs`] (glTF `texCoord`
/// 0); a part with a map and no UVs is [`ExportError::BadTexture`], as is
/// a map that is not PNG or JPEG.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Material {
    /// The material's name in the file; empty takes the part's name.
    pub name: String,
    /// Base colour, components in 0..=1 in `color_space` (three.js
    /// `color`); `None` is white.
    pub color: Option<[f64; 3]>,
    /// 0 dielectric, 1 metal (three.js `metalness`).
    pub metalness: f64,
    /// 0 mirror, 1 matte (three.js `roughness`); `None` is 1.
    pub roughness: Option<f64>,
    /// Light given off, components in 0..=1 in `color_space` (three.js
    /// `emissive`).
    pub emissive: [f64; 3],
    /// 1 opaque, 0 invisible (three.js `opacity`); `None` is 1. Below 1
    /// the material blends, unless `transparent` says otherwise.
    pub opacity: Option<f64>,
    /// Base-colour texture (three.js `map`).
    pub map: Option<Texture>,
    /// How `color` and `emissive` are given.
    pub color_space: ColorSpace,
    /// Multiplies `emissive` (three.js `emissiveIntensity`); `None` is 1.
    /// Above 1 it is written with `KHR_materials_emissive_strength`.
    pub emissive_intensity: Option<f64>,
    /// Blending (glTF `alphaMode` `BLEND`) or not (`OPAQUE`); `None`
    /// blends when `opacity` is below 1 (three.js `transparent`).
    pub transparent: Option<bool>,
    /// Texels whose alpha is below this are cut out (glTF `MASK` with
    /// this `alphaCutoff`; three.js `alphaTest`). Overrides `transparent`.
    pub alpha_test: Option<f64>,
    /// Both sides drawn (glTF `doubleSided`; three.js `side: DoubleSide`).
    pub double_sided: bool,
    /// glTF's `metallicRoughnessTexture`: roughness in green, metalness in
    /// blue, multiplied by `roughness` and `metalness` (three.js sets it
    /// as both `roughnessMap` and `metalnessMap`).
    pub metalness_map: Option<Texture>,
    /// A tangent-space normal map (three.js `normalMap`).
    pub normal_map: Option<Texture>,
    /// The normal map's strength (glTF `normalTexture.scale`, three.js
    /// `normalScale`); `None` is 1.
    pub normal_scale: Option<f64>,
    /// Emissive texture, multiplied by `emissive` (three.js
    /// `emissiveMap`).
    pub emissive_map: Option<Texture>,
    /// Ambient occlusion in red (glTF `occlusionTexture`, three.js
    /// `aoMap`).
    pub ao_map: Option<Texture>,
    /// The occlusion's strength (glTF `occlusionTexture.strength`, three.js
    /// `aoMapIntensity`); `None` is 1.
    pub ao_map_intensity: Option<f64>,
}

/// The kinds of map a [`Material`] carries, in the order reports list
/// them: three.js's names.
pub const MAP_KINDS: [&str; 5] = [
    "map",
    "metalness_map",
    "normal_map",
    "emissive_map",
    "ao_map",
];

impl Material {
    /// Each map the material has, with its kind ([`MAP_KINDS`]).
    pub fn maps(&self) -> Vec<(&'static str, &Texture)> {
        self.map_slots()
            .into_iter()
            .filter_map(|(k, t)| Some((k, t.as_ref()?)))
            .collect()
    }

    /// Each map slot, filled or not, with its kind.
    pub(crate) fn map_slots(&self) -> [(&'static str, &Option<Texture>); 5] {
        [
            (MAP_KINDS[0], &self.map),
            (MAP_KINDS[1], &self.metalness_map),
            (MAP_KINDS[2], &self.normal_map),
            (MAP_KINDS[3], &self.emissive_map),
            (MAP_KINDS[4], &self.ao_map),
        ]
    }

    /// Each map slot, mutably.
    pub fn maps_mut(&mut self) -> [(&'static str, &mut Option<Texture>); 5] {
        [
            (MAP_KINDS[0], &mut self.map),
            (MAP_KINDS[1], &mut self.metalness_map),
            (MAP_KINDS[2], &mut self.normal_map),
            (MAP_KINDS[3], &mut self.emissive_map),
            (MAP_KINDS[4], &mut self.ao_map),
        ]
    }

    /// Whether a map of this kind holds colour (sRGB: base colour and
    /// emissive) rather than data (metalness-roughness, normals,
    /// occlusion).
    pub fn is_color_map(kind: &str) -> bool {
        kind == "map" || kind == "emissive_map"
    }
}

/// How a joint moves its part (`docs/research.md` §1.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JointKind {
    /// Turns about the axis within a range; with no range it is written
    /// `continuous`.
    #[default]
    Revolute,
    /// Turns about the axis without end.
    Continuous,
    /// Slides along the axis.
    Prismatic,
    /// Moves every way: no axis, no range.
    Floating,
    /// Does not move: a named mount point, written `fixed`, with no axis,
    /// range or mimic (URDF's `fixed`).
    Fixed,
}

/// A joint's value follows another's: `multiplier` × the leader's value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mimic {
    /// The leading joint's name. It must be declared before this one.
    pub joint: String,
    /// This joint's value per unit of the leader's, in kernel units
    /// (radians or millimetres).
    pub multiplier: f64,
}

/// How a part moves on its parent.
///
/// Kernel units: millimetres and radians. The GLB carries SI units.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Joint {
    /// Unique among the scene's joints.
    pub name: String,
    /// How it moves.
    pub kind: JointKind,
    /// The pivot, in the world. The joint's node sits here.
    pub pivot: Frame,
    /// The axis in the pivot's own coordinates; zero is the pivot's +Z.
    pub axis: DVec3,
    /// The joint's pose, from rest (0): the part is written moved by it
    /// from where it was built. A follower ignores it and takes its
    /// leader's value × its multiplier.
    pub value: f64,
    /// `(lower, upper)`, containing 0. A follower with none takes its
    /// leader's range × its multiplier.
    pub range: Option<(f64, f64)>,
    /// The joint this one follows.
    pub mimic: Option<Mimic>,
}

/// One part: a solid, in the world, with its material and how it hangs.
#[derive(Clone, Debug)]
pub struct Part<'a> {
    /// Unique among the scene's parts: the node's and the mesh's name.
    pub name: String,
    /// Its geometry in the world at rest, every joint at 0 (millimetres,
    /// Z up).
    pub solid: &'a Solid,
    /// What it is made of.
    pub material: Material,
    /// The part it hangs on, declared before it; empty for the scene's
    /// root.
    pub parent: String,
    /// How it moves on its parent; `None` is welded to it.
    pub joint: Option<Joint>,
    /// Write `TANGENT` (on a solid with UVs), as [`crate::PlainGlb`]
    /// does: the frame a baked normal map was baked in.
    pub tangents: bool,
}

impl<'a> Part<'a> {
    /// A part with three.js's default material, welded to the root.
    pub fn new(name: &str, solid: &'a Solid) -> Part<'a> {
        Part {
            name: name.to_string(),
            solid,
            material: Material::default(),
            parent: String::new(),
            joint: None,
            tangents: false,
        }
    }
}

/// The parts to export or preview.
#[derive(Clone, Debug, Default)]
pub struct Scene<'a> {
    /// The scene's name in the file.
    pub name: String,
    /// Parents before children.
    pub parts: Vec<Part<'a>>,
}

/// Why a scene could not be written.
#[derive(Clone, Debug, PartialEq)]
pub enum ExportError {
    /// Two parts share a name.
    DuplicatePart(String),
    /// Two joints share a name.
    DuplicateJoint(String),
    /// A part names a parent that is not declared before it.
    UnknownParent {
        /// The part.
        part: String,
        /// The parent it names.
        parent: String,
    },
    /// A joint follows one that is not declared before it.
    UnknownLeader {
        /// The following joint.
        joint: String,
        /// The leader it names.
        leader: String,
    },
    /// A value was NaN or infinite (where, named).
    NotFinite(String),
    /// A joint's range is empty or does not contain 0.
    BadRange(String),
    /// A joint's axis or pivot is degenerate.
    BadAxis(String),
    /// A preview asked for more than it draws: the limit that was passed
    /// and what was asked.
    TooLarge(String),
    /// A clip cannot be written: which, and why.
    BadClip {
        /// The clip's name.
        clip: String,
        /// What is wrong with it.
        why: String,
    },
    /// A floating joint was given an axis, a range or a mimic.
    FloatingConstrained(String),
    /// A fixed joint was given an axis, a range, a mimic or a value.
    FixedConstrained(String),
    /// A part's skin cannot be written.
    BadSkin {
        /// The part.
        part: String,
        /// Why.
        why: String,
    },
    /// A part's texture cannot be written: not PNG or JPEG, or the part
    /// has no UVs to sample it with.
    BadTexture {
        /// The part.
        part: String,
        /// Why.
        why: String,
    },
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExportError::DuplicatePart(n) => write!(f, "two parts are named {n:?}"),
            ExportError::DuplicateJoint(n) => write!(f, "two joints are named {n:?}"),
            ExportError::UnknownParent { part, parent } => write!(
                f,
                "part {part:?} hangs on {parent:?}, which is not a part declared before it"
            ),
            ExportError::UnknownLeader { joint, leader } => write!(
                f,
                "joint {joint:?} follows {leader:?}, which is not a joint declared before it"
            ),
            ExportError::NotFinite(w) => write!(f, "{w} is not finite"),
            ExportError::BadRange(j) => write!(
                f,
                "joint {j:?}: its range must run from low to high and contain 0 (the rest pose)"
            ),
            ExportError::BadClip { clip, why } => write!(f, "clip {clip:?}: {why}"),
            ExportError::TooLarge(why) => write!(f, "the preview is too large: {why}"),
            ExportError::BadAxis(j) => {
                write!(f, "joint {j:?}: its axis or pivot frame is degenerate")
            }
            ExportError::FloatingConstrained(j) => write!(
                f,
                "joint {j:?}: a floating joint moves every way: it takes no axis, range or mimic"
            ),
            ExportError::FixedConstrained(j) => write!(
                f,
                "joint {j:?}: a fixed joint does not move: it takes no axis, range, mimic or value"
            ),
            ExportError::BadSkin { part, why } => write!(f, "part {part:?}: its skin: {why}"),
            ExportError::BadTexture { part, why } => {
                write!(f, "part {part:?}: its texture: {why}")
            }
        }
    }
}

impl std::error::Error for ExportError {}

fn finite(v: f64, what: impl Fn() -> String) -> Result<f64, ExportError> {
    if v.is_finite() {
        Ok(v)
    } else {
        Err(ExportError::NotFinite(what()))
    }
}

/// A joint after checks, in SI units and glTF space.
pub(crate) struct Checked {
    pub(crate) name: String,
    pub(crate) kind: &'static str,
    /// The joint's node: the pivot after the joint's motion.
    node: Rigid,
    /// The part's node under it: the pivot's inverse.
    inverse: Rigid,
    axis: Option<DVec3>,
    value: f64,
    pub(crate) span: Option<(f64, f64)>,
    pub(crate) mimic: Option<(String, f64)>,
    /// For clips, in kernel units: the pivot, the unit axis in the
    /// pivot's own coordinates (none for fixed and floating), whether it
    /// slides, its range, and the leader it follows with the multiplier.
    pub(crate) pivot: Rigid,
    pub(crate) local: Option<DVec3>,
    slides: bool,
    kernel_span: Option<(f64, f64)>,
    follows: Option<(String, f64)>,
}

/// One keyframe of a [`Clip`]: a time and the joints' values then.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ClipKey {
    /// Seconds from the clip's start; keys are in rising order.
    pub time: f64,
    /// Joint name and value (radians, or millimetres for a slide), as
    /// [`Joint::value`]. A joint a key leaves out keeps its own track:
    /// only the keys that name it move it.
    pub pose: Vec<(String, f64)>,
}

/// A named motion of a scene's joints over time, written to the GLB as a
/// glTF animation ([`glb_clips`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clip {
    /// Unique among the clips.
    pub name: String,
    /// The keyframes, by time.
    pub keys: Vec<ClipKey>,
}

/// Metres per kernel unit of a joint's value: millimetres for a slide,
/// radians otherwise.
fn si(kind: JointKind) -> f64 {
    if kind == JointKind::Prismatic {
        METRES
    } else {
        1.0
    }
}

/// A rigid motion in kernel space: `p ↦ x·p.x + y·p.y + z·p.z + t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rigid {
    pub x: DVec3,
    pub y: DVec3,
    pub z: DVec3,
    pub t: DVec3,
}

impl Rigid {
    pub(crate) const IDENTITY: Rigid = Rigid {
        x: DVec3::X,
        y: DVec3::Y,
        z: DVec3::Z,
        t: DVec3::ZERO,
    };

    pub(crate) fn of(f: &Frame) -> Rigid {
        Rigid {
            x: f.x,
            y: f.y,
            z: f.z,
            t: f.origin,
        }
    }

    pub(crate) fn rotate(&self, v: DVec3) -> DVec3 {
        self.x * v.x + self.y * v.y + self.z * v.z
    }

    pub(crate) fn apply(&self, p: DVec3) -> DVec3 {
        self.rotate(p) + self.t
    }

    /// `self ∘ other`: `other` first.
    pub(crate) fn then(&self, other: &Rigid) -> Rigid {
        Rigid {
            x: self.rotate(other.x),
            y: self.rotate(other.y),
            z: self.rotate(other.z),
            t: self.apply(other.t),
        }
    }

    pub(crate) fn inverse(&self) -> Rigid {
        // (R, t)⁻¹ = (Rᵀ, −Rᵀ t).
        let tr = |v: DVec3| DVec3::new(self.x.dot(v), self.y.dot(v), self.z.dot(v));
        Rigid {
            x: tr(DVec3::X),
            y: tr(DVec3::Y),
            z: tr(DVec3::Z),
            t: -tr(self.t),
        }
    }

    /// Turn by `angle` radians about the unit axis `k` through the origin.
    fn turn(k: DVec3, angle: f64) -> Rigid {
        let (sn, cs) = trig::sin_cos(angle);
        let r = |v: DVec3| v * cs + k.cross(v) * sn + k * (k.dot(v) * (1.0 - cs));
        Rigid {
            x: r(DVec3::X),
            y: r(DVec3::Y),
            z: r(DVec3::Z),
            t: DVec3::ZERO,
        }
    }

    /// The same motion in glTF space (metres, Y up): translation and
    /// rotation (x, y, z, w).
    pub(crate) fn gltf(&self) -> (DVec3, [f64; 4]) {
        let down = |v: DVec3| DVec3::new(v.x, -v.z, v.y);
        let col = |e: DVec3| y_up(self.rotate(down(e)));
        (
            y_up(self.t) * METRES,
            quaternion(col(DVec3::X), col(DVec3::Y), col(DVec3::Z)),
        )
    }
}

/// The rotation whose columns are `x`, `y`, `z` as a unit quaternion
/// (x, y, z, w), by Shepperd's method: `+ - * /` and `sqrt` only.
fn quaternion(x: DVec3, y: DVec3, z: DVec3) -> [f64; 4] {
    let (m00, m11, m22) = (x.x, y.y, z.z);
    let trace = m00 + m11 + m22;
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [(y.z - z.y) / s, (z.x - x.z) / s, (x.y - y.x) / s, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * s, (y.x + x.y) / s, (z.x + x.z) / s, (y.z - z.y) / s]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(y.x + x.y) / s, 0.25 * s, (z.y + y.z) / s, (z.x - x.z) / s]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(z.x + x.z) / s, (z.y + y.z) / s, 0.25 * s, (x.y - y.x) / s]
    };
    // One sign for every host: w >= 0.
    let q = if q[3] < 0.0 { q.map(|c| -c) } else { q };
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    q.map(|c| c / len)
}

/// Checks a scene as every tool does before using it, and places each
/// part: its joints' motions, composed from the root, in kernel space.
pub(crate) fn check(scene: &Scene) -> Result<Vec<Rigid>, ExportError> {
    Ok(check_joints(scene)?.into_iter().map(|(_, w)| w).collect())
}

type Leader = (JointKind, Option<(f64, f64)>, f64);

pub(crate) fn check_joints(scene: &Scene) -> Result<Vec<(Option<Checked>, Rigid)>, ExportError> {
    // Each part's placement so far, by name.
    let mut parts: BTreeMap<&str, Rigid> = BTreeMap::new();
    let mut joints: BTreeMap<String, Leader> = BTreeMap::new();
    let mut out = Vec::with_capacity(scene.parts.len());
    for p in &scene.parts {
        if !p.parent.is_empty() && !parts.contains_key(p.parent.as_str()) {
            return Err(ExportError::UnknownParent {
                part: p.name.clone(),
                parent: p.parent.clone(),
            });
        }
        if parts.contains_key(p.name.as_str()) {
            return Err(ExportError::DuplicatePart(p.name.clone()));
        }
        let above = if p.parent.is_empty() {
            Rigid::IDENTITY
        } else {
            parts[p.parent.as_str()]
        };
        for v in &p.solid.positions {
            for c in [v.x, v.y, v.z] {
                finite(c, || format!("a position of part {:?}", p.name))?;
            }
        }
        let Some(j) = &p.joint else {
            parts.insert(p.name.as_str(), above);
            out.push((None, above));
            continue;
        };
        let jn = || j.name.clone();
        if joints.contains_key(&j.name) {
            return Err(ExportError::DuplicateJoint(j.name.clone()));
        }
        let f = &j.pivot;
        for c in [f.origin, f.x, f.y, f.z]
            .iter()
            .flat_map(|v| [v.x, v.y, v.z])
            .chain([j.axis.x, j.axis.y, j.axis.z, j.value])
        {
            finite(c, || format!("joint {:?}", j.name))?;
        }
        // The pivot must be a rotation: orthonormal and right-handed.
        let ortho = (f.x.length() - 1.0).abs() < 1e-9
            && (f.y.length() - 1.0).abs() < 1e-9
            && (f.z.length() - 1.0).abs() < 1e-9
            && f.x.dot(f.y).abs() < 1e-9
            && f.x.cross(f.y).dot(f.z) > 0.999_999;
        if !ortho {
            return Err(ExportError::BadAxis(jn()));
        }
        let floating = j.kind == JointKind::Floating;
        if floating
            && (j.axis != DVec3::ZERO || j.range.is_some() || j.mimic.is_some() || j.value != 0.0)
        {
            return Err(ExportError::FloatingConstrained(jn()));
        }
        let fixed = j.kind == JointKind::Fixed;
        if fixed
            && (j.axis != DVec3::ZERO || j.range.is_some() || j.mimic.is_some() || j.value != 0.0)
        {
            return Err(ExportError::FixedConstrained(jn()));
        }
        let local = if floating || fixed {
            None
        } else {
            let a = if j.axis == DVec3::ZERO {
                DVec3::Z
            } else {
                j.axis
            };
            let len = a.length();
            if len == 0.0 || !len.is_finite() {
                return Err(ExportError::BadAxis(jn()));
            }
            Some(a / len)
        };
        let axis = local.map(y_up);
        if let Some((lo, hi)) = j.range {
            finite(lo, || format!("joint {:?}'s range", j.name))?;
            finite(hi, || format!("joint {:?}'s range", j.name))?;
            if !(lo < hi && lo <= 0.0 && 0.0 <= hi) {
                return Err(ExportError::BadRange(jn()));
            }
        }
        let mut span = j.range;
        let mut value = j.value;
        let mut mimic = None;
        if let Some(m) = &j.mimic {
            finite(m.multiplier, || format!("joint {:?}'s multiplier", j.name))?;
            let Some(&(lead_kind, lead_span, lead_value)) = joints.get(&m.joint) else {
                return Err(ExportError::UnknownLeader {
                    joint: j.name.clone(),
                    leader: m.joint.clone(),
                });
            };
            if span.is_none() {
                span = lead_span.map(|(lo, hi)| {
                    let (a, b) = (lo * m.multiplier, hi * m.multiplier);
                    if a <= b { (a, b) } else { (b, a) }
                });
            }
            value = lead_value * m.multiplier;
            mimic = Some((m.joint.clone(), m.multiplier * si(j.kind) / si(lead_kind)));
        }
        joints.insert(j.name.clone(), (j.kind, span, value));
        let kind = match j.kind {
            JointKind::Revolute if span.is_none() => "continuous",
            JointKind::Revolute => "revolute",
            JointKind::Continuous => "continuous",
            JointKind::Prismatic => "prismatic",
            JointKind::Floating => "floating",
            JointKind::Fixed => "fixed",
        };
        // A continuous joint has no bounds.
        let span = if kind == "continuous" { None } else { span };
        // The motion, in the pivot's own coordinates.
        let motion = match (j.kind, local) {
            (JointKind::Prismatic, Some(a)) => Rigid {
                t: a * value,
                ..Rigid::IDENTITY
            },
            (_, Some(a)) => Rigid::turn(a, value),
            (_, None) => Rigid::IDENTITY,
        };
        let pivot = Rigid::of(f);
        let node = pivot.then(&motion);
        let inverse = pivot.inverse();
        let world = above.then(&node).then(&inverse);
        parts.insert(p.name.as_str(), world);
        let k = si(j.kind);
        out.push((
            Some(Checked {
                name: j.name.clone(),
                kind,
                node,
                inverse,
                axis,
                value: value * k,
                span: span.map(|(lo, hi)| (lo * k, hi * k)),
                mimic,
                pivot,
                local,
                slides: j.kind == JointKind::Prismatic,
                kernel_span: span,
                follows: j.mimic.as_ref().map(|m| (m.joint.clone(), m.multiplier)),
            }),
            world,
        ));
    }
    Ok(out)
}

/// sRGB component to linear (glTF factors are linear).
pub(crate) fn linear(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        trig::pow((c + 0.055) / 1.055, 2.4)
    }
}

/// The glTF texture index of each of a material's maps, in
/// [`MAP_KINDS`] order.
pub(crate) type MapIndices = [Option<u64>; 5];

/// A material as glTF JSON, its maps at `maps`, and whether it needs
/// `KHR_materials_emissive_strength`. Fields a zero-value material does
/// not set are left out, so its JSON is as it always was.
pub(crate) fn material_json(
    m: &Material,
    part: &str,
    maps: MapIndices,
) -> Result<(Json, bool), ExportError> {
    let name = if m.name.is_empty() { part } else { &m.name };
    let what = || format!("material {name:?}");
    let color = m.color.unwrap_or([1.0; 3]);
    let opacity = m.opacity.unwrap_or(1.0);
    let roughness = m.roughness.unwrap_or(1.0);
    let extra = [
        m.emissive_intensity.unwrap_or(1.0),
        m.alpha_test.unwrap_or(0.5),
        m.normal_scale.unwrap_or(1.0),
        m.ao_map_intensity.unwrap_or(1.0),
    ];
    for v in color
        .iter()
        .chain(&m.emissive)
        .chain([&opacity, &roughness, &m.metalness])
        .chain(&extra)
    {
        finite(*v, what)?;
    }
    let space = |c: f64| match m.color_space {
        ColorSpace::Srgb => linear(c),
        ColorSpace::Linear => c.clamp(0.0, 1.0),
    };
    let base = [
        space(color[0]),
        space(color[1]),
        space(color[2]),
        opacity.clamp(0.0, 1.0),
    ];
    let index = |k: u64| Json::Obj(vec![("index", Json::Int(k))]);
    let mut pbr = vec![("baseColorFactor", nums(&base))];
    if let Some(k) = maps[0] {
        pbr.push(("baseColorTexture", index(k)));
    }
    pbr.push(("metallicFactor", Json::Num(m.metalness.clamp(0.0, 1.0))));
    pbr.push(("roughnessFactor", Json::Num(roughness.clamp(0.0, 1.0))));
    if let Some(k) = maps[1] {
        pbr.push(("metallicRoughnessTexture", index(k)));
    }
    let mut fields = vec![
        ("name", Json::Str(name.to_string())),
        ("pbrMetallicRoughness", Json::Obj(pbr)),
    ];
    if let Some(k) = maps[2] {
        let mut t = vec![("index", Json::Int(k))];
        if let Some(scale) = m.normal_scale.filter(|&x| x != 1.0) {
            t.push(("scale", Json::Num(scale)));
        }
        fields.push(("normalTexture", Json::Obj(t)));
    }
    if let Some(k) = maps[4] {
        let mut t = vec![("index", Json::Int(k))];
        if let Some(strength) = m.ao_map_intensity.filter(|&x| x != 1.0) {
            t.push(("strength", Json::Num(strength.clamp(0.0, 1.0))));
        }
        fields.push(("occlusionTexture", Json::Obj(t)));
    }
    if let Some(k) = maps[3] {
        fields.push(("emissiveTexture", index(k)));
    }
    if m.emissive != [0.0; 3] {
        fields.push(("emissiveFactor", nums(&m.emissive.map(space))));
    }
    if let Some(cutoff) = m.alpha_test {
        fields.push(("alphaMode", Json::Str("MASK".into())));
        fields.push(("alphaCutoff", Json::Num(cutoff.max(0.0))));
    } else if m.transparent.unwrap_or(opacity < 1.0) {
        fields.push(("alphaMode", Json::Str("BLEND".into())));
    }
    if m.double_sided {
        fields.push(("doubleSided", Json::Bool(true)));
    }
    let strength = m.emissive_intensity.filter(|&x| x != 1.0);
    if let Some(x) = strength {
        fields.push((
            "extensions",
            Json::Obj(vec![(
                "KHR_materials_emissive_strength",
                Json::Obj(vec![("emissiveStrength", Json::Num(x.max(0.0)))]),
            )]),
        ));
    }
    Ok((Json::Obj(fields), strength.is_some()))
}

/// Each of a material's maps checked ([`check_texture`]) and given its
/// index in `textures` (shared when two maps are the same image).
pub(crate) fn map_indices<'t>(
    m: &'t Material,
    part: &str,
    has_uvs: bool,
    textures: &mut Vec<&'t Texture>,
) -> Result<MapIndices, ExportError> {
    let mut out: MapIndices = [None; 5];
    for (k, (_, slot)) in m.map_slots().into_iter().enumerate() {
        if let Some(t) = slot {
            check_texture(t, part, has_uvs)?;
            let i = textures.iter().position(|x| *x == t).unwrap_or_else(|| {
                textures.push(t);
                textures.len() - 1
            });
            out[k] = Some(i as u64);
        }
    }
    Ok(out)
}

fn joint_extras(j: &Checked) -> Json {
    let mut fields = vec![
        ("name", Json::Str(j.name.clone())),
        ("type", Json::Str(j.kind.into())),
        ("value", Json::Num(j.value)),
    ];
    if let Some(a) = j.axis {
        fields.push(("axis", nums(&[a.x, a.y, a.z])));
    }
    if let Some((lo, hi)) = j.span {
        fields.push(("lower", Json::Num(lo)));
        fields.push(("upper", Json::Num(hi)));
    }
    if let Some((leader, k)) = &j.mimic {
        fields.push((
            "mimic",
            Json::Obj(vec![
                ("joint", Json::Str(leader.clone())),
                ("multiplier", Json::Num(*k)),
            ]),
        ));
    }
    Json::Obj(vec![("s3d_joint", Json::Obj(fields))])
}

const ARRAY_BUFFER: u64 = 34962;
const ELEMENT_ARRAY_BUFFER: u64 = 34963;
const FLOAT: u64 = 5126;
const UNSIGNED_INT: u64 = 5125;
const UNSIGNED_SHORT: u64 = 5123;

fn push_f32(bin: &mut Vec<u8>, v: f32) {
    bin.extend_from_slice(&v.to_le_bytes());
}

/// A skin to write: inverse bind bytes, joint node indices, root bone node.
type SkinOut = (Vec<u8>, Vec<usize>, Option<usize>);

/// The scene as glTF 2.0 binary (GLB).
///
/// - Positions in metres, Y up (kernel Z becomes Y, Y becomes −Z), as f32.
/// - Normals smooth within a surface id and split at real edges: each
///   surface has its own vertices.
/// - u32 indices; one node, one mesh and one material per part.
/// - A part with a joint hangs under a node named after the joint, at its
///   pivot, which hangs under the parent part's node; its extras are
///   `{"s3d_joint": {name, type, value, axis, lower, upper, mimic}}` in SI
///   units, the axis in the joint node's frame (`docs/research.md` §1.5).
///   Face and edge names are not written.
/// - A skinned part (no joint) also gets a glTF skin: its bones as nodes
///   under the part's node, each at its rest frame relative to its parent
///   bone, inverse bind matrices, and JOINTS_0 (u16) and WEIGHTS_0 (f32)
///   per vertex (`docs/research.md` §11).
///
/// The bytes depend only on the scene.
pub fn glb(scene: &Scene) -> Result<Vec<u8>, ExportError> {
    glb_with(scene, false, &[])
}

/// [`glb`], with parts whose materials are written alike (the same
/// name, factors and maps) sharing one glTF material, as an atlas they
/// share asks ([`crate::repack`]).
pub fn glb_shared(scene: &Scene) -> Result<Vec<u8>, ExportError> {
    glb_with(scene, true, &[])
}

/// [`glb`], with `clips` written as glTF animations: one a clip, a
/// sampler and a channel per joint it moves (a follower's from its
/// leader's), on the joint's node: `rotation` for a joint that turns,
/// `translation` for one that slides, linear between keys, times in
/// seconds. A turn of more than a quarter between two keys is written
/// with keys between them, so a viewer's shortest-way rotation follows
/// the joint round its axis. The nodes stand at the scene's pose, as
/// [`glb`] writes them; a viewer playing a clip moves them from there.
///
/// A clip is refused, saying why, when its name is empty or used twice,
/// it has no key, its times are not finite, not rising or negative, it
/// names a joint the scene has not, one that is fixed, floating or a
/// follower, or a value outside the joint's range.
pub fn glb_clips(scene: &Scene, clips: &[Clip]) -> Result<Vec<u8>, ExportError> {
    glb_with(scene, false, clips)
}

/// [`glb`], with the clips of its skinned parts as animations of their
/// bones' nodes: `skin_clips` gives each such part's, by its name. Every
/// bone gets its translation and rotation at each of the clip's times,
/// and its scale where the clip scales it. Clips of one name on several
/// parts are one animation.
pub fn glb_skin_clips(
    scene: &Scene,
    skin_clips: &[(&str, &[shape_mesh::SkinClip])],
) -> Result<Vec<u8>, ExportError> {
    glb_all(scene, false, &[], skin_clips)
}

fn glb_with(scene: &Scene, share: bool, clips: &[Clip]) -> Result<Vec<u8>, ExportError> {
    glb_all(scene, share, clips, &[])
}

fn glb_all(
    scene: &Scene,
    share: bool,
    clips: &[Clip],
    skin_clips: &[(&str, &[shape_mesh::SkinClip])],
) -> Result<Vec<u8>, ExportError> {
    let checked = check_joints(scene)?;
    let mut joint_node: BTreeMap<&str, usize> = BTreeMap::new();
    let mut bin: Vec<u8> = Vec::new();
    let mut views: Vec<Json> = Vec::new();
    let mut accessors: Vec<Json> = Vec::new();
    let mut meshes: Vec<Json> = Vec::new();
    let mut materials: Vec<Json> = Vec::new();
    let mut nodes: Vec<Json> = Vec::new();
    // Node index of each part, and the children of each node.
    let mut part_node: BTreeMap<&str, usize> = BTreeMap::new();
    let mut children: Vec<Vec<usize>> = Vec::new();
    let mut roots: Vec<usize> = Vec::new();
    let mut skins: Vec<SkinOut> = Vec::new();
    // Per skinned part: its skin and its bones' nodes.
    let mut skin_nodes: BTreeMap<&str, (&shape_mesh::Skin, Vec<usize>)> = BTreeMap::new();
    // Per part with morph targets: how many.
    let mut morph_counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut textures: Vec<&Texture> = Vec::new();
    let mut emissive_strength = false;
    let mut view = |bin: &mut Vec<u8>, start: usize, target: u64| -> u64 {
        views.push(Json::Obj(vec![
            ("buffer", Json::Int(0)),
            ("byteOffset", Json::Int(start as u64)),
            ("byteLength", Json::Int((bin.len() - start) as u64)),
            ("target", Json::Int(target)),
        ]));
        (views.len() - 1) as u64
    };
    for (part, (joint, _)) in scene.parts.iter().zip(&checked) {
        let skin = crate::skin::checked(part)?;
        let s = split(part.solid);
        // Positions and normals in glTF space, f32.
        let pos: Vec<[f32; 3]> = s
            .positions
            .iter()
            .map(|&p| {
                let q = y_up(p) * METRES;
                [q.x as f32, q.y as f32, q.z as f32]
            })
            .collect();
        let nrm: Vec<[f32; 3]> = s
            .normals
            .iter()
            .map(|&n| {
                let q = y_up(n);
                [q.x as f32, q.y as f32, q.z as f32]
            })
            .collect();
        let mut lo = [f32::INFINITY; 3];
        let mut hi = [f32::NEG_INFINITY; 3];
        for p in &pos {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let mut attributes = Vec::new();
        let mut primitive = Vec::new();
        let mut morph_fields: Vec<(&str, Json)> = Vec::new();
        if !pos.is_empty() {
            let start = bin.len();
            pos.iter().flatten().for_each(|&v| push_f32(&mut bin, v));
            let pv = view(&mut bin, start, ARRAY_BUFFER);
            accessors.push(Json::Obj(vec![
                ("bufferView", Json::Int(pv)),
                ("componentType", Json::Int(FLOAT)),
                ("count", Json::Int(pos.len() as u64)),
                ("type", Json::Str("VEC3".into())),
                ("min", Json::Arr(lo.iter().map(|&v| f32n(v)).collect())),
                ("max", Json::Arr(hi.iter().map(|&v| f32n(v)).collect())),
            ]));
            attributes.push(("POSITION", Json::Int((accessors.len() - 1) as u64)));
            let start = bin.len();
            nrm.iter().flatten().for_each(|&v| push_f32(&mut bin, v));
            let nv = view(&mut bin, start, ARRAY_BUFFER);
            accessors.push(Json::Obj(vec![
                ("bufferView", Json::Int(nv)),
                ("componentType", Json::Int(FLOAT)),
                ("count", Json::Int(nrm.len() as u64)),
                ("type", Json::Str("VEC3".into())),
            ]));
            attributes.push(("NORMAL", Json::Int((accessors.len() - 1) as u64)));
            if !s.uvs.is_empty() {
                if s.uvs.iter().any(|q| !q.is_finite()) {
                    return Err(ExportError::NotFinite(format!(
                        "part {:?}'s UVs",
                        part.name
                    )));
                }
                let start = bin.len();
                s.uvs
                    .iter()
                    .flat_map(|&q| gltf_uv(q))
                    .for_each(|v| push_f32(&mut bin, v));
                let tv = view(&mut bin, start, ARRAY_BUFFER);
                accessors.push(Json::Obj(vec![
                    ("bufferView", Json::Int(tv)),
                    ("componentType", Json::Int(FLOAT)),
                    ("count", Json::Int(s.uvs.len() as u64)),
                    ("type", Json::Str("VEC2".into())),
                ]));
                attributes.push(("TEXCOORD_0", Json::Int((accessors.len() - 1) as u64)));
            }
            if !s.colors.is_empty() {
                if s.colors.iter().flatten().any(|c| !c.is_finite()) {
                    return Err(ExportError::NotFinite(format!(
                        "part {:?}'s vertex colours",
                        part.name
                    )));
                }
                let start = bin.len();
                s.colors
                    .iter()
                    .flatten()
                    .for_each(|&c| push_f32(&mut bin, c.clamp(0.0, 1.0) as f32));
                let cv = view(&mut bin, start, ARRAY_BUFFER);
                accessors.push(Json::Obj(vec![
                    ("bufferView", Json::Int(cv)),
                    ("componentType", Json::Int(FLOAT)),
                    ("count", Json::Int(s.colors.len() as u64)),
                    ("type", Json::Str("VEC4".into())),
                ]));
                attributes.push(("COLOR_0", Json::Int((accessors.len() - 1) as u64)));
            }
            if part.tangents && !s.uvs.is_empty() {
                // Each vertex's frame as the plain GLB has it.
                let frames = crate::surface::corner_frames(part.solid);
                let start = bin.len();
                for &(i, k) in &s.corners {
                    let f = frames[i as usize][usize::from(k)];
                    let t = y_up(f.tangent);
                    for c in [t.x, t.y, t.z, f.w] {
                        push_f32(&mut bin, c as f32);
                    }
                }
                let tv = view(&mut bin, start, ARRAY_BUFFER);
                accessors.push(Json::Obj(vec![
                    ("bufferView", Json::Int(tv)),
                    ("componentType", Json::Int(FLOAT)),
                    ("count", Json::Int(s.corners.len() as u64)),
                    ("type", Json::Str("VEC4".into())),
                ]));
                attributes.push(("TANGENT", Json::Int((accessors.len() - 1) as u64)));
            }
            if let Some(sk) = skin {
                let (j, w) = crate::skin::vertex_data(sk, &s.sources);
                for (bytes, kind, name) in
                    [(j, UNSIGNED_SHORT, "JOINTS_0"), (w, FLOAT, "WEIGHTS_0")]
                {
                    let start = bin.len();
                    bin.extend_from_slice(&bytes);
                    let v = view(&mut bin, start, ARRAY_BUFFER);
                    accessors.push(Json::Obj(vec![
                        ("bufferView", Json::Int(v)),
                        ("componentType", Json::Int(kind)),
                        ("count", Json::Int(s.sources.len() as u64)),
                        ("type", Json::Str("VEC4".into())),
                    ]));
                    attributes.push((name, Json::Int((accessors.len() - 1) as u64)));
                }
            }
            let start = bin.len();
            s.indices
                .iter()
                .for_each(|&i| bin.extend_from_slice(&i.to_le_bytes()));
            let iv = view(&mut bin, start, ELEMENT_ARRAY_BUFFER);
            accessors.push(Json::Obj(vec![
                ("bufferView", Json::Int(iv)),
                ("componentType", Json::Int(UNSIGNED_INT)),
                ("count", Json::Int(s.indices.len() as u64)),
                ("type", Json::Str("SCALAR".into())),
            ]));
            primitive.push(("attributes", Json::Obj(attributes)));
            primitive.push(("indices", Json::Int((accessors.len() - 1) as u64)));
            // Morph targets: each vertex's offset, in glTF space.
            if let Some(m) = &part.solid.morphs {
                m.check(part.solid.positions.len())
                    .map_err(|e| ExportError::NotFinite(format!("part {:?}: {e}", part.name)))?;
                let mut targets = Vec::with_capacity(m.targets.len());
                for t in &m.targets {
                    let start = bin.len();
                    let mut lo = [f32::INFINITY; 3];
                    let mut hi = [f32::NEG_INFINITY; 3];
                    for &v in &s.sources {
                        let q = y_up(t.deltas[v as usize]) * METRES;
                        for (k, c) in [q.x as f32, q.y as f32, q.z as f32].into_iter().enumerate() {
                            lo[k] = lo[k].min(c);
                            hi[k] = hi[k].max(c);
                            push_f32(&mut bin, c);
                        }
                    }
                    let dv = view(&mut bin, start, ARRAY_BUFFER);
                    accessors.push(Json::Obj(vec![
                        ("bufferView", Json::Int(dv)),
                        ("componentType", Json::Int(FLOAT)),
                        ("count", Json::Int(s.sources.len() as u64)),
                        ("type", Json::Str("VEC3".into())),
                        ("min", Json::Arr(lo.iter().map(|&v| f32n(v)).collect())),
                        ("max", Json::Arr(hi.iter().map(|&v| f32n(v)).collect())),
                    ]));
                    targets.push(Json::Obj(vec![(
                        "POSITION",
                        Json::Int((accessors.len() - 1) as u64),
                    )]));
                }
                if !targets.is_empty() {
                    primitive.push(("targets", Json::Arr(targets)));
                    morph_fields = vec![
                        (
                            "weights",
                            Json::Arr(m.targets.iter().map(|t| f32n(t.weight as f32)).collect()),
                        ),
                        (
                            "extras",
                            Json::Obj(vec![(
                                "targetNames",
                                Json::Arr(
                                    m.targets
                                        .iter()
                                        .map(|t| Json::Str(t.name.clone()))
                                        .collect(),
                                ),
                            )]),
                        ),
                    ];
                    morph_counts.insert(part.name.as_str(), m.targets.len());
                }
            }
        }
        let maps = map_indices(
            &part.material,
            &part.name,
            part.solid.uvs.is_some(),
            &mut textures,
        )?;
        let (material, strength) = material_json(&part.material, &part.name, maps)?;
        emissive_strength |= strength;
        let same = |m: &Json| {
            let (mut a, mut b) = (String::new(), String::new());
            m.write(&mut a);
            material.write(&mut b);
            a == b
        };
        let index = match materials.iter().position(same).filter(|_| share) {
            Some(k) => k,
            None => {
                materials.push(material);
                materials.len() - 1
            }
        };
        let mesh_index = meshes.len() as u64;
        if !primitive.is_empty() {
            primitive.push(("material", Json::Int(index as u64)));
            primitive.push(("mode", Json::Int(4)));
            let mut mesh = vec![
                ("name", Json::Str(part.name.clone())),
                ("primitives", Json::Arr(vec![Json::Obj(primitive)])),
            ];
            mesh.extend(morph_fields);
            meshes.push(Json::Obj(mesh));
        }
        // Where the part's node hangs.
        let parent = if part.parent.is_empty() {
            None
        } else {
            Some(part_node[part.parent.as_str()])
        };
        let mut attach = |child: usize, children: &mut Vec<Vec<usize>>| match parent {
            Some(p) => children[p].push(child),
            None => roots.push(child),
        };
        let mut node = vec![("name", Json::Str(part.name.clone()))];
        if !pos.is_empty() {
            node.push(("mesh", Json::Int(mesh_index)));
            if skin.is_some() {
                node.push(("skin", Json::Int(skins.len() as u64)));
            }
        }
        match joint {
            Some(j) => {
                // The joint's node at the pivot; the part's node undoes the
                // pivot, so its mesh stays where it was built.
                let jn = nodes.len();
                joint_node.insert(j.name.as_str(), jn);
                let (t, r) = j.node.gltf();
                nodes.push(Json::Obj(vec![
                    ("name", Json::Str(j.name.clone())),
                    ("translation", vec3(t)),
                    ("rotation", nums(&r)),
                    ("extras", joint_extras(j)),
                ]));
                children.push(Vec::new());
                attach(jn, &mut children);
                let (t, r) = j.inverse.gltf();
                node.push(("translation", vec3(t)));
                node.push(("rotation", nums(&r)));
                let pn = nodes.len();
                nodes.push(Json::Obj(node));
                children.push(Vec::new());
                children[jn].push(pn);
                part_node.insert(part.name.as_str(), pn);
            }
            None => {
                let pn = nodes.len();
                nodes.push(Json::Obj(node));
                children.push(Vec::new());
                attach(pn, &mut children);
                part_node.insert(part.name.as_str(), pn);
                if let (Some(sk), false) = (skin, pos.is_empty()) {
                    // The bones hang under the part's node, at the origin.
                    let mut joints: Vec<usize> = Vec::with_capacity(sk.bones.len());
                    for (fields, parent) in crate::skin::bone_nodes(sk) {
                        let bn = nodes.len();
                        nodes.push(Json::Obj(fields));
                        children.push(Vec::new());
                        match parent {
                            Some(p) => children[joints[p as usize]].push(bn),
                            None => children[pn].push(bn),
                        }
                        joints.push(bn);
                    }
                    let roots_of = sk.bones.iter().filter(|b| b.parent.is_none()).count();
                    let skeleton = (roots_of == 1).then(|| joints[0]);
                    skin_nodes.insert(part.name.as_str(), (sk, joints.clone()));
                    skins.push((crate::skin::inverse_binds(sk), joints, skeleton));
                }
            }
        }
    }
    // Inverse bind matrices go in last: their views carry no target.
    let mut skins_json = Vec::with_capacity(skins.len());
    for (ibm, joints, skeleton) in skins {
        let start = bin.len();
        bin.extend_from_slice(&ibm);
        views.push(Json::Obj(vec![
            ("buffer", Json::Int(0)),
            ("byteOffset", Json::Int(start as u64)),
            ("byteLength", Json::Int(ibm.len() as u64)),
        ]));
        accessors.push(Json::Obj(vec![
            ("bufferView", Json::Int((views.len() - 1) as u64)),
            ("componentType", Json::Int(FLOAT)),
            ("count", Json::Int(joints.len() as u64)),
            ("type", Json::Str("MAT4".into())),
        ]));
        let mut fields = vec![(
            "inverseBindMatrices",
            Json::Int((accessors.len() - 1) as u64),
        )];
        if let Some(root) = skeleton {
            fields.push(("skeleton", Json::Int(root as u64)));
        }
        fields.push((
            "joints",
            Json::Arr(joints.iter().map(|&k| Json::Int(k as u64)).collect()),
        ));
        skins_json.push(Json::Obj(fields));
    }
    let mut animations = animations(
        clips,
        &checked,
        &joint_node,
        &mut bin,
        &mut views,
        &mut accessors,
    )?;
    animations.extend(skin_animations(
        skin_clips,
        &skin_nodes,
        (&part_node, &morph_counts),
        &mut bin,
        &mut views,
        &mut accessors,
    )?);
    let texture_fields = texture_json(&textures, &mut bin, &mut views);
    // Children lists go in last, now that every node exists.
    for (n, kids) in nodes.iter_mut().zip(&children) {
        if let (Json::Obj(fields), false) = (n, kids.is_empty()) {
            fields.push((
                "children",
                Json::Arr(kids.iter().map(|&k| Json::Int(k as u64)).collect()),
            ));
        }
    }
    let mut doc = vec![
        (
            "asset",
            Json::Obj(vec![
                ("version", Json::Str("2.0".into())),
                ("generator", Json::Str("og-shape".into())),
            ]),
        ),
        ("scene", Json::Int(0)),
        (
            "scenes",
            Json::Arr(vec![Json::Obj(vec![
                ("name", Json::Str(scene.name.clone())),
                (
                    "nodes",
                    Json::Arr(roots.iter().map(|&k| Json::Int(k as u64)).collect()),
                ),
            ])]),
        ),
        ("nodes", Json::Arr(nodes)),
    ];
    for (k, field) in extension_fields(emissive_strength, &textures)
        .into_iter()
        .enumerate()
    {
        doc.insert(1 + k, field);
    }
    if !meshes.is_empty() {
        doc.push(("meshes", Json::Arr(meshes)));
        doc.push(("materials", Json::Arr(materials)));
        doc.push(("accessors", Json::Arr(accessors)));
        if !skins_json.is_empty() {
            doc.push(("skins", Json::Arr(skins_json)));
        }
        if !animations.is_empty() {
            doc.push(("animations", Json::Arr(animations)));
        }
        doc.extend(texture_fields);
        doc.push(("bufferViews", Json::Arr(views)));
        doc.push((
            "buffers",
            Json::Arr(vec![Json::Obj(vec![(
                "byteLength",
                Json::Int(bin.len() as u64),
            )])]),
        ));
    }
    let mut text = String::new();
    Json::Obj(doc).write(&mut text);
    let mut json = text.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }
    let with_bin = !bin.is_empty();
    let total = 12 + 8 + json.len() + if with_bin { 8 + bin.len() } else { 0 };
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    if with_bin {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
    }
    Ok(out)
}

/// The extension a texture's image format needs, if it is not one of
/// glTF's core formats.
fn texture_extension(t: &Texture) -> Option<&'static str> {
    match t.mime.as_str() {
        "image/ktx2" => Some("KHR_texture_basisu"),
        "image/webp" => Some("EXT_texture_webp"),
        _ => None,
    }
}

/// `extensionsUsed` and `extensionsRequired` for what a file carries: a
/// material with an emissive strength (used), and textures in KTX2 or
/// WebP, written as they came (used and required: they have no image a
/// reader without the extension could take).
pub(crate) fn extension_fields(
    emissive_strength: bool,
    textures: &[&Texture],
) -> Vec<(&'static str, Json)> {
    let mut required: Vec<&'static str> = textures
        .iter()
        .filter_map(|t| texture_extension(t))
        .collect();
    required.sort_unstable();
    required.dedup();
    let mut used = required.clone();
    if emissive_strength {
        used.insert(0, "KHR_materials_emissive_strength");
    }
    let list = |names: &[&str]| Json::Arr(names.iter().map(|n| Json::Str((*n).into())).collect());
    let mut out = Vec::new();
    if !used.is_empty() {
        out.push(("extensionsUsed", list(&used)));
    }
    if !required.is_empty() {
        out.push(("extensionsRequired", list(&required)));
    }
    out
}

/// Checks that a texture can be written: PNG or JPEG (glTF's core
/// formats), or KTX2 or WebP (written as they came, under their
/// extensions), on a part with UVs.
pub(crate) fn check_texture(t: &Texture, part: &str, has_uvs: bool) -> Result<(), ExportError> {
    let why = if !has_uvs {
        "the part has no UVs to sample it with"
    } else if t.mime != "image/png" && t.mime != "image/jpeg" && texture_extension(t).is_none() {
        "only PNG, JPEG, KTX2 and WebP images are written"
    } else {
        return Ok(());
    };
    Err(ExportError::BadTexture {
        part: part.to_string(),
        why: why.to_string(),
    })
}

/// The textures' images, appended to `bin` (each 4-byte aligned) with
/// their buffer views, and the glTF `images`, `textures` and `samplers`
/// (one sampler: linear, repeat).
pub(crate) fn texture_json(
    textures: &[&Texture],
    bin: &mut Vec<u8>,
    views: &mut Vec<Json>,
) -> Vec<(&'static str, Json)> {
    if textures.is_empty() {
        return Vec::new();
    }
    let mut images = Vec::with_capacity(textures.len());
    for t in textures {
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
        let start = bin.len();
        bin.extend_from_slice(&t.bytes);
        views.push(Json::Obj(vec![
            ("buffer", Json::Int(0)),
            ("byteOffset", Json::Int(start as u64)),
            ("byteLength", Json::Int(t.bytes.len() as u64)),
        ]));
        let mut image = vec![
            ("bufferView", Json::Int(views.len() as u64 - 1)),
            ("mimeType", Json::Str(t.mime.clone())),
        ];
        if !t.name.is_empty() {
            image.insert(0, ("name", Json::Str(t.name.clone())));
        }
        images.push(Json::Obj(image));
    }
    vec![
        ("images", Json::Arr(images)),
        (
            "samplers",
            Json::Arr(vec![Json::Obj(vec![
                ("magFilter", Json::Int(9729)),
                ("minFilter", Json::Int(9987)),
            ])]),
        ),
        (
            "textures",
            Json::Arr(
                textures
                    .iter()
                    .enumerate()
                    .map(|(k, t)| {
                        // A core image is the texture's source; another
                        // format's is under its extension.
                        let source = ("source", Json::Int(k as u64));
                        Json::Obj(vec![
                            ("sampler", Json::Int(0)),
                            match texture_extension(t) {
                                Some(ext) => (
                                    "extensions",
                                    Json::Obj(vec![(ext, Json::Obj(vec![source]))]),
                                ),
                                None => source,
                            },
                        ])
                    })
                    .collect(),
            ),
        ),
    ]
}

/// UVs as glTF writes them: f32, v flipped (glTF's origin is the top
/// left).
pub(crate) fn gltf_uv(q: shape_core::DVec2) -> [f32; 2] {
    [q.x as f32, (1.0 - q.y) as f32]
}

/// Skinned parts' clips as glTF animations ([`glb_skin_clips`]): a clip
/// name is one animation, with channels for every part that has it.
fn skin_animations(
    skin_clips: &[(&str, &[shape_mesh::SkinClip])],
    skin_nodes: &BTreeMap<&str, (&shape_mesh::Skin, Vec<usize>)>,
    (part_node, morph_counts): (&BTreeMap<&str, usize>, &BTreeMap<&str, usize>),
    bin: &mut Vec<u8>,
    views: &mut Vec<Json>,
    accessors: &mut Vec<Json>,
) -> Result<Vec<Json>, ExportError> {
    use shape_mesh::skin::{affine_inverse, mat_mul};
    let mut names: Vec<&str> = Vec::new();
    for (_, clips) in skin_clips {
        for c in *clips {
            if !names.contains(&c.name.as_str()) {
                names.push(&c.name);
            }
        }
    }
    let mut out = Vec::new();
    for name in names {
        let bad = |why: String| ExportError::BadClip {
            clip: name.to_string(),
            why,
        };
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for (part, clips) in skin_clips {
            let Some(clip) = clips.iter().find(|c| c.name == name) else {
                continue;
            };
            let targets = morph_counts.get(part).copied().unwrap_or(0);
            let driven = !clip.morphs.is_empty();
            let none = (&shape_mesh::Skin::default(), Vec::new());
            let (skin, nodes) = match skin_nodes.get(part) {
                Some((skin, nodes)) => (*skin, nodes),
                // A clip of morph weights alone needs no skin.
                None if driven && targets > 0 => (none.0, &none.1),
                None => return Err(bad(format!("part {part:?} has no skin in the scene"))),
            };
            if driven
                && (clip.morphs.len() != clip.times.len()
                    || clip
                        .morphs
                        .iter()
                        .any(|r| r.len() != targets || r.iter().any(|w| !w.is_finite())))
            {
                return Err(bad(format!(
                    "part {part:?}: a weight a morph target ({targets}) at each time"
                )));
            }
            let n = skin.bones.len();
            if clip.times.is_empty()
                || clip.poses.len() != clip.times.len()
                || clip.poses.iter().any(|p| p.len() != n)
                || clip.times.iter().any(|t| !t.is_finite())
                || clip.times.windows(2).any(|w| w[1] <= w[0])
            {
                return Err(bad(format!(
                    "part {part:?}: its times must rise, with a matrix a bone at each"
                )));
            }
            let unbind: Vec<Mat4> = skin
                .bones
                .iter()
                .map(|b| {
                    affine_inverse(&b.inverse_bind).ok_or_else(|| {
                        bad(format!(
                            "bone {:?}'s inverse bind matrix is singular",
                            b.name
                        ))
                    })
                })
                .collect::<Result<_, _>>()?;
            // Per bone, per key: translation, rotation, scale (glTF space).
            let mut tracks: Vec<Vec<Trs>> = vec![Vec::with_capacity(clip.times.len()); n];
            for pose in &clip.poses {
                // Each bone in the world then: its skinning matrix undone
                // of its inverse bind. Its node is that under its parent's.
                let world: Vec<Mat4> = (0..n).map(|b| mat_mul(&pose[b], &unbind[b])).collect();
                for b in 0..n {
                    let local = match skin.bones[b].parent {
                        Some(p) => {
                            let up = affine_inverse(&world[p as usize]).ok_or_else(|| {
                                bad(format!(
                                    "bone {:?} is flattened by the clip",
                                    skin.bones[p as usize].name
                                ))
                            })?;
                            mat_mul(&up, &world[b])
                        }
                        None => world[b],
                    };
                    let mut key = decompose(&crate::skin::gltf_matrix(&local));
                    if let Some(last) = tracks[b].last() {
                        // The same rotation the short way from the last key.
                        let dot: f64 = (0..4).map(|k| key.1[k] * last.1[k]).sum();
                        if dot < 0.0 {
                            key.1 = key.1.map(|c| -c);
                        }
                    }
                    if !key
                        .0
                        .iter()
                        .chain(&key.1)
                        .chain(&key.2)
                        .all(|c| c.is_finite())
                    {
                        return Err(bad(format!(
                            "bone {:?} is not finite in it",
                            skin.bones[b].name
                        )));
                    }
                    tracks[b].push(key);
                }
            }
            let mut plain = |bin: &Vec<u8>, start: usize| {
                views.push(Json::Obj(vec![
                    ("buffer", Json::Int(0)),
                    ("byteOffset", Json::Int(start as u64)),
                    ("byteLength", Json::Int((bin.len() - start) as u64)),
                ]));
                (views.len() - 1) as u64
            };
            for (b, track) in tracks.iter().enumerate() {
                // A scale channel only for a bone the clip scales.
                let scaled = track
                    .iter()
                    .any(|k| k.2.iter().any(|s| (s - 1.0).abs() > 1e-6));
                for (path, kind, on) in [
                    ("translation", "VEC3", true),
                    ("rotation", "VEC4", true),
                    ("scale", "VEC3", scaled),
                ] {
                    if !on {
                        continue;
                    }
                    let values: Vec<[f64; 4]> = track
                        .iter()
                        .map(|k| match path {
                            "translation" => [k.0[0], k.0[1], k.0[2], 0.0],
                            "rotation" => k.1,
                            _ => [k.2[0], k.2[1], k.2[2], 0.0],
                        })
                        .collect();
                    let width = if path == "rotation" { 4 } else { 3 };
                    // The samples a straight line between the kept ones
                    // does not give back, to a hundredth of a millimetre
                    // (a few thousandths of a degree).
                    let tolerance = match path {
                        "translation" => KEEP_METRES,
                        "rotation" => KEEP_TURN,
                        _ => KEEP_SCALE,
                    };
                    // The first bone's translation keeps the clip's last
                    // time, so the clip is as long as it was.
                    let whole = b == 0 && path == "translation";
                    let kept = thin(&clip.times, &values, tolerance, whole);
                    let start = bin.len();
                    for &k in &kept {
                        push_f32(bin, clip.times[k] as f32);
                    }
                    let tv = plain(bin, start);
                    accessors.push(Json::Obj(vec![
                        ("bufferView", Json::Int(tv)),
                        ("componentType", Json::Int(FLOAT)),
                        ("count", Json::Int(kept.len() as u64)),
                        ("type", Json::Str("SCALAR".into())),
                        ("min", Json::Arr(vec![f32n(clip.times[kept[0]] as f32)])),
                        (
                            "max",
                            Json::Arr(vec![f32n(clip.times[kept[kept.len() - 1]] as f32)]),
                        ),
                    ]));
                    let input = (accessors.len() - 1) as u64;
                    let start = bin.len();
                    for &k in &kept {
                        for &c in &values[k][..width] {
                            push_f32(bin, c as f32);
                        }
                    }
                    let ov = plain(bin, start);
                    accessors.push(Json::Obj(vec![
                        ("bufferView", Json::Int(ov)),
                        ("componentType", Json::Int(FLOAT)),
                        ("count", Json::Int(kept.len() as u64)),
                        ("type", Json::Str(kind.into())),
                    ]));
                    let output = (accessors.len() - 1) as u64;
                    channels.push(Json::Obj(vec![
                        ("sampler", Json::Int(samplers.len() as u64)),
                        (
                            "target",
                            Json::Obj(vec![
                                ("node", Json::Int(nodes[b] as u64)),
                                ("path", Json::Str(path.into())),
                            ]),
                        ),
                    ]));
                    samplers.push(Json::Obj(vec![
                        ("input", Json::Int(input)),
                        ("interpolation", Json::Str("LINEAR".into())),
                        ("output", Json::Int(output)),
                    ]));
                }
            }
            if driven {
                // A weight a target at each key kept, the targets in order.
                let Some(&node) = part_node.get(part) else {
                    return Err(bad(format!("part {part:?} is not in the scene")));
                };
                let kept = thin(&clip.times, &clip.morphs, KEEP_WEIGHT, true);
                let start = bin.len();
                for &k in &kept {
                    push_f32(bin, clip.times[k] as f32);
                }
                let tv = plain(bin, start);
                accessors.push(Json::Obj(vec![
                    ("bufferView", Json::Int(tv)),
                    ("componentType", Json::Int(FLOAT)),
                    ("count", Json::Int(kept.len() as u64)),
                    ("type", Json::Str("SCALAR".into())),
                    ("min", Json::Arr(vec![f32n(clip.times[kept[0]] as f32)])),
                    (
                        "max",
                        Json::Arr(vec![f32n(clip.times[kept[kept.len() - 1]] as f32)]),
                    ),
                ]));
                let input = (accessors.len() - 1) as u64;
                let start = bin.len();
                for &k in &kept {
                    for &w in &clip.morphs[k] {
                        push_f32(bin, w as f32);
                    }
                }
                let ov = plain(bin, start);
                accessors.push(Json::Obj(vec![
                    ("bufferView", Json::Int(ov)),
                    ("componentType", Json::Int(FLOAT)),
                    ("count", Json::Int((kept.len() * targets) as u64)),
                    ("type", Json::Str("SCALAR".into())),
                ]));
                let output = (accessors.len() - 1) as u64;
                channels.push(Json::Obj(vec![
                    ("sampler", Json::Int(samplers.len() as u64)),
                    (
                        "target",
                        Json::Obj(vec![
                            ("node", Json::Int(node as u64)),
                            ("path", Json::Str("weights".into())),
                        ]),
                    ),
                ]));
                samplers.push(Json::Obj(vec![
                    ("input", Json::Int(input)),
                    ("interpolation", Json::Str("LINEAR".into())),
                    ("output", Json::Int(output)),
                ]));
            }
        }
        if !channels.is_empty() {
            out.push(Json::Obj(vec![
                ("name", Json::Str(name.to_string())),
                ("channels", Json::Arr(channels)),
                ("samplers", Json::Arr(samplers)),
            ]));
        }
    }
    Ok(out)
}

/// How far a translation may be from the straight line between the keys
/// kept round it, in metres (a hundredth of a millimetre).
const KEEP_METRES: f64 = 1e-5;
/// The same for a rotation's quaternion, by component: about two
/// thousandths of a degree (0.04 mm at the end of a metre).
const KEEP_TURN: f64 = 2e-5;
/// The same for a scale.
const KEEP_SCALE: f64 = 1e-5;

/// The keys of a curve to keep (indices, rising, the first always): the
/// fewest by halving such that every key left out is within `tolerance`,
/// in each of its numbers, of the straight line between the kept keys
/// either side of it. A curve that never leaves its first value keeps
/// that one key, unless `whole` asks for its last too (so a clip keeps
/// its length).
fn thin<V: AsRef<[f64]>>(times: &[f64], values: &[V], tolerance: f64, whole: bool) -> Vec<usize> {
    let n = values.len();
    let values: Vec<&[f64]> = values.iter().map(AsRef::as_ref).collect();
    let off = |a: usize, b: usize, k: usize| {
        let f = if times[b] > times[a] {
            (times[k] - times[a]) / (times[b] - times[a])
        } else {
            0.0
        };
        (0..values[k].len())
            .map(|i| (values[a][i] + (values[b][i] - values[a][i]) * f - values[k][i]).abs())
            .fold(0.0, f64::max)
    };
    let mut keep = vec![false; n];
    keep[0] = true;
    if n > 1 {
        keep[n - 1] = true;
        // Spans still to check, each split at the key furthest off.
        let mut spans = vec![(0usize, n - 1)];
        while let Some((a, b)) = spans.pop() {
            let mut worst = (tolerance, None);
            for k in a + 1..b {
                let d = off(a, b, k);
                if d > worst.0 {
                    worst = (d, Some(k));
                }
            }
            if let Some(k) = worst.1 {
                keep[k] = true;
                spans.push((a, k));
                spans.push((k, b));
            }
        }
        // A curve that stays where it starts is one key.
        if !whole && keep.iter().filter(|&&k| k).count() == 2 && off(0, 0, n - 1) <= tolerance {
            keep[n - 1] = false;
        }
    }
    (0..n).filter(|&k| keep[k]).collect()
}

/// The same for a morph target's weight.
const KEEP_WEIGHT: f64 = 1e-4;

/// A translation, a rotation (a unit quaternion x, y, z, w) and a scale.
type Trs = ([f64; 3], [f64; 4], [f64; 3]);

/// An affine matrix as a translation, a rotation (a unit quaternion x, y,
/// z, w) and a scale: the columns' lengths are the scale (the first
/// negative for a mirror), what is left the rotation.
fn decompose(m: &Mat4) -> Trs {
    let col = |c: usize| DVec3::new(m[4 * c], m[4 * c + 1], m[4 * c + 2]);
    let (mut x, y, z) = (col(0), col(1), col(2));
    let mut scale = [x.length(), y.length(), z.length()];
    if x.cross(y).dot(z) < 0.0 {
        scale[0] = -scale[0];
        x = -x;
    }
    let unit = |v: DVec3, fallback: DVec3| {
        let l = v.length();
        if l > 0.0 { v / l } else { fallback }
    };
    let (x, y, z) = (unit(x, DVec3::X), unit(y, DVec3::Y), unit(z, DVec3::Z));
    // The quaternion of the rotation with columns x, y, z.
    let (m00, m11, m22) = (x.x, y.y, z.z);
    let trace = m00 + m11 + m22;
    let q = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        [(y.z - z.y) / s, (z.x - x.z) / s, (x.y - y.x) / s, 0.25 * s]
    } else if m00 > m11 && m00 > m22 {
        let s = (1.0 + m00 - m11 - m22).sqrt() * 2.0;
        [0.25 * s, (y.x + x.y) / s, (z.x + x.z) / s, (y.z - z.y) / s]
    } else if m11 > m22 {
        let s = (1.0 + m11 - m00 - m22).sqrt() * 2.0;
        [(y.x + x.y) / s, 0.25 * s, (z.y + y.z) / s, (z.x - x.z) / s]
    } else {
        let s = (1.0 + m22 - m00 - m11).sqrt() * 2.0;
        [(z.x + x.z) / s, (z.y + y.z) / s, 0.25 * s, (x.y - y.x) / s]
    };
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    let q = if len > 0.0 {
        q.map(|c| c / len)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    };
    ([m[12], m[13], m[14]], q, scale)
}

/// The clips as glTF animations ([`glb_clips`]): their accessors and
/// views are appended.
fn animations(
    clips: &[Clip],
    checked: &[(Option<Checked>, Rigid)],
    joint_node: &BTreeMap<&str, usize>,
    bin: &mut Vec<u8>,
    views: &mut Vec<Json>,
    accessors: &mut Vec<Json>,
) -> Result<Vec<Json>, ExportError> {
    let joints: BTreeMap<&str, &Checked> = checked
        .iter()
        .filter_map(|(c, _)| c.as_ref())
        .map(|c| (c.name.as_str(), c))
        .collect();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut out = Vec::new();
    for clip in clips {
        let bad = |why: String| ExportError::BadClip {
            clip: clip.name.clone(),
            why,
        };
        if clip.name.is_empty() {
            return Err(bad("it has no name".into()));
        }
        if !seen.insert(clip.name.as_str()) {
            return Err(bad("another clip has this name".into()));
        }
        if clip.keys.is_empty() {
            return Err(bad("it has no key".into()));
        }
        let mut last = -1.0;
        // Each leader's own keys, in kernel units.
        let mut tracks: BTreeMap<&str, Vec<(f64, f64)>> = BTreeMap::new();
        for key in &clip.keys {
            if !(key.time.is_finite() && key.time >= 0.0 && key.time > last) {
                return Err(bad(format!(
                    "its times must be 0 or more and rise: {} after {}",
                    key.time,
                    if last < 0.0 { 0.0 } else { last }
                )));
            }
            last = key.time;
            for (name, value) in &key.pose {
                let Some(j) = joints.get(name.as_str()) else {
                    return Err(bad(format!(
                        "no joint {name:?}; joints: {}",
                        joints.keys().copied().collect::<Vec<_>>().join(", ")
                    )));
                };
                if j.local.is_none() {
                    return Err(bad(format!(
                        "joint {name:?} is fixed or floating: it has no value"
                    )));
                }
                if let Some((leader, _)) = &j.follows {
                    return Err(bad(format!(
                        "joint {name:?} follows {leader:?}: key that one"
                    )));
                }
                if !value.is_finite() {
                    return Err(bad(format!("joint {name:?}'s value is not finite")));
                }
                let track = tracks.entry(j.name.as_str()).or_default();
                if track.last().is_some_and(|&(t, _)| t == key.time) {
                    return Err(bad(format!(
                        "joint {name:?} is keyed twice at {}",
                        key.time
                    )));
                }
                track.push((key.time, *value));
            }
        }
        // Followers take their leader's keys.
        let led: Vec<(&str, Vec<(f64, f64)>)> = joints
            .values()
            .filter_map(|j| {
                let (leader, k) = j.follows.as_ref()?;
                let track = tracks.get(leader.as_str())?;
                Some((
                    j.name.as_str(),
                    track.iter().map(|&(t, v)| (t, v * k)).collect(),
                ))
            })
            .collect();
        tracks.extend(led);

        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for (name, track) in &tracks {
            let j = joints[name];
            let (Some(axis), Some(&node)) = (j.local, joint_node.get(name)) else {
                continue;
            };
            if let Some((lo, hi)) = j.kernel_span {
                let slack = 1e-9 * lo.abs().max(hi.abs()).max(1.0);
                if let Some(&(t, v)) = track
                    .iter()
                    .find(|&&(_, v)| !(lo - slack <= v && v <= hi + slack))
                {
                    return Err(bad(format!(
                        "joint {name:?} at {t}: {v} is outside its range {lo} to {hi}"
                    )));
                }
            }
            // A turn is written a quarter at a time at most.
            let mut keys: Vec<(f64, f64)> = Vec::with_capacity(track.len());
            for &(t, v) in track {
                if let (Some(&(t0, v0)), false) = (keys.last(), j.slides) {
                    let steps = ((v - v0).abs() / core::f64::consts::FRAC_PI_2).ceil();
                    let mut k = 1.0;
                    while k < steps {
                        let f = k / steps;
                        keys.push((t0 + (t - t0) * f, v0 + (v - v0) * f));
                        k += 1.0;
                    }
                }
                keys.push((t, v));
            }
            let start = bin.len();
            for &(t, _) in &keys {
                push_f32(bin, t as f32);
            }
            let mut plain = |bin: &Vec<u8>, start: usize| {
                views.push(Json::Obj(vec![
                    ("buffer", Json::Int(0)),
                    ("byteOffset", Json::Int(start as u64)),
                    ("byteLength", Json::Int((bin.len() - start) as u64)),
                ]));
                (views.len() - 1) as u64
            };
            let tv = plain(bin, start);
            accessors.push(Json::Obj(vec![
                ("bufferView", Json::Int(tv)),
                ("componentType", Json::Int(FLOAT)),
                ("count", Json::Int(keys.len() as u64)),
                ("type", Json::Str("SCALAR".into())),
                ("min", Json::Arr(vec![f32n(keys[0].0 as f32)])),
                ("max", Json::Arr(vec![f32n(keys[keys.len() - 1].0 as f32)])),
            ]));
            let input = (accessors.len() - 1) as u64;
            let start = bin.len();
            let mut before: Option<[f64; 4]> = None;
            for &(_, v) in &keys {
                let motion = if j.slides {
                    Rigid {
                        t: axis * v,
                        ..Rigid::IDENTITY
                    }
                } else {
                    Rigid::turn(axis, v)
                };
                let (t, mut q) = j.pivot.then(&motion).gltf();
                if j.slides {
                    for c in [t.x, t.y, t.z] {
                        push_f32(bin, c as f32);
                    }
                } else {
                    // The same rotation the short way from the last key.
                    if let Some(b) = before {
                        let dot: f64 = (0..4).map(|k| q[k] * b[k]).sum();
                        if dot < 0.0 {
                            q = q.map(|c| -c);
                        }
                    }
                    before = Some(q);
                    for c in q {
                        push_f32(bin, c as f32);
                    }
                }
            }
            let ov = plain(bin, start);
            accessors.push(Json::Obj(vec![
                ("bufferView", Json::Int(ov)),
                ("componentType", Json::Int(FLOAT)),
                ("count", Json::Int(keys.len() as u64)),
                (
                    "type",
                    Json::Str(if j.slides { "VEC3" } else { "VEC4" }.into()),
                ),
            ]));
            let output = (accessors.len() - 1) as u64;
            channels.push(Json::Obj(vec![
                ("sampler", Json::Int(samplers.len() as u64)),
                (
                    "target",
                    Json::Obj(vec![
                        ("node", Json::Int(node as u64)),
                        (
                            "path",
                            Json::Str(if j.slides { "translation" } else { "rotation" }.into()),
                        ),
                    ]),
                ),
            ]));
            samplers.push(Json::Obj(vec![
                ("input", Json::Int(input)),
                ("interpolation", Json::Str("LINEAR".into())),
                ("output", Json::Int(output)),
            ]));
        }
        if channels.is_empty() {
            return Err(bad("it moves no joint".into()));
        }
        out.push(Json::Obj(vec![
            ("name", Json::Str(clip.name.clone())),
            ("channels", Json::Arr(channels)),
            ("samplers", Json::Arr(samplers)),
        ]));
    }
    Ok(out)
}

fn vec3(v: DVec3) -> Json {
    nums(&[v.x, v.y, v.z])
}

/// Where [`obj_at`] and [`stl_at`] put jointed parts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Placement {
    /// As built: every joint at 0, whatever its value (the zero value).
    #[default]
    Rest,
    /// Moved by the joints' values (followers by their leaders'), as the
    /// GLB's nodes and the preview place them. The file's header says
    /// `posed`.
    Pose,
}

/// A part's name, its positions in metres (kernel axes) and its triangles.
type Metres<'s> = (&'s str, Vec<DVec3>, &'s [[u32; 3]]);

/// Each part's motion from rest at `at`, after the scene's checks: `None`
/// where no non-zero joint value moves it (at rest, every part).
fn motions(scene: &Scene, at: Placement) -> Result<Vec<Option<Rigid>>, ExportError> {
    let checked = check_joints(scene)?;
    let mut moved: BTreeMap<&str, Option<Rigid>> = BTreeMap::new();
    let mut out = Vec::with_capacity(scene.parts.len());
    for (p, (c, _)) in scene.parts.iter().zip(&checked) {
        let above = moved.get(p.parent.as_str()).copied().flatten();
        let mine = match c {
            Some(c) if at == Placement::Pose && c.value != 0.0 => Some(
                above
                    .unwrap_or(Rigid::IDENTITY)
                    .then(&c.node)
                    .then(&c.inverse),
            ),
            _ => above,
        };
        moved.insert(p.name.as_str(), mine);
        out.push(mine);
    }
    Ok(out)
}

/// Each part's solid at the scene's pose (millimetres, kernel axes), as
/// [`obj_at`] with [`Placement::Pose`] places it: a part that no non-zero
/// joint value moves is its solid, bit for bit; the others are moved as
/// the preview moves them (names, surfaces and tags kept).
///
/// Errors as [`glb`] does on an invalid scene.
pub fn posed(scene: &Scene) -> Result<Vec<Solid>, ExportError> {
    Ok(scene
        .parts
        .iter()
        .zip(motions(scene, Placement::Pose)?)
        .map(|(p, m)| {
            let mut s = p.solid.clone();
            if let Some(m) = m {
                s.map_positions(|v| m.apply(v), false);
            }
            s
        })
        .collect())
}

/// Every part's triangles in metres, after the scene's checks, at `at`.
fn meshes<'s>(scene: &'s Scene, at: Placement) -> Result<Vec<Metres<'s>>, ExportError> {
    Ok(scene
        .parts
        .iter()
        .zip(motions(scene, at)?)
        .map(|(p, m)| {
            let positions = p
                .solid
                .positions
                .iter()
                .map(|&v| m.map_or(v, |m| m.apply(v)) * METRES)
                .collect();
            (p.name.as_str(), positions, p.solid.triangles.as_slice())
        })
        .collect())
}

/// The scene's meshes as Wavefront OBJ text, at rest: [`obj_at`] with
/// [`Placement::Rest`].
pub fn obj(scene: &Scene) -> Result<String, ExportError> {
    obj_at(scene, Placement::Rest)
}

/// The scene's meshes as Wavefront OBJ text: one `o` per part, in metres,
/// Y up (the GLB's axes), coordinates as f32, placed at `at`. No normals,
/// materials or joints.
pub fn obj_at(scene: &Scene, at: Placement) -> Result<String, ExportError> {
    let posed = if at == Placement::Pose { ", posed" } else { "" };
    let mut out = format!(
        "# {} - written by og-shape, metres, Y up{posed}\n",
        scene.name
    );
    let mut base = 1u64;
    for (name, pos, tris) in meshes(scene, at)? {
        out.push_str(&format!("o {name}\n"));
        for p in &pos {
            let q = y_up(*p);
            out.push_str(&format!("v {} {} {}\n", q.x as f32, q.y as f32, q.z as f32));
        }
        for t in tris {
            out.push_str(&format!(
                "f {} {} {}\n",
                base + u64::from(t[0]),
                base + u64::from(t[1]),
                base + u64::from(t[2])
            ));
        }
        base += pos.len() as u64;
    }
    Ok(out)
}

/// The scene's meshes as binary STL, at rest: [`stl_at`] with
/// [`Placement::Rest`].
pub fn stl(scene: &Scene) -> Result<Vec<u8>, ExportError> {
    stl_at(scene, Placement::Rest)
}

/// The scene's meshes as binary STL: an 80-byte header, the triangle
/// count, then per triangle its unit normal and corners, in metres, Z up
/// (as slicers expect), placed at `at`.
pub fn stl_at(scene: &Scene, at: Placement) -> Result<Vec<u8>, ExportError> {
    let parts = meshes(scene, at)?;
    let count: usize = parts.iter().map(|p| p.2.len()).sum();
    let mut out = Vec::with_capacity(84 + 50 * count);
    let posed = if at == Placement::Pose { ", posed" } else { "" };
    let mut header =
        format!("{} - written by og-shape, metres, Z up{posed}", scene.name).into_bytes();
    header.truncate(80);
    header.resize(80, b' ');
    out.extend_from_slice(&header);
    out.extend_from_slice(&(count as u32).to_le_bytes());
    for (_, pos, tris) in &parts {
        for t in tris.iter() {
            let [a, b, c] = t.map(|v| pos[v as usize]);
            let n = face_normal(a, b, c);
            for v in [n, a, b, c] {
                for x in [v.x, v.y, v.z] {
                    push_f32(&mut out, x as f32);
                }
            }
            out.extend_from_slice(&0u16.to_le_bytes());
        }
    }
    Ok(out)
}

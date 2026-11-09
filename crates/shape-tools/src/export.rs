//! GLB, OBJ and STL from a [`Scene`] of named solids.

use std::collections::BTreeMap;
use std::fmt;

use shape_core::{DVec3, Frame, trig};
use shape_mesh::Solid;

use crate::json::{Json, f32n, nums};
use crate::surface::{METRES, face_normal, split, y_up};

/// What a part is made of: glTF's metallic-roughness model with
/// three.js's `MeshStandardMaterial` names. The zero value is three.js's
/// default material (white, roughness 1, not metal, opaque).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Material {
    /// The material's name in the file; empty takes the part's name.
    pub name: String,
    /// Base colour, sRGB components in 0..=1 (three.js `color`); `None`
    /// is white.
    pub color: Option<[f64; 3]>,
    /// 0 dielectric, 1 metal (three.js `metalness`).
    pub metalness: f64,
    /// 0 mirror, 1 matte (three.js `roughness`); `None` is 1.
    pub roughness: Option<f64>,
    /// Light given off, sRGB components in 0..=1 (three.js `emissive`).
    pub emissive: [f64; 3],
    /// 1 opaque, 0 invisible (three.js `opacity`); `None` is 1. Below 1
    /// the material blends.
    pub opacity: Option<f64>,
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
    /// A floating joint was given an axis, a range or a mimic.
    FloatingConstrained(String),
    /// A fixed joint was given an axis, a range, a mimic or a value.
    FixedConstrained(String),
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
struct Checked {
    name: String,
    kind: &'static str,
    /// The joint's node: the pivot after the joint's motion.
    node: Rigid,
    /// The part's node under it: the pivot's inverse.
    inverse: Rigid,
    axis: Option<DVec3>,
    value: f64,
    span: Option<(f64, f64)>,
    mimic: Option<(String, f64)>,
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

    fn of(f: &Frame) -> Rigid {
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
    fn then(&self, other: &Rigid) -> Rigid {
        Rigid {
            x: self.rotate(other.x),
            y: self.rotate(other.y),
            z: self.rotate(other.z),
            t: self.apply(other.t),
        }
    }

    fn inverse(&self) -> Rigid {
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
    fn gltf(&self) -> (DVec3, [f64; 4]) {
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

fn check_joints(scene: &Scene) -> Result<Vec<(Option<Checked>, Rigid)>, ExportError> {
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
            }),
            world,
        ));
    }
    Ok(out)
}

/// sRGB component to linear (glTF factors are linear).
fn linear(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.04045 {
        c / 12.92
    } else {
        trig::pow((c + 0.055) / 1.055, 2.4)
    }
}

fn material_json(m: &Material, part: &str) -> Result<Json, ExportError> {
    let name = if m.name.is_empty() { part } else { &m.name };
    let what = || format!("material {name:?}");
    let color = m.color.unwrap_or([1.0; 3]);
    let opacity = m.opacity.unwrap_or(1.0);
    let roughness = m.roughness.unwrap_or(1.0);
    for v in color
        .iter()
        .chain(&m.emissive)
        .chain([&opacity, &roughness, &m.metalness])
    {
        finite(*v, what)?;
    }
    let base = [
        linear(color[0]),
        linear(color[1]),
        linear(color[2]),
        opacity.clamp(0.0, 1.0),
    ];
    let mut fields = vec![
        ("name", Json::Str(name.to_string())),
        (
            "pbrMetallicRoughness",
            Json::Obj(vec![
                ("baseColorFactor", nums(&base)),
                ("metallicFactor", Json::Num(m.metalness.clamp(0.0, 1.0))),
                ("roughnessFactor", Json::Num(roughness.clamp(0.0, 1.0))),
            ]),
        ),
    ];
    if m.emissive != [0.0; 3] {
        fields.push(("emissiveFactor", nums(&m.emissive.map(linear))));
    }
    if opacity < 1.0 {
        fields.push(("alphaMode", Json::Str("BLEND".into())));
    }
    Ok(Json::Obj(fields))
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

fn push_f32(bin: &mut Vec<u8>, v: f32) {
    bin.extend_from_slice(&v.to_le_bytes());
}

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
///
/// The bytes depend only on the scene.
pub fn glb(scene: &Scene) -> Result<Vec<u8>, ExportError> {
    let checked = check_joints(scene)?;
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
        }
        materials.push(material_json(&part.material, &part.name)?);
        let mesh_index = meshes.len() as u64;
        if !primitive.is_empty() {
            primitive.push(("material", Json::Int((materials.len() - 1) as u64)));
            primitive.push(("mode", Json::Int(4)));
            meshes.push(Json::Obj(vec![
                ("name", Json::Str(part.name.clone())),
                ("primitives", Json::Arr(vec![Json::Obj(primitive)])),
            ]));
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
        }
        match joint {
            Some(j) => {
                // The joint's node at the pivot; the part's node undoes the
                // pivot, so its mesh stays where it was built.
                let jn = nodes.len();
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
            }
        }
    }
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
    if !meshes.is_empty() {
        doc.push(("meshes", Json::Arr(meshes)));
        doc.push(("materials", Json::Arr(materials)));
        doc.push(("accessors", Json::Arr(accessors)));
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

fn vec3(v: DVec3) -> Json {
    nums(&[v.x, v.y, v.z])
}

/// A part's name, its positions in metres (kernel axes) and its triangles.
type Metres<'s> = (&'s str, Vec<DVec3>, &'s [[u32; 3]]);

/// Every part's triangles, after the scene's checks.
fn meshes<'s>(scene: &'s Scene) -> Result<Vec<Metres<'s>>, ExportError> {
    check(scene)?;
    Ok(scene
        .parts
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                p.solid.positions.iter().map(|&v| v * METRES).collect(),
                p.solid.triangles.as_slice(),
            )
        })
        .collect())
}

/// The scene's meshes as Wavefront OBJ text: one `o` per part, in metres,
/// Y up (the GLB's axes), coordinates as f32. No normals, materials or
/// joints.
pub fn obj(scene: &Scene) -> Result<String, ExportError> {
    let mut out = format!("# {} - written by og-shape, metres, Y up\n", scene.name);
    let mut base = 1u64;
    for (name, pos, tris) in meshes(scene)? {
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

/// The scene's meshes as binary STL: an 80-byte header, the triangle
/// count, then per triangle its unit normal and corners, in metres, Z up
/// (as slicers expect).
pub fn stl(scene: &Scene) -> Result<Vec<u8>, ExportError> {
    let parts = meshes(scene)?;
    let count: usize = parts.iter().map(|p| p.2.len()).sum();
    let mut out = Vec::with_capacity(84 + 50 * count);
    let mut header = format!("{} - written by og-shape, metres, Z up", scene.name).into_bytes();
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

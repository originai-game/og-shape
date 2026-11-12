//! Import of glTF 2.0 (`.glb`, and `.gltf` with embedded or external
//! buffers), Wavefront OBJ and FBX 7 ([`crate::import_fbx`]) into parts: a [`Solid`] each, in its own
//! coordinates, with the transform that places it.
//!
//! Units and axes: the kernel works in millimetres with Z up. glTF is
//! metres with Y up by its specification, and OBJ is read the same way
//! unless [`Import`] says otherwise; both are turned into kernel space
//! (glTF −Z becomes kernel +Y, +Y becomes +Z, as the exporter does in
//! reverse). Transforms are conjugated by the same axis change exactly
//! (only signs and swaps), so no rounding enters there.
//!
//! Names: each part's faces carry the part's name (the glTF node's name,
//! else its mesh's, else `node<i>`; the OBJ `o` or `g` name, else
//! `object`), made unique with `.1`, `.2`… Each glTF primitive and each
//! OBJ material run is its own surface.
//!
//! Every polygon is triangulated on the way in (glTF strips and fans,
//! OBJ faces of more than three corners). Triangles stay in file order and
//! positions are not welded; repair welds. The first UV set (glTF
//! `TEXCOORD_0`, OBJ `vt`, FBX's first UV layer) goes into
//! [`Solid::uvs`], per triangle corner, when every triangle of the part
//! has one; a part where only some do has none, and the import says so
//! in [`Imported::skipped`]. Vertex colours (glTF `COLOR_0`, OBJ
//! `v x y z r g b`) go into [`Solid::colors`], linear RGBA per corner.
//! The part's first material is kept as a [`Material`]
//! ([`ImportedPart::material`]): its factors as the file means them
//! (glTF's defaults where it leaves one unset, listed in
//! [`ImportedPart::defaults`]) and its maps (base colour, metallic-
//! roughness, normal, emissive, occlusion) as their files' bytes.

use std::collections::BTreeMap;
use std::fmt;

use shape_core::{DVec2, DVec3, Frame};
use shape_mesh::{Bone, Skin, Solid, Weights};

use crate::export::{ColorSpace, Material};
use crate::image::{ImageInfo, Texture};
use crate::jsonread::{self, Value};
use crate::meshopt;
use crate::place::{Axis, AxisTurn, turned};
use crate::polygon::triangulate;

/// An affine map in kernel space: `p ↦ x·p.x + y·p.y + z·p.z + t`.
/// The zero value maps everything to the origin; start from
/// [`Affine::IDENTITY`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Affine {
    /// Image of local +X.
    pub x: DVec3,
    /// Image of local +Y.
    pub y: DVec3,
    /// Image of local +Z.
    pub z: DVec3,
    /// Image of the local origin.
    pub t: DVec3,
}

impl Affine {
    /// The map that changes nothing.
    pub const IDENTITY: Affine = Affine {
        x: DVec3::X,
        y: DVec3::Y,
        z: DVec3::Z,
        t: DVec3::ZERO,
    };

    /// The linear part applied to a direction.
    pub fn rotate(&self, v: DVec3) -> DVec3 {
        self.x * v.x + self.y * v.y + self.z * v.z
    }

    /// The map applied to a point.
    pub fn apply(&self, p: DVec3) -> DVec3 {
        self.rotate(p) + self.t
    }

    /// `self ∘ other`: `other` first.
    pub fn then(&self, other: &Affine) -> Affine {
        Affine {
            x: self.rotate(other.x),
            y: self.rotate(other.y),
            z: self.rotate(other.z),
            t: self.apply(other.t),
        }
    }

    /// The determinant of the linear part: negative for a mirror.
    pub fn determinant(&self) -> f64 {
        self.x.dot(self.y.cross(self.z))
    }
}

/// How to read a file. The zero value reads every format by its own
/// convention.
///
/// `up` and `front` (OBJ and glTF) name the file's own axes that are the
/// object's up and the way it faces; they become the kernel's +Z and −Y
/// by one of the 24 exact axis turns (`crate::place`), through
/// [`import`]. Give one and the
/// other follows by the smallest turn. Without either, the file is read
/// Y up and facing +Z, as glTF says (`z_up`: Z up and facing −Y). FBX
/// says its axes in its `GlobalSettings`, and takes none of these.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Import {
    /// OBJ only: millimetres per file unit; 0 reads metres (1000), as
    /// glTF is.
    pub unit_mm: f64,
    /// OBJ and glTF: the file is Z up already (no axis change); the same
    /// as `up` +z and `front` −y, and not given with them.
    pub z_up: bool,
    /// OBJ and glTF: the file's axis that is up.
    pub up: Option<Axis>,
    /// OBJ and glTF: the file's axis the object faces.
    pub front: Option<Axis>,
}

impl Import {
    /// The turn from the file's axes to the kernel's, or why the options
    /// disagree.
    fn axes(&self) -> Result<AxisTurn, ImportError> {
        let given = self.up.is_some() || self.front.is_some();
        if self.z_up && given {
            return Err(ImportError::Unsupported(
                "z_up together with up or front: z_up is up +z, front -y; give one or the other"
                    .into(),
            ));
        }
        if self.z_up {
            return Ok(AxisTurn::IDENTITY);
        }
        let (up, front) = if given {
            (self.up, self.front)
        } else {
            (Some(Axis::PosY), None)
        };
        AxisTurn::new(up, front).map_err(ImportError::Unsupported)
    }

    /// The same reading as a plain read and a turn after it: these
    /// options without `z_up`, `up` and `front`, and the kernel axes, of
    /// the file read so, that the options make up and front (for
    /// [`crate::Place`]'s `up` and `front`; `None` when they turn
    /// nothing). For OBJ and glTF; FBX takes no axes.
    pub fn as_turn(&self) -> Result<(Import, Option<Axis>, Option<Axis>), ImportError> {
        let plain = Import {
            unit_mm: self.unit_mm,
            ..Default::default()
        };
        if !self.turns() {
            return Ok((plain, None, None));
        }
        let y_up = AxisTurn::new(Some(Axis::PosY), None).map_err(ImportError::Unsupported)?;
        let back = y_up.then(&self.axes()?.inverse());
        Ok((
            plain,
            Some(back.image(Axis::PosZ)),
            Some(back.image(Axis::NegY)),
        ))
    }

    /// Whether the options ask for other axes than the format's own.
    pub fn turns(&self) -> bool {
        self.z_up || self.up.is_some() || self.front.is_some()
    }
}

/// One placed piece of an imported model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportedPart {
    /// Unique among the parts; every face carries it as a name.
    pub name: String,
    /// The geometry in the part's own coordinates, kernel units.
    pub solid: Solid,
    /// Places the part in the model (kernel units).
    pub transform: Affine,
    /// The first material the part uses (its name "" when it has none):
    /// factors as the file means them, and the maps whose bytes could be
    /// read (glTF's five; OBJ `map_Kd`, `map_Ke`, `norm`; FBX
    /// `DiffuseColor`, `EmissiveColor`, `NormalMap`), sampled at the
    /// solid's [`Solid::uvs`]. glTF's factors are linear
    /// ([`ColorSpace::Linear`]); MTL's and FBX's colours sRGB.
    pub material: Material,
    /// The glTF material factors the file leaves unset, which take the
    /// specification's defaults (`metallicFactor`: 1, a metal), by their
    /// glTF names; empty for OBJ and FBX.
    pub defaults: Vec<String>,
    /// The file's animations of this part's skin, each sampled at its key
    /// times (`shape_mesh::SkinClip`): glTF `animations` on the skin's
    /// joints and the nodes above them. Empty for a part without a skin,
    /// and for OBJ.
    pub clips: Vec<shape_mesh::SkinClip>,
}

/// What an import read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Imported {
    /// The model's pieces, in file order (glTF: the scene's nodes, depth
    /// first).
    pub parts: Vec<ImportedPart>,
    /// The textures the file references, with their sizes.
    pub images: Vec<ImageInfo>,
    /// Polygons of more than three corners that were triangulated.
    pub polygons_triangulated: usize,
    /// Things read past and not imported (points, lines, skins, morph
    /// targets), one line each.
    pub skipped: Vec<String>,
    /// The uniform scales taken out of skinned parts' bones, one per
    /// skinned part that had one.
    pub skin_scales: Vec<SkinScale>,
}

/// A uniform scale taken out of a skinned part's bones: their bind
/// transforms were rigid up to it (an armature scaled 0.01 for
/// centimetres, say). The bones' rest frames are their bind transforms in
/// the world (from the file's inverse bind matrices) with it divided out;
/// the vertices are placed where the file's bind pose puts them in the
/// world; the inverse bind matrices are recomputed from the rest frames.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkinScale {
    /// The part.
    pub part: String,
    /// The scale every bone's world transform carried.
    pub scale: f64,
    /// How far, in mm, the file's inverse bind matrices, with the scale
    /// factored back in, put a vertex from where the recomputed ones do:
    /// they agree to this.
    pub bind_error: f64,
    /// The largest departure of a bone's world transform from a uniform
    /// scale (relative axis lengths and cosines; exporters' rounding,
    /// below [`UNEVEN`]), made exact in its rest frame.
    pub uneven: f64,
    /// How far, in mm, the file's own bone nodes stand from the bind pose
    /// at the vertices they weight: 0 to rounding when the file is saved
    /// at its bind pose, more where it is saved posed. The bind pose (the
    /// inverse bind matrices) is what is kept.
    pub node_offset: f64,
}

/// Departures from a uniform scale up to this (relative: axis lengths,
/// and the cosines between axes) are exporters' rounding, and taken as
/// uniform; beyond it a scale is uneven or sheared.
pub const UNEVEN: f64 = 1e-4;

/// `a` as a uniform scale times a rigid motion, `(scale, rigid, uneven)`,
/// if it is one to [`UNEVEN`]: `scale` is the mean axis length, `rigid`
/// the axes made exactly orthonormal (Gram–Schmidt, x first), `uneven`
/// how far `a` was from that.
pub(crate) fn similarity(a: &Affine) -> Option<(f64, Affine, f64)> {
    let l = [a.x.length(), a.y.length(), a.z.length()];
    let s = (l[0] + l[1] + l[2]) / 3.0;
    if !(s > 0.0 && s.is_finite()) || a.determinant() <= 0.0 {
        return None;
    }
    let (x, y, z) = (a.x / l[0], a.y / l[1], a.z / l[2]);
    let uneven = [
        (l[0] - s).abs() / s,
        (l[1] - s).abs() / s,
        (l[2] - s).abs() / s,
        x.dot(y).abs(),
        y.dot(z).abs(),
        x.dot(z).abs(),
    ]
    .into_iter()
    .fold(0.0, f64::max);
    if uneven > UNEVEN {
        return None;
    }
    let y = (y - x * x.dot(y)).normalize();
    let z = x.cross(y);
    Some((s, Affine { x, y, z, t: a.t }, uneven))
}

/// The inverse of an affine map, if it is not singular.
pub(crate) fn affine_inverse(a: &Affine) -> Option<Affine> {
    let det = a.determinant();
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    // Rows of the inverse: the cross products of the columns over det.
    let (r0, r1, r2) = (
        a.y.cross(a.z) / det,
        a.z.cross(a.x) / det,
        a.x.cross(a.y) / det,
    );
    let l = Affine {
        x: DVec3::new(r0.x, r1.x, r2.x),
        y: DVec3::new(r0.y, r1.y, r2.y),
        z: DVec3::new(r0.z, r1.z, r2.z),
        t: DVec3::ZERO,
    };
    Some(Affine {
        t: -l.rotate(a.t),
        ..l
    })
}

/// Each vertex placed by the weighted blend of its bones' bind matrices
/// (`bind(joint)`, kernel space), and the largest distance between one of
/// those bones' placement and the blend. A weight naming no bone, a
/// mirroring bind matrix, or a vertex with no weight is an error.
pub(crate) fn place(
    positions: &[DVec3],
    raw: &[Vec<(u32, f64)>],
    bind: &dyn Fn(u32) -> Option<Affine>,
) -> Result<(Vec<DVec3>, f64), String> {
    let mut out = Vec::with_capacity(positions.len());
    let mut error: f64 = 0.0;
    for (v, (p, pairs)) in positions.iter().zip(raw).enumerate() {
        let mut each = Vec::with_capacity(4);
        let mut total = 0.0;
        for &(j, w) in pairs {
            if w > 0.0 {
                let m = bind(j)
                    .ok_or_else(|| format!("vertex {v} names joint {j}, which the skin lacks"))?;
                if m.determinant() <= 0.0 {
                    return Err(format!(
                        "joint {j}'s bind pose mirrors or flattens the mesh"
                    ));
                }
                each.push((m.apply(*p), w));
                total += w;
            }
        }
        if total <= 0.0 {
            return Err(format!("vertex {v} has no bone weight"));
        }
        let q = each
            .iter()
            .fold(DVec3::ZERO, |acc, (x, w)| acc + *x * (*w / total));
        for (x, _) in &each {
            error = error.max((*x - q).length());
        }
        out.push(q);
    }
    Ok((out, error))
}

/// The inverse of a rigid motion: its rotation transposed.
pub(crate) fn rigid_inverse(a: &Affine) -> Affine {
    let r = Affine {
        x: DVec3::new(a.x.x, a.y.x, a.z.x),
        y: DVec3::new(a.x.y, a.y.y, a.z.y),
        z: DVec3::new(a.x.z, a.y.z, a.z.z),
        t: DVec3::ZERO,
    };
    Affine {
        t: -r.rotate(a.t),
        ..r
    }
}

/// A rigid motion as a frame, and as its inverse matrix (column-major).
pub(crate) fn rest_and_inverse(r: &Affine) -> (Frame, [f64; 16]) {
    let i = rigid_inverse(r);
    (
        Frame {
            origin: r.t,
            x: r.x,
            y: r.y,
            z: r.z,
        },
        [
            i.x.x, i.x.y, i.x.z, 0.0, i.y.x, i.y.y, i.y.z, 0.0, i.z.x, i.z.y, i.z.z, 0.0, i.t.x,
            i.t.y, i.t.z, 1.0,
        ],
    )
}

/// Why a file could not be imported.
#[derive(Clone, Debug, PartialEq)]
pub enum ImportError {
    /// Not a file of the format, or broken (what and where).
    Format(String),
    /// A feature the importer does not read, such as a required glTF
    /// extension.
    Unsupported(String),
    /// An external buffer could not be had.
    External {
        /// The URI the file names.
        uri: String,
        /// What the resolver said.
        why: String,
    },
    /// A position is NaN or infinite (which part).
    NotFinite(String),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ImportError::Format(w) => write!(f, "not a readable file: {w}"),
            ImportError::Unsupported(w) => write!(f, "not supported: {w}"),
            ImportError::External { uri, why } => write!(f, "cannot read {uri:?}: {why}"),
            ImportError::NotFinite(p) => write!(f, "part {p:?} has a position that is not finite"),
        }
    }
}

impl std::error::Error for ImportError {}

/// Fetches a file a model names by relative URI (percent-decoded), for
/// external glTF buffers and images and OBJ material libraries.
pub type Resolve<'a> = &'a dyn Fn(&str) -> Result<Vec<u8>, String>;

/// A resolver that has no files: external references fail.
pub fn no_files(uri: &str) -> Result<Vec<u8>, String> {
    Err(format!("no file system to read {uri:?} from"))
}

pub(crate) fn fmt_err(what: impl Into<String>) -> ImportError {
    ImportError::Format(what.into())
}

/// Imports a file by its content: GLB by its magic, glTF JSON by a
/// leading `{`, binary FBX by its magic and ASCII FBX by its header,
/// otherwise OBJ text.
pub fn import(bytes: &[u8], d: &Import, resolve: Resolve) -> Result<Imported, ImportError> {
    let first = bytes
        .iter()
        .find(|b| !b.is_ascii_whitespace() && **b != 0xEF && **b != 0xBB && **b != 0xBF);
    let head = &bytes[..bytes.len().min(4096)];
    let ascii_fbx = head.windows(18).any(|w| w == b"FBXHeaderExtension");
    if bytes.starts_with(b"glTF") || first == Some(&b'{') {
        let mut m = import_gltf(bytes, resolve)?;
        if d.turns() {
            // From glTF's Y up to the file's own axes, as the kernel
            // reads them.
            let y_up = AxisTurn::new(Some(Axis::PosY), None).map_err(ImportError::Unsupported)?;
            let c = d.axes()?.then(&y_up.inverse());
            for p in &mut m.parts {
                turn_part(p, &c);
            }
        }
        Ok(m)
    } else if bytes.starts_with(b"Kaydara FBX Binary") || ascii_fbx {
        if d.turns() {
            return Err(ImportError::Unsupported(
                "up, front or z_up for FBX: its GlobalSettings say its axes; turn it with `place`"
                    .into(),
            ));
        }
        crate::fbx::import_fbx(bytes, resolve)
    } else {
        import_obj(bytes, d, resolve)
    }
}

/// A part turned by an axis turn of kernel space: an unskinned part's
/// transform is turned after; a skinned part's mesh and bones are turned
/// (`apply_transform` and `Mesh::placed` take it as it is), and its
/// transform conjugated, so either way it lands turned.
fn turn_part(p: &mut ImportedPart, c: &AxisTurn) {
    if p.solid.skin.is_some() {
        p.solid = turned(&p.solid, c);
        let back = c.inverse().after(&Affine::IDENTITY);
        p.transform = c.after(&p.transform.then(&back));
    } else {
        p.transform = c.after(&p.transform);
    }
}

/// glTF space (metres, Y up) to kernel space (Z up), unit-free:
/// `(x, y, z) ↦ (x, −z, y)`.
pub(crate) fn from_y_up(v: DVec3) -> DVec3 {
    DVec3::new(v.x, -v.z, v.y)
}

/// A glTF-space affine map, conjugated into kernel space with the unit
/// change applied to its translation.
fn conjugate(m: &Affine, unit: f64) -> Affine {
    // P(x, y, z) = (x, −z, y); P⁻¹ e_x = e_x, P⁻¹ e_y = −e_z, P⁻¹ e_z = e_y.
    Affine {
        x: from_y_up(m.x),
        y: -from_y_up(m.z),
        z: from_y_up(m.y),
        t: from_y_up(m.t) * unit,
    }
}

/// A translation, a rotation (a quaternion x, y, z, w, made unit) and a
/// scale as one map: scale, then turn, then move (glTF's order).
fn trs(t: DVec3, q: [f64; 4], s: DVec3) -> Affine {
    let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    let (x, y, z, w) = if len > 0.0 {
        (q[0] / len, q[1] / len, q[2] / len, q[3] / len)
    } else {
        (0.0, 0.0, 0.0, 1.0)
    };
    let cx = DVec3::new(
        1.0 - 2.0 * (y * y + z * z),
        2.0 * (x * y + z * w),
        2.0 * (x * z - y * w),
    );
    let cy = DVec3::new(
        2.0 * (x * y - z * w),
        1.0 - 2.0 * (x * x + z * z),
        2.0 * (y * z + x * w),
    );
    let cz = DVec3::new(
        2.0 * (x * z + y * w),
        2.0 * (y * z - x * w),
        1.0 - 2.0 * (x * x + y * y),
    );
    Affine {
        x: cx * s.x,
        y: cy * s.y,
        z: cz * s.z,
        t,
    }
}

/// A curve of one node's translation, rotation or scale: key times and
/// 3 or 4 numbers a key.
struct Track {
    times: Vec<f64>,
    values: Vec<f64>,
    width: usize,
    step: bool,
}

impl Track {
    /// The value at `time`: held before the first key and after the last;
    /// between two, straight (a rotation along the shorter way, made unit
    /// by [`trs`]), or the earlier key's for a stepped curve.
    fn at(&self, time: f64) -> [f64; 4] {
        let w = self.width;
        let key = |k: usize| {
            let mut v = [0.0; 4];
            v[..w].copy_from_slice(&self.values[k * w..k * w + w]);
            v
        };
        let n = self.times.len();
        let k = self.times.partition_point(|&t| t <= time);
        if k == 0 {
            return key(0);
        }
        if k >= n || self.step {
            return key(k - 1);
        }
        let (t0, t1) = (self.times[k - 1], self.times[k]);
        let f = if t1 > t0 {
            (time - t0) / (t1 - t0)
        } else {
            0.0
        };
        let (a, mut b) = (key(k - 1), key(k));
        if w == 4 && a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3] < 0.0 {
            b = b.map(|x| -x);
        }
        let mut v = [0.0; 4];
        for i in 0..w {
            v[i] = a[i] + (b[i] - a[i]) * f;
        }
        v
    }
}

/// The space a skin was read into: what a clip needs to follow it.
pub(crate) struct SkinSpace {
    /// Each bone's node in the file, in the skin's bone order.
    nodes: Vec<usize>,
    /// Each bone's inverse bind matrix as the file has it (glTF space).
    ibms: Vec<Affine>,
    /// Kernel space: from where the solid's vertices stand to where the
    /// file's own mesh coordinates would put them (the identity unless a
    /// scale was taken out of the bones, which moved the vertices to
    /// their bind pose).
    unbind: Affine,
}

/// More keys than this in one animation and it is sampled evenly instead
/// of at every key time.
pub(crate) const MAX_KEYS: usize = 4096;

/// How densely an animation is sampled between its own keys.
pub(crate) const SAMPLES_A_SECOND: f64 = 60.0;

pub(crate) fn unique(name: &str, used: &mut BTreeMap<String, ()>) -> String {
    let base = if name.is_empty() { "part" } else { name };
    let mut n = base.to_string();
    let mut k = 1;
    while used.contains_key(&n) {
        n = format!("{base}.{k}");
        k += 1;
    }
    used.insert(n.clone(), ());
    n
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn base64(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// The bytes a URI names: a `data:` URI decoded, or the resolver's file.
pub(crate) fn fetch(uri: &str, resolve: Resolve) -> Result<Vec<u8>, ImportError> {
    if let Some(rest) = uri.strip_prefix("data:") {
        let (head, data) = rest
            .split_once(',')
            .ok_or_else(|| fmt_err("a data URI without ','"))?;
        if head.ends_with(";base64") {
            return base64(data).ok_or_else(|| fmt_err("a data URI that is not base64"));
        }
        return Ok(percent_decode(data).into_bytes());
    }
    let path = percent_decode(uri);
    resolve(&path).map_err(|why| ImportError::External { uri: path, why })
}

// ── glTF ────────────────────────────────────────────────────────────────────

const SUPPORTED_EXTENSIONS: [&str; 8] = [
    "EXT_meshopt_compression",
    "KHR_meshopt_compression",
    "KHR_mesh_quantization",
    "KHR_texture_transform",
    "KHR_materials_emissive_strength",
    "KHR_texture_basisu",
    "EXT_texture_webp",
    "KHR_materials_unlit",
];

/// A skin read, and the placed vertices and the scale taken out of its
/// bones, if one was.
pub(crate) type ReadSkin = (Skin, Option<(Vec<DVec3>, SkinScale)>);

/// Per vertex, its `(joint, weight)` pairs as the file has them.
type RawWeights = Vec<Vec<(u32, f64)>>;

struct Doc<'a> {
    json: Value,
    buffers: Vec<Vec<u8>>,
    /// The meshopt-compressed buffer views, decoded.
    decoded: BTreeMap<usize, Vec<u8>>,
    resolve: Resolve<'a>,
}

/// Reads a glTF 2.0 file: binary (GLB) or JSON text. External buffers and
/// images are read through `resolve`; `data:` URIs are decoded.
///
/// Read: triangle primitives (also strips and fans), `POSITION`,
/// `TEXCOORD_0`, indices of every integer width, normalized and quantized
/// attributes (`KHR_mesh_quantization`), sparse accessors, the default
/// scene's node tree (matrices and TRS), and images' sizes. A node's
/// skin becomes the part's `Solid::skin`: its joints as bones, parents
/// first, with world rest frames and the file's inverse bind matrices,
/// and `JOINTS_0`/`WEIGHTS_0` as weights (`docs/research.md` §11.4). The
/// mesh is in its bind pose, at its node's origin (glTF does not use a
/// skinned mesh's node transform). Bones under one uniform scale (an
/// armature scaled 0.01) have it taken out and the mesh is placed where
/// the file's bind pose puts it; [`Imported::skin_scales`] records it. A
/// bone scaled unevenly or sheared, bones scaled differently, or a skin
/// that does not check (an unweighted vertex), is
/// [`ImportError::Unsupported`]. Skipped and listed in
/// [`Imported::skipped`]: points and lines, morph targets, and bone
/// weights with no skin. `COLOR_0` (float or normalised integer, RGB or
/// RGBA, linear as glTF has it) becomes [`Solid::colors`]; a primitive
/// without it is white, as glTF draws it. The material is read as
/// [`ImportedPart::material`] says. Buffer views compressed with
/// `EXT_meshopt_compression` or `KHR_meshopt_compression` are decoded (`docs/research.md` §42). A
/// required extension the importer does not read (Draco) is
/// [`ImportError::Unsupported`].
pub fn import_gltf(bytes: &[u8], resolve: Resolve) -> Result<Imported, ImportError> {
    let (json_bytes, bin) = if bytes.starts_with(b"glTF") {
        let u32_at = |i: usize| -> Result<u32, ImportError> {
            bytes
                .get(i..i + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .ok_or_else(|| fmt_err("GLB is cut short"))
        };
        if u32_at(4)? != 2 {
            return Err(ImportError::Unsupported(
                "a GLB container other than version 2".into(),
            ));
        }
        let total = (u32_at(8)? as usize).min(bytes.len());
        let mut at = 12;
        let (mut json, mut bin) = (None, None);
        while at + 8 <= total {
            let len = u32_at(at)? as usize;
            let kind = u32_at(at + 4)?;
            let body = bytes
                .get(at + 8..at + 8 + len)
                .ok_or_else(|| fmt_err("a GLB chunk runs past the end"))?;
            match kind {
                0x4E4F_534A if json.is_none() => json = Some(body),
                0x004E_4942 if bin.is_none() => bin = Some(body.to_vec()),
                _ => {}
            }
            at += 8 + len.div_ceil(4) * 4;
        }
        (json.ok_or_else(|| fmt_err("GLB has no JSON chunk"))?, bin)
    } else {
        (bytes, None)
    };
    let json = jsonread::parse(json_bytes).map_err(|e| fmt_err(format!("glTF JSON: {e}")))?;
    let version = json.get("asset").get("version").str().unwrap_or("");
    if !version.starts_with("2.") {
        return Err(ImportError::Unsupported(format!(
            "glTF version {version:?} (only 2.x)"
        )));
    }
    for e in json.get("extensionsRequired").items() {
        let name = e.str().unwrap_or("");
        if !SUPPORTED_EXTENSIONS.contains(&name) {
            return Err(ImportError::Unsupported(format!(
                "the required glTF extension {name}"
            )));
        }
    }
    let compressed = json
        .get("bufferViews")
        .items()
        .iter()
        .any(|v| !meshopt_of(v).is_null());
    let mut buffers = Vec::new();
    let mut bin = bin;
    for (i, b) in json.get("buffers").items().iter().enumerate() {
        let data = match b.get("uri").str() {
            _ if meshopt_of(b).get("fallback").bool() == Some(true) => {
                buffers.push(Vec::new());
                continue;
            }
            Some(uri) => fetch(uri, resolve)?,
            None => match bin.take().filter(|_| i == 0) {
                Some(bin) => bin,
                // The buffer a file names for its compressed views'
                // decoded bytes, and does not carry.
                None if compressed => {
                    buffers.push(Vec::new());
                    continue;
                }
                None if i == 0 => {
                    return Err(fmt_err("buffer 0 has no URI and there is no BIN chunk"));
                }
                None => return Err(fmt_err(format!("buffer {i} has no URI"))),
            },
        };
        let len = b.get("byteLength").index().unwrap_or(0);
        if data.len() < len {
            return Err(fmt_err(format!(
                "buffer {i} holds {} bytes, fewer than its byteLength {len}",
                data.len()
            )));
        }
        buffers.push(data);
    }
    let mut decoded = BTreeMap::new();
    for (i, v) in json.get("bufferViews").items().iter().enumerate() {
        let c = meshopt_of(v);
        if c.is_null() {
            continue;
        }
        let bad = |why: String| fmt_err(format!("bufferView {i}'s meshopt data: {why}"));
        let off = c.get("byteOffset").index().unwrap_or(0);
        let (Some(len), Some(stride), Some(count)) = (
            c.get("byteLength").index(),
            c.get("byteStride").index(),
            c.get("count").index(),
        ) else {
            return Err(bad("no byteLength, byteStride or count".into()));
        };
        let data = c
            .get("buffer")
            .index()
            .and_then(|b| buffers.get(b))
            .and_then(|b| b.get(off..off.checked_add(len)?))
            .ok_or_else(|| bad("it runs past its buffer".into()))?;
        let mode = match c.get("mode").str() {
            Some("ATTRIBUTES") => meshopt::Mode::Attributes,
            Some("TRIANGLES") => meshopt::Mode::Triangles,
            Some("INDICES") => meshopt::Mode::Indices,
            m => return Err(bad(format!("mode {m:?}"))),
        };
        let filter = match c.get("filter").str() {
            None | Some("NONE") => meshopt::Filter::None,
            Some("OCTAHEDRAL") => meshopt::Filter::Octahedral,
            Some("QUATERNION") => meshopt::Filter::Quaternion,
            Some("EXPONENTIAL") => meshopt::Filter::Exponential,
            Some("COLOR") => meshopt::Filter::Color,
            Some(f) => return Err(bad(format!("filter {f:?}"))),
        };
        if v.get("byteLength").index() != count.checked_mul(stride) {
            return Err(bad("its count and stride are not the view's length".into()));
        }
        decoded.insert(
            i,
            meshopt::decode(data, count, stride, mode, filter).map_err(bad)?,
        );
    }
    let doc = Doc {
        json,
        buffers,
        decoded,
        resolve,
    };
    doc.read()
}

/// A buffer view's or buffer's meshopt extension, under either name.
fn meshopt_of(v: &Value) -> &Value {
    let e = v.get("extensions");
    let c = e.get("EXT_meshopt_compression");
    if c.is_null() {
        e.get("KHR_meshopt_compression")
    } else {
        c
    }
}

/// Component bytes of a glTF component type.
fn component_size(ct: usize) -> Option<usize> {
    match ct {
        5120 | 5121 => Some(1),
        5122 | 5123 => Some(2),
        5125 | 5126 => Some(4),
        _ => None,
    }
}

fn read_component(b: &[u8], ct: usize, normalized: bool) -> f64 {
    match ct {
        5120 => {
            let v = f64::from(b[0] as i8);
            if normalized { (v / 127.0).max(-1.0) } else { v }
        }
        5121 => {
            let v = f64::from(b[0]);
            if normalized { v / 255.0 } else { v }
        }
        5122 => {
            let v = f64::from(i16::from_le_bytes([b[0], b[1]]));
            if normalized {
                (v / 32767.0).max(-1.0)
            } else {
                v
            }
        }
        5123 => {
            let v = f64::from(u16::from_le_bytes([b[0], b[1]]));
            if normalized { v / 65535.0 } else { v }
        }
        5125 => f64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        _ => f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
    }
}

impl Doc<'_> {
    /// The bytes of a buffer view, and its stride.
    fn view(&self, i: usize) -> Result<(&[u8], Option<usize>), ImportError> {
        let v = self.json.get("bufferViews").at(i);
        if let Some(bytes) = self.decoded.get(&i) {
            return Ok((bytes, v.get("byteStride").index()));
        }
        let buffer = v
            .get("buffer")
            .index()
            .and_then(|b| self.buffers.get(b))
            .ok_or_else(|| fmt_err(format!("bufferView {i} names no buffer")))?;
        let off = v.get("byteOffset").index().unwrap_or(0);
        let len = v
            .get("byteLength")
            .index()
            .ok_or_else(|| fmt_err(format!("bufferView {i} has no byteLength")))?;
        let bytes = buffer
            .get(off..off + len)
            .ok_or_else(|| fmt_err(format!("bufferView {i} runs past its buffer")))?;
        Ok((bytes, v.get("byteStride").index()))
    }

    /// An accessor's values, `width` per element, as f64.
    fn accessor(&self, i: usize) -> Result<(Vec<f64>, usize), ImportError> {
        let a = self.json.get("accessors").at(i);
        if a.is_null() {
            return Err(fmt_err(format!("no accessor {i}")));
        }
        let width = match a.get("type").str().unwrap_or("") {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            t => return Err(fmt_err(format!("accessor {i} has type {t:?}"))),
        };
        let ct = a.get("componentType").index().unwrap_or(0);
        let size = component_size(ct)
            .ok_or_else(|| fmt_err(format!("accessor {i} has component type {ct}")))?;
        let count = a
            .get("count")
            .index()
            .ok_or_else(|| fmt_err(format!("accessor {i} has no count")))?;
        let normalized = a.get("normalized").bool().unwrap_or(false);
        let elem = width * size;
        let mut out = vec![0.0; count * width];
        if let Some(bv) = a.get("bufferView").index() {
            let (bytes, stride) = self.view(bv)?;
            let stride = stride.unwrap_or(elem);
            let off = a.get("byteOffset").index().unwrap_or(0);
            if count > 0 && off + (count - 1) * stride + elem > bytes.len() {
                return Err(fmt_err(format!("accessor {i} runs past its bufferView")));
            }
            for e in 0..count {
                for c in 0..width {
                    let at = off + e * stride + c * size;
                    out[e * width + c] = read_component(&bytes[at..at + size], ct, normalized);
                }
            }
        }
        let sparse = a.get("sparse");
        if !sparse.is_null() {
            let n = sparse.get("count").index().unwrap_or(0);
            let ind = sparse.get("indices");
            let ict = ind.get("componentType").index().unwrap_or(0);
            let isize = component_size(ict).ok_or_else(|| fmt_err("sparse index type"))?;
            let (ib, _) = self.view(ind.get("bufferView").index().unwrap_or(usize::MAX))?;
            let ioff = ind.get("byteOffset").index().unwrap_or(0);
            let val = sparse.get("values");
            let (vb, _) = self.view(val.get("bufferView").index().unwrap_or(usize::MAX))?;
            let voff = val.get("byteOffset").index().unwrap_or(0);
            if ioff + n * isize > ib.len() || voff + n * elem > vb.len() {
                return Err(fmt_err(format!(
                    "accessor {i}'s sparse data runs past its views"
                )));
            }
            for k in 0..n {
                let at = ioff + k * isize;
                let target = read_component(&ib[at..at + isize], ict, false) as usize;
                if target >= count {
                    return Err(fmt_err(format!(
                        "accessor {i}'s sparse index is out of range"
                    )));
                }
                for c in 0..width {
                    let at = voff + k * elem + c * size;
                    out[target * width + c] = read_component(&vb[at..at + size], ct, normalized);
                }
            }
        }
        Ok((out, width))
    }

    /// The local transform of node `i`, in glTF space.
    fn local(&self, n: &Value) -> Affine {
        if let Some(m) = n.get("matrix").nums().filter(|m| m.len() == 16) {
            return Affine {
                x: DVec3::new(m[0], m[1], m[2]),
                y: DVec3::new(m[4], m[5], m[6]),
                z: DVec3::new(m[8], m[9], m[10]),
                t: DVec3::new(m[12], m[13], m[14]),
            };
        }
        let t = n
            .get("translation")
            .nums()
            .filter(|v| v.len() == 3)
            .map_or(DVec3::ZERO, |v| DVec3::new(v[0], v[1], v[2]));
        let s = n
            .get("scale")
            .nums()
            .filter(|v| v.len() == 3)
            .map_or(DVec3::ONE, |v| DVec3::new(v[0], v[1], v[2]));
        let q = n
            .get("rotation")
            .nums()
            .filter(|v| v.len() == 4)
            .map_or([0.0, 0.0, 0.0, 1.0], |v| [v[0], v[1], v[2], v[3]]);
        trs(t, q, s)
    }

    fn read(&self) -> Result<Imported, ImportError> {
        let mut out = Imported::default();
        // Images: sizes, and the bytes for the parts' base-colour maps.
        let mut image_bytes: Vec<Option<Vec<u8>>> = Vec::new();
        for (i, img) in self.json.get("images").items().iter().enumerate() {
            let name = img
                .get("name")
                .str()
                .or(img.get("uri").str().filter(|u| !u.starts_with("data:")))
                .map_or_else(|| format!("image{i}"), str::to_string);
            let bytes = if let Some(uri) = img.get("uri").str() {
                fetch(uri, self.resolve).map_err(|e| e.to_string())
            } else if let Some(bv) = img.get("bufferView").index() {
                self.view(bv)
                    .map(|(b, _)| b.to_vec())
                    .map_err(|e| e.to_string())
            } else {
                Err("no URI or bufferView".to_string())
            };
            out.images.push(match &bytes {
                Ok(b) => ImageInfo::from_bytes(&name, b),
                Err(why) => ImageInfo::missing(&name, why),
            });
            image_bytes.push(bytes.ok());
        }
        // The scene's roots: the default scene, else the first, else every
        // node that is nobody's child.
        let nodes = self.json.get("nodes").items();
        let scene = self.json.get("scene").index().unwrap_or(0);
        let roots: Vec<usize> = match self.json.get("scenes").at(scene).get("nodes").nums() {
            Some(r) => r.iter().map(|&v| v as usize).collect(),
            None => {
                let mut child = vec![false; nodes.len()];
                for n in nodes {
                    for c in n.get("children").items() {
                        if let Some(c) = c.index().filter(|&c| c < nodes.len()) {
                            child[c] = true;
                        }
                    }
                }
                (0..nodes.len()).filter(|&i| !child[i]).collect()
            }
        };
        let mut used = BTreeMap::new();
        let mut stack: Vec<(usize, Affine, usize)> = roots
            .iter()
            .rev()
            .map(|&r| (r, Affine::IDENTITY, 0))
            .collect();
        while let Some((i, above, depth)) = stack.pop() {
            let n = self.json.get("nodes").at(i);
            if n.is_null() || depth > 256 {
                return Err(fmt_err(format!(
                    "node {i} does not exist or the tree loops"
                )));
            }
            let world = above.then(&self.local(n));
            for c in n.get("children").items().iter().rev() {
                let c = c
                    .index()
                    .ok_or_else(|| fmt_err("a child that is not an index"))?;
                stack.push((c, world, depth + 1));
            }
            let Some(m) = n.get("mesh").index() else {
                continue;
            };
            let mesh = self.json.get("meshes").at(m);
            let name = n
                .get("name")
                .str()
                .or(mesh.get("name").str())
                .map_or_else(|| format!("node{i}"), str::to_string);
            let name = unique(&name, &mut used);
            let images = out.images.clone();
            let (mut part, raw) =
                self.part(&name, mesh, &world, &mut out, (&images, &image_bytes))?;
            let weighted = raw.iter().any(|r| r.iter().any(|p| p.1 > 0.0));
            match n.get("skin").index() {
                Some(k) => {
                    let ((skin, scaled), space) =
                        self.skin(k, &name, &raw, &part.solid.positions)?;
                    part.clips = self.clips(&space)?;
                    if let Some((placed, info)) = scaled {
                        // Where the file's bind pose puts the vertices.
                        part.solid.positions = placed;
                        out.skin_scales.push(info);
                    }
                    // glTF: a skinned mesh's own node transform is not used;
                    // its bones place it.
                    if part.transform != Affine::IDENTITY {
                        out.skipped.push(format!(
                            "{name}: skinned, so its node transform is not used (glTF)"
                        ));
                    }
                    part.transform = Affine::IDENTITY;
                    part.solid.skin = Some(skin);
                }
                None if weighted => out
                    .skipped
                    .push(format!("{name}: bone weights with no skin on its node")),
                None => {}
            }
            out.parts.push(part);
        }
        Ok(out)
    }

    /// Each node's parent, by index.
    fn parents(&self) -> Vec<Option<usize>> {
        let nodes = self.json.get("nodes").items();
        let mut parent = vec![None; nodes.len()];
        for (i, n) in nodes.iter().enumerate() {
            for c in n.get("children").items() {
                if let Some(c) = c.index().filter(|&c| c < nodes.len()) {
                    parent[c] = Some(i);
                }
            }
        }
        parent
    }

    /// A node's world transform in glTF space.
    fn world(&self, mut i: usize, parents: &[Option<usize>]) -> Affine {
        let mut m = Affine::IDENTITY;
        for _ in 0..=parents.len() {
            m = self.local(self.json.get("nodes").at(i)).then(&m);
            match parents.get(i).copied().flatten() {
                Some(p) => i = p,
                None => break,
            }
        }
        m
    }

    /// Skin `k` for a mesh whose vertices (`positions`, kernel space) carry
    /// `raw` (joint, weight) pairs: its joints as bones, parents first, with
    /// their world rest frames and the file's inverse bind matrices, in
    /// kernel space; the weights through `Weights::from_raw`.
    ///
    /// Bones rigid up to one uniform scale (a scaled armature) have it
    /// taken out: the rest frames are the world transforms divided by it,
    /// the inverse bind matrices their inverses, and the vertices are
    /// returned where the file's bind pose puts them, with the scale and
    /// how well the file's matrices agree. A scale that
    /// is not uniform, differs between bones, or a skin that does not
    /// check, is an error.
    fn skin(
        &self,
        k: usize,
        name: &str,
        raw: &[Vec<(u32, f64)>],
        positions: &[DVec3],
    ) -> Result<(ReadSkin, SkinSpace), ImportError> {
        let skin = self.json.get("skins").at(k);
        let joints: Vec<usize> = skin
            .get("joints")
            .items()
            .iter()
            .map(|j| {
                j.index()
                    .ok_or_else(|| fmt_err(format!("{name}: a skin joint is not a node")))
            })
            .collect::<Result<_, _>>()?;
        if joints.is_empty()
            || joints
                .iter()
                .any(|&j| self.json.get("nodes").at(j).is_null())
        {
            return Err(fmt_err(format!(
                "{name}: its skin names no or missing joints"
            )));
        }
        let ibm: Option<Vec<f64>> = match skin.get("inverseBindMatrices").index() {
            Some(a) => {
                let (m, w) = self.accessor(a)?;
                if w != 16 || m.len() / 16 != joints.len() {
                    return Err(fmt_err(format!(
                        "{name}: its inverse bind matrices do not match its joints"
                    )));
                }
                Some(m)
            }
            None => None,
        };
        let parents = self.parents();
        // The bone each joint's nearest jointed ancestor is, and depth.
        let joint_of = |node: usize| joints.iter().position(|&j| j == node);
        let mut info: Vec<(usize, Option<usize>, usize)> = Vec::new();
        for (ji, &node) in joints.iter().enumerate() {
            let (mut at, mut depth, mut parent) = (node, 0, None);
            while let Some(p) = parents[at] {
                depth += 1;
                if parent.is_none() {
                    parent = joint_of(p);
                }
                at = p;
                if depth > parents.len() {
                    return Err(fmt_err(format!("{name}: the node tree loops")));
                }
            }
            info.push((depth, parent, ji));
        }
        // Parents first: by depth, then file order.
        let mut order: Vec<usize> = (0..joints.len()).collect();
        order.sort_by_key(|&ji| (info[ji].0, ji));
        let mut bone_of = vec![0u32; joints.len()];
        for (b, &ji) in order.iter().enumerate() {
            bone_of[ji] = b as u32;
        }
        let names: Vec<String> = {
            let mut used = BTreeMap::new();
            order
                .iter()
                .map(|&ji| {
                    let node = joints[ji];
                    unique(
                        &self
                            .json
                            .get("nodes")
                            .at(node)
                            .get("name")
                            .str()
                            .map_or_else(|| format!("node{node}"), str::to_string),
                        &mut used,
                    )
                })
                .collect()
        };
        // Each joint's world transform (glTF space) as a uniform scale
        // times a rigid motion; one scale for all, or none of this.
        let mut worlds: Vec<(f64, Affine, f64)> = Vec::with_capacity(order.len());
        for (b, &ji) in order.iter().enumerate() {
            let w = self.world(joints[ji], &parents);
            let sim = similarity(&w).ok_or_else(|| {
                ImportError::Unsupported(format!(
                    "{name}: bone {:?} is scaled unevenly or sheared (beyond {UNEVEN}); a bone must be rigid up to one uniform scale",
                    names[b]
                ))
            })?;
            worlds.push(sim);
        }
        let scale = worlds[0].0;
        if let Some(b) = worlds
            .iter()
            .position(|(s, _, _)| (s - scale).abs() > UNEVEN * scale)
        {
            return Err(ImportError::Unsupported(format!(
                "{name}: bones {:?} and {:?} are scaled differently ({} and {})",
                names[0], names[b], scale, worlds[b].0
            )));
        }
        let ibm_of = |ji: usize, bone: &str| -> Result<Affine, ImportError> {
            Ok(match &ibm {
                Some(m) => {
                    let m = &m[ji * 16..ji * 16 + 16];
                    if m[3] != 0.0 || m[7] != 0.0 || m[11] != 0.0 || m[15] != 1.0 {
                        return Err(ImportError::Unsupported(format!(
                            "{name}: bone {bone:?}'s inverse bind matrix is projective"
                        )));
                    }
                    Affine {
                        x: DVec3::new(m[0], m[1], m[2]),
                        y: DVec3::new(m[4], m[5], m[6]),
                        z: DVec3::new(m[8], m[9], m[10]),
                        t: DVec3::new(m[12], m[13], m[14]),
                    }
                }
                // No matrices: each is the identity (glTF).
                None => Affine::IDENTITY,
            })
        };
        // Rigid already (every axis of unit length, to 1e-9): kept as it is.
        let rigid = order.iter().all(|&ji| {
            let w = self.world(joints[ji], &parents);
            [w.x, w.y, w.z]
                .iter()
                .all(|a| (a.length() - 1.0).abs() < 1e-9)
        });
        let mut bones = Vec::with_capacity(joints.len());
        let mut scaled = None;
        let mut unbind = Affine::IDENTITY;
        if rigid {
            // Rest frames are the world transforms; the file's inverse bind
            // matrices are kept as given.
            for (b, &ji) in order.iter().enumerate() {
                let w = conjugate(&self.world(joints[ji], &parents), 1000.0);
                let a = conjugate(&ibm_of(ji, &names[b])?, 1000.0);
                bones.push(Bone {
                    name: names[b].clone(),
                    parent: info[ji].1.map(|p| bone_of[p]),
                    rest: Frame {
                        origin: w.t,
                        x: w.x,
                        y: w.y,
                        z: w.z,
                    },
                    inverse_bind: [
                        a.x.x, a.x.y, a.x.z, 0.0, a.y.x, a.y.y, a.y.z, 0.0, a.z.x, a.z.y, a.z.z,
                        0.0, a.t.x, a.t.y, a.t.z, 1.0,
                    ],
                });
            }
        } else {
            // The scale comes out of the bones. The bind pose is the file's
            // inverse bind matrices: with `bind` = `world · inverse bind` of
            // the bone that weights the most vertices (where the file puts
            // the mesh), each bone's rest is `bind · inverse bind⁻¹`, the
            // uniform scale divided out, and its new inverse bind matrix is
            // that rest frame's inverse; the vertices go to `bind · v`.
            let ibms: Vec<Affine> = order
                .iter()
                .enumerate()
                .map(|(b, &ji)| ibm_of(ji, &names[b]))
                .collect::<Result<_, _>>()?;
            let nodes: Vec<Affine> = order
                .iter()
                .map(|&ji| self.world(joints[ji], &parents))
                .collect();
            let mut uses = vec![0usize; order.len()];
            for pairs in raw {
                for &(j, w) in pairs {
                    if let Some(&b) = bone_of.get(j as usize).filter(|_| w > 0.0) {
                        uses[b as usize] += 1;
                    }
                }
            }
            let main = (0..order.len())
                .max_by(|&a, &b| uses[a].cmp(&uses[b]).then(b.cmp(&a)))
                .unwrap_or(0);
            let bind = nodes[main].then(&ibms[main]);
            if bind.determinant() <= 0.0 {
                return Err(ImportError::Unsupported(format!(
                    "{name}: its bind pose mirrors or flattens the mesh"
                )));
            }
            let mut uneven: f64 = 0.0;
            let mut rests = Vec::with_capacity(order.len());
            for (b, ibm) in ibms.iter().enumerate() {
                let inv = affine_inverse(ibm).ok_or_else(|| {
                    ImportError::Unsupported(format!(
                        "{name}: bone {:?}'s inverse bind matrix is singular",
                        names[b]
                    ))
                })?;
                let (sb, rigid, u) = similarity(&bind.then(&inv)).ok_or_else(|| {
                    ImportError::Unsupported(format!(
                        "{name}: bone {:?} is scaled unevenly or sheared at bind time (beyond {UNEVEN}); a bone must be rigid up to one uniform scale",
                        names[b]
                    ))
                })?;
                if (sb - scale).abs() > UNEVEN * scale {
                    return Err(ImportError::Unsupported(format!(
                        "{name}: bone {:?} is scaled {sb} at bind time, the others {scale}",
                        names[b]
                    )));
                }
                uneven = uneven.max(u);
                rests.push(conjugate(&rigid, 1000.0));
            }
            let kbind = conjugate(&bind, 1000.0);
            unbind = affine_inverse(&kbind).ok_or_else(|| {
                ImportError::Unsupported(format!("{name}: its bind pose flattens the mesh"))
            })?;
            let placed: Vec<DVec3> = positions.iter().map(|p| kbind.apply(*p)).collect();
            // How far the file's own nodes stand from that bind pose, at the
            // vertices each bone weights: 0 to rounding when the file is
            // saved at its bind pose; more where it is saved posed.
            let mut offset: f64 = 0.0;
            let at_nodes: Vec<Affine> = (0..order.len())
                .map(|b| conjugate(&nodes[b].then(&ibms[b]), 1000.0))
                .collect();
            for (p, pairs) in positions.iter().zip(raw) {
                let q = kbind.apply(*p);
                for &(j, w) in pairs {
                    if let Some(&b) = bone_of.get(j as usize).filter(|_| w > 0.0) {
                        offset = offset.max((at_nodes[b as usize].apply(*p) - q).length());
                    }
                }
            }
            // The recomputed inverse bind matrices against the file's, the
            // scale factored out: `rest · S(scale) · file's` should be `bind`.
            let mut error: f64 = 0.0;
            for (p, pairs) in positions.iter().zip(raw) {
                let q = kbind.apply(*p);
                for &(j, w) in pairs {
                    if let Some(&b) = bone_of.get(j as usize).filter(|_| w > 0.0) {
                        let b = b as usize;
                        let scaled_rest = Affine {
                            x: rests[b].x * scale,
                            y: rests[b].y * scale,
                            z: rests[b].z * scale,
                            t: rests[b].t,
                        };
                        let file = scaled_rest.then(&conjugate(&ibms[b], 1000.0));
                        error = error.max((file.apply(*p) - q).length());
                    }
                }
            }
            for (b, &ji) in order.iter().enumerate() {
                let (rest, inverse_bind) = rest_and_inverse(&rests[b]);
                bones.push(Bone {
                    name: names[b].clone(),
                    parent: info[ji].1.map(|p| bone_of[p]),
                    rest,
                    inverse_bind,
                });
            }
            scaled = Some((
                placed,
                SkinScale {
                    part: name.to_string(),
                    scale,
                    bind_error: error,
                    uneven,
                    node_offset: offset,
                },
            ));
        }
        let mut weights = Vec::with_capacity(raw.len());
        for pairs in raw {
            let mut mapped = Vec::with_capacity(4);
            for &(j, w) in pairs {
                if w > 0.0 {
                    let b = *bone_of.get(j as usize).ok_or_else(|| {
                        fmt_err(format!(
                            "{name}: a vertex names joint {j}, which the skin lacks"
                        ))
                    })?;
                    mapped.push((b, w));
                }
            }
            weights.push(Weights::from_raw(mapped));
        }
        let skin = Skin { bones, weights };
        skin.check(raw.len())
            .map_err(|e| ImportError::Unsupported(format!("{name}: its skin: {e}")))?;
        let space = SkinSpace {
            nodes: order.iter().map(|&ji| joints[ji]).collect(),
            ibms: order
                .iter()
                .enumerate()
                .map(|(b, &ji)| ibm_of(ji, &names[b]))
                .collect::<Result<_, _>>()?,
            unbind,
        };
        Ok(((skin, scaled), space))
    }

    /// The file's animations as they move the skin read into `space`:
    /// each sampled at its own key times and sixty times a second between
    /// them (or evenly, past [`MAX_KEYS`]).
    /// A channel on a node that is no ancestor of a bone moves nothing
    /// here. A cubic-spline curve is read through its key values, straight
    /// between them.
    fn clips(&self, space: &SkinSpace) -> Result<Vec<shape_mesh::SkinClip>, ImportError> {
        let nodes = self.json.get("nodes").items();
        let parents = self.parents();
        // The nodes that move a bone: the bones and all above them.
        let mut above = vec![false; nodes.len()];
        for &j in &space.nodes {
            let mut at = Some(j);
            let mut steps = 0;
            while let Some(i) = at {
                if above[i] || steps > nodes.len() {
                    break;
                }
                above[i] = true;
                at = parents[i];
                steps += 1;
            }
        }
        let mut out = Vec::new();
        let mut used = BTreeMap::new();
        for (ai, anim) in self.json.get("animations").items().iter().enumerate() {
            // Per node: its translation, rotation and scale curves.
            let mut tracks: BTreeMap<usize, [Option<Track>; 3]> = BTreeMap::new();
            let mut times: Vec<f64> = Vec::new();
            for ch in anim.get("channels").items() {
                let target = ch.get("target");
                let (Some(node), Some(path)) =
                    (target.get("node").index(), target.get("path").str())
                else {
                    continue;
                };
                let (slot, width) = match path {
                    "translation" => (0, 3),
                    "rotation" => (1, 4),
                    "scale" => (2, 3),
                    _ => continue,
                };
                if !above.get(node).copied().unwrap_or(false)
                    || !nodes[node].get("matrix").is_null()
                {
                    continue;
                }
                let sampler = anim
                    .get("samplers")
                    .at(ch.get("sampler").index().unwrap_or(usize::MAX));
                let (Some(input), Some(output)) =
                    (sampler.get("input").index(), sampler.get("output").index())
                else {
                    continue;
                };
                let (t, tw) = self.accessor(input)?;
                let (mut v, vw) = self.accessor(output)?;
                let interpolation = sampler.get("interpolation").str().unwrap_or("LINEAR");
                if interpolation == "CUBICSPLINE" && v.len() == t.len() * width * 3 {
                    // In-tangent, value, out-tangent a key: the value.
                    v = (0..t.len())
                        .flat_map(|k| v[(k * 3 + 1) * width..(k * 3 + 2) * width].to_vec())
                        .collect();
                }
                if tw != 1 || vw != width || t.is_empty() || v.len() != t.len() * width {
                    return Err(fmt_err(format!(
                        "animation {ai}: a channel's times and values do not match"
                    )));
                }
                if t.iter().any(|x| !x.is_finite()) || t.windows(2).any(|w| w[1] < w[0]) {
                    return Err(fmt_err(format!(
                        "animation {ai}: a channel's times do not rise"
                    )));
                }
                times.extend(&t);
                tracks.entry(node).or_insert([None, None, None])[slot] = Some(Track {
                    times: t,
                    values: v,
                    width,
                    step: interpolation == "STEP",
                });
            }
            if tracks.is_empty() {
                continue;
            }
            // Between samples the matrices run straight, which is not how
            // a bone turns between two keys far apart: sixty samples a
            // second go in beside the keys' own times.
            let (start, end) = times
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &t| {
                    (lo.min(t), hi.max(t))
                });
            let steps = ((end - start) * SAMPLES_A_SECOND).ceil();
            if steps >= 1.0 && steps < MAX_KEYS as f64 {
                times.extend(
                    (1..steps as usize).map(|k| start + (end - start) * (k as f64 / steps)),
                );
            }
            times.sort_by(f64::total_cmp);
            times.dedup();
            if times.len() > MAX_KEYS {
                let (first, last) = (times[0], times[times.len() - 1]);
                times = (0..MAX_KEYS)
                    .map(|k| first + (last - first) * (k as f64 / (MAX_KEYS - 1) as f64))
                    .collect();
            }
            let mut poses = Vec::with_capacity(times.len());
            for &time in &times {
                // Each node above a bone, in the world at this time.
                let mut world: Vec<Option<Affine>> = vec![None; nodes.len()];
                let local = |i: usize| -> Affine {
                    let n = &nodes[i];
                    let Some(tr) = tracks.get(&i) else {
                        return self.local(n);
                    };
                    let rest = |key: &str, d: [f64; 4], w: usize| {
                        n.get(key).nums().filter(|v| v.len() == w).map_or(d, |v| {
                            let mut a = d;
                            a[..w].copy_from_slice(&v);
                            a
                        })
                    };
                    let t = tr[0]
                        .as_ref()
                        .map_or(rest("translation", [0.0; 4], 3), |c| c.at(time));
                    let q = tr[1]
                        .as_ref()
                        .map_or(rest("rotation", [0.0, 0.0, 0.0, 1.0], 4), |c| c.at(time));
                    let s = tr[2]
                        .as_ref()
                        .map_or(rest("scale", [1.0, 1.0, 1.0, 0.0], 3), |c| c.at(time));
                    trs(
                        DVec3::new(t[0], t[1], t[2]),
                        q,
                        DVec3::new(s[0], s[1], s[2]),
                    )
                };
                let mut pose = Vec::with_capacity(space.nodes.len());
                for (b, &j) in space.nodes.iter().enumerate() {
                    // Up from the bone to a node already placed, then down.
                    let mut chain = Vec::new();
                    let mut at = Some(j);
                    while let Some(i) = at {
                        if world[i].is_some() || chain.len() > nodes.len() {
                            break;
                        }
                        chain.push(i);
                        at = parents[i];
                    }
                    let mut m = at.and_then(|i| world[i]).unwrap_or(Affine::IDENTITY);
                    for &i in chain.iter().rev() {
                        m = m.then(&local(i));
                        world[i] = Some(m);
                    }
                    let k = conjugate(&m.then(&space.ibms[b]), 1000.0).then(&space.unbind);
                    pose.push([
                        k.x.x, k.x.y, k.x.z, 0.0, k.y.x, k.y.y, k.y.z, 0.0, k.z.x, k.z.y, k.z.z,
                        0.0, k.t.x, k.t.y, k.t.z, 1.0,
                    ]);
                }
                poses.push(pose);
            }
            let name = anim
                .get("name")
                .str()
                .map_or_else(|| format!("animation{ai}"), str::to_string);
            // From 0: a clip's time is from its first key.
            let first = times[0];
            out.push(shape_mesh::SkinClip {
                name: unique(&name, &mut used),
                times: times.iter().map(|t| t - first).collect(),
                poses,
            });
        }
        Ok(out)
    }

    fn part(
        &self,
        name: &str,
        mesh: &Value,
        world: &Affine,
        out: &mut Imported,
        (images, image_bytes): (&[ImageInfo], &[Option<Vec<u8>>]),
    ) -> Result<(ImportedPart, RawWeights), ImportError> {
        let mut solid = Solid::default();
        let mut uvs: Vec<[DVec2; 3]> = Vec::new();
        let mut all_uv = true;
        let mut any_uv = false;
        let mut materials: Vec<Option<usize>> = Vec::new();
        let mut raw: Vec<Vec<(u32, f64)>> = Vec::new();
        for (pi, prim) in mesh.get("primitives").items().iter().enumerate() {
            let mode = prim.get("mode").index().unwrap_or(4);
            if !(4..=6).contains(&mode) {
                out.skipped
                    .push(format!("{name}: primitive {pi} draws points or lines"));
                continue;
            }
            if !prim.get("targets").items().is_empty() {
                out.skipped
                    .push(format!("{name}: primitive {pi}'s morph targets"));
            }
            let attrs = prim.get("attributes");
            let Some(pa) = attrs.get("POSITION").index() else {
                return Err(fmt_err(format!("{name}: primitive {pi} has no POSITION")));
            };
            let (pos, w) = self.accessor(pa)?;
            if w != 3 {
                return Err(fmt_err(format!("{name}: POSITION is not VEC3")));
            }
            let count = pos.len() / 3;
            let uv = match attrs.get("TEXCOORD_0").index() {
                Some(ua) => {
                    let (uv, w) = self.accessor(ua)?;
                    if w != 2 || uv.len() / 2 != count {
                        return Err(fmt_err(format!(
                            "{name}: TEXCOORD_0 does not match POSITION"
                        )));
                    }
                    Some(uv)
                }
                None => None,
            };
            let color = match attrs.get("COLOR_0").index() {
                Some(ca) => {
                    let (c, w) = self.accessor(ca)?;
                    if !(w == 3 || w == 4) || c.len() / w != count {
                        return Err(fmt_err(format!("{name}: COLOR_0 does not match POSITION")));
                    }
                    if c.iter().any(|x| !x.is_finite()) {
                        return Err(ImportError::NotFinite(name.to_string()));
                    }
                    let at = |v: usize| -> [f64; 4] {
                        let mut q = [1.0; 4];
                        q[..w].copy_from_slice(&c[v * w..v * w + w]);
                        q.map(|x| x.clamp(0.0, 1.0))
                    };
                    Some((0..count).map(at).collect::<Vec<_>>())
                }
                None => None,
            };
            let index: Vec<u32> = match prim.get("indices").index() {
                Some(ia) => {
                    let (idx, w) = self.accessor(ia)?;
                    if w != 1 {
                        return Err(fmt_err(format!("{name}: indices are not SCALAR")));
                    }
                    idx.iter().map(|&v| v as u32).collect()
                }
                None => (0..count as u32).collect(),
            };
            if index.iter().any(|&v| v as usize >= count) {
                return Err(fmt_err(format!("{name}: an index is out of range")));
            }
            let tris: Vec<[u32; 3]> = match mode {
                4 => index.as_chunks::<3>().0.to_vec(),
                5 => (2..index.len())
                    .map(|k| {
                        if k % 2 == 0 {
                            [index[k - 2], index[k - 1], index[k]]
                        } else {
                            [index[k - 1], index[k - 2], index[k]]
                        }
                    })
                    .collect(),
                _ => (2..index.len())
                    .map(|k| [index[k - 1], index[k], index[0]])
                    .collect(),
            };
            let mi = prim.get("material").index();
            if !materials.contains(&mi) {
                materials.push(mi);
            }
            // Bone weights per vertex: (joint, weight) pairs, as the file has
            // them; empty where the primitive has none.
            match (
                attrs.get("JOINTS_0").index(),
                attrs.get("WEIGHTS_0").index(),
            ) {
                (Some(ja), Some(wa)) => {
                    let (j, jw) = self.accessor(ja)?;
                    let (w, ww) = self.accessor(wa)?;
                    if jw != 4 || ww != 4 || j.len() / 4 != count || w.len() / 4 != count {
                        return Err(fmt_err(format!(
                            "{name}: JOINTS_0 or WEIGHTS_0 does not match POSITION"
                        )));
                    }
                    for v in 0..count {
                        raw.push(
                            (0..4)
                                .map(|k| (j[v * 4 + k] as u32, w[v * 4 + k]))
                                .collect(),
                        );
                    }
                }
                _ => raw.extend((0..count).map(|_| Vec::new())),
            }
            let base = solid.positions.len() as u32;
            for v in pos.as_chunks::<3>().0 {
                let p = from_y_up(DVec3::new(v[0], v[1], v[2])) * 1000.0;
                if !p.is_finite() {
                    return Err(ImportError::NotFinite(name.to_string()));
                }
                solid.push_vertex(p);
            }
            for t in &tris {
                solid.push_triangle(t.map(|v| v + base), pi as u32, &[name]);
                if let Some(c) = &color {
                    let i = solid.triangles.len() - 1;
                    solid.set_colors(i, t.map(|v| c[v as usize]));
                }
                match &uv {
                    Some(uv) => {
                        any_uv = true;
                        // glTF's v runs down the image; the kernel's up.
                        uvs.push(
                            t.map(|v| DVec2::new(uv[v as usize * 2], 1.0 - uv[v as usize * 2 + 1])),
                        )
                    }
                    None => {
                        all_uv = false;
                        uvs.push([DVec2::ZERO; 3]);
                    }
                }
            }
        }
        if materials.len() > 1 {
            out.skipped.push(format!(
                "{name}: its primitives use {} materials; only the first is kept",
                materials.len()
            ));
        }
        let (material, defaults) = match materials.first() {
            Some(&mi) => self.material(name, mi, images, image_bytes, &mut out.skipped),
            None => (Material::default(), Vec::new()),
        };
        Ok((
            ImportedPart {
                clips: Vec::new(),
                name: name.to_string(),
                transform: conjugate(world, 1000.0),
                solid: Solid {
                    uvs: if all_uv && !uvs.is_empty() {
                        Some(uvs)
                    } else {
                        if any_uv {
                            out.skipped.push(format!(
                                "{name}: TEXCOORD_0 on some primitives only, so no UVs"
                            ));
                        }
                        None
                    },
                    ..solid
                },
                material,
                defaults,
            },
            raw,
        ))
    }

    /// Material `mi` (`None`: glTF's default material) as the file means
    /// it, and the factors it leaves unset. What is read past (a map on a
    /// second UV set, a texture transform, a sampler that does not
    /// repeat) is noted in `skipped`.
    fn material(
        &self,
        name: &str,
        mi: Option<usize>,
        images: &[ImageInfo],
        bytes: &[Option<Vec<u8>>],
        skipped: &mut Vec<String>,
    ) -> (Material, Vec<String>) {
        let m = match mi {
            Some(mi) => self.json.get("materials").at(mi),
            None => &Value::Null,
        };
        let pbr = m.get("pbrMetallicRoughness");
        let mut defaults = Vec::new();
        let mut num = |v: &Value, key: &str, default: f64| -> f64 {
            match v.get(key).num() {
                Some(x) if x.is_finite() => x,
                _ => {
                    defaults.push(key.to_string());
                    default
                }
            }
        };
        let metalness = num(pbr, "metallicFactor", 1.0);
        let roughness = num(pbr, "roughnessFactor", 1.0);
        let base = pbr
            .get("baseColorFactor")
            .nums()
            .filter(|v| v.len() == 4 && v.iter().all(|x| x.is_finite()))
            .unwrap_or_else(|| {
                defaults.push("baseColorFactor".into());
                vec![1.0; 4]
            });
        let emissive = m
            .get("emissiveFactor")
            .nums()
            .filter(|v| v.len() == 3 && v.iter().all(|x| x.is_finite()))
            .map_or([0.0; 3], |v| [v[0], v[1], v[2]]);
        let ext = m.get("extensions");
        let strength = ext
            .get("KHR_materials_emissive_strength")
            .get("emissiveStrength")
            .num()
            .filter(|x| x.is_finite() && *x != 1.0);
        let mode = m.get("alphaMode").str().unwrap_or("OPAQUE");
        let cutoff = m.get("alphaCutoff").num().filter(|x| x.is_finite());
        let mut tex =
            |info: &Value, what: &str| self.texture(name, info, what, images, bytes, skipped);
        let material = Material {
            name: match mi {
                Some(mi) => m
                    .get("name")
                    .str()
                    .map_or_else(|| format!("material{mi}"), str::to_string),
                None => String::new(),
            },
            color: Some([base[0], base[1], base[2]]),
            metalness,
            roughness: Some(roughness),
            emissive,
            opacity: Some(base[3]),
            map: tex(pbr.get("baseColorTexture"), "base-colour"),
            color_space: ColorSpace::Linear,
            emissive_intensity: strength,
            transparent: Some(mode == "BLEND"),
            alpha_test: (mode == "MASK").then(|| cutoff.unwrap_or(0.5)),
            double_sided: m.get("doubleSided").bool().unwrap_or(false),
            metalness_map: tex(pbr.get("metallicRoughnessTexture"), "metallic-roughness"),
            normal_map: tex(m.get("normalTexture"), "normal"),
            normal_scale: m
                .get("normalTexture")
                .get("scale")
                .num()
                .filter(|x| x.is_finite() && *x != 1.0),
            emissive_map: tex(m.get("emissiveTexture"), "emissive"),
            ao_map: tex(m.get("occlusionTexture"), "occlusion"),
            ao_map_intensity: m
                .get("occlusionTexture")
                .get("strength")
                .num()
                .filter(|x| x.is_finite() && *x != 1.0),
        };
        for (k, _) in ext.entries() {
            if k != "KHR_materials_emissive_strength" {
                skipped.push(format!("{name}: its material's extension {k} is not read"));
            }
        }
        (material, defaults)
    }

    /// The texture a material's texture info names, if its image could be
    /// read. What is read past is noted in `skipped`.
    fn texture(
        &self,
        name: &str,
        info: &Value,
        what: &str,
        images: &[ImageInfo],
        bytes: &[Option<Vec<u8>>],
        skipped: &mut Vec<String>,
    ) -> Option<Texture> {
        let ti = info.get("index").index()?;
        if info.get("texCoord").index().unwrap_or(0) != 0 {
            skipped.push(format!(
                "{name}: its {what} texture uses a second UV set, which is not read"
            ));
            return None;
        }
        if !info
            .get("extensions")
            .get("KHR_texture_transform")
            .is_null()
        {
            skipped.push(format!(
                "{name}: its {what} texture's KHR_texture_transform is not applied to the UVs"
            ));
        }
        let tex = self.json.get("textures").at(ti);
        let sampler = self
            .json
            .get("samplers")
            .at(tex.get("sampler").index().unwrap_or(usize::MAX));
        for key in ["wrapS", "wrapT"] {
            if let Some(w) = sampler.get(key).index().filter(|&w| w != 10497) {
                skipped.push(format!(
                    "{name}: its {what} texture's {key} {w} is written as repeat"
                ));
            }
        }
        let ext = tex.get("extensions");
        let source = tex
            .get("source")
            .index()
            .or(ext.get("EXT_texture_webp").get("source").index())
            .or(ext.get("KHR_texture_basisu").get("source").index())?;
        let Some(b) = bytes.get(source).and_then(Option::as_ref) else {
            skipped.push(format!(
                "{name}: its {what} texture's image could not be read"
            ));
            return None;
        };
        let n = images.get(source).map_or("", |i| i.name.as_str());
        Some(Texture::from_bytes(n, b.clone()))
    }
}

// ── OBJ ─────────────────────────────────────────────────────────────────────

/// A material as an MTL file gives it: the statements read, its maps by
/// keyword (lower case), and the keywords not read.
#[derive(Clone, Debug, Default)]
struct Mtl {
    kd: Option<[f64; 3]>,
    ke: Option<[f64; 3]>,
    d: Option<f64>,
    pr: Option<f64>,
    pm: Option<f64>,
    maps: BTreeMap<String, String>,
    unread: Vec<String>,
}

/// Reads Wavefront OBJ text. Each `o` starts a part (or each `g`, in a
/// file with no `o`); `usemtl` starts a new surface. Faces of more than
/// three corners are triangulated; negative (relative) indices are read;
/// `vt` becomes [`Solid::uvs`]; `v x y z r g b` becomes
/// [`Solid::colors`] (sRGB in the file, as three.js reads it, made
/// linear; a file whose colours exceed 1 is read as 0..=255), white on a
/// vertex without one. `mtllib` files are read through `resolve` for their
/// texture maps' sizes, and a part's first material becomes its
/// [`ImportedPart::material`]: `Kd` (sRGB), `d` or `Tr`, `Ke`, `Pr`, `Pm`
/// (the PBR extension), `map_Kd`, `map_Ke` and `norm`. Other statements
/// (specular `Ks`, `Ns`, a `bump` height map, …) are listed in
/// [`Imported::skipped`], as are points and lines.
/// Every part's transform is the identity.
pub fn import_obj(bytes: &[u8], d: &Import, resolve: Resolve) -> Result<Imported, ImportError> {
    let text = String::from_utf8_lossy(bytes);
    let unit = if d.unit_mm > 0.0 { d.unit_mm } else { 1000.0 };
    let axes = d.axes()?;
    let has_o = text.lines().any(|l| l.trim_start().starts_with("o "));
    let mut out = Imported::default();
    let mut positions: Vec<DVec3> = Vec::new();
    let mut texcoords: Vec<DVec2> = Vec::new();
    let mut used = BTreeMap::new();
    let mut image_names: BTreeMap<String, ()> = BTreeMap::new();
    // Each material as the MTL gives it, and the bytes of every image read.
    let mut mtl: BTreeMap<String, Mtl> = BTreeMap::new();
    let mut image_bytes: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    // Per OBJ vertex, its colour as the file gives it.
    let mut vertex_colors: Vec<Option<[f64; 3]>> = Vec::new();
    // The part being read: name, global → local vertex map, solid, uvs.
    struct Building {
        name: String,
        local: BTreeMap<u32, u32>,
        solid: Solid,
        uvs: Vec<[DVec2; 3]>,
        all_uv: bool,
        any_uv: bool,
        surface: u32,
        material: String,
        materials: BTreeMap<String, u32>,
        /// Per local vertex, its OBJ vertex.
        global: Vec<usize>,
    }
    let start = |name: &str, used: &mut BTreeMap<String, ()>| Building {
        name: unique(name, used),
        local: BTreeMap::new(),
        solid: Solid::default(),
        uvs: Vec::new(),
        all_uv: true,
        any_uv: false,
        surface: 0,
        material: String::new(),
        materials: BTreeMap::new(),
        global: Vec::new(),
    };
    let finish = |b: Building, out: &mut Imported, globals: &mut Vec<Vec<usize>>| {
        if !b.solid.triangles.is_empty() {
            if !b.all_uv && b.any_uv {
                out.skipped
                    .push(format!("{}: vt on some faces only, so no UVs", b.name));
            }
            globals.push(b.global);
            out.parts.push(ImportedPart {
                clips: Vec::new(),
                name: b.name,
                solid: Solid {
                    uvs: if b.all_uv { Some(b.uvs) } else { None },
                    ..b.solid
                },
                transform: Affine::IDENTITY,
                material: Material {
                    name: b.material,
                    ..Default::default()
                },
                defaults: Vec::new(),
            });
        }
    };
    let mut globals: Vec<Vec<usize>> = Vec::new();
    let mut cur: Option<Building> = None;
    let (mut points, mut lines) = (0usize, 0usize);
    let mut joined = String::new();
    for (ln, raw) in text.lines().enumerate() {
        // Backslash continues a line.
        if let Some(head) = raw.strip_suffix('\\') {
            joined.push_str(head);
            joined.push(' ');
            continue;
        }
        joined.push_str(raw);
        let line = std::mem::take(&mut joined);
        let line = line.split('#').next().unwrap_or("");
        let mut tok = line.split_whitespace();
        let Some(key) = tok.next() else { continue };
        let bad = |what: &str| fmt_err(format!("OBJ line {}: {what}", ln + 1));
        let num = |s: Option<&str>| -> Result<f64, ImportError> {
            s.and_then(|s| s.parse::<f64>().ok())
                .ok_or_else(|| bad("a number is missing or unreadable"))
        };
        match key {
            "v" => {
                let p = DVec3::new(num(tok.next())?, num(tok.next())?, num(tok.next())?);
                let p = axes.apply(p) * unit;
                if !p.is_finite() {
                    return Err(ImportError::NotFinite("OBJ vertex".into()));
                }
                positions.push(p);
                // `v x y z r g b`: a vertex colour.
                vertex_colors.push(if tok.clone().nth(2).is_some() {
                    Some([num(tok.next())?, num(tok.next())?, num(tok.next())?])
                } else {
                    None
                });
            }
            "vt" => {
                let u = num(tok.next())?;
                let v = tok.next().map_or(Ok(0.0), |s| num(Some(s)))?;
                texcoords.push(DVec2::new(u, v));
            }
            "o" | "g" if (key == "o") == has_o => {
                let name = tok.collect::<Vec<_>>().join(" ");
                if let Some(b) = cur.take() {
                    finish(b, &mut out, &mut globals);
                }
                cur = Some(start(&name, &mut used));
            }
            "usemtl" => {
                let b = cur.get_or_insert_with(|| start("object", &mut used));
                let name = tok.collect::<Vec<_>>().join(" ");
                if b.material.is_empty() {
                    b.material = name.clone();
                }
                let next = b.materials.len() as u32;
                b.surface = *b.materials.entry(name).or_insert(next);
            }
            "mtllib" => {
                let file = tok.collect::<Vec<_>>().join(" ");
                match fetch(&file, resolve) {
                    Ok(bytes) => {
                        let mut current = String::new();
                        for l in String::from_utf8_lossy(&bytes).lines() {
                            let l = l.split('#').next().unwrap_or("").trim();
                            let Some(key) = l.split_whitespace().next() else {
                                continue;
                            };
                            let lower = key.to_ascii_lowercase();
                            let rest: Vec<&str> = l.split_whitespace().skip(1).collect();
                            if lower == "newmtl" {
                                current = rest.join(" ");
                                mtl.entry(current.clone()).or_default();
                                continue;
                            }
                            let m = mtl.entry(current.clone()).or_default();
                            let nums: Vec<f64> =
                                rest.iter().filter_map(|x| x.parse().ok()).collect();
                            let rgb = || (nums.len() >= 3).then(|| [nums[0], nums[1], nums[2]]);
                            let one = || nums.first().copied();
                            let is_map = lower.starts_with("map_")
                                || ["bump", "disp", "norm", "decal", "refl"]
                                    .contains(&lower.as_str());
                            let image =
                                is_map.then(|| rest.last().map(|x| x.to_string())).flatten();
                            let read = match lower.as_str() {
                                "kd" => rgb().map(|c| m.kd = Some(c)).is_some(),
                                "ke" => rgb().map(|c| m.ke = Some(c)).is_some(),
                                "d" => one().map(|x| m.d = Some(x)).is_some(),
                                "tr" => one().map(|x| m.d = Some(1.0 - x)).is_some(),
                                "pr" => one().map(|x| m.pr = Some(x)).is_some(),
                                "pm" => one().map(|x| m.pm = Some(x)).is_some(),
                                "map_kd" | "map_ke" | "norm" => {
                                    if let Some(img) = &image {
                                        m.maps.entry(lower.clone()).or_insert_with(|| img.clone());
                                    }
                                    image.is_some()
                                }
                                _ => false,
                            };
                            if !read && !m.unread.contains(&key.to_string()) {
                                m.unread.push(key.to_string());
                            }
                            if let Some(img) = image {
                                if image_names.insert(img.clone(), ()).is_some() {
                                    continue;
                                }
                                out.images.push(match fetch(&img, resolve) {
                                    Ok(b) => {
                                        let info = ImageInfo::from_bytes(&img, &b);
                                        image_bytes.insert(img.clone(), b);
                                        info
                                    }
                                    Err(e) => ImageInfo::missing(&img, &e.to_string()),
                                });
                            }
                        }
                    }
                    Err(e) => out.skipped.push(format!("material library {file:?}: {e}")),
                }
            }
            "p" => points += 1,
            "l" => lines += 1,
            "f" => {
                let b = cur.get_or_insert_with(|| start("object", &mut used));
                let mut corners: Vec<u32> = Vec::new();
                let mut corner_uv: Vec<Option<DVec2>> = Vec::new();
                for t in tok {
                    let mut f = t.split('/');
                    let resolve_index = |s: Option<&str>, len: usize| -> Option<usize> {
                        let v: i64 = s.filter(|s| !s.is_empty())?.parse().ok()?;
                        let i = if v < 0 { len as i64 + v } else { v - 1 };
                        (0..len as i64).contains(&i).then_some(i as usize)
                    };
                    let vi = resolve_index(f.next(), positions.len())
                        .ok_or_else(|| bad("a face names a vertex that does not exist"))?;
                    let ti = f.next();
                    let uv = match ti.filter(|s| !s.is_empty()) {
                        Some(_) => Some(
                            texcoords[resolve_index(ti, texcoords.len())
                                .ok_or_else(|| bad("a face names a vt that does not exist"))?],
                        ),
                        None => None,
                    };
                    let next = b.solid.positions.len() as u32;
                    let local = *b.local.entry(vi as u32).or_insert(next);
                    if local == next {
                        b.solid.push_vertex(positions[vi]);
                        b.global.push(vi);
                    }
                    corners.push(local);
                    corner_uv.push(uv);
                }
                if corners.len() < 3 {
                    return Err(bad("a face with fewer than three corners"));
                }
                if corners.len() > 3 {
                    out.polygons_triangulated += 1;
                }
                let tris = triangulate(&b.solid.positions, &corners);
                let name = b.name.clone();
                for t in tris {
                    b.solid.push_triangle(t, b.surface, &[&name]);
                    let uv = t.map(|v| {
                        let k = corners.iter().position(|&c| c == v).unwrap_or(0);
                        corner_uv[k]
                    });
                    if uv.iter().all(Option::is_some) {
                        b.any_uv = true;
                        b.uvs.push(uv.map(|u| u.unwrap_or(DVec2::ZERO)));
                    } else {
                        b.all_uv = false;
                        b.uvs.push([DVec2::ZERO; 3]);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(b) = cur.take() {
        finish(b, &mut out, &mut globals);
    }
    // Vertex colours: sRGB in the file, linear here; 0..=255 if any
    // exceeds 1.
    let colored = vertex_colors.iter().flatten().count();
    let scale = if vertex_colors.iter().flatten().flatten().any(|&c| c > 1.0) {
        255.0
    } else {
        1.0
    };
    let lin = |c: [f64; 3]| {
        let l = c.map(|x| crate::export::linear(x / scale));
        [l[0], l[1], l[2], 1.0]
    };
    for (p, global) in out.parts.iter_mut().zip(&globals) {
        if colored == 0 || !global.iter().any(|&g| vertex_colors[g].is_some()) {
            continue;
        }
        let per_vertex: Vec<[f64; 4]> = global
            .iter()
            .map(|&g| vertex_colors[g].map_or(shape_mesh::WHITE, lin))
            .collect();
        let colors = p
            .solid
            .triangles
            .iter()
            .map(|t| t.map(|v| per_vertex[v as usize]))
            .collect();
        p.solid.colors = Some(colors);
    }
    if colored > 0 && colored < vertex_colors.len() {
        out.skipped.push(format!(
            "{} of {} vertices have no colour and are white",
            vertex_colors.len() - colored,
            vertex_colors.len()
        ));
    }
    for p in &mut out.parts {
        let Some(m) = mtl.get(&p.material.name) else {
            continue;
        };
        let tex = |key: &str| {
            let img = m.maps.get(key)?;
            Some(Texture::from_bytes(img, image_bytes.get(img)?.clone()))
        };
        p.material = Material {
            name: p.material.name.clone(),
            color: m.kd,
            metalness: m.pm.unwrap_or(0.0),
            roughness: m.pr,
            emissive: m.ke.unwrap_or([0.0; 3]),
            opacity: m.d,
            map: tex("map_kd"),
            emissive_map: tex("map_ke"),
            normal_map: tex("norm"),
            ..Default::default()
        };
        for (key, img) in &m.maps {
            if !image_bytes.contains_key(img) {
                out.skipped.push(format!(
                    "{}: its {key} image {img:?} could not be read",
                    p.name
                ));
            }
        }
        if !m.unread.is_empty() {
            out.skipped.push(format!(
                "{}: its material's {} are not read",
                p.name,
                m.unread.join(", ")
            ));
        }
    }
    if points > 0 {
        out.skipped.push(format!("{points} point elements"));
    }
    if lines > 0 {
        out.skipped.push(format!("{lines} line elements"));
    }
    if out.parts.is_empty() {
        return Err(fmt_err("the file has no faces"));
    }
    Ok(out)
}

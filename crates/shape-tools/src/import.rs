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
//! positions are not welded, so per-corner UVs line up with
//! [`ImportedPart::uvs`]; repair welds.

use std::collections::BTreeMap;
use std::fmt;

use shape_core::{DVec2, DVec3};
use shape_mesh::Solid;

use crate::image::ImageInfo;
use crate::jsonread::{self, Value};
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
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Import {
    /// OBJ only: millimetres per file unit; 0 reads metres (1000), as
    /// glTF is.
    pub unit_mm: f64,
    /// OBJ only: the file is Z up already (no axis change).
    pub z_up: bool,
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
    /// Per triangle, its corners' texture coordinates as the file has
    /// them (glTF `TEXCOORD_0`, OBJ `vt`), if every triangle has them.
    pub uvs: Option<Vec<[DVec2; 3]>>,
    /// The name of the first material the part uses, or "".
    pub material: String,
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
        import_gltf(bytes, resolve)
    } else if bytes.starts_with(b"Kaydara FBX Binary") || ascii_fbx {
        crate::fbx::import_fbx(bytes, resolve)
    } else {
        import_obj(bytes, d, resolve)
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

const SUPPORTED_EXTENSIONS: [&str; 6] = [
    "KHR_mesh_quantization",
    "KHR_texture_transform",
    "KHR_materials_emissive_strength",
    "KHR_texture_basisu",
    "EXT_texture_webp",
    "KHR_materials_unlit",
];

struct Doc<'a> {
    json: Value,
    buffers: Vec<Vec<u8>>,
    resolve: Resolve<'a>,
}

/// Reads a glTF 2.0 file: binary (GLB) or JSON text. External buffers and
/// images are read through `resolve`; `data:` URIs are decoded.
///
/// Read: triangle primitives (also strips and fans), `POSITION`,
/// `TEXCOORD_0`, indices of every integer width, normalized and quantized
/// attributes (`KHR_mesh_quantization`), sparse accessors, the default
/// scene's node tree (matrices and TRS), and images' sizes. Skipped and
/// listed in [`Imported::skipped`]: points and lines, skins (meshes stay
/// in their bind pose) and morph targets. A required extension the
/// importer does not read (Draco, meshopt compression) is
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
    let mut buffers = Vec::new();
    let mut bin = bin;
    for (i, b) in json.get("buffers").items().iter().enumerate() {
        let data = match b.get("uri").str() {
            Some(uri) => fetch(uri, resolve)?,
            None if i == 0 => bin
                .take()
                .ok_or_else(|| fmt_err("buffer 0 has no URI and there is no BIN chunk"))?,
            None => return Err(fmt_err(format!("buffer {i} has no URI"))),
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
    let doc = Doc {
        json,
        buffers,
        resolve,
    };
    doc.read()
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
            .unwrap_or_else(|| vec![0.0, 0.0, 0.0, 1.0]);
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

    fn read(&self) -> Result<Imported, ImportError> {
        let mut out = Imported::default();
        // Images: sizes only.
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
            out.images.push(match bytes {
                Ok(b) => ImageInfo::from_bytes(&name, &b),
                Err(why) => ImageInfo::missing(&name, &why),
            });
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
        let mut skinned = 0;
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
            if !n.get("skin").is_null() {
                skinned += 1;
            }
            let mesh = self.json.get("meshes").at(m);
            let name = n
                .get("name")
                .str()
                .or(mesh.get("name").str())
                .map_or_else(|| format!("node{i}"), str::to_string);
            let name = unique(&name, &mut used);
            let part = self.part(&name, mesh, &world, &mut out)?;
            out.parts.push(part);
        }
        if skinned > 0 {
            out.skipped
                .push(format!("{skinned} skinned meshes: read in their bind pose"));
        }
        Ok(out)
    }

    fn part(
        &self,
        name: &str,
        mesh: &Value,
        world: &Affine,
        out: &mut Imported,
    ) -> Result<ImportedPart, ImportError> {
        let mut solid = Solid::default();
        let mut uvs: Vec<[DVec2; 3]> = Vec::new();
        let mut all_uv = true;
        let mut material = String::new();
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
            if material.is_empty()
                && let Some(mi) = prim.get("material").index()
            {
                material = self
                    .json
                    .get("materials")
                    .at(mi)
                    .get("name")
                    .str()
                    .map_or_else(|| format!("material{mi}"), str::to_string);
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
                match &uv {
                    Some(uv) => {
                        uvs.push(t.map(|v| DVec2::new(uv[v as usize * 2], uv[v as usize * 2 + 1])))
                    }
                    None => {
                        all_uv = false;
                        uvs.push([DVec2::ZERO; 3]);
                    }
                }
            }
        }
        Ok(ImportedPart {
            name: name.to_string(),
            solid,
            transform: conjugate(world, 1000.0),
            uvs: if all_uv && !uvs.is_empty() {
                Some(uvs)
            } else {
                None
            },
            material,
        })
    }
}

// ── OBJ ─────────────────────────────────────────────────────────────────────

/// Reads Wavefront OBJ text. Each `o` starts a part (or each `g`, in a
/// file with no `o`); `usemtl` starts a new surface. Faces of more than
/// three corners are triangulated; negative (relative) indices are read;
/// `vt` becomes [`ImportedPart::uvs`]. `mtllib` files are read through
/// `resolve` for their texture maps' sizes. Points and lines are skipped.
/// Every part's transform is the identity.
pub fn import_obj(bytes: &[u8], d: &Import, resolve: Resolve) -> Result<Imported, ImportError> {
    let text = String::from_utf8_lossy(bytes);
    let unit = if d.unit_mm > 0.0 { d.unit_mm } else { 1000.0 };
    let has_o = text.lines().any(|l| l.trim_start().starts_with("o "));
    let mut out = Imported::default();
    let mut positions: Vec<DVec3> = Vec::new();
    let mut texcoords: Vec<DVec2> = Vec::new();
    let mut used = BTreeMap::new();
    let mut image_names: BTreeMap<String, ()> = BTreeMap::new();
    // The part being read: name, global → local vertex map, solid, uvs.
    struct Building {
        name: String,
        local: BTreeMap<u32, u32>,
        solid: Solid,
        uvs: Vec<[DVec2; 3]>,
        all_uv: bool,
        surface: u32,
        material: String,
        materials: BTreeMap<String, u32>,
    }
    let start = |name: &str, used: &mut BTreeMap<String, ()>| Building {
        name: unique(name, used),
        local: BTreeMap::new(),
        solid: Solid::default(),
        uvs: Vec::new(),
        all_uv: true,
        surface: 0,
        material: String::new(),
        materials: BTreeMap::new(),
    };
    let finish = |b: Building, out: &mut Imported| {
        if !b.solid.triangles.is_empty() {
            out.parts.push(ImportedPart {
                name: b.name,
                solid: b.solid,
                transform: Affine::IDENTITY,
                uvs: if b.all_uv { Some(b.uvs) } else { None },
                material: b.material,
            });
        }
    };
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
                let p = if d.z_up { p } else { from_y_up(p) } * unit;
                if !p.is_finite() {
                    return Err(ImportError::NotFinite("OBJ vertex".into()));
                }
                positions.push(p);
            }
            "vt" => {
                let u = num(tok.next())?;
                let v = tok.next().map_or(Ok(0.0), |s| num(Some(s)))?;
                texcoords.push(DVec2::new(u, v));
            }
            "o" | "g" if (key == "o") == has_o => {
                let name = tok.collect::<Vec<_>>().join(" ");
                if let Some(b) = cur.take() {
                    finish(b, &mut out);
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
                    Ok(mtl) => {
                        for l in String::from_utf8_lossy(&mtl).lines() {
                            let l = l.trim();
                            let lower = l.to_ascii_lowercase();
                            if lower.starts_with("map_")
                                || lower.starts_with("bump ")
                                || lower.starts_with("disp ")
                                || lower.starts_with("norm ")
                            {
                                let Some(img) = l.split_whitespace().last() else {
                                    continue;
                                };
                                if image_names.insert(img.to_string(), ()).is_some() {
                                    continue;
                                }
                                out.images.push(match fetch(img, resolve) {
                                    Ok(b) => ImageInfo::from_bytes(img, &b),
                                    Err(e) => ImageInfo::missing(img, &e.to_string()),
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
        finish(b, &mut out);
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

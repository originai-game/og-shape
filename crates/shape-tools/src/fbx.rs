//! Import of FBX 7.x, binary and ASCII, into parts. Written here on
//! `miniz_oxide` (zlib for compressed arrays): ufbx is C, and the
//! pure-Rust `fbxcel` is a parser only, unmaintained since 2022
//! (`docs/research.md` §9).
//!
//! Read: meshes (`Geometry` with `Vertices` and `PolygonVertexIndex`;
//! polygons triangulated), the first UV layer, per-polygon materials as
//! surfaces, the model tree with FBX's full local transform (translation,
//! rotation offset and pivot, pre- and post-rotation, rotation in any
//! Euler order, scaling offset and pivot, scaling) and the geometric
//! transform, the file's axes and unit (`GlobalSettings`), and texture
//! images' sizes (embedded `Video` content or files through the
//! resolver), each part's first material ([`crate::ImportedPart::material`]:
//! `DiffuseColor` × `DiffuseFactor` as its sRGB colour, `EmissiveColor`
//! × `EmissiveFactor`, `Opacity`, and the textures linked to
//! `DiffuseColor`, `NormalMap` and `EmissiveColor`; other linked textures
//! are listed as skipped), the first UV layer as `Solid::uvs`,
//! and skins: a `Skin` deformer's clusters as bones, parents
//! first, their rest frames and inverse bind matrices from the clusters'
//! `TransformLink` (the bone at bind time) and `Transform` (the mesh
//! then, in the bone's space), their `Indexes` and `Weights` as
//! weights, on the part's `Solid::skin` in the mesh's own coordinates (a
//! bone scaled at bind time is refused). Skipped and listed: blend
//! shapes, NURBS and other geometry, vertex colours. FBX 6 and
//! older are [`ImportError::Unsupported`].

use std::collections::BTreeMap;

use shape_core::{DVec2, DVec3, Frame, trig};
use shape_mesh::{Bone, Skin, Solid, Weights};

use crate::export::Material;
use crate::image::{ImageInfo, Texture};
use crate::import::{
    Affine, ImportError, Imported, ImportedPart, Resolve, SkinScale, fetch, fmt_err, from_y_up,
    place, rest_and_inverse, similarity, unique,
};
use crate::polygon::triangulate;

/// One property of an FBX node.
#[derive(Clone, Debug, PartialEq)]
enum Prop {
    Int(i64),
    Num(f64),
    Str(String),
    Bytes(Vec<u8>),
    Arr(Vec<f64>),
}

impl Prop {
    fn num(&self) -> Option<f64> {
        match self {
            Prop::Int(v) => Some(*v as f64),
            Prop::Num(v) => Some(*v),
            _ => None,
        }
    }
    fn int(&self) -> Option<i64> {
        match self {
            Prop::Int(v) => Some(*v),
            Prop::Num(v) if v.fract() == 0.0 => Some(*v as i64),
            _ => None,
        }
    }
    fn str(&self) -> Option<&str> {
        match self {
            Prop::Str(s) => Some(s),
            _ => None,
        }
    }
    fn arr(&self) -> Option<&[f64]> {
        match self {
            Prop::Arr(v) => Some(v),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Node {
    name: String,
    props: Vec<Prop>,
    children: Vec<Node>,
}

impl Node {
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
    /// The values of a `Properties70` entry named `key`.
    fn p70(&self, key: &str) -> Option<&[Prop]> {
        self.child("Properties70")?
            .all("P")
            .find(|p| p.props.first().and_then(Prop::str) == Some(key))
            .map(|p| p.props.get(4..).unwrap_or(&[]))
    }
    fn p70_vec(&self, key: &str, default: DVec3) -> DVec3 {
        match self.p70(key) {
            Some(v) if v.len() >= 3 => DVec3::new(
                v[0].num().unwrap_or(default.x),
                v[1].num().unwrap_or(default.y),
                v[2].num().unwrap_or(default.z),
            ),
            _ => default,
        }
    }
    fn p70_num(&self, key: &str) -> Option<f64> {
        self.p70(key)?.first()?.num()
    }
    /// A child's array: `Vertices: *n { a: … }` or a binary array.
    fn array(&self, name: &str) -> Option<&[f64]> {
        self.child(name)?.props.first()?.arr()
    }
    fn text(&self, name: &str) -> Option<&str> {
        self.child(name)?.props.first()?.str()
    }
}

// ── binary ──────────────────────────────────────────────────────────────────

struct Bin<'a> {
    b: &'a [u8],
    wide: bool,
}

impl Bin<'_> {
    fn u(&self, at: usize, n: usize) -> Result<u64, ImportError> {
        let s = self
            .b
            .get(at..at + n)
            .ok_or_else(|| fmt_err("FBX is cut short"))?;
        let mut v = 0u64;
        for (k, &x) in s.iter().enumerate() {
            v |= u64::from(x) << (8 * k);
        }
        Ok(v)
    }

    fn word(&self, at: usize) -> Result<(u64, usize), ImportError> {
        if self.wide {
            Ok((self.u(at, 8)?, at + 8))
        } else {
            Ok((self.u(at, 4)?, at + 4))
        }
    }

    /// The record at `at`: the node (or `None` for the closing null
    /// record) and where the next record starts.
    fn node(&self, at: usize, depth: usize) -> Result<(Option<Node>, usize), ImportError> {
        if depth > 64 {
            return Err(fmt_err("FBX nodes nest too deep"));
        }
        let (end, p) = self.word(at)?;
        let (count, p) = self.word(p)?;
        let (_list_len, p) = self.word(p)?;
        let name_len = self.u(p, 1)? as usize;
        let p = p + 1;
        if end == 0 {
            return Ok((None, p + name_len));
        }
        let end = end as usize;
        if end > self.b.len() || end <= at {
            return Err(fmt_err("an FBX record ends outside the file"));
        }
        let name = String::from_utf8_lossy(
            self.b
                .get(p..p + name_len)
                .ok_or_else(|| fmt_err("FBX is cut short"))?,
        )
        .into_owned();
        let mut p = p + name_len;
        let mut props = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let (prop, next) = self.prop(p)?;
            props.push(prop);
            p = next;
        }
        let mut children = Vec::new();
        while p < end {
            let (child, next) = self.node(p, depth + 1)?;
            p = next;
            match child {
                Some(c) => children.push(c),
                None => break,
            }
        }
        Ok((
            Some(Node {
                name,
                props,
                children,
            }),
            end,
        ))
    }

    fn prop(&self, at: usize) -> Result<(Prop, usize), ImportError> {
        let kind = self.u(at, 1)? as u8;
        let p = at + 1;
        let f = |bits: u64, n: usize| -> f64 {
            if n == 4 {
                f64::from(f32::from_bits(bits as u32))
            } else {
                f64::from_bits(bits)
            }
        };
        let signed = |v: u64, n: usize| -> i64 {
            let shift = 64 - 8 * n as u32;
            ((v << shift) as i64) >> shift
        };
        Ok(match kind {
            b'Y' => (Prop::Int(signed(self.u(p, 2)?, 2)), p + 2),
            b'C' => (Prop::Int(self.u(p, 1)? as i64), p + 1),
            b'I' => (Prop::Int(signed(self.u(p, 4)?, 4)), p + 4),
            b'L' => (Prop::Int(self.u(p, 8)? as i64), p + 8),
            b'F' => (Prop::Num(f(self.u(p, 4)?, 4)), p + 4),
            b'D' => (Prop::Num(f(self.u(p, 8)?, 8)), p + 8),
            b'S' | b'R' => {
                let n = self.u(p, 4)? as usize;
                let data = self
                    .b
                    .get(p + 4..p + 4 + n)
                    .ok_or_else(|| fmt_err("an FBX string runs past the end"))?;
                let prop = if kind == b'S' {
                    Prop::Str(String::from_utf8_lossy(data).into_owned())
                } else {
                    Prop::Bytes(data.to_vec())
                };
                (prop, p + 4 + n)
            }
            b'f' | b'd' | b'l' | b'i' | b'b' => {
                let count = self.u(p, 4)? as usize;
                let encoding = self.u(p + 4, 4)?;
                let len = self.u(p + 8, 4)? as usize;
                let data = self
                    .b
                    .get(p + 12..p + 12 + len)
                    .ok_or_else(|| fmt_err("an FBX array runs past the end"))?;
                let size = match kind {
                    b'f' | b'i' => 4,
                    b'b' => 1,
                    _ => 8,
                };
                let raw: Vec<u8> = match encoding {
                    0 => data.to_vec(),
                    1 => {
                        miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, count * size)
                            .map_err(|_| fmt_err("an FBX array does not inflate"))?
                    }
                    e => return Err(fmt_err(format!("FBX array encoding {e}"))),
                };
                if raw.len() < count * size {
                    return Err(fmt_err("an FBX array is shorter than its count"));
                }
                let mut v = Vec::with_capacity(count);
                for k in 0..count {
                    let mut bits = 0u64;
                    for j in 0..size {
                        bits |= u64::from(raw[k * size + j]) << (8 * j);
                    }
                    v.push(match kind {
                        b'f' | b'd' => f(bits, size),
                        b'b' => bits as f64,
                        _ => signed(bits, size) as f64,
                    });
                }
                (Prop::Arr(v), p + 12 + len)
            }
            k => return Err(fmt_err(format!("FBX property type {:?}", k as char))),
        })
    }
}

fn parse_binary(b: &[u8]) -> Result<(u32, Node), ImportError> {
    let version = u32::from_le_bytes(
        b.get(23..27)
            .ok_or_else(|| fmt_err("FBX header is cut short"))?
            .try_into()
            .map_err(|_| fmt_err("FBX header"))?,
    );
    let r = Bin {
        b,
        wide: version >= 7500,
    };
    let mut root = Node::default();
    let mut p = 27;
    let header = if r.wide { 25 } else { 13 };
    while p + header <= b.len() {
        let (node, next) = r.node(p, 0)?;
        match node {
            Some(n) => root.children.push(n),
            None => break,
        }
        p = next;
    }
    Ok((version, root))
}

// ── ASCII ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Key(String),
    Val(Prop),
    Count,
    Open,
    Close,
    Comma,
}

fn lex(text: &str) -> Result<Vec<Tok>, ImportError> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b';' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                out.push(Tok::Open);
                i += 1;
            }
            b'}' => {
                out.push(Tok::Close);
                i += 1;
            }
            b',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            b'*' => {
                out.push(Tok::Count);
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
            }
            b'"' => {
                let start = i + 1;
                i = start;
                while i < b.len() && b[i] != b'"' {
                    i += 1;
                }
                let s = text[start..i.min(b.len())].replace("&quot;", "\"");
                out.push(Tok::Val(Prop::Str(s)));
                i += 1;
            }
            _ => {
                let start = i;
                while i < b.len() && !b" \t\r\n,{};\"".contains(&b[i]) {
                    i += 1;
                }
                if i == start {
                    return Err(fmt_err(format!("FBX text: unexpected {:?}", c as char)));
                }
                let word = &text[start..i];
                if let Some(key) = word.strip_suffix(':') {
                    out.push(Tok::Key(key.to_string()));
                } else if let Ok(v) = word.parse::<i64>() {
                    out.push(Tok::Val(Prop::Int(v)));
                } else if let Ok(v) = word.parse::<f64>() {
                    out.push(Tok::Val(Prop::Num(v)));
                } else {
                    out.push(Tok::Val(Prop::Str(word.to_string())));
                }
            }
        }
    }
    Ok(out)
}

fn parse_ascii(text: &str) -> Result<Node, ImportError> {
    let toks = lex(text)?;
    let mut i = 0;
    let mut root = Node::default();
    fn node(toks: &[Tok], i: &mut usize, depth: usize) -> Result<Node, ImportError> {
        if depth > 64 {
            return Err(fmt_err("FBX nodes nest too deep"));
        }
        let Some(Tok::Key(name)) = toks.get(*i) else {
            return Err(fmt_err("FBX text: expected a key"));
        };
        *i += 1;
        let mut n = Node {
            name: name.clone(),
            ..Default::default()
        };
        let mut counted = false;
        loop {
            match toks.get(*i) {
                Some(Tok::Val(v)) => {
                    n.props.push(v.clone());
                    *i += 1;
                    if toks.get(*i) == Some(&Tok::Comma) {
                        *i += 1;
                    } else {
                        break;
                    }
                }
                Some(Tok::Count) => {
                    counted = true;
                    *i += 1;
                    break;
                }
                _ => break,
            }
        }
        if toks.get(*i) == Some(&Tok::Open) {
            *i += 1;
            while let Some(t) = toks.get(*i) {
                if *t == Tok::Close {
                    *i += 1;
                    break;
                }
                n.children.push(node(toks, i, depth + 1)?);
            }
        }
        if counted {
            // `Name: *n { a: v, v, … }` is an array.
            let values: Vec<f64> = n
                .child("a")
                .map(|a| a.props.iter().filter_map(Prop::num).collect())
                .unwrap_or_default();
            n.props = vec![Prop::Arr(values)];
            n.children.clear();
        }
        Ok(n)
    }
    while i < toks.len() {
        root.children.push(node(&toks, &mut i, 0)?);
    }
    Ok(root)
}

// ── the scene ───────────────────────────────────────────────────────────────

/// An object's name without its class: binary `Name\0\x01Class`, ASCII
/// `Class::Name`.
fn object_name(raw: &str) -> String {
    if let Some(i) = raw.find("\u{0}\u{1}") {
        return raw[..i].to_string();
    }
    match raw.find("::") {
        Some(i) => raw[i + 2..].to_string(),
        None => raw.to_string(),
    }
}

/// A turn about one axis, by `degrees`.
fn turn(axis: usize, degrees: f64) -> Affine {
    let (s, c) = trig::sin_cos(trig::to_radians(degrees));
    let mut m = Affine::IDENTITY;
    match axis {
        0 => {
            m.y = DVec3::new(0.0, c, s);
            m.z = DVec3::new(0.0, -s, c);
        }
        1 => {
            m.x = DVec3::new(c, 0.0, -s);
            m.z = DVec3::new(s, 0.0, c);
        }
        _ => {
            m.x = DVec3::new(c, s, 0.0);
            m.y = DVec3::new(-s, c, 0.0);
        }
    }
    m
}

/// Euler angles (degrees) applied in `order` (FBX's `RotationOrder`: 0
/// XYZ, 1 XZY, 2 YZX, 3 YXZ, 4 ZXY, 5 ZYX; the first letter turns
/// first).
fn euler(r: DVec3, order: i64) -> Affine {
    let seq: [usize; 3] = match order {
        1 => [0, 2, 1],
        2 => [1, 2, 0],
        3 => [1, 0, 2],
        4 => [2, 0, 1],
        5 => [2, 1, 0],
        _ => [0, 1, 2],
    };
    let a = [r.x, r.y, r.z];
    let mut m = Affine::IDENTITY;
    for k in seq {
        m = turn(k, a[k]).then(&m);
    }
    m
}

fn translate(t: DVec3) -> Affine {
    Affine {
        t,
        ..Affine::IDENTITY
    }
}

fn scale(s: DVec3) -> Affine {
    Affine {
        x: DVec3::X * s.x,
        y: DVec3::Y * s.y,
        z: DVec3::Z * s.z,
        t: DVec3::ZERO,
    }
}

fn transpose(m: &Affine) -> Affine {
    Affine {
        x: DVec3::new(m.x.x, m.y.x, m.z.x),
        y: DVec3::new(m.x.y, m.y.y, m.z.y),
        z: DVec3::new(m.x.z, m.y.z, m.z.z),
        t: DVec3::ZERO,
    }
}

/// FBX's local transform of a model:
/// `T · Roff · Rp · Rpre · R · Rpost⁻¹ · Rp⁻¹ · Soff · Sp · S · Sp⁻¹`.
fn local(m: &Node) -> Affine {
    local_with(
        m,
        m.p70_vec("Lcl Translation", DVec3::ZERO),
        m.p70_vec("Lcl Rotation", DVec3::ZERO),
        m.p70_vec("Lcl Scaling", DVec3::ONE),
    )
}

/// [`local`] with the model's translation, rotation (Euler degrees, in
/// its own order) and scaling given: what an animation curve changes.
fn local_with(m: &Node, lcl_t: DVec3, lcl_r: DVec3, lcl_s: DVec3) -> Affine {
    let v = |k: &str| m.p70_vec(k, DVec3::ZERO);
    let order = m
        .p70("RotationOrder")
        .and_then(|p| p.first())
        .and_then(Prop::int)
        .unwrap_or(0);
    let (rp, sp) = (v("RotationPivot"), v("ScalingPivot"));
    let chain = [
        translate(lcl_t),
        translate(v("RotationOffset")),
        translate(rp),
        euler(v("PreRotation"), 0),
        euler(lcl_r, order),
        transpose(&euler(v("PostRotation"), 0)),
        translate(-rp),
        translate(v("ScalingOffset")),
        translate(sp),
        scale(lcl_s),
        translate(-sp),
    ];
    chain
        .iter()
        .rev()
        .fold(Affine::IDENTITY, |acc, step| step.then(&acc))
}

/// The geometric transform: moves the mesh, not the model's children.
fn geometric(m: &Node) -> Affine {
    let t = translate(m.p70_vec("GeometricTranslation", DVec3::ZERO));
    let r = euler(m.p70_vec("GeometricRotation", DVec3::ZERO), 0);
    let s = scale(m.p70_vec("GeometricScaling", DVec3::ONE));
    t.then(&r).then(&s)
}

/// The file's axes as a signed permutation onto glTF's (+X right, +Y up,
/// +Z front), from `GlobalSettings`.
fn axes(root: &Node) -> [(usize, f64); 3] {
    let g = root.child("GlobalSettings");
    let get = |k: &str, d: i64| {
        g.and_then(|g| g.p70(k))
            .and_then(|p| p.first())
            .and_then(Prop::int)
            .unwrap_or(d)
    };
    let sign = |v: i64| if v < 0 { -1.0 } else { 1.0 };
    let (c, u, f) = (get("CoordAxis", 0), get("UpAxis", 1), get("FrontAxis", 2));
    let ok = [c, u, f].iter().all(|a| (0..3).contains(a)) && c != u && u != f && c != f;
    if !ok {
        return [(0, 1.0), (1, 1.0), (2, 1.0)];
    }
    [
        (c as usize, sign(get("CoordAxisSign", 1))),
        (u as usize, sign(get("UpAxisSign", 1))),
        (f as usize, sign(get("FrontAxisSign", 1))),
    ]
}

/// Reads an FBX 7.x file, binary or ASCII. See the module docs for what
/// is read.
pub fn import_fbx(bytes: &[u8], resolve: Resolve) -> Result<Imported, ImportError> {
    let root = if bytes.starts_with(b"Kaydara FBX Binary") {
        let (version, root) = parse_binary(bytes)?;
        if version < 7000 {
            return Err(ImportError::Unsupported(format!("FBX version {version}")));
        }
        root
    } else {
        let text = String::from_utf8_lossy(bytes);
        let root = parse_ascii(&text)?;
        let version = root
            .child("FBXHeaderExtension")
            .and_then(|h| h.child("FBXVersion"))
            .and_then(|v| v.props.first())
            .and_then(Prop::int)
            .unwrap_or(0);
        if version < 7000 {
            return Err(ImportError::Unsupported(format!(
                "FBX version {version} (only 7.x)"
            )));
        }
        root
    };
    let objects = root
        .child("Objects")
        .ok_or_else(|| fmt_err("FBX has no Objects"))?;
    let mut by_id: BTreeMap<i64, &Node> = BTreeMap::new();
    for o in &objects.children {
        if let Some(id) = o.props.first().and_then(Prop::int) {
            by_id.insert(id, o);
        }
    }
    // Connections: object → object, in file order.
    let mut children_of: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    let mut parent_of: BTreeMap<i64, i64> = BTreeMap::new();
    // Material → its textures, by the property each OP link names
    // (`DiffuseColor`, `NormalMap`, …), first link of each.
    let mut textures_of: BTreeMap<i64, BTreeMap<String, i64>> = BTreeMap::new();
    // Every property link: (child, parent, the parent's property).
    let mut op_links: Vec<(i64, i64, String)> = Vec::new();
    if let Some(c) = root.child("Connections") {
        for link in c.all("C") {
            let kind = link.props.first().and_then(Prop::str).unwrap_or("");
            let (Some(child), Some(parent)) = (
                link.props.get(1).and_then(Prop::int),
                link.props.get(2).and_then(Prop::int),
            ) else {
                continue;
            };
            if kind == "OP"
                && let Some(prop) = link.props.get(3).and_then(Prop::str)
            {
                textures_of
                    .entry(parent)
                    .or_default()
                    .entry(prop.to_string())
                    .or_insert(child);
                op_links.push((child, parent, prop.to_string()));
            }
            if kind == "OO" || kind == "OP" {
                children_of.entry(parent).or_default().push(child);
                let is_model = |id: i64| by_id.get(&id).is_some_and(|n| n.name == "Model");
                // A bone is also linked under its cluster; only a model (or
                // the root, 0) is a parent in the tree.
                if is_model(child) && (parent == 0 || is_model(parent)) {
                    parent_of.entry(child).or_insert(parent);
                }
            }
        }
    }
    let unit = root
        .child("GlobalSettings")
        .and_then(|g| g.p70_num("UnitScaleFactor"))
        .filter(|v| *v > 0.0 && v.is_finite())
        .unwrap_or(1.0)
        * 10.0;
    let ax = axes(&root);
    // File space to kernel space, unit-free: a signed permutation.
    let to_kernel = |v: DVec3| {
        let a = [v.x, v.y, v.z];
        from_y_up(DVec3::new(
            a[ax[0].0] * ax[0].1,
            a[ax[1].0] * ax[1].1,
            a[ax[2].0] * ax[2].1,
        ))
    };
    let p_det = {
        let e = |k: usize| {
            let mut v = DVec3::ZERO;
            v[k] = 1.0;
            to_kernel(v)
        };
        e(0).dot(e(1).cross(e(2)))
    };
    let conj = |m: &Affine| Affine {
        // P M P⁻¹ with P a signed permutation: P⁻¹ = Pᵀ.
        x: to_kernel(m.rotate(back(&to_kernel, DVec3::X))),
        y: to_kernel(m.rotate(back(&to_kernel, DVec3::Y))),
        z: to_kernel(m.rotate(back(&to_kernel, DVec3::Z))),
        t: to_kernel(m.t) * unit,
    };
    let mut out = Imported::default();
    let mut used = BTreeMap::new();
    let mut shapes = 0;
    for o in &objects.children {
        let class = o.props.get(2).and_then(Prop::str).unwrap_or("");
        if o.name == "Deformer" && class != "Skin" && class != "Cluster" {
            shapes += 1;
        }
        if o.name == "Geometry" && o.props.get(2).and_then(Prop::str) != Some("Mesh") {
            out.skipped.push(format!(
                "geometry {:?} of kind {:?}",
                object_name(o.props.get(1).and_then(Prop::str).unwrap_or("")),
                o.props.get(2).and_then(Prop::str).unwrap_or("")
            ));
        }
    }
    if shapes > 0 {
        out.skipped.push(format!(
            "{shapes} blend-shape deformers: meshes read without them"
        ));
    }
    // A geometry's skin: its Skin deformer's clusters, each a bone, with
    // the bind matrices the clusters carry (`Transform`: the mesh at bind
    // time; `TransformLink`: the bone), in the mesh's own coordinates.
    let skin_of = |g: &Node,
                   name: &str,
                   positions: &[DVec3]|
     -> Result<Option<(FbxSkin, FbxSpace)>, ImportError> {
        let vertices = positions.len();
        let Some(gid) = g.props.first().and_then(Prop::int) else {
            return Ok(None);
        };
        let class = |n: &Node| n.props.get(2).and_then(Prop::str).unwrap_or("").to_string();
        let Some(skin) = children_of
            .get(&gid)
            .into_iter()
            .flatten()
            .filter_map(|c| by_id.get(c))
            .find(|n| n.name == "Deformer" && class(n) == "Skin")
        else {
            return Ok(None);
        };
        let unsupported =
            |why: String| ImportError::Unsupported(format!("{name}: its skin: {why}"));
        let sid = skin.props.first().and_then(Prop::int).unwrap_or(0);
        // (bone model id, name, cluster)
        let mut clusters: Vec<(i64, String, &Node)> = Vec::new();
        for c in children_of.get(&sid).into_iter().flatten() {
            let Some(cluster) = by_id.get(c).filter(|n| class(n) == "Cluster") else {
                continue;
            };
            let cid = cluster.props.first().and_then(Prop::int).unwrap_or(0);
            let Some(bone) = children_of
                .get(&cid)
                .into_iter()
                .flatten()
                .find(|b| by_id.get(b).is_some_and(|n| n.name == "Model"))
            else {
                return Err(unsupported("a cluster has no bone".into()));
            };
            let bname = object_name(by_id[bone].props.get(1).and_then(Prop::str).unwrap_or(""));
            clusters.push((*bone, bname, cluster));
        }
        if clusters.is_empty() {
            return Err(unsupported("no clusters".into()));
        }
        let matrix = |c: &Node, key: &str| -> Result<Affine, ImportError> {
            let m = c
                .array(key)
                .filter(|m| m.len() == 16)
                .ok_or_else(|| unsupported(format!("a cluster has no {key}")))?;
            Ok(conj(&Affine {
                x: DVec3::new(m[0], m[1], m[2]),
                y: DVec3::new(m[4], m[5], m[6]),
                z: DVec3::new(m[8], m[9], m[10]),
                t: DVec3::new(m[12], m[13], m[14]),
            }))
        };
        let rigid = |a: &Affine| {
            [a.x, a.y, a.z]
                .iter()
                .all(|v| (v.length() - 1.0).abs() < 1e-9)
                && a.x.dot(a.y).abs() < 1e-9
                && a.y.dot(a.z).abs() < 1e-9
                && a.x.dot(a.z).abs() < 1e-9
                && a.determinant() > 0.0
        };
        let inverse = |a: &Affine| {
            let r = transpose(a);
            Affine {
                t: -r.rotate(a.t),
                ..r
            }
        };
        // Parents first: by depth in the model tree, then cluster order.
        let ids: Vec<i64> = clusters.iter().map(|c| c.0).collect();
        let mut info: Vec<(usize, Option<usize>)> = Vec::new();
        for &id in &ids {
            let (mut at, mut depth, mut parent) = (id, 0usize, None);
            while let Some(&p) = parent_of.get(&at) {
                if p == 0 || depth > 512 {
                    break;
                }
                depth += 1;
                if parent.is_none() {
                    parent = ids.iter().position(|&x| x == p);
                }
                at = p;
            }
            info.push((depth, parent));
        }
        let mut order: Vec<usize> = (0..clusters.len()).collect();
        order.sort_by_key(|&k| (info[k].0, k));
        let mut bone_of = vec![0u32; clusters.len()];
        for (b, &k) in order.iter().enumerate() {
            bone_of[k] = b as u32;
        }
        let mut used = BTreeMap::new();
        let mut names = Vec::with_capacity(order.len());
        let mut binds = Vec::with_capacity(order.len());
        for &k in &order {
            let (_, bname, cluster) = &clusters[k];
            names.push(unique(bname, &mut used));
            // `TransformLink` is the bone in the world at bind time.
            // `Transform` is the mesh then, as the bone sees it (the bone's
            // inverse times the mesh in the world): a file whose mesh
            // stands at the origin has each cluster's `Transform` the
            // inverse of its `TransformLink`. So the mesh in the world is
            // their product.
            let link = matrix(cluster, "TransformLink")?;
            binds.push((link.then(&matrix(cluster, "Transform")?), link));
        }
        let mut bones = Vec::with_capacity(clusters.len());
        let mut scaled = None;
        if binds.iter().all(|(m, b)| rigid(m) && rigid(b)) {
            for (i, &k) in order.iter().enumerate() {
                let (mesh_bind, bone_bind) = &binds[i];
                // The bone at rest in the mesh's coordinates, and its inverse.
                let rest = inverse(mesh_bind).then(bone_bind);
                let ibm = inverse(&rest);
                bones.push(Bone {
                    name: names[i].clone(),
                    parent: info[k].1.map(|p| bone_of[p]),
                    rest: Frame {
                        origin: rest.t,
                        x: rest.x,
                        y: rest.y,
                        z: rest.z,
                    },
                    inverse_bind: [
                        ibm.x.x, ibm.x.y, ibm.x.z, 0.0, ibm.y.x, ibm.y.y, ibm.y.z, 0.0, ibm.z.x,
                        ibm.z.y, ibm.z.z, 0.0, ibm.t.x, ibm.t.y, ibm.t.z, 1.0,
                    ],
                });
            }
        } else {
            // A uniform scale on the bones comes out: the rest frames are the
            // bones' bind transforms divided by it, and the vertices go where
            // the mesh's bind transform puts them, in the world.
            let mut scale = None;
            for (i, (mesh_bind, bone_bind)) in binds.iter().enumerate() {
                let (sb, _, _) = similarity(bone_bind).ok_or_else(|| {
                    unsupported(format!(
                        "bone {:?} is scaled unevenly or sheared at bind time",
                        names[i]
                    ))
                })?;
                if similarity(mesh_bind).is_none() {
                    return Err(unsupported(
                        "the mesh is scaled unevenly or sheared at bind time".into(),
                    ));
                }
                let s0 = *scale.get_or_insert(sb);
                if (sb - s0).abs() > crate::import::UNEVEN * s0 {
                    return Err(unsupported(format!(
                        "bones {:?} and {:?} are scaled differently ({s0} and {sb})",
                        names[0], names[i]
                    )));
                }
            }
            // Each vertex goes where its clusters' mesh bind transforms put
            // it, blended by weight; how far they disagree is the error.
            let mut raw: Vec<Vec<(u32, f64)>> = vec![Vec::new(); vertices];
            for (i, &k) in order.iter().enumerate() {
                let c = clusters[k].2;
                let idx = c.array("Indexes").unwrap_or(&[]);
                let w = c.array("Weights").unwrap_or(&[]);
                for (&v, &x) in idx.iter().zip(w) {
                    if let Some(r) = raw.get_mut(v as usize) {
                        r.push((i as u32, x));
                    }
                }
            }
            let (placed, error) = place(positions, &raw, &|b| binds.get(b as usize).map(|m| m.0))
                .map_err(unsupported)?;
            let size = placed
                .iter()
                .fold(0.0f64, |m, q| m.max(q.abs().max_element()));
            if error > 1e-4 * size.max(1.0) {
                return Err(unsupported(format!(
                    "its clusters place a vertex differently at bind time, by {error} mm"
                )));
            }
            let mut uneven: f64 = 0.0;
            for (i, &k) in order.iter().enumerate() {
                let (_, rigid_link, u) = similarity(&binds[i].1).unwrap_or((1.0, binds[i].1, 0.0));
                uneven = uneven.max(u);
                let (rest, inverse_bind) = rest_and_inverse(&rigid_link);
                bones.push(Bone {
                    name: names[i].clone(),
                    parent: info[k].1.map(|p| bone_of[p]),
                    rest,
                    inverse_bind,
                });
            }
            scaled = Some((
                placed,
                SkinScale {
                    part: name.to_string(),
                    scale: scale.unwrap_or(1.0),
                    bind_error: error,
                    uneven,
                    node_offset: 0.0,
                },
            ));
        }
        let mut raw: Vec<Vec<(u32, f64)>> = vec![Vec::new(); vertices];
        for (k, (_, _, cluster)) in clusters.iter().enumerate() {
            let idx = cluster.array("Indexes").unwrap_or(&[]);
            let w = cluster.array("Weights").unwrap_or(&[]);
            for (&i, &x) in idx.iter().zip(w) {
                let i = i as usize;
                if i >= vertices {
                    return Err(unsupported(
                        "a cluster names a vertex that does not exist".into(),
                    ));
                }
                raw[i].push((bone_of[k], x));
            }
        }
        let skin = Skin {
            bones,
            weights: raw.into_iter().map(Weights::from_raw).collect(),
        };
        skin.check(vertices)
            .map_err(|e| unsupported(e.to_string()))?;
        // What a clip needs to follow this skin. The solid stands in the
        // mesh's own coordinates (rigid bones) or where its bind pose puts
        // it in the world (a scale taken out).
        let in_world = scaled.is_some();
        let mut unlink = Vec::with_capacity(order.len());
        for (i, (_, bone_bind)) in binds.iter().enumerate() {
            unlink.push(crate::import::affine_inverse(bone_bind).ok_or_else(|| {
                unsupported(format!("bone {:?}'s bind transform is singular", names[i]))
            })?);
        }
        let space = FbxSpace {
            ids: order.iter().map(|&k| clusters[k].0).collect(),
            unlink,
            mesh: binds
                .iter()
                .map(|b| if in_world { Affine::IDENTITY } else { b.0 })
                .collect(),
            out: if in_world {
                Affine::IDENTITY
            } else {
                inverse(&binds[0].0)
            },
        };
        Ok(Some(((skin, scaled), space)))
    };
    // World transform of a model, by its parents.
    let world = |id: i64| -> Affine {
        let mut m = Affine::IDENTITY;
        let mut at = id;
        for _ in 0..256 {
            let Some(node) = by_id.get(&at) else { break };
            m = local(node).then(&m);
            match parent_of.get(&at) {
                Some(&p) if p != 0 => at = p,
                _ => break,
            }
        }
        m
    };
    // The file's animation stacks as they move a skin: each sampled at
    // its curves' key times and sixty times a second between them. A
    // stack's first layer is read; a curve is straight between its keys
    // (its easing is not read).
    let kids = |id: i64, name: &str| -> Vec<i64> {
        children_of
            .get(&id)
            .into_iter()
            .flatten()
            .copied()
            .filter(|c| by_id.get(c).is_some_and(|n| n.name == name))
            .collect()
    };
    let mut layered = Vec::new();
    // Per stack: its name, and per model its (translation, rotation,
    // scaling) curves, each axis as key times (seconds) and values.
    type Curve = (Vec<f64>, Vec<f64>);
    type Driven = BTreeMap<i64, [[Option<Curve>; 3]; 3]>;
    let mut stacks: Vec<(String, Driven)> = Vec::new();
    for o in objects.all("AnimationStack") {
        let Some(sid) = o.props.first().and_then(Prop::int) else {
            continue;
        };
        let sname = object_name(o.props.get(1).and_then(Prop::str).unwrap_or(""));
        let layers = kids(sid, "AnimationLayer");
        if layers.len() > 1 {
            layered.push(format!(
                "animation {sname:?}: {} layers, the first read",
                layers.len()
            ));
        }
        let Some(&layer) = layers.first() else {
            continue;
        };
        let mut driven: Driven = BTreeMap::new();
        for cn in kids(layer, "AnimationCurveNode") {
            // The model property this curve node drives.
            let Some((model, slot)) = op_links.iter().find_map(|(c, p, prop)| {
                let slot = match prop.as_str() {
                    "Lcl Translation" => 0,
                    "Lcl Rotation" => 1,
                    "Lcl Scaling" => 2,
                    _ => return None,
                };
                (*c == cn && by_id.get(p).is_some_and(|n| n.name == "Model")).then_some((*p, slot))
            }) else {
                continue;
            };
            for (c, p, prop) in &op_links {
                let axis = match prop.as_str() {
                    "d|X" => 0,
                    "d|Y" => 1,
                    "d|Z" => 2,
                    _ => continue,
                };
                if *p != cn {
                    continue;
                }
                let Some(curve) = by_id.get(c).filter(|n| n.name == "AnimationCurve") else {
                    continue;
                };
                let (Some(t), Some(v)) = (curve.array("KeyTime"), curve.array("KeyValueFloat"))
                else {
                    continue;
                };
                if t.is_empty() || t.len() != v.len() || t.windows(2).any(|w| w[1] < w[0]) {
                    return Err(fmt_err(format!(
                        "animation {sname:?}: a curve's times and values do not match or do not rise"
                    )));
                }
                // FBX time: 46 186 158 000 ticks a second.
                let seconds: Vec<f64> = t.iter().map(|x| x / 46_186_158_000.0).collect();
                driven.entry(model).or_default()[slot][axis] = Some((seconds, v.to_vec()));
            }
        }
        if !driven.is_empty() {
            stacks.push((sname, driven));
        }
    }
    out.skipped.extend(layered);
    let value = |c: &Curve, time: f64| -> f64 {
        let (t, v) = c;
        let k = t.partition_point(|&x| x <= time);
        if k == 0 {
            return v[0];
        }
        if k >= t.len() {
            return v[t.len() - 1];
        }
        let f = if t[k] > t[k - 1] {
            (time - t[k - 1]) / (t[k] - t[k - 1])
        } else {
            0.0
        };
        v[k - 1] + (v[k] - v[k - 1]) * f
    };
    let clips_of = |space: &FbxSpace| -> Result<Vec<shape_mesh::SkinClip>, ImportError> {
        let mut clips = Vec::new();
        let mut used = BTreeMap::new();
        for (sname, driven) in &stacks {
            let mut times: Vec<f64> = driven
                .values()
                .flatten()
                .flatten()
                .flatten()
                .flat_map(|c| c.0.iter().copied())
                .collect();
            if times.is_empty() {
                continue;
            }
            let (start, end) = times
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &t| {
                    (lo.min(t), hi.max(t))
                });
            let steps = ((end - start) * crate::import::SAMPLES_A_SECOND).ceil();
            if steps >= 1.0 && steps < crate::import::MAX_KEYS as f64 {
                times.extend(
                    (1..steps as usize).map(|k| start + (end - start) * (k as f64 / steps)),
                );
            }
            times.sort_by(f64::total_cmp);
            times.dedup();
            if times.len() > crate::import::MAX_KEYS {
                let n = crate::import::MAX_KEYS;
                times = (0..n)
                    .map(|k| start + (end - start) * (k as f64 / (n - 1) as f64))
                    .collect();
            }
            let mut poses = Vec::with_capacity(times.len());
            for &time in &times {
                // A model's local transform at this time.
                let local_at = |id: i64, node: &Node| -> Affine {
                    let Some(d) = driven.get(&id) else {
                        return local(node);
                    };
                    let rest = [
                        node.p70_vec("Lcl Translation", DVec3::ZERO),
                        node.p70_vec("Lcl Rotation", DVec3::ZERO),
                        node.p70_vec("Lcl Scaling", DVec3::ONE),
                    ];
                    let at = |slot: usize| {
                        let mut v = rest[slot];
                        for axis in 0..3 {
                            if let Some(c) = &d[slot][axis] {
                                v[axis] = value(c, time);
                            }
                        }
                        v
                    };
                    local_with(node, at(0), at(1), at(2))
                };
                let mut pose = Vec::with_capacity(space.ids.len());
                for (b, &id) in space.ids.iter().enumerate() {
                    let mut m = Affine::IDENTITY;
                    let mut at = id;
                    for _ in 0..256 {
                        let Some(node) = by_id.get(&at) else { break };
                        m = local_at(at, node).then(&m);
                        match parent_of.get(&at) {
                            Some(&p) if p != 0 => at = p,
                            _ => break,
                        }
                    }
                    let k = space
                        .out
                        .then(&conj(&m))
                        .then(&space.unlink[b])
                        .then(&space.mesh[b]);
                    pose.push([
                        k.x.x, k.x.y, k.x.z, 0.0, k.y.x, k.y.y, k.y.z, 0.0, k.z.x, k.z.y, k.z.z,
                        0.0, k.t.x, k.t.y, k.t.z, 1.0,
                    ]);
                }
                poses.push(pose);
            }
            clips.push(shape_mesh::SkinClip {
                name: unique(sname, &mut used),
                times: times.iter().map(|t| t - start).collect(),
                poses,
            });
        }
        Ok(clips)
    };
    // A video's name and its bytes (embedded content, or the file).
    let video = |v: &Node| -> (String, Result<Vec<u8>, String>) {
        let file = v
            .text("RelativeFilename")
            .or(v.text("Filename"))
            .unwrap_or("")
            .replace('\\', "/");
        let name = if file.is_empty() {
            object_name(v.props.get(1).and_then(Prop::str).unwrap_or("video"))
        } else {
            file.clone()
        };
        let content = v.child("Content").and_then(|c| match c.props.first() {
            Some(Prop::Bytes(b)) if !b.is_empty() => Some(b.clone()),
            _ => None,
        });
        let bytes = match content {
            Some(b) => Ok(b),
            None => fetch(&file, resolve).map_err(|e| e.to_string()),
        };
        (name, bytes)
    };
    for o in objects.all("Model") {
        let Some(id) = o.props.first().and_then(Prop::int) else {
            continue;
        };
        let geometries: Vec<&Node> = children_of
            .get(&id)
            .map(|cs| {
                cs.iter()
                    .filter_map(|c| by_id.get(c))
                    .filter(|n| {
                        n.name == "Geometry" && n.props.get(2).and_then(Prop::str) == Some("Mesh")
                    })
                    .copied()
                    .collect()
            })
            .unwrap_or_default();
        let materials: Vec<(i64, String)> = children_of
            .get(&id)
            .map(|cs| {
                cs.iter()
                    .filter_map(|c| Some((*c, *by_id.get(c)?)))
                    .filter(|(_, n)| n.name == "Material")
                    .map(|(c, n)| {
                        (
                            c,
                            object_name(n.props.get(1).and_then(Prop::str).unwrap_or("")),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        // The first material, and its textures through their videos.
        let mut unread: Vec<String> = Vec::new();
        let material = materials.first().map(|(m, mname)| {
            let node = by_id.get(m);
            let texture = |prop: &str| {
                let t = textures_of.get(m)?.get(prop)?;
                let v = children_of
                    .get(t)?
                    .iter()
                    .filter_map(|c| by_id.get(c))
                    .find(|n| n.name == "Video")?;
                let (name, bytes) = video(v);
                Some(Texture::from_bytes(&name, bytes.ok()?))
            };
            if let Some(links) = textures_of.get(m) {
                for prop in links.keys() {
                    if !["DiffuseColor", "NormalMap", "EmissiveColor"].contains(&prop.as_str()) {
                        unread.push(prop.clone());
                    }
                }
            }
            let rgb = |key: &str, factor: &str| {
                let n = node?;
                let c = n.p70_vec(key, DVec3::NAN);
                if !c.is_finite() {
                    return None;
                }
                let f = n.p70_num(factor).unwrap_or(1.0);
                Some([c.x * f, c.y * f, c.z * f])
            };
            Material {
                name: mname.clone(),
                color: rgb("DiffuseColor", "DiffuseFactor"),
                emissive: rgb("EmissiveColor", "EmissiveFactor").unwrap_or([0.0; 3]),
                opacity: node
                    .and_then(|n| n.p70_num("Opacity"))
                    .filter(|x| x.is_finite()),
                map: texture("DiffuseColor"),
                normal_map: texture("NormalMap"),
                emissive_map: texture("EmissiveColor"),
                ..Default::default()
            }
        });
        let name = unique(
            &object_name(o.props.get(1).and_then(Prop::str).unwrap_or("")),
            &mut used,
        );
        let geo = geometric(o);
        for g in geometries {
            let (mut solid, uvs, polys) = mesh(g, &name, &geo, unit, &to_kernel, p_det < 0.0)?;
            if g.all("LayerElementColor").next().is_some() {
                out.skipped.push(format!(
                    "{name}: its vertex colours (LayerElementColor) are not read"
                ));
            }
            if uvs.is_none() && g.all("LayerElementUV").next().is_some() {
                out.skipped.push(format!(
                    "{name}: its UV layer misses some corners, so no UVs"
                ));
            }
            solid.uvs = uvs;
            let read = skin_of(g, &name, &solid.positions)?;
            let mut clips = Vec::new();
            if let Some(((skin, scaled), space)) = read {
                clips = clips_of(&space)?;
                if let Some((placed, info)) = scaled {
                    // Where the file's bind pose puts the vertices.
                    solid.positions = placed;
                    out.skin_scales.push(info);
                }
                solid.skin = Some(skin);
            }
            let mut transform = conj(&world(id));
            if solid.skin.is_some() {
                // The bones place a skinned mesh; its vertices and bones
                // share the mesh's own coordinates.
                if geo != Affine::IDENTITY || p_det < 0.0 {
                    return Err(ImportError::Unsupported(format!(
                        "{name}: a skinned mesh with a geometric transform or mirrored axes"
                    )));
                }
                if transform != Affine::IDENTITY {
                    out.skipped.push(format!(
                        "{name}: skinned, so its model transform is not used (its bones place it)"
                    ));
                }
                transform = Affine::IDENTITY;
            }
            out.polygons_triangulated += polys;
            out.parts.push(ImportedPart {
                clips,
                name: name.clone(),
                solid,
                transform,
                material: material.clone().unwrap_or_default(),
                defaults: Vec::new(),
            });
            if !unread.is_empty() {
                out.skipped.push(format!(
                    "{name}: its material's textures on {} are not read",
                    unread.join(", ")
                ));
            }
        }
    }
    // Texture images.
    let mut seen = BTreeMap::new();
    for v in objects.all("Video") {
        let (name, bytes) = video(v);
        if seen.insert(name.clone(), ()).is_some() {
            continue;
        }
        out.images.push(match bytes {
            Ok(b) => ImageInfo::from_bytes(&name, &b),
            Err(e) => ImageInfo::missing(&name, &e),
        });
    }
    if out.parts.is_empty() {
        let what = if out.skipped.is_empty() {
            String::new()
        } else {
            format!(" (it has {})", out.skipped.join("; "))
        };
        return Err(ImportError::Unsupported(format!(
            "an FBX file with no polygon meshes{what}"
        )));
    }
    Ok(out)
}

/// The inverse of a signed permutation applied to `v`.
fn back(p: &impl Fn(DVec3) -> DVec3, v: DVec3) -> DVec3 {
    let mut out = DVec3::ZERO;
    for k in 0..3 {
        let mut e = DVec3::ZERO;
        e[k] = 1.0;
        out[k] = p(e).dot(v);
    }
    out
}

type Mesh = (Solid, Option<Vec<[DVec2; 3]>>, usize);

use crate::import::ReadSkin as FbxSkin;

/// The space a skin was read into: what a clip needs to follow it. Bone
/// `b`'s skinning matrix when its model stands at `world` (kernel space)
/// is `out · world · unlink[b] · mesh[b]`.
struct FbxSpace {
    /// Each bone's model, in the skin's bone order.
    ids: Vec<i64>,
    /// Each bone's bind transform (`TransformLink`), inverted.
    unlink: Vec<Affine>,
    /// Each cluster's mesh bind transform (`Transform`); the identity
    /// where the vertices were placed by it already.
    mesh: Vec<Affine>,
    /// From the world back to where the solid's vertices stand.
    out: Affine,
}

fn mesh(
    g: &Node,
    name: &str,
    geo: &Affine,
    unit: f64,
    to_kernel: &impl Fn(DVec3) -> DVec3,
    mirrored: bool,
) -> Result<Mesh, ImportError> {
    let verts = g.array("Vertices").unwrap_or(&[]);
    let index = g.array("PolygonVertexIndex").unwrap_or(&[]);
    let mut solid = Solid::default();
    for v in verts.as_chunks::<3>().0 {
        let p = to_kernel(geo.apply(DVec3::new(v[0], v[1], v[2]))) * unit;
        if !p.is_finite() {
            return Err(ImportError::NotFinite(name.to_string()));
        }
        solid.push_vertex(p);
    }
    let n = solid.positions.len();
    // UVs: the first layer.
    let uv_layer = g.all("LayerElementUV").next();
    let uv_values = uv_layer.and_then(|l| l.array("UV"));
    let uv_index = uv_layer.and_then(|l| l.array("UVIndex"));
    let uv_mapping = uv_layer
        .and_then(|l| l.text("MappingInformationType"))
        .unwrap_or("");
    let uv_ref = uv_layer
        .and_then(|l| l.text("ReferenceInformationType"))
        .unwrap_or("");
    let uv_at = |corner: usize, vertex: usize| -> Option<DVec2> {
        let values = uv_values?;
        let k = match uv_mapping {
            "ByPolygonVertex" => corner,
            "ByVertice" | "ByVertex" | "ByControlPoint" => vertex,
            _ => return None,
        };
        let k = if uv_ref == "Direct" {
            k
        } else {
            *uv_index?.get(k)? as usize
        };
        Some(DVec2::new(*values.get(k * 2)?, *values.get(k * 2 + 1)?))
    };
    // Materials per polygon.
    let mat_layer = g.all("LayerElementMaterial").next();
    let mats = mat_layer.and_then(|l| l.array("Materials"));
    let by_polygon = mat_layer.and_then(|l| l.text("MappingInformationType")) == Some("ByPolygon");
    let mut uvs: Vec<[DVec2; 3]> = Vec::new();
    let mut all_uv = uv_values.is_some();
    let mut polys = 0;
    let mut corners: Vec<u32> = Vec::new();
    let mut corner_ids: Vec<usize> = Vec::new();
    let mut polygon = 0usize;
    for (c, &raw) in index.iter().enumerate() {
        let raw = raw as i64;
        let (v, last) = if raw < 0 {
            ((-raw - 1) as usize, true)
        } else {
            (raw as usize, false)
        };
        if v >= n {
            return Err(fmt_err(format!(
                "{name}: a polygon names a vertex that does not exist"
            )));
        }
        corners.push(v as u32);
        corner_ids.push(c);
        if !last {
            continue;
        }
        if corners.len() >= 3 {
            if corners.len() > 3 {
                polys += 1;
            }
            let surface = if by_polygon {
                mats.and_then(|m| m.get(polygon)).map_or(0, |&m| m as u32)
            } else {
                0
            };
            for t in triangulate(&solid.positions, &corners) {
                let k = t.map(|v| corners.iter().position(|&x| x == v).unwrap_or(0));
                let t = if mirrored { [t[0], t[2], t[1]] } else { t };
                let k = if mirrored { [k[0], k[2], k[1]] } else { k };
                solid.push_triangle(t, surface, &[name]);
                let uv = k.map(|j| uv_at(corner_ids[j], corners[j] as usize));
                if uv.iter().all(Option::is_some) {
                    uvs.push(uv.map(|u| u.unwrap_or(DVec2::ZERO)));
                } else {
                    all_uv = false;
                    uvs.push([DVec2::ZERO; 3]);
                }
            }
        }
        corners.clear();
        corner_ids.clear();
        polygon += 1;
    }
    Ok((solid, if all_uv { Some(uvs) } else { None }, polys))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::no_files;

    const CUBE: &str = r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {
	FBXVersion: 7400
}
GlobalSettings:  {
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",2
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",1
		P: "FrontAxisSign", "int", "Integer", "",-1
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",0.1
	}
}
Objects:  {
	Geometry: 10, "Geometry::Quad", "Mesh" {
		Vertices: *12 {
			a: 0,0,0,1,0,0,1,1,0,0,1,0
		}
		PolygonVertexIndex: *4 {
			a: 0,1,2,-4
		}
		LayerElementUV: 0 {
			MappingInformationType: "ByPolygonVertex"
			ReferenceInformationType: "IndexToDirect"
			UV: *8 {
				a: 0,0,1,0,1,1,0,1
			}
			UVIndex: *4 {
				a: 0,1,2,3
			}
		}
	}
	Model: 20, "Model::Plate", "Mesh" {
		Properties70:  {
			P: "Lcl Translation", "Lcl Translation", "", "A",0,0,5
			P: "Lcl Rotation", "Lcl Rotation", "", "A",0,0,90
		}
	}
}
Connections:  {
	C: "OO",20,0
	C: "OO",10,20
}
"#;

    #[test]
    fn reads_an_ascii_quad_with_z_up_millimetres() {
        let imported = import_fbx(CUBE.as_bytes(), &no_files).unwrap();
        assert_eq!(imported.parts.len(), 1);
        assert_eq!(imported.polygons_triangulated, 1);
        let p = &imported.parts[0];
        assert_eq!(p.name, "Plate");
        // Units of 0.1 cm (1 mm), Z up: kernel coordinates are the file's.
        let s = crate::apply_transform(p).unwrap();
        let mut pts = s.positions.clone();
        for q in &mut pts {
            *q = DVec3::new(q.x.round(), q.y.round(), q.z.round()) + DVec3::ZERO;
        }
        pts.sort_by(|a, b| shape_core::order::cmp_dvec3(*a, *b));
        // Turned 90° about z, lifted 5 mm.
        assert_eq!(
            pts,
            [
                DVec3::new(-1.0, 0.0, 5.0),
                DVec3::new(-1.0, 1.0, 5.0),
                DVec3::new(0.0, 0.0, 5.0),
                DVec3::new(0.0, 1.0, 5.0),
            ]
        );
        // The quad faces +z still.
        let m = s.measure();
        assert!((m.area - 1.0).abs() < 1e-9);
        let [a, b, c] = s.triangles[0].map(|v| s.positions[v as usize]);
        assert!((b - a).cross(c - a).z > 0.0);
        assert_eq!(p.solid.uvs.as_ref().map(Vec::len), Some(2));
    }

    /// Two legs' worth of FBX skin and animation, as exporters write it:
    /// each cluster's `TransformLink` is its bone in the world at bind
    /// time and its `Transform` the mesh in the bone's space (here the
    /// bone's inverse, the mesh standing at the origin). The knee turns a
    /// quarter turn about y in one second.
    const LEGS: &str = r#"; FBX 7.4.0 project file
FBXHeaderExtension:  {
	FBXVersion: 7400
}
GlobalSettings:  {
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",2
		P: "UpAxisSign", "int", "Integer", "",1
		P: "FrontAxis", "int", "Integer", "",1
		P: "FrontAxisSign", "int", "Integer", "",-1
		P: "CoordAxis", "int", "Integer", "",0
		P: "CoordAxisSign", "int", "Integer", "",1
		P: "UnitScaleFactor", "double", "Number", "",0.1
	}
}
Objects:  {
	Geometry: 10, "Geometry::Leg", "Mesh" {
		Vertices: *12 {
			a: 0,0,0,100,0,0,100,0,500,0,0,500
		}
		PolygonVertexIndex: *4 {
			a: 0,1,2,-4
		}
	}
	Model: 20, "Model::Legs", "Mesh" {
	}
	Model: 30, "Model::hip", "LimbNode" {
	}
	Model: 31, "Model::knee", "LimbNode" {
		Properties70:  {
			P: "Lcl Translation", "Lcl Translation", "", "A",0,0,250
		}
	}
	Deformer: 40, "Deformer::", "Skin" {
	}
	Deformer: 41, "SubDeformer::", "Cluster" {
		Indexes: *2 {
			a: 0,1
		}
		Weights: *2 {
			a: 1,1
		}
		Transform: *16 {
			a: 1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1
		}
		TransformLink: *16 {
			a: 1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1
		}
	}
	Deformer: 42, "SubDeformer::", "Cluster" {
		Indexes: *2 {
			a: 2,3
		}
		Weights: *2 {
			a: 1,1
		}
		Transform: *16 {
			a: 1,0,0,0,0,1,0,0,0,0,1,0,0,0,-250,1
		}
		TransformLink: *16 {
			a: 1,0,0,0,0,1,0,0,0,0,1,0,0,0,250,1
		}
	}
	AnimationStack: 50, "AnimStack::bend", "" {
	}
	AnimationLayer: 51, "AnimLayer::base", "" {
	}
	AnimationCurveNode: 52, "AnimCurveNode::R", "" {
	}
	AnimationCurve: 53, "AnimCurve::", "" {
		KeyTime: *2 {
			a: 0,46186158000
		}
		KeyValueFloat: *2 {
			a: 0,90
		}
	}
}
Connections:  {
	C: "OO",20,0
	C: "OO",10,20
	C: "OO",30,0
	C: "OO",31,30
	C: "OO",40,10
	C: "OO",41,40
	C: "OO",42,40
	C: "OO",30,41
	C: "OO",31,42
	C: "OO",51,50
	C: "OO",52,51
	C: "OP",52,31, "Lcl Rotation"
	C: "OP",53,52, "d|Y"
}
"#;

    #[test]
    fn reads_a_skin_s_bones_where_they_stand_and_its_animation() {
        let imported = import_fbx(LEGS.as_bytes(), &no_files).unwrap();
        let part = &imported.parts[0];
        let skin = part.solid.skin.as_ref().expect("a skin");
        let names: Vec<&str> = skin.bones.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, ["hip", "knee"]);
        // The knee stands at its `TransformLink`, not at twice it (which
        // reading `Transform` as the mesh in the world gave).
        assert_eq!(skin.bones[0].rest.origin, DVec3::ZERO);
        assert_eq!(skin.bones[1].rest.origin, DVec3::new(0.0, 0.0, 250.0));
        assert_eq!(skin.bones[1].parent, Some(0));
        assert_eq!(part.clips.len(), 1);
        let clip = &part.clips[0];
        assert_eq!((clip.name.as_str(), clip.duration()), ("bend", 1.0));
        let top = |time: f64| {
            let i = part
                .solid
                .positions
                .iter()
                .position(|p| (*p - DVec3::new(0.0, 0.0, 500.0)).length() < 1e-9)
                .unwrap();
            let s = shape_skin::pose_by(&part.solid, &clip.at(time).unwrap()).unwrap();
            s.positions[i]
        };
        let near = |a: DVec3, b: DVec3| assert!((a - b).length() < 0.05, "{a:?} {b:?}");
        near(top(0.0), DVec3::new(0.0, 0.0, 500.0));
        // About +y, +z turns towards +x: 45° halfway, a quarter at the end.
        let d = 250.0 * trig::sin(trig::PI / 4.0);
        near(top(0.5), DVec3::new(d, 0.0, 250.0 + d));
        near(top(1.0), DVec3::new(250.0, 0.0, 250.0));
    }
}

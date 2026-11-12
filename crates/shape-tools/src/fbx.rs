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
    local_turned(m, lcl_t, &euler(lcl_r, rotation_order(m)), lcl_s)
}

/// A model's `RotationOrder`.
fn rotation_order(m: &Node) -> i64 {
    m.p70("RotationOrder")
        .and_then(|p| p.first())
        .and_then(Prop::int)
        .unwrap_or(0)
}

/// [`local_with`], the rotation given as a matrix (what layers composed
/// leave).
fn local_turned(m: &Node, lcl_t: DVec3, lcl_r: &Affine, lcl_s: DVec3) -> Affine {
    let v = |k: &str| m.p70_vec(k, DVec3::ZERO);
    let (rp, sp) = (v("RotationPivot"), v("ScalingPivot"));
    let chain = [
        translate(lcl_t),
        translate(v("RotationOffset")),
        translate(rp),
        euler(v("PreRotation"), 0),
        *lcl_r,
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

/// A blend-shape channel's weight in percent: its `DeformPercent`.
fn deform_percent(channel: &&Node) -> f64 {
    channel
        .p70_num("DeformPercent")
        .or(channel
            .child("DeformPercent")
            .and_then(|c| c.props.first())
            .and_then(Prop::num))
        .filter(|w| w.is_finite())
        .unwrap_or(0.0)
}

/// A rotation matrix taken `w` of its way from no turn (`w` in 0..=1):
/// the same axis, the angle scaled.
fn part_turn(r: &Affine, w: f64) -> Affine {
    if w >= 1.0 {
        return *r;
    }
    let trace = r.x.x + r.y.y + r.z.z;
    let angle = trig::acos(((trace - 1.0) * 0.5).clamp(-1.0, 1.0));
    // The axis: the matrix's skew part, or by its diagonal at a half turn.
    let mut axis = DVec3::new(r.y.z - r.z.y, r.z.x - r.x.z, r.x.y - r.y.x);
    if axis.length() < 1e-9 {
        if angle < 1.0 {
            return Affine::IDENTITY;
        }
        axis = DVec3::new(
            ((r.x.x + 1.0) * 0.5).max(0.0).sqrt(),
            ((r.y.y + 1.0) * 0.5).max(0.0).sqrt(),
            ((r.z.z + 1.0) * 0.5).max(0.0).sqrt(),
        );
        if r.x.y + r.y.x < 0.0 {
            axis.y = -axis.y;
        }
        if r.x.z + r.z.x < 0.0 {
            axis.z = -axis.z;
        }
    }
    let Some(n) = axis.try_normalize() else {
        return Affine::IDENTITY;
    };
    // Rodrigues: each basis vector turned about the axis.
    let (s, c) = trig::sin_cos(angle * w);
    let turn = |v: DVec3| v * c + n.cross(v) * s + n * (n.dot(v) * (1.0 - c));
    Affine {
        x: turn(DVec3::X),
        y: turn(DVec3::Y),
        z: turn(DVec3::Z),
        t: DVec3::ZERO,
    }
}

/// A model's translation, rotation and scaling as an animation's layers
/// leave them (`docs/research.md` §45).
struct Layered {
    t: DVec3,
    /// Euler degrees, until a layer composes a turn onto them.
    r: DVec3,
    /// The rotation once layers were composed as turns.
    turned: Option<Affine>,
    s: DVec3,
}

/// One layer of an animation stack: how it lies on those under it.
struct Layer {
    /// 0 adds, 2 mixes towards its value by its weight, others replace.
    mode: i64,
    /// 0..=1.
    weight: f64,
    /// Rotations add as turns (not angle by angle); scalings multiply.
    turns: bool,
    multiplies: bool,
    /// Per model: its translation, rotation and scaling curves, an axis
    /// each, and the value an axis without a curve has in this layer.
    driven: BTreeMap<i64, [Channel; 3]>,
    /// Per blend-shape channel: its `DeformPercent` curve, and its value
    /// in this layer without one.
    shapes: BTreeMap<i64, (Option<Curve>, f64)>,
}

/// A property's three curves in a layer, and its values without them.
#[derive(Default)]
struct Channel {
    on: bool,
    curves: [Option<Curve>; 3],
    rest: [f64; 3],
}

impl Layered {
    /// Lays `value` of property `slot` (0 translation, 1 rotation, 2
    /// scaling) on, as `layer` does; the stack's first layer replaces.
    fn lay(&mut self, first: bool, layer: &Layer, slot: usize, value: DVec3, order: i64) {
        let w = layer.weight;
        let mode = if first { 1 } else { layer.mode };
        let power = |v: f64, e: f64| {
            if e <= 0.0 {
                1.0
            } else if e >= 1.0 {
                v
            } else {
                trig::pow(v.abs(), e) * if v < 0.0 { -1.0 } else { 1.0 }
            }
        };
        match (slot, mode) {
            (0, 0) => self.t += value * w,
            (0, 2) => self.t = self.t * (1.0 - w) + value * w,
            (0, _) => self.t = value,
            (2, 0) if layer.multiplies => {
                self.s *= DVec3::new(power(value.x, w), power(value.y, w), power(value.z, w));
            }
            (2, 0) => self.s += value * w,
            (2, 2) if layer.multiplies => {
                let mix = |a: f64, b: f64| power(a, 1.0 - w) * power(b, w);
                self.s = DVec3::new(
                    mix(self.s.x, value.x),
                    mix(self.s.y, value.y),
                    mix(self.s.z, value.z),
                );
            }
            (2, 2) => self.s = self.s * (1.0 - w) + value * w,
            (2, _) => self.s = value,
            (_, 0 | 2) if layer.turns => {
                let under = self.turned.unwrap_or_else(|| euler(self.r, order));
                let over = euler(value, order);
                // Adding: the layer's turn, part of it, after those
                // under. Mixing: part of the way from those to it.
                let step = if mode == 0 {
                    part_turn(&over, w)
                } else {
                    part_turn(&transpose(&under).then(&over), w)
                };
                self.turned = Some(under.then(&step));
            }
            (_, 0) => self.r += value * w,
            (_, 2) => self.r = self.r * (1.0 - w) + value * w,
            _ => {
                self.r = value;
                self.turned = None;
            }
        }
    }
}

/// How a curve runs from one key to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Ease {
    /// The key's value, until the next.
    Hold,
    /// The next key's value, from this one on.
    HoldNext,
    /// Straight.
    Line,
    /// A cubic Bézier in time and value, by the handles either side.
    Cubic,
}

/// One number of a model's property over time (an `AnimationCurve`): its
/// keys, and for each how it runs to the next (`docs/research.md` §45).
#[derive(Clone, Debug)]
struct Curve {
    /// Key times, seconds, rising.
    times: Vec<f64>,
    values: Vec<f64>,
    ease: Vec<Ease>,
    /// Per key, the handle leaving it: seconds and value from the key.
    right: Vec<(f64, f64)>,
    /// Per key, the handle arriving at it: seconds and value before it.
    left: Vec<(f64, f64)>,
}

const KEY_CONSTANT: i64 = 0x2;
const KEY_CUBIC: i64 = 0x8;
const KEY_AUTO_OR_NEXT: i64 = 0x100;
const KEY_TCB: i64 = 0x200;
const KEY_USER: i64 = 0x400;
const KEY_CLAMP: i64 = 0x1000;
const KEY_TIME_INDEPENDENT: i64 = 0x2000;
const KEY_CLAMP_PROGRESSIVE: i64 = 0x4000;
const KEY_WEIGHTED_RIGHT: i64 = 0x100_0000;
const KEY_WEIGHTED_NEXT_LEFT: i64 = 0x200_0000;

impl Curve {
    /// The curve of `times` and `values` with the file's key attributes:
    /// `flags` and four numbers of `data` for each run of keys that
    /// `refs` counts. Without them every key runs straight to the next.
    fn new(times: Vec<f64>, values: Vec<f64>, flags: &[f64], data: &[f64], refs: &[f64]) -> Curve {
        let n = times.len();
        let third = 1.0 / 3.0;
        let mut c = Curve {
            ease: vec![Ease::Line; n],
            right: vec![(0.0, 0.0); n],
            left: vec![(0.0, 0.0); n],
            times,
            values,
        };
        // Each key's run of attributes.
        let mut run_of = Vec::with_capacity(n);
        for (run, count) in refs.iter().enumerate() {
            for _ in 0..(*count).max(0.0) as usize {
                if run_of.len() < n {
                    run_of.push(run);
                }
            }
        }
        if run_of.len() < n || flags.len() < refs.len() || data.len() < refs.len() * 4 {
            run_of.clear();
        }
        // What the key before left for this key's arriving handle.
        let (mut slope_left, mut weight_left) = (0.0f64, third);
        for i in 0..n {
            let (t, v) = (c.times[i], c.values[i]);
            let before = (i > 0 && t > c.times[i - 1]).then(|| (c.times[i - 1], c.values[i - 1]));
            let after =
                (i + 1 < n && c.times[i + 1] > t).then(|| (c.times[i + 1], c.values[i + 1]));
            let chord = after.map_or(0.0, |(nt, nv)| (nv - v) / (nt - t));
            let (flag, attr) = match run_of.get(i) {
                Some(&run) => (flags[run] as i64, &data[run * 4..run * 4 + 4]),
                None => (0, &[0.0; 4][..]),
            };
            let (mut slope_right, mut next_slope_left) = (attr[0], attr[1]);
            let (mut weight_right, mut next_weight_left) = (third, third);
            // Weights: two decimal fixed-point numbers in the third
            // number's bits.
            let packed = (attr[2] as f32).to_bits();
            if flag & KEY_WEIGHTED_RIGHT != 0 {
                weight_right = f64::from(packed & 0xffff) * 0.0001;
            }
            if flag & KEY_WEIGHTED_NEXT_LEFT != 0 {
                next_weight_left = f64::from(packed >> 16) * 0.0001;
            }
            if flag & KEY_CONSTANT != 0 {
                c.ease[i] = if flag & KEY_AUTO_OR_NEXT != 0 {
                    Ease::HoldNext
                } else {
                    Ease::Hold
                };
                (weight_right, next_weight_left) = (third, third);
                (slope_right, next_slope_left) = (0.0, 0.0);
            } else if flag & KEY_CUBIC != 0 {
                c.ease[i] = Ease::Cubic;
                if flag & KEY_TCB != 0 {
                    // Tension, continuity and bias mix the chords either side.
                    let (tension, continuity, bias) = (attr[0], attr[1], attr[2]);
                    let chord_left = before.map_or(0.0, |(pt, pv)| (v - pv) / (t - pt));
                    let factor = if before.is_some() && after.is_some() {
                        0.5
                    } else {
                        1.0
                    };
                    let k = factor * (1.0 - tension);
                    slope_left = k
                        * ((1.0 + bias) * (1.0 - continuity) * chord_left
                            + (1.0 - bias) * (1.0 + continuity) * chord);
                    slope_right = k
                        * ((1.0 + bias) * (1.0 + continuity) * chord_left
                            + (1.0 - bias) * (1.0 - continuity) * chord);
                    next_slope_left = 0.0;
                    next_weight_left = third;
                } else if flag & KEY_USER == 0 {
                    // Automatic: the slope through the keys either side;
                    // what the file stores is a bias.
                    let auto = |bias: f64| -> f64 {
                        match (before, after) {
                            (Some((pt, pv)), Some((nt, nv))) => {
                                if flag & KEY_CLAMP != 0 && (pv == v || nv == v) {
                                    return 0.0;
                                }
                                let mut slope = (nv - pv) / (nt - pt);
                                let (sl, sr) = ((v - pv) / (t - pt), (nv - v) / (nt - t));
                                if flag & KEY_TIME_INDEPENDENT == 0 {
                                    let delta = (t - pt) / (nt - pt);
                                    slope = slope * 0.5 + (sl * (1.0 - delta) + sr * delta) * 0.5;
                                    let w = bias.abs() / 100.0;
                                    if w > 0.0001 {
                                        let target = if bias > 0.0 { sr } else { sl };
                                        let toward = target - slope;
                                        slope = slope * (1.0 - w) + target * w;
                                        if w > 5.0 {
                                            let sign =
                                                if toward.abs() > 0.00001 { toward } else { bias };
                                            let sign = if sign > 0.0 { 1.0 } else { -1.0 };
                                            slope += (w - 5.0) * (w - 5.0) * sign * 40.0;
                                        }
                                    }
                                }
                                if flag & KEY_CLAMP_PROGRESSIVE != 0 {
                                    // Neither handle passes its neighbour's value.
                                    let sign = if slope >= 0.0 { 1.0 } else { -1.0 };
                                    let reach_left = weight_left * (t - pt);
                                    let reach_right = weight_right * (nt - t);
                                    let mut most = slope * sign;
                                    for limit in [
                                        sign * (v - pv) / reach_left,
                                        sign * (nv - v) / reach_right,
                                    ] {
                                        most = most.min(if limit > 0.0 { limit } else { 0.0 });
                                    }
                                    slope = sign * most;
                                }
                                slope
                            }
                            (side_before, side_after) => {
                                let Some((ot, ov)) = side_before.or(side_after) else {
                                    return 0.0;
                                };
                                if flag & KEY_CLAMP_PROGRESSIVE != 0
                                    || (flag & KEY_CLAMP != 0 && ov == v)
                                {
                                    return 0.0;
                                }
                                let mut slope = (ov - v) / (ot - t);
                                let w = bias.abs() / 100.0 - 5.0;
                                if flag & KEY_TIME_INDEPENDENT == 0 && w > 0.0 {
                                    slope += w * w * if bias > 0.0 { 40.0 } else { -40.0 };
                                }
                                slope
                            }
                        }
                    };
                    if before.is_some()
                        && after.is_some()
                        && (slope_left + slope_right).abs() > 0.0001
                    {
                        (slope_left, slope_right) = (auto(-slope_left), auto(slope_right));
                    } else if before.is_some() && after.is_none() {
                        slope_left = auto(-slope_left);
                        slope_right = slope_left;
                    } else {
                        slope_right = auto(slope_right);
                        slope_left = slope_right;
                    }
                }
            } else {
                (weight_right, next_weight_left) = (third, third);
                (slope_right, next_slope_left) = (chord, chord);
            }
            if let Some((pt, _)) = before {
                let dx = weight_left * (t - pt);
                c.left[i] = (dx, dx * slope_left);
            }
            if let Some((nt, _)) = after {
                let dx = weight_right * (nt - t);
                c.right[i] = (dx, dx * slope_right);
            }
            (slope_left, weight_left) = (next_slope_left, next_weight_left);
        }
        c
    }

    /// The value at `time`: the first key's before it, the last key's
    /// after it.
    fn at(&self, time: f64) -> f64 {
        let (t, v) = (&self.times, &self.values);
        let k = t.partition_point(|&x| x <= time);
        if k == 0 {
            return v[0];
        }
        if k >= t.len() || t[k - 1] == time {
            return v[k - 1];
        }
        let span = t[k] - t[k - 1];
        let x = (time - t[k - 1]) / span;
        match self.ease[k - 1] {
            Ease::Hold => v[k - 1],
            Ease::HoldNext => v[k],
            Ease::Line => v[k - 1] + (v[k] - v[k - 1]) * x,
            Ease::Cubic => {
                // Where along the Bézier the time is `x`: its time runs
                // 0, x1, x2, 1 and rises, so halving finds it.
                let x1 = self.right[k - 1].0 / span;
                let x2 = 1.0 - self.left[k].0 / span;
                let time_at = |s: f64| {
                    let u = 1.0 - s;
                    3.0 * (u * u * s * x1 + u * s * s * x2) + s * s * s
                };
                let (mut lo, mut hi) = (0.0f64, 1.0f64);
                // Handles a third of the way each: the time is the place.
                let even = (x1 - 1.0 / 3.0).abs() < 1e-9 && (x2 - 2.0 / 3.0).abs() < 1e-9;
                if even {
                    (lo, hi) = (x, x);
                }
                for _ in 0..if even { 0 } else { 60 } {
                    let mid = (lo + hi) * 0.5;
                    if time_at(mid) < x {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let s = (lo + hi) * 0.5;
                let u = 1.0 - s;
                let (y0, y3) = (v[k - 1], v[k]);
                let (y1, y2) = (y0 + self.right[k - 1].1, y3 - self.left[k].1);
                u * u * u * y0 + 3.0 * (u * u * s * y1 + u * s * s * y2) + s * s * s * y3
            }
        }
    }
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
        if o.name == "Deformer"
            && !["Skin", "Cluster", "BlendShape", "BlendShapeChannel"].contains(&class)
        {
            shapes += 1;
        }
        if o.name == "Geometry" && !["Mesh", "Shape"].contains(&class) {
            out.skipped.push(format!(
                "geometry {:?} of kind {:?}",
                object_name(o.props.get(1).and_then(Prop::str).unwrap_or("")),
                o.props.get(2).and_then(Prop::str).unwrap_or("")
            ));
        }
    }
    if shapes > 0 {
        out.skipped.push(format!(
            "{shapes} deformers that are neither skins nor blend shapes: meshes read without them"
        ));
    }
    // A geometry's blend shapes: per channel a target (and how many
    // shapes the channel has), and the channels' ids in that order.
    let shapes_of = |g: &Node,
                     turned: &dyn Fn(DVec3) -> DVec3,
                     vertices: usize|
     -> (Vec<(shape_mesh::Morph, usize)>, Vec<i64>) {
        let class = |n: &Node| n.props.get(2).and_then(Prop::str).unwrap_or("").to_string();
        let id_of = |n: &Node| n.props.first().and_then(Prop::int);
        let under = |id: Option<i64>, name: &'static str, kind: &'static str| -> Vec<&Node> {
            id.and_then(|id| children_of.get(&id))
                .into_iter()
                .flatten()
                .filter_map(|c| by_id.get(c))
                .filter(|n| n.name == name && class(n) == kind)
                .copied()
                .collect()
        };
        let (mut targets, mut ids) = (Vec::new(), Vec::new());
        for deformer in under(id_of(g), "Deformer", "BlendShape") {
            for channel in under(id_of(deformer), "Deformer", "BlendShapeChannel") {
                let shapes = under(id_of(channel), "Geometry", "Shape");
                // The shape at the channel's full weight: the last, or
                // the one `FullWeights` gives the most.
                let full = channel
                    .array("FullWeights")
                    .filter(|w| w.len() == shapes.len())
                    .and_then(|w| (0..w.len()).rev().max_by(|a, b| w[*a].total_cmp(&w[*b])))
                    .unwrap_or(shapes.len().saturating_sub(1));
                let (Some(shape), Some(cid)) = (shapes.get(full), id_of(channel)) else {
                    continue;
                };
                let mut deltas = vec![DVec3::ZERO; vertices];
                let index = shape.array("Indexes").unwrap_or(&[]);
                let offsets = shape.array("Vertices").unwrap_or(&[]);
                for (i, d) in index.iter().zip(offsets.as_chunks::<3>().0) {
                    let d = turned(DVec3::new(d[0], d[1], d[2]));
                    if let Some(slot) = deltas.get_mut(*i as usize).filter(|_| d.is_finite()) {
                        *slot = d;
                    }
                }
                targets.push((
                    shape_mesh::Morph {
                        name: object_name(channel.props.get(1).and_then(Prop::str).unwrap_or("")),
                        weight: deform_percent(&channel) / 100.0,
                        deltas,
                    },
                    shapes.len(),
                ));
                ids.push(cid);
            }
        }
        (targets, ids)
    };
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
    // stack's first layer is read; a curve's easing between its keys
    // (`Curve`) is in the samples.
    let kids = |id: i64, name: &str| -> Vec<i64> {
        children_of
            .get(&id)
            .into_iter()
            .flatten()
            .copied()
            .filter(|c| by_id.get(c).is_some_and(|n| n.name == name))
            .collect()
    };
    // Per stack: its name and its layers, the first undermost.
    let mut stacks: Vec<(String, Vec<Layer>)> = Vec::new();
    for o in objects.all("AnimationStack") {
        let Some(sid) = o.props.first().and_then(Prop::int) else {
            continue;
        };
        let sname = object_name(o.props.get(1).and_then(Prop::str).unwrap_or(""));
        let mut layers = Vec::new();
        for lid in kids(sid, "AnimationLayer") {
            let first = layers.is_empty();
            let node = by_id.get(&lid);
            let int = |key: &str| {
                node.and_then(|n| n.p70(key))
                    .and_then(|p| p.first())
                    .and_then(Prop::int)
                    .unwrap_or(0)
            };
            let mut layer = Layer {
                mode: int("BlendMode"),
                weight: node
                    .and_then(|n| n.p70_num("Weight"))
                    .map_or(1.0, |w| (w / 100.0).clamp(0.0, 1.0)),
                turns: int("RotationAccumulationMode") == 0,
                multiplies: int("ScaleAccumulationMode") == 0,
                driven: BTreeMap::new(),
                shapes: BTreeMap::new(),
            };
            for cn in kids(lid, "AnimationCurveNode") {
                // A blend-shape channel's weight, in percent.
                let channel = op_links.iter().find_map(|(c, p, prop)| {
                    (*c == cn
                        && prop == "DeformPercent"
                        && by_id.get(p).is_some_and(|n| n.name == "Deformer"))
                    .then_some(*p)
                });
                if let Some(channel) = channel {
                    let own = by_id.get(&channel).map_or(0.0, deform_percent);
                    let rest = by_id
                        .get(&cn)
                        .and_then(|n| n.p70_num("d|DeformPercent"))
                        .unwrap_or(if first { own } else { 0.0 });
                    let mut found = None;
                    for (c, p, prop) in &op_links {
                        if *p != cn || prop != "d|DeformPercent" {
                            continue;
                        }
                        let Some(curve) = by_id.get(c).filter(|n| n.name == "AnimationCurve")
                        else {
                            continue;
                        };
                        let (Some(t), Some(v)) =
                            (curve.array("KeyTime"), curve.array("KeyValueFloat"))
                        else {
                            continue;
                        };
                        if t.is_empty() || t.len() != v.len() || t.windows(2).any(|w| w[1] < w[0]) {
                            return Err(fmt_err(format!(
                                "animation {sname:?}: a curve's times and values do not match or do not rise"
                            )));
                        }
                        found = Some(Curve::new(
                            t.iter().map(|x| x / 46_186_158_000.0).collect(),
                            v.to_vec(),
                            curve.array("KeyAttrFlags").unwrap_or(&[]),
                            curve.array("KeyAttrDataFloat").unwrap_or(&[]),
                            curve.array("KeyAttrRefCount").unwrap_or(&[]),
                        ));
                    }
                    layer.shapes.insert(channel, (found, rest));
                    continue;
                }
                // The model property this curve node drives.
                let Some((model, slot)) = op_links.iter().find_map(|(c, p, prop)| {
                    let slot = match prop.as_str() {
                        "Lcl Translation" => 0,
                        "Lcl Rotation" => 1,
                        "Lcl Scaling" => 2,
                        _ => return None,
                    };
                    (*c == cn && by_id.get(p).is_some_and(|n| n.name == "Model"))
                        .then_some((*p, slot))
                }) else {
                    continue;
                };
                let channel = &mut layer.driven.entry(model).or_default()[slot];
                channel.on = true;
                // What an axis without a curve has in this layer.
                for (axis, key) in ["d|X", "d|Y", "d|Z"].into_iter().enumerate() {
                    // The undermost layer, where the file says nothing:
                    // the model's own value.
                    let own = by_id.get(&model).filter(|_| first).map(|m| {
                        let key = ["Lcl Translation", "Lcl Rotation", "Lcl Scaling"][slot];
                        m.p70_vec(key, if slot == 2 { DVec3::ONE } else { DVec3::ZERO })[axis]
                    });
                    channel.rest[axis] = by_id
                        .get(&cn)
                        .and_then(|n| n.p70_num(key))
                        .or(own)
                        .unwrap_or(if slot == 2 { 1.0 } else { 0.0 });
                }
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
                    layer.driven.entry(model).or_default()[slot].curves[axis] = Some(Curve::new(
                        seconds,
                        v.to_vec(),
                        curve.array("KeyAttrFlags").unwrap_or(&[]),
                        curve.array("KeyAttrDataFloat").unwrap_or(&[]),
                        curve.array("KeyAttrRefCount").unwrap_or(&[]),
                    ));
                }
            }
            layers.push(layer);
        }
        if layers
            .iter()
            .any(|l| !l.driven.is_empty() || !l.shapes.is_empty())
        {
            stacks.push((sname, layers));
        }
    }
    // `channels`: the part's blend-shape channels, in its targets' order.
    let clips_of = |space: &FbxSpace,
                    channels: &[i64]|
     -> Result<Vec<shape_mesh::SkinClip>, ImportError> {
        let mut clips = Vec::new();
        let mut used = BTreeMap::new();
        for (sname, layers) in &stacks {
            let mut times: Vec<f64> = layers
                .iter()
                .flat_map(|l| l.driven.values())
                .flatten()
                .flat_map(|ch| ch.curves.iter().flatten())
                .chain(
                    layers
                        .iter()
                        .flat_map(|l| l.shapes.values())
                        .filter_map(|s| s.0.as_ref()),
                )
                .flat_map(|c| c.times.iter().copied())
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
                    if !layers.iter().any(|l| l.driven.contains_key(&id)) {
                        return local(node);
                    }
                    let order = rotation_order(node);
                    let mut at = Layered {
                        t: node.p70_vec("Lcl Translation", DVec3::ZERO),
                        r: node.p70_vec("Lcl Rotation", DVec3::ZERO),
                        turned: None,
                        s: node.p70_vec("Lcl Scaling", DVec3::ONE),
                    };
                    for (k, layer) in layers.iter().enumerate() {
                        let Some(channels) = layer.driven.get(&id) else {
                            continue;
                        };
                        for (slot, ch) in channels.iter().enumerate().filter(|(_, ch)| ch.on) {
                            let mut v = DVec3::from_array(ch.rest);
                            for axis in 0..3 {
                                if let Some(c) = &ch.curves[axis] {
                                    v[axis] = c.at(time);
                                }
                            }
                            at.lay(k == 0, layer, slot, v, order);
                        }
                    }
                    let turn = at.turned.unwrap_or_else(|| euler(at.r, order));
                    local_turned(node, at.t, &turn, at.s)
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
            // Each channel's weight at each time, through the layers.
            let mut morphs = Vec::new();
            let driven = |c: &i64| layers.iter().any(|l| l.shapes.contains_key(c));
            if channels.iter().any(driven) {
                for &time in &times {
                    let row = channels
                        .iter()
                        .map(|c| {
                            let mut at = Layered {
                                t: DVec3::new(by_id.get(c).map_or(0.0, deform_percent), 0.0, 0.0),
                                r: DVec3::ZERO,
                                turned: None,
                                s: DVec3::ONE,
                            };
                            for (k, layer) in layers.iter().enumerate() {
                                if let Some((curve, rest)) = layer.shapes.get(c) {
                                    let v = curve.as_ref().map_or(*rest, |x| x.at(time));
                                    at.lay(k == 0, layer, 0, DVec3::new(v, 0.0, 0.0), 0);
                                }
                            }
                            at.t.x / 100.0
                        })
                        .collect();
                    morphs.push(row);
                }
            }
            clips.push(shape_mesh::SkinClip {
                morphs,
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
            // Blend shapes: each channel a morph target, its last shape's
            // offsets (control points the shape does not list stay).
            let (mut targets, channels) = shapes_of(
                g,
                &|d: DVec3| to_kernel(geo.rotate(d)) * unit,
                solid.positions.len(),
            );
            if !channels.is_empty() {
                let lost = targets.iter().filter(|t| t.1 > 1).count();
                if lost > 0 {
                    out.skipped.push(format!(
                        "{name}: {lost} blend-shape channels have in-between shapes; the full one is read"
                    ));
                }
            }
            let read = skin_of(g, &name, &solid.positions)?;
            let mut clips = Vec::new();
            if let Some(((skin, scaled), space)) = read {
                clips = clips_of(&space, &channels)?;
                if let Some((placed, info)) = scaled {
                    // Where the file's bind pose puts the vertices, and
                    // each target's ends with them.
                    for (t, _) in &mut targets {
                        let moved: Vec<DVec3> = solid
                            .positions
                            .iter()
                            .zip(&t.deltas)
                            .map(|(p, d)| *p + *d)
                            .collect();
                        if let Some(((_, Some((ends, _))), _)) = skin_of(g, &name, &moved)? {
                            for ((d, e), p) in t.deltas.iter_mut().zip(&ends).zip(&placed) {
                                *d = *e - *p;
                            }
                        }
                    }
                    solid.positions = placed;
                    out.skin_scales.push(info);
                }
                solid.skin = Some(skin);
            } else if !channels.is_empty() {
                // No bones: the clips drive the targets alone.
                let none = FbxSpace {
                    ids: Vec::new(),
                    unlink: Vec::new(),
                    mesh: Vec::new(),
                    out: Affine::IDENTITY,
                };
                clips = clips_of(&none, &channels)?;
            }
            if !targets.is_empty() {
                let mut names = BTreeMap::new();
                solid.morphs = Some(shape_mesh::Morphs {
                    targets: targets
                        .into_iter()
                        .map(|(mut t, _)| {
                            t.name = unique(&t.name, &mut names);
                            t
                        })
                        .collect(),
                });
            }
            let mut transform = conj(&world(id));
            // A mesh with no skin that an animation moves by its model
            // (or a model above it) is carried as a skin of one bone, as
            // a glTF mesh moved by its node is (`docs/research.md` §47).
            let driven = {
                let mut at = id;
                let mut found = false;
                for _ in 0..256 {
                    if stacks
                        .iter()
                        .any(|(_, layers)| layers.iter().any(|l| l.driven.contains_key(&at)))
                    {
                        found = true;
                        break;
                    }
                    match parent_of.get(&at) {
                        Some(&p) if p != 0 => at = p,
                        _ => break,
                    }
                }
                found
            };
            let rest = crate::import::affine_inverse(&transform)
                .filter(|_| driven && solid.skin.is_none() && transform.determinant() > 0.0);
            if let Some(unlink) = rest {
                let space = FbxSpace {
                    ids: vec![id],
                    unlink: vec![unlink],
                    mesh: vec![Affine::IDENTITY],
                    out: Affine::IDENTITY,
                };
                let moved = clips_of(&space, &channels)?;
                let placed = crate::repair::apply_transform(&ImportedPart {
                    name: name.clone(),
                    solid: solid.clone(),
                    transform,
                    ..Default::default()
                });
                if let (false, Ok(mut placed)) = (moved.is_empty(), placed) {
                    placed.skin = Some(shape_mesh::Skin {
                        bones: vec![shape_mesh::Bone::new(
                            &format!("{name}.node"),
                            None,
                            shape_core::Frame::IDENTITY,
                        )],
                        weights: vec![shape_mesh::Weights::single(0); placed.positions.len()],
                    });
                    solid = placed;
                    clips = moved;
                    transform = Affine::IDENTITY;
                    out.skipped.push(format!(
                        "{name}: moved by its model's animation, so carried as a skin of one bone ({name}.node)"
                    ));
                }
            } else if solid.skin.is_some() {
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

    /// A second layer on the knee's bend (a steady 30° about the same
    /// axis) lies on the first as its blend mode and weight say: added
    /// (120° at the end), added at half weight (105°), replacing (30°),
    /// and mixed half way (60°); added angle by angle it is 120° too.
    #[test]
    fn an_animation_s_layers_lie_on_each_other() {
        let with_layer = |props: &str| {
            let objects = format!(
                "\tAnimationLayer: 60, \"AnimLayer::over\", \"\" {{\n\t\tProperties70:  {{\n{props}\t\t}}\n\t}}\n\
                 \tAnimationCurveNode: 61, \"AnimCurveNode::R\", \"\" {{\n\t}}\n\
                 \tAnimationCurve: 62, \"AnimCurve::\", \"\" {{\n\t\tKeyTime: *2 {{\n\t\t\ta: 0,46186158000\n\t\t}}\n\
                 \t\tKeyValueFloat: *2 {{\n\t\t\ta: 30,30\n\t\t}}\n\t}}\n}}\nConnections:  {{"
            );
            let links = "\tC: \"OP\",53,52, \"d|Y\"\n\tC: \"OO\",60,50\n\tC: \"OO\",61,60\n\
                         \tC: \"OP\",61,31, \"Lcl Rotation\"\n\tC: \"OP\",62,61, \"d|Y\"\n";
            let text = LEGS
                .replace("}\nConnections:  {", &objects)
                .replace("\tC: \"OP\",53,52, \"d|Y\"\n", links);
            assert_ne!(text.len(), LEGS.len());
            text
        };
        let p = |name: &str, kind: &str, value: i64| {
            format!("\t\t\tP: \"{name}\", \"{kind}\", \"\", \"\",{value}\n")
        };
        let cases = [
            (String::new(), 120.0),
            (p("Weight", "Number", 50), 105.0),
            (p("BlendMode", "enum", 1), 30.0),
            (p("BlendMode", "enum", 2) + &p("Weight", "Number", 50), 60.0),
            (p("RotationAccumulationMode", "enum", 1), 120.0),
        ];
        for (props, degrees) in cases {
            let imported = import_fbx(with_layer(&props).as_bytes(), &no_files).unwrap();
            assert!(
                imported.skipped.iter().all(|s| !s.contains("layers")),
                "{:?}",
                imported.skipped
            );
            let part = &imported.parts[0];
            let i = part
                .solid
                .positions
                .iter()
                .position(|p| (*p - DVec3::new(0.0, 0.0, 500.0)).length() < 1e-9)
                .unwrap();
            let clip = &part.clips[0];
            let top = shape_skin::pose_by(&part.solid, &clip.at(1.0).unwrap())
                .unwrap()
                .positions[i];
            let (sin, cos) = trig::sin_cos(trig::to_radians(degrees));
            let want = DVec3::new(250.0 * sin, 0.0, 250.0 + 250.0 * cos);
            assert!(
                (top - want).length() < 0.05,
                "{props:?}: {top:?} for {want:?}"
            );
        }
    }

    /// The first curve of a file that moves, as [`Curve`] reads it.
    fn moving_curve(bytes: &[u8]) -> Curve {
        fn find(n: &Node) -> Option<Curve> {
            if n.name == "AnimationCurve" {
                let v = n.array("KeyValueFloat")?;
                if v.iter().any(|x| (x - v[0]).abs() > 1e-3) {
                    let t = n.array("KeyTime")?;
                    return Some(Curve::new(
                        t.iter().map(|x| x / 46_186_158_000.0).collect(),
                        v.to_vec(),
                        n.array("KeyAttrFlags")?,
                        n.array("KeyAttrDataFloat")?,
                        n.array("KeyAttrRefCount")?,
                    ));
                }
            }
            n.children.iter().find_map(find)
        }
        find(&parse_binary(bytes).unwrap().1).unwrap()
    }

    /// Curves Maya wrote, against the values Maya gives them at 24 frames
    /// a second (ufbx's test files and tables, `third_party/ufbx`): keys
    /// that hold, run straight, and ease by automatic, clamped, broken
    /// and weighted tangents.
    #[test]
    fn eased_curves_are_what_maya_evaluates() {
        macro_rules! file {
            ($name:literal) => {
                include_bytes!(concat!("../../../third_party/ufbx/data/", $name)).as_slice()
            };
        }
        #[rustfmt::skip]
        let modes = [
            -8.653366,
            -8.653366,-8.602998,-8.464664,-8.257528,-8.00075,-7.713489,-7.414906,-7.124163,-6.86042,
            -6.642837,-6.490576,-6.388305,-6.306414,-6.242637,-6.19471,-6.160368,-6.137348,-6.123385,
            -6.116215,-6.113573,-6.113196,-5.969524,-5.825851,-5.682179,-5.538507,-5.394835,-5.251163,
            -5.107491,-4.963819,-4.820146,-4.676474,-4.532802,-4.38913,-4.245458,-4.101785,-3.958113,-4.1529,
            -4.347686,-4.542472,-4.737258,-4.932045,-5.126832,-5.321618,-5.516404,-5.71119,-5.905977,-5.767788,
            -5.315578,-4.954943,-4.83559,-4.856855,-4.960766,-5.118543,-4.976541,-4.885909,-4.865979,-4.93845,
            -5.099224,-5.270246,-5.359269,-5.349404,-5.261964,-5.118543,-5.264501,-5.33535,-5.285445,-5.058857,
            -4.69383,-4.357775,-4.124978,-3.981697,-3.904232,-3.875225,-3.875225,-3.875225,-3.875225,-3.875225,
            -3.875225,-3.875225,-2.942738,-2.942738,-2.942738,-2.942738,-2.942738,-2.942738,-2.942738,-2.942738,
            -2.942738,-1.243537,-1.243537,-1.243537,-1.243537,-1.243537,-1.243537,-1.243537,5.603338,5.603338,
            5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,
            5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,5.603338,
        ];
        #[rustfmt::skip]
        let clamp = [
            0.000, 0.000, 0.273, 0.515, 0.718, 0.868, 0.945, 0.920, 0.779, 0.611,
            0.591, 0.747, 1.206, 2.059, 3.191, 4.489, 5.837, 7.121, 8.228, 9.042,
            9.449, 9.694, 10.128, 10.610, 10.873, 10.927, 10.854, 10.704, 10.502,
            10.264, 10.000,
        ];
        #[rustfmt::skip]
        let spline = [
            0.00000, 3.50000, 6.00000, 6.51562, 6.37500, 6.04688, 6.00000,
            6.67313, 7.71167, 8.39438, 8.00000, 3.63667, -1.04000, -1.96474,
            -1.68059, -1.00000, 0.27700, 2.00000,
        ];
        // The first table is read a moment after each frame, so a key
        // that holds has given way to the next.
        let cases: [(&[u8], &[f64], f64, f64); 3] = [
            (
                file!("maya_interpolation_modes_7500_binary.fbx"),
                &modes,
                1e-6,
                1e-4,
            ),
            (file!("maya_auto_clamp_7700_binary.fbx"), &clamp, 0.0, 1e-3),
            (
                file!("maya_tangent_spline_7700_binary.fbx"),
                &spline,
                0.0,
                1e-4,
            ),
        ];
        for (k, (bytes, values, after, within)) in cases.into_iter().enumerate() {
            let curve = moving_curve(bytes);
            for (frame, want) in values.iter().enumerate() {
                let got = curve.at(frame as f64 / 24.0 + after);
                assert!(
                    (got - want).abs() < within,
                    "case {k}, frame {frame}: {got} for {want}"
                );
            }
        }
    }

    /// A cube Maya gave two blend shapes and animated (ufbx's test file
    /// and its expected values, `third_party/ufbx`): each channel is a
    /// morph target with the offsets and rest weight Maya has, and the
    /// clip drives the two weights as Maya evaluates them.
    #[test]
    fn blend_shapes_are_morph_targets_with_maya_s_weights() {
        let bytes =
            include_bytes!("../../../third_party/ufbx/data/maya_blend_shape_cube_7700_binary.fbx");
        let imported = import_fbx(bytes, &no_files).unwrap();
        assert!(imported.skipped.is_empty(), "{:?}", imported.skipped);
        let part = &imported.parts[0];
        let m = part.solid.morphs.as_ref().expect("targets");
        m.check(part.solid.positions.len()).unwrap();
        let names: Vec<&str> = m.targets.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Squish.TopH", "Squish.TopV"]);
        assert!(m.targets.iter().all(|t| (t.weight - 1.0).abs() < 1e-6));
        // The file's centimetres, Y up: 0.317 along x is 3.17 mm along
        // x, and 0.284 along z is 2.84 mm along −y.
        let (h, v) = (3.17, 2.84);
        let want = [
            [
                [0.0; 3],
                [0.0; 3],
                [h, 0.0, 0.0],
                [-h, 0.0, 0.0],
                [h, 0.0, 0.0],
                [-h, 0.0, 0.0],
                [0.0; 3],
                [0.0; 3],
            ],
            [
                [0.0; 3],
                [0.0; 3],
                [0.0, v, 0.0],
                [0.0, v, 0.0],
                [0.0, -v, 0.0],
                [0.0, -v, 0.0],
                [0.0; 3],
                [0.0; 3],
            ],
        ];
        for (t, want) in m.targets.iter().zip(want) {
            for (d, w) in t.deltas.iter().zip(want) {
                assert!(
                    (*d - DVec3::from_array(w)).length() < 0.01,
                    "{}: {d:?} for {w:?}",
                    t.name
                );
            }
        }
        // Maya's weights at frames 16 and 53 (the clip starts at frame 1).
        let clip = &part.clips[0];
        for (frame, want) in [
            (1.0, [1.0, 1.0]),
            (16.0, [0.279, 0.670]),
            (53.0, [0.901, 0.168]),
        ] {
            let got = clip.weights_at((frame - 1.0) / 24.0).unwrap();
            for k in 0..2 {
                assert!(
                    (got[k] - want[k]).abs() < 2e-3,
                    "frame {frame}: {got:?} for {want:?}"
                );
            }
        }
    }

    /// A cube with no skin that Maya moved along x by a curve (ufbx's
    /// file again): it is read as a skin of one bone, and the clip puts
    /// it where Maya's values say, frame by frame.
    #[test]
    fn a_model_s_animation_moves_its_mesh_as_one_bone() {
        let bytes = include_bytes!(
            "../../../third_party/ufbx/data/maya_interpolation_modes_7500_binary.fbx"
        );
        let imported = import_fbx(bytes, &no_files).unwrap();
        let part = &imported.parts[0];
        let skin = part.solid.skin.as_ref().expect("one bone");
        assert_eq!(skin.bones.len(), 1);
        assert!(skin.bones[0].name.ends_with(".node"));
        let clip = &part.clips[0];
        let centre = |s: &Solid| s.positions.iter().sum::<DVec3>() / s.positions.len() as f64;
        // Maya's x at frames 1, 11, 21, 36, 46 and 60 (the file's
        // centimetres; the clip starts at frame 1).
        for (frame, x) in [
            (1.0, -8.653366),
            (11.0, -6.490576),
            (21.0, -6.113196),
            (36.0, -3.958113),
            (46.0, -5.905977),
            (60.0, -5.359269),
        ] {
            let at = clip.at((frame - 1.0) / 24.0).unwrap();
            let posed = shape_skin::pose_by(&part.solid, &at).unwrap();
            // Between its samples (sixty a second) a clip runs straight.
            assert!(
                (centre(&posed).x - x * 10.0).abs() < 0.05,
                "frame {frame}: {:?}",
                centre(&posed)
            );
        }
    }
}

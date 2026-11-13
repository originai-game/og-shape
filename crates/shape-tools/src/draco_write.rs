//! Writes Draco: a triangle list with its attributes quantized, each
//! value coded as its difference from the one before
//! (`docs/research.md` §60).
//!
//! Entry: [`encode`] (one mesh) and [`compress_draco`] (a GLB's
//! primitives under `KHR_draco_mesh_compression`). The bitstream is
//! version 2.2 with the sequential connectivity: points and triangles
//! keep their order, so the file's accessors still count right. This
//! is the mirror of what `draco.rs` reads; Draco's own encoder was not
//! read.

use crate::compress::{num, obj, set, write};
use crate::jsonread::{Value, parse};

/// One attribute to write.
pub(crate) struct Attribute {
    /// Draco's kind: 0 position, 1 normal, 2 colour, 3 texture
    /// coordinate, 4 generic.
    pub kind: u8,
    /// The id the glTF extension names it by.
    pub id: u32,
    pub components: usize,
    pub data: Data,
}

/// An attribute's values, a point after a point.
pub(crate) enum Data {
    /// Floats kept to `bits` bits of their range, or of `frame` (a
    /// least value a component and one extent for all) where several
    /// meshes must land on one grid.
    Quantized {
        values: Vec<f32>,
        bits: u8,
        frame: Option<(Vec<f32>, f32)>,
    },
    /// Whole numbers, kept as they are (`wide`: 16 bits, else 8).
    Whole { values: Vec<u32>, wide: bool },
    /// Floats kept as they are.
    Plain(Vec<f32>),
}

fn varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// Symbols under the tagged scheme: `components` numbers a point, each
/// point's numbers written in as many bits as its largest needs, that
/// bit count entropy coded (rANS over a table of 4 096).
fn symbols(out: &mut Vec<u8>, values: &[u32], components: usize) {
    const PRECISION: u32 = 4096;
    const BASE: u32 = 16384;
    let sizes: Vec<u32> = values
        .chunks(components)
        .map(|p| p.iter().map(|v| 32 - v.leading_zeros()).max().unwrap_or(0))
        .collect();
    let count = sizes.iter().max().map_or(1, |m| *m as usize + 1);
    let mut uses = vec![0u64; count];
    for &s in &sizes {
        uses[s as usize] += 1;
    }
    // Probabilities that sum to the table's precision, none of a used
    // symbol nothing: the largest takes up the rounding.
    let total: u64 = uses.iter().sum::<u64>().max(1);
    let mut probs: Vec<u32> = uses
        .iter()
        .map(|&u| {
            if u == 0 {
                0
            } else {
                ((u * u64::from(PRECISION) / total) as u32).max(1)
            }
        })
        .collect();
    let sum: i64 = probs.iter().map(|&p| i64::from(p)).sum();
    if let Some(most) = (0..count).max_by_key(|&i| (probs[i], core::cmp::Reverse(i))) {
        probs[most] = (i64::from(probs[most]) + i64::from(PRECISION) - sum).max(1) as u32;
    }
    // If the largest could not absorb it (many rare symbols), take the
    // excess from the others in turn.
    let mut over = probs.iter().map(|&p| i64::from(p)).sum::<i64>() - i64::from(PRECISION);
    while over > 0 {
        for p in probs.iter_mut().filter(|p| **p > 1) {
            if over == 0 {
                break;
            }
            *p -= 1;
            over -= 1;
        }
    }
    out.push(0);
    varint(out, count as u64);
    let mut i = 0;
    while i < count {
        if probs[i] == 0 {
            let mut run = 1;
            while i + run < count && probs[i + run] == 0 && run < 64 {
                run += 1;
            }
            out.push((((run - 1) as u8) << 2) | 3);
            i += run;
            continue;
        }
        let p = probs[i];
        if p < 1 << 6 {
            out.push((p << 2) as u8);
        } else if p < 1 << 14 {
            out.extend_from_slice(&[((p << 2) as u8) | 1, (p >> 6) as u8]);
        } else {
            out.extend_from_slice(&[((p << 2) as u8) | 2, (p >> 6) as u8, (p >> 14) as u8]);
        }
        i += 1;
    }
    let mut starts = vec![0u32; count];
    let mut cum = 0;
    for (s, p) in starts.iter_mut().zip(&probs) {
        *s = cum;
        cum += p;
    }
    // rANS, the symbols last to first; the decoder reads the bytes from
    // the end.
    let mut coded: Vec<u8> = Vec::new();
    let mut state = BASE;
    for &s in sizes.iter().rev() {
        let p = probs[s as usize];
        while u64::from(state) >= u64::from(BASE / PRECISION) * 256 * u64::from(p) {
            coded.push(state as u8);
            state >>= 8;
        }
        state = (state / p) * PRECISION + state % p + starts[s as usize];
    }
    let rest = state - BASE;
    if rest < 1 << 6 {
        coded.push(rest as u8);
    } else if rest < 1 << 14 {
        coded.extend_from_slice(&((1u32 << 14) + rest).to_le_bytes()[..2]);
    } else if rest < 1 << 22 {
        coded.extend_from_slice(&((2u32 << 22) + rest).to_le_bytes()[..3]);
    } else {
        coded.extend_from_slice(&((3u32 << 30) + rest).to_le_bytes());
    }
    varint(out, coded.len() as u64);
    out.extend_from_slice(&coded);
    // The numbers, the low bit of each byte first.
    let mut used = 0usize;
    let start = out.len();
    for (point, &size) in values.chunks(components).zip(&sizes) {
        for &v in point {
            for k in 0..size as usize {
                if used % 8 == 0 {
                    out.push(0);
                }
                if (v >> k) & 1 == 1 {
                    let at = start + used / 8;
                    out[at] |= 1 << (used % 8);
                }
                used += 1;
            }
        }
    }
}

/// Whole numbers as differences from the point before, wrapped into
/// their range, signed into symbols.
fn differences(out: &mut Vec<u8>, ints: &[i32], components: usize) {
    let (min, max) = ints
        .iter()
        .fold((i32::MAX, i32::MIN), |(lo, hi), &v| (lo.min(v), hi.max(v)));
    let (min, max) = if ints.is_empty() { (0, 0) } else { (min, max) };
    let span = i64::from(max) - i64::from(min) + 1;
    let mut most = span / 2;
    let least = -most;
    if span % 2 == 0 {
        most -= 1;
    }
    let mut coded = Vec::with_capacity(ints.len());
    for (p, point) in ints.chunks(components).enumerate() {
        for (c, &v) in point.iter().enumerate() {
            let before = if p == 0 {
                0
            } else {
                ints[(p - 1) * components + c]
            };
            let mut d = i64::from(v) - i64::from(before.clamp(min, max));
            if d > most {
                d -= span;
            } else if d < least {
                d += span;
            }
            coded.push(if d >= 0 {
                (d as u32) << 1
            } else {
                (((-d - 1) as u32) << 1) | 1
            });
        }
    }
    // Difference prediction, the wrap transform, compressed symbols.
    out.extend_from_slice(&[0, 1, 1]);
    symbols(out, &coded, components);
    out.extend_from_slice(&min.to_le_bytes());
    out.extend_from_slice(&max.to_le_bytes());
}

/// A mesh as Draco: `faces` over `points` points, each attribute a
/// value a point.
pub(crate) fn encode(faces: &[[u32; 3]], points: usize, attributes: &[Attribute]) -> Vec<u8> {
    let mut out = b"DRACO".to_vec();
    // Version 2.2, a triangle mesh, sequential, no flags.
    out.extend_from_slice(&[2, 2, 1, 0, 0, 0]);
    varint(&mut out, faces.len() as u64);
    varint(&mut out, points as u64);
    // The indices, each as its signed difference from the last.
    out.push(0);
    let mut last = 0i64;
    let steps: Vec<u32> = faces
        .iter()
        .flatten()
        .map(|&v| {
            let d = i64::from(v) - last;
            last = i64::from(v);
            ((d.unsigned_abs() as u32) << 1) | u32::from(d < 0)
        })
        .collect();
    symbols(&mut out, &steps, 1);
    // One decoder for every attribute.
    out.push(1);
    varint(&mut out, attributes.len() as u64);
    for a in attributes {
        let data_type = match &a.data {
            Data::Whole { wide: false, .. } => 2,
            Data::Whole { wide: true, .. } => 4,
            _ => 9,
        };
        out.extend_from_slice(&[a.kind, data_type, a.components as u8, 0]);
        varint(&mut out, u64::from(a.id));
    }
    for a in attributes {
        out.push(match &a.data {
            Data::Plain(_) => 0,
            Data::Whole { .. } => 1,
            Data::Quantized { .. } => 2,
        });
    }
    // Each attribute's values; then what the quantized ones need to
    // come back as floats.
    let mut ranges: Vec<Option<(Vec<f32>, f32, u8)>> = Vec::new();
    for a in attributes {
        match &a.data {
            Data::Plain(values) => {
                for v in values {
                    out.extend_from_slice(&v.to_le_bytes());
                }
                ranges.push(None);
            }
            Data::Whole { values, .. } => {
                let ints: Vec<i32> = values.iter().map(|&v| v as i32).collect();
                differences(&mut out, &ints, a.components);
                ranges.push(None);
            }
            Data::Quantized {
                values,
                bits,
                frame,
            } => {
                let n = a.components;
                let mut min = vec![f32::INFINITY; n];
                let mut max = vec![f32::NEG_INFINITY; n];
                for point in values.chunks(n) {
                    for c in 0..n {
                        min[c] = min[c].min(point[c]);
                        max[c] = max[c].max(point[c]);
                    }
                }
                if values.is_empty() {
                    min = vec![0.0; n];
                    max = vec![0.0; n];
                }
                // One range for every component: the widest.
                let range = (0..n).map(|c| max[c] - min[c]).fold(0.0f32, f32::max);
                let (min, range) = match frame {
                    Some((least, extent)) if least.len() == n => (least.clone(), *extent),
                    _ => (min, range),
                };
                let range = if range > 0.0 { range } else { 1.0 };
                let top = ((1u32 << bits) - 1) as f32;
                let ints: Vec<i32> = values
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        (((v - min[i % n]) / range * top + 0.5) as i32).clamp(0, top as i32)
                    })
                    .collect();
                differences(&mut out, &ints, n);
                ranges.push(Some((min, range, *bits)));
            }
        }
    }
    for r in ranges.into_iter().flatten() {
        for v in &r.0 {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&r.1.to_le_bytes());
        out.push(r.2);
    }
    out
}

/// How finely [`compress_draco`] keeps each kind of value; a 0 takes
/// the default in brackets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Draco {
    /// Bits of a position's range [14].
    pub position_bits: u8,
    /// Bits of a normal's or a tangent's [10].
    pub normal_bits: u8,
    /// Bits of a texture coordinate's [12].
    pub uv_bits: u8,
    /// Bits of a colour's [8].
    pub color_bits: u8,
}

/// What [`compress_draco`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DracoReport {
    /// Primitives in the file.
    pub primitives: usize,
    /// How many of them were compressed.
    pub compressed: usize,
    /// Bytes of the compressed primitives' geometry, before.
    pub bytes_before: usize,
    /// The same, after.
    pub bytes_after: usize,
}

/// An accessor's values as floats (normalized whole numbers as their
/// fractions' numerators), its component count, and its component type.
fn floats(doc: &Value, bin: &[u8], index: usize) -> Option<(Vec<f64>, usize, usize)> {
    let a = doc.get("accessors").at(index);
    if !a.get("sparse").is_null() {
        return None;
    }
    let view = doc.get("bufferViews").at(a.get("bufferView").index()?);
    if !view.get("extensions").is_null() {
        return None;
    }
    let kind = a.get("componentType").index()?;
    let size = match kind {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => return None,
    };
    let n = match a.get("type").str()? {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        _ => return None,
    };
    let count = a.get("count").index()?;
    let stride = view.get("byteStride").index().unwrap_or(size * n);
    let at = view.get("byteOffset").index().unwrap_or(0) + a.get("byteOffset").index().unwrap_or(0);
    let mut out = Vec::with_capacity(count * n);
    for i in 0..count {
        for c in 0..n {
            let from = at + i * stride + c * size;
            let b = bin.get(from..from + size)?;
            out.push(match kind {
                5120 => f64::from(b[0] as i8),
                5121 => f64::from(b[0]),
                5122 => f64::from(i16::from_le_bytes([b[0], b[1]])),
                5123 => f64::from(u16::from_le_bytes([b[0], b[1]])),
                5125 => f64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
                _ => f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            });
        }
    }
    Some((out, n, kind))
}

/// `glb` with each primitive's triangles and attributes under
/// `KHR_draco_mesh_compression`, and what was done. Lossy: positions,
/// normals, tangents, texture coordinates and colours are kept to
/// `d`'s bits of their range; joints and weights are kept as they are.
/// A primitive with morph targets, without indices, or that does not
/// draw triangles stays as it is.
pub fn compress_draco(glb: &[u8], d: &Draco) -> Result<(Vec<u8>, DracoReport), String> {
    let word = |at: usize| -> Result<usize, String> {
        let b = glb.get(at..at + 4).ok_or("the GLB is cut short")?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    if !glb.starts_with(b"glTF") {
        return Err("not a GLB file".into());
    }
    let json_len = word(12)?;
    let json = glb.get(20..20 + json_len).ok_or("the GLB is cut short")?;
    let bin_at = 20 + json_len;
    let bin: &[u8] = if bin_at + 8 <= glb.len() {
        let n = word(bin_at)?;
        glb.get(bin_at + 8..bin_at + 8 + n)
            .ok_or("the GLB is cut short")?
    } else {
        &[]
    };
    let mut doc = parse(json).map_err(|e| format!("the GLB's JSON: {e}"))?;
    let uses = |name: &str| {
        doc.get("extensionsUsed")
            .items()
            .iter()
            .any(|e| e.str().is_some_and(|s| s.contains(name)))
    };
    if uses("draco_mesh_compression") || uses("meshopt_compression") {
        return Err("the GLB is compressed already".into());
    }
    if doc.get("buffers").items().len() != 1 || !doc.get("buffers").at(0).get("uri").is_null() {
        return Err("a GLB with one buffer, its own, is compressed".into());
    }
    let bits = |given: u8, default: u8| {
        if given == 0 {
            default
        } else {
            given.clamp(2, 30)
        }
    };
    let mut report = DracoReport::default();
    let mut streams: Vec<Vec<u8>> = Vec::new();
    // Accessors a compressed primitive reads, and ones something else
    // still reads from the buffer.
    let accessor_count = doc.get("accessors").items().len();
    let mut packed_accessors = vec![false; accessor_count];
    let mut meshes = doc.get("meshes").items().to_vec();
    let first_new_view = doc.get("bufferViews").items().len();
    for mesh in &mut meshes {
        let mut primitives = mesh.get("primitives").items().to_vec();
        // One grid for the positions of all of a mesh's primitives, so
        // vertices two of them share stay at one place.
        let mut low = [f32::INFINITY; 3];
        let mut high = [f32::NEG_INFINITY; 3];
        for p in &primitives {
            let Some(index) = p.get("attributes").get("POSITION").index() else {
                continue;
            };
            if let Some((values, 3, 5126)) = floats(&doc, bin, index) {
                for point in values.as_chunks::<3>().0 {
                    for c in 0..3 {
                        low[c] = low[c].min(point[c] as f32);
                        high[c] = high[c].max(point[c] as f32);
                    }
                }
            }
        }
        let extent = (0..3).map(|c| high[c] - low[c]).fold(0.0f32, f32::max);
        let frame = (extent > 0.0 && extent.is_finite()).then(|| (low.to_vec(), extent));
        for p in &mut primitives {
            report.primitives += 1;
            let indices = p.get("indices").index();
            if p.get("mode").index().unwrap_or(4) != 4
                || !p.get("targets").is_null()
                || !p.get("extensions").is_null()
            {
                continue;
            }
            let Some(indices) = indices else { continue };
            let Some((ids, 1, _)) = floats(&doc, bin, indices) else {
                continue;
            };
            let faces: Vec<[u32; 3]> = ids
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| [t[0] as u32, t[1] as u32, t[2] as u32])
                .collect();
            let mut attributes = Vec::new();
            let mut names: Vec<(&str, Value)> = Vec::new();
            let mut touched = vec![indices];
            let mut points = None;
            let mut before = doc
                .get("accessors")
                .at(indices)
                .get("count")
                .index()
                .unwrap_or(0)
                * if ids.iter().all(|&v| v < 65536.0) {
                    2
                } else {
                    4
                };
            let mut whole = true;
            for (name, accessor) in p.get("attributes").entries() {
                let Some(index) = accessor.index() else {
                    whole = false;
                    break;
                };
                let Some((values, n, kind)) = floats(&doc, bin, index) else {
                    whole = false;
                    break;
                };
                let count = values.len() / n;
                if *points.get_or_insert(count) != count {
                    whole = false;
                    break;
                }
                before += values.len() * if kind == 5126 { 4 } else { 2 };
                let as_f32 = || values.iter().map(|&v| v as f32).collect::<Vec<f32>>();
                let stem = name.split('_').next().unwrap_or("");
                let (draco_kind, data) = match (stem, kind) {
                    ("POSITION", 5126) => (
                        0,
                        Data::Quantized {
                            values: as_f32(),
                            bits: bits(d.position_bits, 14),
                            frame: frame.clone(),
                        },
                    ),
                    ("NORMAL" | "TANGENT", 5126) => (
                        if stem == "NORMAL" { 1 } else { 4 },
                        Data::Quantized {
                            values: as_f32(),
                            bits: bits(d.normal_bits, 10),
                            frame: None,
                        },
                    ),
                    ("TEXCOORD", 5126) => (
                        3,
                        Data::Quantized {
                            values: as_f32(),
                            bits: bits(d.uv_bits, 12),
                            frame: None,
                        },
                    ),
                    ("COLOR", 5126) => (
                        2,
                        Data::Quantized {
                            values: as_f32(),
                            bits: bits(d.color_bits, 8),
                            frame: None,
                        },
                    ),
                    (_, 5121 | 5123) => (
                        4,
                        Data::Whole {
                            values: values.iter().map(|&v| v as u32).collect(),
                            wide: kind == 5123,
                        },
                    ),
                    (_, 5126) => (4, Data::Plain(as_f32())),
                    _ => {
                        whole = false;
                        break;
                    }
                };
                let id = attributes.len() as u32;
                attributes.push(Attribute {
                    kind: draco_kind,
                    id,
                    components: n,
                    data,
                });
                names.push((name.as_str(), num(id as usize)));
                touched.push(index);
            }
            let Some(points) = points.filter(|_| whole) else {
                continue;
            };
            if faces.iter().flatten().any(|&v| v as usize >= points) {
                continue;
            }
            let stream = encode(&faces, points, &attributes);
            report.compressed += 1;
            report.bytes_before += before;
            report.bytes_after += stream.len();
            for i in touched {
                packed_accessors[i] = true;
            }
            let ext = obj(vec![
                ("bufferView", num(first_new_view + streams.len())),
                ("attributes", obj(names)),
            ]);
            set(
                p,
                "extensions",
                obj(vec![("KHR_draco_mesh_compression", ext)]),
            );
            streams.push(stream);
        }
        set(mesh, "primitives", Value::Arr(primitives));
    }
    if report.compressed == 0 {
        return Ok((glb.to_vec(), report));
    }
    // An accessor only compressed primitives read no longer has a view;
    // a view nothing reads any more is dropped, the rest renumbered.
    let mut still = vec![false; accessor_count];
    for mesh in &meshes {
        for p in mesh.get("primitives").items() {
            if !p
                .get("extensions")
                .get("KHR_draco_mesh_compression")
                .is_null()
            {
                continue;
            }
            for (_, a) in p.get("attributes").entries() {
                if let Some(i) = a.index() {
                    still[i] = true;
                }
            }
            if let Some(i) = p.get("indices").index() {
                still[i] = true;
            }
            for t in p.get("targets").items() {
                for (_, a) in t.entries() {
                    if let Some(i) = a.index() {
                        still[i] = true;
                    }
                }
            }
        }
    }
    let mut accessors = doc.get("accessors").items().to_vec();
    for (i, a) in accessors.iter_mut().enumerate() {
        if packed_accessors[i]
            && !still[i]
            && let Value::Obj(fields) = a
        {
            fields.retain(|(k, _)| k != "bufferView" && k != "byteOffset");
        }
    }
    let old_views = doc.get("bufferViews").items().to_vec();
    let mut kept = vec![false; old_views.len()];
    for a in &accessors {
        if let Some(v) = a.get("bufferView").index() {
            kept[v] = true;
        }
    }
    for image in doc.get("images").items() {
        if let Some(v) = image.get("bufferView").index() {
            kept[v] = true;
        }
    }
    let mut renumber = vec![usize::MAX; old_views.len()];
    let mut out_bin: Vec<u8> = Vec::new();
    let mut views = Vec::new();
    for (i, view) in old_views.iter().enumerate() {
        if !kept[i] {
            continue;
        }
        let at = view.get("byteOffset").index().unwrap_or(0);
        let len = view
            .get("byteLength")
            .index()
            .ok_or("a buffer view with no length")?;
        let data = at
            .checked_add(len)
            .and_then(|end| bin.get(at..end))
            .ok_or("a buffer view outside the buffer")?;
        out_bin.resize(out_bin.len().next_multiple_of(4), 0);
        let mut view = view.clone();
        set(&mut view, "byteOffset", num(out_bin.len()));
        out_bin.extend_from_slice(data);
        renumber[i] = views.len();
        views.push(view);
    }
    let stream_base = views.len();
    for s in &streams {
        out_bin.resize(out_bin.len().next_multiple_of(4), 0);
        views.push(obj(vec![
            ("buffer", num(0)),
            ("byteOffset", num(out_bin.len())),
            ("byteLength", num(s.len())),
        ]));
        out_bin.extend_from_slice(s);
    }
    out_bin.resize(out_bin.len().next_multiple_of(4), 0);
    for a in &mut accessors {
        if let Some(v) = a.get("bufferView").index() {
            set(a, "bufferView", num(renumber[v]));
        }
    }
    let mut images = doc.get("images").items().to_vec();
    for image in &mut images {
        if let Some(v) = image.get("bufferView").index() {
            set(image, "bufferView", num(renumber[v]));
        }
    }
    for mesh in &mut meshes {
        let mut primitives = mesh.get("primitives").items().to_vec();
        for p in &mut primitives {
            let ext = p.get("extensions").get("KHR_draco_mesh_compression");
            if let Some(v) = ext.get("bufferView").index() {
                let mut ext = ext.clone();
                set(
                    &mut ext,
                    "bufferView",
                    num(stream_base + v - first_new_view),
                );
                set(
                    p,
                    "extensions",
                    obj(vec![("KHR_draco_mesh_compression", ext)]),
                );
            }
        }
        set(mesh, "primitives", Value::Arr(primitives));
    }
    set(&mut doc, "meshes", Value::Arr(meshes));
    set(&mut doc, "accessors", Value::Arr(accessors));
    if !images.is_empty() {
        set(&mut doc, "images", Value::Arr(images));
    }
    set(&mut doc, "bufferViews", Value::Arr(views));
    set(
        &mut doc,
        "buffers",
        Value::Arr(vec![obj(vec![("byteLength", num(out_bin.len()))])]),
    );
    for key in ["extensionsUsed", "extensionsRequired"] {
        let mut names = doc.get(key).items().to_vec();
        names.push(Value::Str("KHR_draco_mesh_compression".into()));
        set(&mut doc, key, Value::Arr(names));
    }
    let mut text = String::new();
    write(&doc, &mut text);
    let mut json = text.into_bytes();
    json.resize(json.len().next_multiple_of(4), b' ');
    let total = 12 + 8 + json.len() + 8 + out_bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    out.extend_from_slice(&(out_bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&out_bin);
    Ok((out, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What is written reads back through the decoder that reads
    /// Draco's own files: the triangles as given, whole numbers and
    /// plain floats exactly, quantized floats to half a step of their
    /// range.
    #[test]
    fn a_written_mesh_reads_back() {
        let points = 300usize;
        let mut state = 99u32;
        let mut noise = || {
            state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (state >> 16) as f32 / 65535.0
        };
        let positions: Vec<f32> = (0..points * 3)
            .map(|i| (i / 3) as f32 * 0.25 + noise() * 3.0 - 50.0)
            .collect();
        let uvs: Vec<f32> = (0..points * 2).map(|_| noise()).collect();
        let joints: Vec<u32> = (0..points * 4).map(|i| (i as u32 * 7) % 251).collect();
        let weights: Vec<f32> = (0..points * 4).map(|_| noise()).collect();
        let faces: Vec<[u32; 3]> = (0..points as u32 - 2)
            .map(|i| {
                if i % 5 == 0 {
                    [i + 2, 0, i + 1]
                } else {
                    [i, i + 1, i + 2]
                }
            })
            .collect();
        let attributes = [
            Attribute {
                kind: 0,
                id: 0,
                components: 3,
                data: Data::Quantized {
                    values: positions.clone(),
                    bits: 14,
                    frame: None,
                },
            },
            Attribute {
                kind: 3,
                id: 1,
                components: 2,
                data: Data::Quantized {
                    values: uvs.clone(),
                    bits: 12,
                    frame: None,
                },
            },
            Attribute {
                kind: 4,
                id: 2,
                components: 4,
                data: Data::Whole {
                    values: joints.clone(),
                    wide: false,
                },
            },
            Attribute {
                kind: 4,
                id: 3,
                components: 4,
                data: Data::Plain(weights.clone()),
            },
        ];
        let bytes = encode(&faces, points, &attributes);
        let back = crate::draco::decode(&bytes).unwrap();
        assert_eq!(back.faces, faces);
        assert_eq!(back.points, points);
        let of = |id: u32| &back.attributes.iter().find(|a| a.id == id).unwrap().values;
        let range = |v: &[f32], n: usize| {
            (0..n)
                .map(|c| {
                    let col = v.iter().skip(c).step_by(n);
                    col.clone().fold(f32::MIN, |a, b| a.max(*b))
                        - col.fold(f32::MAX, |a, b| a.min(*b))
                })
                .fold(0.0f32, f32::max)
        };
        for (got, want, n, bits) in [(of(0), &positions, 3, 14u32), (of(1), &uvs, 2, 12)] {
            let step = f64::from(range(want, n)) / f64::from((1u32 << bits) - 1);
            for (g, w) in got.iter().zip(want.iter()) {
                assert!((g - f64::from(*w)).abs() <= step * 0.51, "{g} {w} {step}");
            }
        }
        assert_eq!(
            of(2),
            &joints.iter().map(|&v| f64::from(v)).collect::<Vec<_>>()
        );
        assert_eq!(
            of(3),
            &weights.iter().map(|&v| f64::from(v)).collect::<Vec<_>>()
        );
        // Under two thirds of the bytes those values and indices take
        // in a GLB, noise as they are.
        let plain = points * (12 + 8 + 4 + 16) + faces.len() * 6;
        assert!(bytes.len() * 3 < plain * 2, "{} of {plain}", bytes.len());
    }
}

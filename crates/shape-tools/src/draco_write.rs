//! Writes Draco: a triangle mesh with its attributes quantized, each
//! value coded as its difference from a prediction
//! (`docs/research.md` §60, §67).
//!
//! Entry: [`encode`] (one mesh) and [`compress_draco`] (a GLB's
//! primitives under `KHR_draco_mesh_compression`). The bitstream is
//! version 2.2. The connectivity is an EdgeBreaker walk
//! (`draco_edge.rs`, ported from Draco's encoder) with the
//! parallelogram prediction, or sequential: points and triangles in
//! the file's order, each value predicted by the one before. The rest
//! is the mirror of what `draco.rs` reads.

use crate::compress::{num, obj, set, write};
use crate::draco_edge::{Walk, mended, walk};
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

impl Attribute {
    /// The attribute over other points: point `i` of the result is
    /// this one's point `from[i]`.
    fn picked(&self, from: &[u32]) -> Attribute {
        let n = self.components;
        fn pick<T: Copy>(values: &[T], from: &[u32], n: usize) -> Vec<T> {
            from.iter()
                .flat_map(|&v| values[v as usize * n..v as usize * n + n].iter().copied())
                .collect()
        }
        Attribute {
            kind: self.kind,
            id: self.id,
            components: n,
            data: match &self.data {
                Data::Quantized {
                    values,
                    bits,
                    frame,
                } => Data::Quantized {
                    values: pick(values, from, n),
                    bits: *bits,
                    frame: frame.clone(),
                },
                Data::Whole { values, wide } => Data::Whole {
                    values: pick(values, from, n),
                    wide: *wide,
                },
                Data::Plain(values) => Data::Plain(pick(values, from, n)),
            },
        }
    }
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

/// Symbols, under whichever of Draco's two schemes takes fewer bytes:
/// tagged (`components` numbers a point, each point's numbers written
/// in as many bits as its largest needs, that bit count entropy
/// coded), or raw (every number entropy coded as it is, which wins
/// where few values are common, as corrections to a good prediction
/// are).
fn symbols(out: &mut Vec<u8>, values: &[u32], components: usize) {
    let sizes: Vec<u32> = values
        .chunks(components)
        .map(|p| p.iter().map(|v| 32 - v.leading_zeros()).max().unwrap_or(0))
        .collect();
    let mut tagged = vec![0u8];
    let count = sizes.iter().max().map_or(1, |m| *m as usize + 1);
    rans(&mut tagged, &sizes, count, 12);
    // The numbers, the low bit of each byte first.
    let mut used = 0usize;
    let start = tagged.len();
    for (point, &size) in values.chunks(components).zip(&sizes) {
        for &v in point {
            for k in 0..size as usize {
                if used.is_multiple_of(8) {
                    tagged.push(0);
                }
                if (v >> k) & 1 == 1 {
                    let at = start + used / 8;
                    tagged[at] |= 1 << (used % 8);
                }
                used += 1;
            }
        }
    }
    // Raw: up to 2¹⁸ different numbers; the table's precision follows
    // from how many are in use, as the decoder works it out.
    let most = values.iter().copied().max().unwrap_or(0);
    if most < 1 << 18 && !values.is_empty() {
        let mut in_use = vec![false; most as usize + 1];
        for &v in values {
            in_use[v as usize] = true;
        }
        let distinct = in_use.iter().filter(|u| **u).count() as u32;
        let bits = (32 - distinct.leading_zeros()).clamp(1, 18);
        let mut raw = vec![1u8, bits as u8];
        rans(
            &mut raw,
            values,
            most as usize + 1,
            (3 * bits / 2).clamp(12, 20),
        );
        if raw.len() < tagged.len() {
            out.extend_from_slice(&raw);
            return;
        }
    }
    out.extend_from_slice(&tagged);
}

/// `symbols` (each under `count`) entropy coded: how many there can
/// be, the table of their probabilities (summing to 2^`precision_bits`,
/// none of a used symbol nothing), the size, and the rANS bytes (the
/// symbols run last to first; the decoder reads the bytes from the
/// end).
fn rans(out: &mut Vec<u8>, symbols: &[u32], count: usize, precision_bits: u32) {
    let precision = 1u32 << precision_bits;
    let base = precision * 4;
    let mut uses = vec![0u64; count];
    for &s in symbols {
        uses[s as usize] += 1;
    }
    // The largest takes up the rounding.
    let total: u64 = uses.iter().sum::<u64>().max(1);
    let mut probs: Vec<u32> = uses
        .iter()
        .map(|&u| {
            if u == 0 {
                0
            } else {
                ((u * u64::from(precision) / total) as u32).max(1)
            }
        })
        .collect();
    let sum: i64 = probs.iter().map(|&p| i64::from(p)).sum();
    if let Some(most) = (0..count).max_by_key(|&i| (probs[i], core::cmp::Reverse(i))) {
        probs[most] = (i64::from(probs[most]) + i64::from(precision) - sum).max(1) as u32;
    }
    // If the largest could not absorb it (many rare symbols), take the
    // excess from the others in turn.
    let mut over = probs.iter().map(|&p| i64::from(p)).sum::<i64>() - i64::from(precision);
    while over > 0 {
        for p in probs.iter_mut().filter(|p| **p > 1) {
            if over == 0 {
                break;
            }
            *p -= 1;
            over -= 1;
        }
    }
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
    let mut coded: Vec<u8> = Vec::new();
    let mut state = base;
    for &s in symbols.iter().rev() {
        let p = probs[s as usize];
        while u64::from(state) >= 1024 * u64::from(p) {
            coded.push(state as u8);
            state >>= 8;
        }
        state = (state / p) * precision + state % p + starts[s as usize];
    }
    let rest = state - base;
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
}

/// Whole numbers, a point after a point in the order they are
/// written, each as its difference from a prediction, wrapped into
/// their range and signed into symbols. The prediction is the point
/// before; or, with a walk, the parallelogram across the edge the
/// walk reached the point by where its three points are already
/// written (the point before where they are not), if that takes
/// fewer bytes.
fn predicted(out: &mut Vec<u8>, ints: &[i32], components: usize, walk: Option<&Walk>) {
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
    let n = components;
    let code = |parallelograms: Option<&Walk>| -> Vec<u8> {
        let mut coded = Vec::with_capacity(ints.len());
        let mut guess = vec![0i64; n];
        for p in 0..ints.len() / n {
            let mut found = false;
            if let Some(w) = parallelograms
                && p > 0
            {
                let across = w.opposite[w.entry_corner[p] as usize];
                if across >= 0 {
                    let at = |c: i32| w.vertex_entry[w.vertex[c as usize] as usize] as usize;
                    let corner = |k: i32| 3 * (across / 3) + (across % 3 + k) % 3;
                    let (o, a, b) = (at(across), at(corner(1)), at(corner(2)));
                    if o < p && a < p && b < p {
                        for c in 0..n {
                            guess[c] = i64::from(
                                ints[a * n + c]
                                    .wrapping_add(ints[b * n + c])
                                    .wrapping_sub(ints[o * n + c]),
                            );
                        }
                        found = true;
                    }
                }
            }
            if !found {
                for c in 0..n {
                    guess[c] = if p == 0 {
                        0
                    } else {
                        i64::from(ints[(p - 1) * n + c])
                    };
                }
            }
            for c in 0..n {
                let mut d =
                    i64::from(ints[p * n + c]) - guess[c].clamp(i64::from(min), i64::from(max));
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
        // The prediction (0 the difference, 1 the parallelogram), the
        // wrap transform, compressed symbols.
        let mut bytes = vec![u8::from(parallelograms.is_some()), 1, 1];
        symbols(&mut bytes, &coded, n);
        bytes
    };
    let mut best = code(None);
    if let Some(w) = walk {
        let other = code(Some(w));
        if other.len() < best.len() {
            best = other;
        }
    }
    out.extend_from_slice(&best);
    out.extend_from_slice(&min.to_le_bytes());
    out.extend_from_slice(&max.to_le_bytes());
}

/// A mesh as Draco: `faces` over `points` points, each attribute a
/// value a point. With `walked`, and where the mesh can be walked
/// ([`walk`]), the triangles are an EdgeBreaker walk and the points go
/// in the walk's order (so the decoded mesh has other indices for the
/// same triangles); otherwise the connectivity is sequential and every
/// point and triangle keeps its place. The second value says which.
pub(crate) fn encode(
    faces: &[[u32; 3]],
    points: usize,
    attributes: &[Attribute],
    walked: bool,
) -> (Vec<u8>, bool) {
    let walk = if walked { walk(faces, points) } else { None };
    let walk = walk.as_ref();
    let mut out = b"DRACO".to_vec();
    // Version 2.2, a triangle mesh, sequential or EdgeBreaker, no flags.
    out.extend_from_slice(&[2, 2, 1, u8::from(walk.is_some()), 0, 0]);
    // One decoder for every attribute.
    match walk {
        Some(w) => {
            out.extend_from_slice(&w.bytes);
            // On the positions' own connectivity, a value a vertex,
            // depth first.
            out.extend_from_slice(&[1, 0xff, 0, 0]);
        }
        None => {
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
            out.push(1);
        }
    }
    // A point's values, in the order the points are written.
    let ordered = |n: usize| -> Vec<usize> {
        match walk {
            Some(w) => w
                .entry_corner
                .iter()
                .flat_map(|&c| {
                    let v = w.vertex[c as usize] as usize;
                    v * n..v * n + n
                })
                .collect(),
            None => (0..points * n).collect(),
        }
    };
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
                for i in ordered(a.components) {
                    out.extend_from_slice(&values[i].to_le_bytes());
                }
                ranges.push(None);
            }
            Data::Whole { values, .. } => {
                let ints: Vec<i32> = ordered(a.components)
                    .into_iter()
                    .map(|i| values[i] as i32)
                    .collect();
                predicted(&mut out, &ints, a.components, walk);
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
                let ints: Vec<i32> = ordered(n)
                    .into_iter()
                    .map(|i| {
                        (((values[i] - min[i % n]) / range * top + 0.5) as i32).clamp(0, top as i32)
                    })
                    .collect();
                predicted(&mut out, &ints, n, walk);
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
    (out, walk.is_some())
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
    /// Bits of a skin weight's; 0 keeps the weights as they are (a
    /// reader that wants them to sum to 1 exactly gets that).
    pub weight_bits: u8,
    /// Every primitive keeps its points and triangles in the file's
    /// order (the sequential connectivity). Off, a primitive that can
    /// be walked is written as an EdgeBreaker walk, in fewer bytes and
    /// in the walk's order.
    pub sequential: bool,
}

/// What [`compress_draco`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DracoReport {
    /// Primitives in the file.
    pub primitives: usize,
    /// How many of them were compressed.
    pub compressed: usize,
    /// How many of those as an EdgeBreaker walk (the rest keep the
    /// file's order).
    pub walked: usize,
    /// How many of the walked were mended first ([`mended`]): they
    /// have other counts of points and triangles than the file had,
    /// for the same surface.
    pub mended: usize,
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
    // Accessors made here, for primitives whose counts changed.
    let mut added: Vec<Value> = Vec::new();
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
            let mut names: Vec<(String, Value)> = Vec::new();
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
                    ("WEIGHTS", 5126) if d.weight_bits > 0 => (
                        4,
                        Data::Quantized {
                            values: as_f32(),
                            bits: d.weight_bits.clamp(2, 30),
                            frame: None,
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
                names.push((name.clone(), num(id as usize)));
                touched.push(index);
            }
            let Some(points) = points.filter(|_| whole) else {
                continue;
            };
            if faces.iter().flatten().any(|&v| v as usize >= points) {
                continue;
            }
            // As it is where it can be walked; else mended, if that
            // can be; else in the file's order.
            let (mut stream, mut walked) = encode(&faces, points, &attributes, !d.sequential);
            if !walked && !d.sequential {
                let (faces, from) = mended(&faces, points);
                let picked: Vec<Attribute> = attributes.iter().map(|a| a.picked(&from)).collect();
                let (again, did) = encode(&faces, from.len(), &picked, true);
                if did {
                    (stream, walked) = (again, true);
                    report.mended += 1;
                    // Other counts: accessors of this primitive's own.
                    let renew = |index: usize, count: usize, added: &mut Vec<Value>| {
                        let mut a = doc.get("accessors").at(index).clone();
                        set(&mut a, "count", num(count));
                        if let Value::Obj(fields) = &mut a {
                            fields.retain(|(k, _)| k != "bufferView" && k != "byteOffset");
                        }
                        added.push(a);
                        num(accessor_count + added.len() - 1)
                    };
                    let at = renew(indices, faces.len() * 3, &mut added);
                    set(p, "indices", at);
                    let entries: Vec<(String, Value)> = p
                        .get("attributes")
                        .entries()
                        .iter()
                        .map(|(name, a)| {
                            let at = match a.index() {
                                Some(i) => renew(i, from.len(), &mut added),
                                None => a.clone(),
                            };
                            (name.clone(), at)
                        })
                        .collect();
                    set(p, "attributes", Value::Obj(entries));
                }
            }
            report.compressed += 1;
            report.walked += usize::from(walked);
            report.bytes_before += before;
            report.bytes_after += stream.len();
            for i in touched {
                packed_accessors[i] = true;
            }
            let ext = obj(vec![
                ("bufferView", num(first_new_view + streams.len())),
                ("attributes", Value::Obj(names)),
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
    accessors.extend(added);
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
        let (bytes, _) = encode(&faces, points, &attributes, false);
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

    /// A grid of `w` by `h` squares over a surface, its triangles, and
    /// which squares are left out; `wrap_u` and `wrap_v` join its
    /// edges.
    fn grid(
        w: u32,
        h: u32,
        (wrap_u, wrap_v): (bool, bool),
        at: impl Fn(f32, f32) -> [f32; 3],
        skip: impl Fn(u32, u32) -> bool,
    ) -> (Vec<f32>, Vec<[u32; 3]>) {
        let (cols, rows) = (
            if wrap_u { w } else { w + 1 },
            if wrap_v { h } else { h + 1 },
        );
        let mut positions = Vec::new();
        for j in 0..rows {
            for i in 0..cols {
                positions.extend(at(i as f32 / w as f32, j as f32 / h as f32));
            }
        }
        let id = |i: u32, j: u32| (j % rows) * cols + (i % cols);
        let mut faces = Vec::new();
        for j in 0..h {
            for i in 0..w {
                if skip(i, j) {
                    continue;
                }
                let (a, b, c, d) = (id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1));
                faces.push([a, b, c]);
                faces.push([a, c, d]);
            }
        }
        (positions, faces)
    }

    /// Walked meshes read back as the same triangles between the same
    /// points, in the walk's order: a sheet with a hole in it (open
    /// boundaries), a ring (a handle: topology splits), two pieces,
    /// and a sheet with squares gone from its sides. Each takes fewer bytes than in the file's order.
    #[test]
    fn a_walked_mesh_reads_back() {
        use shape_core::trig::{cos, sin};
        let tau = 2.0 * shape_core::trig::PI;
        let sheet = |u: f32, v: f32| {
            let bump = sin(f64::from(u) * 9.0) * cos(f64::from(v) * 7.0) * 3.0;
            [u * 40.0, v * 30.0, bump as f32]
        };
        let ring = |u: f32, v: f32| {
            let (a, b) = (f64::from(u) * tau, f64::from(v) * tau);
            let r = 20.0 + 5.0 * cos(b);
            [
                (r * cos(a)) as f32,
                (r * sin(a)) as f32,
                (5.0 * sin(b)) as f32,
            ]
        };
        let mut two = grid(12, 9, (false, false), sheet, |_, _| false);
        let far = grid(24, 12, (true, true), ring, |_, _| false);
        let base = (two.0.len() / 3) as u32;
        two.0.extend(
            far.0
                .iter()
                .enumerate()
                .map(|(i, v)| v + if i % 3 == 0 { 100.0 } else { 0.0 }),
        );
        two.1.extend(far.1.iter().map(|t| t.map(|v| v + base)));
        let cases = [
            (
                "holed sheet",
                grid(20, 16, (false, false), sheet, |i, j| {
                    (i == 8 && j == 6) || (i == 3 && j == 11)
                }),
            ),
            ("ring", grid(24, 12, (true, true), ring, |_, _| false)),
            ("tube", grid(24, 12, (true, false), ring, |_, _| false)),
            ("two pieces", two),
            (
                "notched sheet",
                grid(9, 7, (false, false), sheet, |i, j| {
                    (i == 0 || i == 8) && j == 3
                }),
            ),
        ];
        for (name, (positions, faces)) in cases {
            let points = positions.len() / 3;
            let tags: Vec<u32> = (0..points as u32).map(|i| i % 200).collect();
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
                    kind: 4,
                    id: 1,
                    components: 1,
                    data: Data::Whole {
                        values: tags.clone(),
                        wide: false,
                    },
                },
                Attribute {
                    kind: 4,
                    id: 2,
                    components: 3,
                    data: Data::Plain(positions.clone()),
                },
            ];
            let (bytes, walked) = encode(&faces, points, &attributes, true);
            assert!(walked, "{name}");
            let back = crate::draco::decode(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                (back.points, back.faces.len()),
                (points, faces.len()),
                "{name}"
            );
            let of = |id: u32| &back.attributes.iter().find(|a| a.id == id).unwrap().values;
            // Each decoded point's first tag and exact position say
            // which of the file's it is.
            let key = |p: &[f32]| p.iter().map(|v| v.to_bits()).collect::<Vec<u32>>();
            let mut index = std::collections::BTreeMap::new();
            for (i, p) in positions.chunks(3).enumerate() {
                index.insert(key(p), i as u32);
            }
            let to_file: Vec<u32> = of(2)
                .chunks(3)
                .map(|p| index[&key(&[p[0] as f32, p[1] as f32, p[2] as f32])])
                .collect();
            for (i, &v) in to_file.iter().enumerate() {
                assert_eq!(of(1)[i], f64::from(tags[v as usize]), "{name}: tag of {i}");
            }
            // The same triangles, each from its own first corner on.
            let turned = |t: [u32; 3]| {
                let k = (0..3).min_by_key(|&k| t[k]).unwrap_or(0);
                [t[k], t[(k + 1) % 3], t[(k + 2) % 3]]
            };
            let mut got: Vec<[u32; 3]> = back
                .faces
                .iter()
                .map(|t| turned(t.map(|v| to_file[v as usize])))
                .collect();
            let mut want: Vec<[u32; 3]> = faces.iter().map(|t| turned(*t)).collect();
            got.sort_unstable();
            want.sort_unstable();
            assert!(got == want, "{name}: other triangles");
            let (kept, _) = encode(&faces, points, &attributes, false);
            assert!(
                bytes.len() < kept.len(),
                "{name}: {} {}",
                bytes.len(),
                kept.len()
            );
        }
    }

    /// What cannot be walked as it is, and is walked once mended: an
    /// edge of three triangles, a point no triangle uses, two triangles
    /// that meet at one vertex only, a triangle with a vertex twice.
    /// Mended, the triangles are the same between the same places.
    #[test]
    fn what_cannot_be_walked_is_mended() {
        let fin = [[0, 1, 2], [1, 0, 3], [0, 1, 4]];
        assert!(walk(&fin, 5).is_none());
        assert!(walk(&[[0, 1, 2]], 4).is_none());
        assert!(walk(&[[0, 1, 2], [0, 3, 4]], 5).is_none());
        assert!(walk(&[[0, 1, 2], [2, 1, 3]], 4).is_some());
        let faces = [[0, 1, 2], [1, 0, 3], [0, 1, 4], [4, 4, 5], [2, 6, 7]];
        let (mesh, from) = mended(&faces, 9);
        assert!(walk(&mesh, from.len()).is_some());
        let back: Vec<[u32; 3]> = mesh.iter().map(|t| t.map(|v| from[v as usize])).collect();
        assert_eq!(back, [[0, 1, 2], [1, 0, 3], [0, 1, 4], [2, 6, 7]]);
        // Three triangles on one edge come apart, and the one at the
        // tip is on points of its own: four triangles, three points
        // each; the unused point is gone.
        assert_eq!(from.len(), 12);
    }
}

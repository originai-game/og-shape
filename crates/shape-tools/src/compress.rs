//! Compresses a GLB's geometry and animation with meshopt
//! (`EXT_meshopt_compression`), losslessly: every buffer view an
//! accessor reads is written as an attribute or triangle stream where
//! that is smaller, and reads back to the same bytes, a triangle
//! perhaps from another of its corners (`docs/research.md` §51, §68).
//!
//! Entry: [`compress`]. Images are left as they are. The streams are
//! version 0, the one every reader of the extension takes.

use std::collections::BTreeMap;

use crate::json::string;
use crate::jsonread::{Value, parse};
use crate::meshopt::{Filter, Mode, decode};

/// What [`compress`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Compressed {
    /// Buffer views in the file.
    pub views: usize,
    /// How many of them were compressed.
    pub compressed: usize,
    /// Bytes of the views compressed, before.
    pub bytes_before: usize,
    /// The same, after.
    pub bytes_after: usize,
}

fn zigzag8(d: u8) -> u8 {
    if d & 0x80 != 0 { !(d << 1) } else { d << 1 }
}

/// 16 deltas in `width` bits each, those that do not fit after them.
fn group(out: &mut Vec<u8>, d: &[u8], width: usize) {
    if width == 8 {
        out.extend(d);
        return;
    }
    let sentinel = (1u8 << width) - 1;
    let per = 8 / width;
    let mut packed = vec![0u8; 16 / per];
    for (i, v) in d.iter().enumerate() {
        packed[i / per] |= (*v).min(sentinel) << (8 - width * (i % per + 1));
    }
    out.extend(packed);
    out.extend(d.iter().filter(|v| **v >= sentinel));
}

/// Elements of `stride` bytes as an attribute stream (version 0): each
/// byte of an element as its difference from the element before, in
/// groups of 16 at the narrowest width that holds them.
pub(crate) fn attributes(data: &[u8], stride: usize) -> Vec<u8> {
    let count = data.len() / stride;
    let mut out = vec![0xa0];
    let block_max = ((8192 / stride) & !15).min(256);
    let base = data[..stride].to_vec();
    let mut last = base.clone();
    let mut done = 0;
    while done < count {
        let n = (count - done).min(block_max);
        let groups = n.div_ceil(16);
        for k in 0..stride {
            let mut row = vec![0u8; groups * 16];
            for (i, slot) in row.iter_mut().enumerate().take(n) {
                let v = data[(done + i) * stride + k];
                *slot = zigzag8(v.wrapping_sub(last[k]));
                last[k] = v;
            }
            let cost = |d: &[u8], w: usize| match w {
                0 if d.iter().all(|v| *v == 0) => 0,
                0 => usize::MAX,
                8 => 16,
                w => 2 * w + d.iter().filter(|v| **v >= (1 << w) - 1).count(),
            };
            const WIDTHS: [usize; 4] = [0, 2, 4, 8];
            let picks: Vec<usize> = row
                .chunks(16)
                .map(|d| (0..4).min_by_key(|i| cost(d, WIDTHS[*i])).unwrap_or(3))
                .collect();
            for p in picks.chunks(4) {
                out.push(
                    p.iter()
                        .enumerate()
                        .map(|(i, v)| (*v as u8) << (i * 2))
                        .sum(),
                );
            }
            for (d, p) in row.chunks(16).zip(&picks) {
                if WIDTHS[*p] > 0 {
                    group(&mut out, d, WIDTHS[*p]);
                }
            }
        }
        done += n;
    }
    out.extend(vec![0u8; 32usize.saturating_sub(stride)]);
    out.extend(base);
    out
}

fn varint(out: &mut Vec<u8>, mut v: u32) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn zigzag(d: u32) -> u32 {
    (d << 1) ^ (((d as i32) >> 31) as u32)
}

/// A triangle list as a triangle stream: a triangle on an edge of the
/// last few is that edge and one vertex, and a vertex is the next new
/// one, one of the last few, or its distance from the last named.
pub(crate) fn triangles(tris: &[[u32; 3]]) -> Vec<u8> {
    let mut codes = Vec::new();
    let mut extra = Vec::new();
    let (mut next, mut last) = (0u32, 0u32);
    let mut edges = [(u32::MAX, u32::MAX); 16];
    let mut verts = [u32::MAX; 16];
    let (mut eo, mut vo) = (0usize, 0usize);
    for t in tris {
        let hit = (0..3).find_map(|r| {
            let (a, b, c) = (t[r], t[(r + 1) % 3], t[(r + 2) % 3]);
            (0..15)
                .find(|x| edges[eo.wrapping_sub(1 + x) & 15] == (a, b))
                .map(|x| (x, a, b, c))
        });
        let (a, b, c);
        if let Some((x, ea, eb, ec)) = hit {
            (a, b, c) = (ea, eb, ec);
            let seen = (1..13).find(|y| verts[vo.wrapping_sub(1 + y) & 15] == c);
            let y = if c == next {
                next += 1;
                0
            } else if let Some(y) = seen {
                y
            } else if c == last.wrapping_sub(1) {
                last = c;
                13
            } else if c == last.wrapping_add(1) {
                last = c;
                14
            } else {
                varint(&mut extra, zigzag(c.wrapping_sub(last)));
                last = c;
                15
            };
            codes.push(((x << 4) | y) as u8);
            if !(1..13).contains(&y) {
                verts[vo & 15] = c;
                vo += 1;
            }
        } else {
            // From its new vertex, if it has one: an old vertex first
            // and two new ones after would write the byte that means
            // "count the new ones from nothing again".
            let r = (0..3).find(|&r| t[r] == next).unwrap_or(0);
            (a, b, c) = (t[r], t[(r + 1) % 3], t[(r + 2) % 3]);
            if (a, b, c) == (next, next + 1, next + 2) {
                // The table's first entry: three new vertices.
                codes.push(0xf0);
                next += 3;
            } else {
                let first = a == next;
                if first {
                    next += 1;
                }
                let z = if b == next { 0 } else { 15 };
                next += u32::from(z == 0);
                let w = if c == next { 0 } else { 15 };
                next += u32::from(w == 0);
                codes.push(if first { 0xfe } else { 0xff });
                extra.push((z << 4) | w);
                for (v, explicit) in [(a, !first), (b, z == 15), (c, w == 15)] {
                    if explicit {
                        varint(&mut extra, zigzag(v.wrapping_sub(last)));
                        last = v;
                    }
                }
            }
            for v in [a, b, c] {
                verts[vo & 15] = v;
                vo += 1;
            }
            edges[eo & 15] = (b, a);
            eo += 1;
        }
        edges[eo & 15] = (c, b);
        edges[(eo + 1) & 15] = (a, c);
        eo += 2;
    }
    let mut out = vec![0xe1];
    out.extend(codes);
    out.extend(extra);
    out.extend([0u8; 16]);
    out
}

/// A parsed value as JSON text, keys in their order.
pub(crate) fn write(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Num(n) => out.push_str(&format!("{}", if *n == 0.0 { 0.0 } else { *n })),
        Value::Str(s) => string(s, out),
        Value::Arr(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write(item, out);
            }
            out.push(']');
        }
        Value::Obj(fields) => {
            out.push('{');
            for (i, (k, item)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                string(k, out);
                out.push(':');
                write(item, out);
            }
            out.push('}');
        }
    }
}

pub(crate) fn set(obj: &mut Value, key: &str, v: Value) {
    if let Value::Obj(fields) = obj {
        match fields.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = v,
            None => fields.push((key.to_string(), v)),
        }
    }
}

pub(crate) fn num(v: usize) -> Value {
    Value::Num(v as f64)
}

pub(crate) fn obj(fields: Vec<(&str, Value)>) -> Value {
    Value::Obj(
        fields
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
    )
}

/// Bytes of an accessor's element.
fn element(a: &Value) -> Option<usize> {
    let component = match a.get("componentType").index()? {
        5120 | 5121 => 1,
        5122 | 5123 => 2,
        5125 | 5126 => 4,
        _ => return None,
    };
    let count = match a.get("type").str()? {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" | "MAT2" => 4,
        "MAT3" => 9,
        "MAT4" => 16,
        _ => return None,
    };
    Some(component * count)
}

/// The stream for one view, where it can be one: its mode, element
/// size, element count and bytes.
fn stream(
    doc: &Value,
    index: usize,
    view: &Value,
    data: &[u8],
) -> Option<(Mode, usize, usize, Vec<u8>)> {
    let mut size = view.get("byteStride").index();
    let mut triangles_of = false;
    let mut used = false;
    for a in doc.get("accessors").items() {
        if a.get("bufferView").index() != Some(index) || !a.get("sparse").is_null() {
            continue;
        }
        used = true;
        let e = element(a)?;
        match size {
            None => size = Some(e),
            Some(s) if s == e || !view.get("byteStride").is_null() => {}
            Some(_) => return None,
        }
    }
    let size = size.filter(|_| used)?;
    // Every primitive that draws with this view's indices draws
    // triangles: then the view is a triangle list.
    let accessors = doc.get("accessors");
    let mut drawn = 0;
    for mesh in doc.get("meshes").items() {
        for p in mesh.get("primitives").items() {
            let Some(i) = p.get("indices").index() else {
                continue;
            };
            if accessors.at(i).get("bufferView").index() == Some(index) {
                if p.get("mode").index().unwrap_or(4) != 4 {
                    return None;
                }
                drawn += 1;
            }
        }
    }
    if drawn > 0 {
        triangles_of = true;
    }
    if data.is_empty() || !data.len().is_multiple_of(size) {
        return None;
    }
    let count = data.len() / size;
    if triangles_of {
        if (size != 2 && size != 4) || count % 3 != 0 {
            return None;
        }
        let ids: Vec<u32> = data
            .chunks(size)
            .map(|b| match size {
                2 => u32::from(u16::from_le_bytes([b[0], b[1]])),
                _ => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            })
            .collect();
        let tris: Vec<[u32; 3]> = ids.chunks(3).map(|t| [t[0], t[1], t[2]]).collect();
        return Some((Mode::Triangles, size, count, triangles(&tris)));
    }
    if size % 4 != 0 || size > 256 {
        return None;
    }
    Some((Mode::Attributes, size, count, attributes(data, size)))
}

/// `glb` with its geometry and animation under
/// `EXT_meshopt_compression`, and what was done. A file that already
/// uses the extension, or keeps its data outside the GLB, is refused.
pub fn compress(glb: &[u8]) -> Result<(Vec<u8>, Compressed), String> {
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
    let uses = |key: &str| {
        doc.get(key)
            .items()
            .iter()
            .any(|e| e.str().is_some_and(|s| s.contains("meshopt_compression")))
    };
    if uses("extensionsUsed") {
        return Err("the GLB is meshopt-compressed already".into());
    }
    if doc.get("buffers").items().len() != 1 || !doc.get("buffers").at(0).get("uri").is_null() {
        return Err("a GLB with one buffer, its own, is compressed".into());
    }
    let mut report = Compressed::default();
    let mut out_bin: Vec<u8> = Vec::new();
    let mut views = doc.get("bufferViews").items().to_vec();
    report.views = views.len();
    for (i, view) in views.iter_mut().enumerate() {
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
        let start = out_bin.len();
        let packed = stream(&doc, i, view, data).filter(|(mode, size, count, bytes)| {
            // Smaller, and the same bytes when read back; a triangle
            // may come back from another of its corners (the codec
            // names it by an edge it shares with one before).
            let same = |back: &[u8]| {
                if *mode != Mode::Triangles {
                    return back == data;
                }
                let step = 3 * size;
                back.len() == data.len()
                    && back.chunks(step).zip(data.chunks(step)).all(|(b, d)| {
                        (0..3).any(|r| {
                            (0..3).all(|k| {
                                b[k * size..(k + 1) * size]
                                    == d[((k + r) % 3) * size..((k + r) % 3 + 1) * size]
                            })
                        })
                    })
            };
            bytes.len() < data.len()
                && decode(bytes, *count, *size, *mode, Filter::None).is_ok_and(|b| same(&b))
        });
        match packed {
            Some((mode, size, count, bytes)) => {
                out_bin.extend_from_slice(&bytes);
                report.compressed += 1;
                report.bytes_before += len;
                report.bytes_after += bytes.len();
                set(view, "buffer", num(1));
                let ext = obj(vec![
                    ("buffer", num(0)),
                    ("byteOffset", num(start)),
                    ("byteLength", num(bytes.len())),
                    ("byteStride", num(size)),
                    ("count", num(count)),
                    (
                        "mode",
                        Value::Str(
                            if mode == Mode::Triangles {
                                "TRIANGLES"
                            } else {
                                "ATTRIBUTES"
                            }
                            .into(),
                        ),
                    ),
                ]);
                set(
                    view,
                    "extensions",
                    obj(vec![("EXT_meshopt_compression", ext)]),
                );
            }
            None => {
                out_bin.extend_from_slice(data);
                set(view, "byteOffset", num(start));
            }
        }
    }
    if report.compressed == 0 {
        return Ok((glb.to_vec(), report));
    }
    out_bin.resize(out_bin.len().next_multiple_of(4), 0);
    set(&mut doc, "bufferViews", Value::Arr(views));
    let fallback = obj(vec![
        ("byteLength", num(bin.len())),
        (
            "extensions",
            obj(vec![(
                "EXT_meshopt_compression",
                obj(vec![("fallback", Value::Bool(true))]),
            )]),
        ),
    ]);
    set(
        &mut doc,
        "buffers",
        Value::Arr(vec![
            obj(vec![("byteLength", num(out_bin.len()))]),
            fallback,
        ]),
    );
    for key in ["extensionsUsed", "extensionsRequired"] {
        let mut names = doc.get(key).items().to_vec();
        names.push(Value::Str("EXT_meshopt_compression".into()));
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

/// What [`quantize`] did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Quantized {
    /// Accessors rewritten in fewer bytes.
    pub accessors: usize,
    /// Meshes whose positions became whole numbers (their nodes took
    /// the scale and the offset).
    pub meshes: usize,
    /// Bytes of the rewritten accessors, before.
    pub bytes_before: usize,
    /// The same, after.
    pub bytes_after: usize,
}

/// `glb` with its vertex attributes in fewer bytes
/// (`KHR_mesh_quantization`), and what was done; lossy. Normals and
/// tangents become signed bytes, texture coordinates within 0..1
/// unsigned 16-bit, float colours unsigned bytes. Positions become
/// whole numbers of `position_bits` bits (0 is 14) of the mesh's size,
/// the scale and offset that restore them put on the mesh's nodes:
/// only for a mesh with no skin and no morph targets whose nodes have
/// no children and are not animated. What cannot be rewritten stays.
pub fn quantize(glb: &[u8], position_bits: u8) -> Result<(Vec<u8>, Quantized), String> {
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
    if doc.get("buffers").items().len() != 1 || !doc.get("buffers").at(0).get("uri").is_null() {
        return Err("a GLB with one buffer, its own, is quantized".into());
    }
    if doc.get("extensionsUsed").items().iter().any(|e| {
        e.str()
            .is_some_and(|s| s.contains("compression") || s.contains("quantization"))
    }) {
        return Err("the GLB is compressed or quantized already".into());
    }
    let bits = if position_bits == 0 {
        14
    } else {
        position_bits.clamp(2, 16)
    };
    let accessors = doc.get("accessors").items().to_vec();
    // What reads each accessor: an attribute of one name (its stem), or
    // something a rewrite would break.
    let mut reader: Vec<Option<&str>> = vec![None; accessors.len()];
    let mut fixed = vec![false; accessors.len()];
    let mut note = |i: Option<usize>, stem: Option<&'static str>| {
        let Some(i) = i.filter(|&i| i < fixed.len()) else {
            return;
        };
        match (stem, reader[i]) {
            (Some(s), None) => reader[i] = Some(s),
            (Some(s), Some(r)) if s == r => {}
            _ => fixed[i] = true,
        }
    };
    const STEMS: [&str; 5] = ["POSITION", "NORMAL", "TANGENT", "TEXCOORD", "COLOR"];
    for mesh in doc.get("meshes").items() {
        for p in mesh.get("primitives").items() {
            note(p.get("indices").index(), None);
            for (name, a) in p.get("attributes").entries() {
                let stem = name.split('_').next().unwrap_or("");
                note(a.index(), STEMS.iter().copied().find(|s| *s == stem));
            }
            for t in p.get("targets").items() {
                for (name, a) in t.entries() {
                    note(a.index(), (name == "POSITION").then_some("MORPH"));
                }
            }
            if !p.get("extensions").is_null() {
                for (_, a) in p.get("attributes").entries() {
                    note(a.index(), None);
                }
            }
        }
    }
    for skin in doc.get("skins").items() {
        note(skin.get("inverseBindMatrices").index(), None);
    }
    for anim in doc.get("animations").items() {
        for s in anim.get("samplers").items() {
            note(s.get("input").index(), None);
            note(s.get("output").index(), None);
        }
    }
    // An accessor's floats, where it is plain float data in the buffer.
    let floats_of = |i: usize| -> Option<(Vec<f32>, usize)> {
        let a = &accessors[i];
        if fixed[i] || !a.get("sparse").is_null() || a.get("componentType").index() != Some(5126) {
            return None;
        }
        let n = match a.get("type").str()? {
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            _ => return None,
        };
        let view = doc.get("bufferViews").at(a.get("bufferView").index()?);
        let stride = view.get("byteStride").index().unwrap_or(4 * n);
        let at =
            view.get("byteOffset").index().unwrap_or(0) + a.get("byteOffset").index().unwrap_or(0);
        let count = a.get("count").index()?;
        let mut out = Vec::with_capacity(count * n);
        for k in 0..count {
            for c in 0..n {
                let from = at + k * stride + c * 4;
                let b = bin.get(from..from + 4)?;
                out.push(f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            }
        }
        out.iter().all(|v| v.is_finite()).then_some((out, n))
    };
    // Which meshes' positions may become whole numbers: those whose
    // position accessors (the targets' too) are plain floats no other
    // mesh reads, a target's offsets within 16 bits of the grid.
    let nodes = doc.get("nodes").items().to_vec();
    let mesh_count = doc.get("meshes").items().len();
    let mut held = vec![false; mesh_count];
    let mut animated = vec![false; nodes.len()];
    for anim in doc.get("animations").items() {
        for c in anim.get("channels").items() {
            if let Some(n) = c
                .get("target")
                .get("node")
                .index()
                .filter(|&n| n < nodes.len())
            {
                animated[n] = true;
            }
        }
    }
    let mut position_mesh: Vec<Option<usize>> = vec![None; accessors.len()];
    let mut morph_mesh: Vec<Option<usize>> = vec![None; accessors.len()];
    for (m, mesh) in doc.get("meshes").items().iter().enumerate() {
        for p in mesh.get("primitives").items() {
            for t in p.get("targets").items() {
                let Some(a) = t.get("POSITION").index().filter(|&a| a < accessors.len()) else {
                    continue;
                };
                match morph_mesh[a] {
                    None => morph_mesh[a] = Some(m),
                    Some(other) if other != m => {
                        held[m] = true;
                        held[other] = true;
                    }
                    _ => {}
                }
                if reader[a] != Some("MORPH") || floats_of(a).is_none_or(|(_, n)| n != 3) {
                    held[m] = true;
                }
            }
            if let Some(a) = p
                .get("attributes")
                .get("POSITION")
                .index()
                .filter(|&a| a < accessors.len())
            {
                match position_mesh[a] {
                    None => position_mesh[a] = Some(m),
                    Some(other) if other != m => {
                        held[m] = true;
                        held[other] = true;
                    }
                    _ => {}
                }
                if floats_of(a).is_none_or(|(_, n)| n != 3) {
                    held[m] = true;
                }
            }
        }
    }
    let mut frames: Vec<Option<([f32; 3], f32)>> = vec![None; mesh_count];
    for (m, mesh) in doc.get("meshes").items().iter().enumerate() {
        if held[m] {
            continue;
        }
        let (mut low, mut high) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for p in mesh.get("primitives").items() {
            if let Some((values, 3)) = p
                .get("attributes")
                .get("POSITION")
                .index()
                .and_then(floats_of)
            {
                for point in values.as_chunks::<3>().0 {
                    for c in 0..3 {
                        low[c] = low[c].min(point[c]);
                        high[c] = high[c].max(point[c]);
                    }
                }
            }
        }
        let extent = (0..3).map(|c| high[c] - low[c]).fold(0.0f32, f32::max);
        if extent > 0.0 && extent.is_finite() {
            frames[m] = Some((low, extent));
        }
    }
    let top = ((1u32 << bits) - 1) as f32;
    // A target's offsets are whole steps of the grid, signed 16 bits.
    for (a, m) in morph_mesh.iter().enumerate() {
        let Some((_, extent)) = m.and_then(|m| frames[m]) else {
            continue;
        };
        let fits = floats_of(a)
            .is_some_and(|(values, _)| values.iter().all(|v| (v / extent * top).abs() <= 32767.0));
        if !fits && let Some(m) = *m {
            frames[m] = None;
        }
    }
    // Rewrite.
    let mut report = Quantized::default();
    let mut new_accessors = accessors.clone();
    let mut fresh: Vec<(usize, Vec<u8>, usize)> = Vec::new();
    for i in 0..accessors.len() {
        let Some(stem) = reader[i] else { continue };
        let Some((values, n)) = floats_of(i) else {
            continue;
        };
        let count = values.len() / n;
        // The bytes, an element's size (a multiple of 4), the component
        // type, whether normalized, and whole-number bounds.
        let mut bytes: Vec<u8> = Vec::new();
        let (size, component, normalized, bounds) = match stem {
            "POSITION" => {
                let Some((low, extent)) = position_mesh[i].and_then(|m| frames[m]) else {
                    continue;
                };
                let (mut least, mut most) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
                for point in values.as_chunks::<3>().0 {
                    for c in 0..3 {
                        let q = (((point[c] - low[c]) / extent * top + 0.5) as u32).min(top as u32)
                            as u16;
                        least[c] = least[c].min(f64::from(q));
                        most[c] = most[c].max(f64::from(q));
                        bytes.extend_from_slice(&q.to_le_bytes());
                    }
                    bytes.extend_from_slice(&[0, 0]);
                }
                (8, 5123, false, Some((least, most)))
            }
            "MORPH" => {
                let Some((_, extent)) = morph_mesh[i].and_then(|m| frames[m]) else {
                    continue;
                };
                let (mut least, mut most) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
                for point in values.as_chunks::<3>().0 {
                    for c in 0..3 {
                        let q = (point[c] / extent * top).round() as i16;
                        least[c] = least[c].min(f64::from(q));
                        most[c] = most[c].max(f64::from(q));
                        bytes.extend_from_slice(&q.to_le_bytes());
                    }
                    bytes.extend_from_slice(&[0, 0]);
                }
                (8, 5122, false, Some((least, most)))
            }
            "NORMAL" | "TANGENT" if (stem == "NORMAL") == (n == 3) => {
                for point in values.chunks(n) {
                    for &v in point {
                        let q = (v.clamp(-1.0, 1.0) * 127.0).round() as i8;
                        bytes.push(q as u8);
                    }
                    bytes.resize(bytes.len().next_multiple_of(4), 0);
                }
                (4, 5120, true, None)
            }
            "TEXCOORD" if n == 2 && values.iter().all(|v| (0.0..=1.0).contains(v)) => {
                for &v in &values {
                    bytes.extend_from_slice(&((v * 65535.0).round() as u16).to_le_bytes());
                }
                (4, 5123, true, None)
            }
            "COLOR" => {
                for point in values.chunks(n) {
                    for &v in point {
                        bytes.push((v.clamp(0.0, 1.0) * 255.0).round() as u8);
                    }
                    bytes.resize(bytes.len().next_multiple_of(4), 0);
                }
                (4, 5121, true, None)
            }
            _ => continue,
        };
        report.accessors += 1;
        report.bytes_before += count * n * 4;
        report.bytes_after += bytes.len();
        let a = &mut new_accessors[i];
        if let Value::Obj(fields) = a {
            fields.retain(|(k, _)| {
                k != "byteOffset" && k != "min" && k != "max" && k != "normalized"
            });
        }
        set(a, "componentType", num(component));
        if normalized {
            set(a, "normalized", Value::Bool(true));
        }
        if let Some((least, most)) = bounds {
            set(
                a,
                "min",
                Value::Arr(least.iter().map(|&v| Value::Num(v)).collect()),
            );
            set(
                a,
                "max",
                Value::Arr(most.iter().map(|&v| Value::Num(v)).collect()),
            );
        }
        fresh.push((i, bytes, size));
    }
    if report.accessors == 0 {
        return Ok((glb.to_vec(), report));
    }
    // What restores the positions of a mesh that changed: the scale
    // and offset on its node; on a node of its own under one that has
    // children, a camera or an animation; or, for a skinned node, in
    // inverse bind matrices of its own (a skinned mesh's node does not
    // place it).
    let mut new_nodes = nodes.clone();
    let mut new_skins = doc.get("skins").items().to_vec();
    let mut new_animations = doc.get("animations").items().to_vec();
    let mut skin_for: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    let mats_of = |i: usize| -> Option<Vec<f32>> {
        let a = accessors.get(i)?;
        if a.get("componentType").index() != Some(5126) || a.get("type").str() != Some("MAT4") {
            return None;
        }
        let view = doc.get("bufferViews").at(a.get("bufferView").index()?);
        let stride = view.get("byteStride").index().unwrap_or(64);
        let at =
            view.get("byteOffset").index().unwrap_or(0) + a.get("byteOffset").index().unwrap_or(0);
        let mut out = Vec::new();
        for k in 0..a.get("count").index()? {
            for c in 0..16 {
                let from = at + k * stride + c * 4;
                let b = bin.get(from..from + 4)?;
                out.push(f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
            }
        }
        Some(out)
    };
    for (m, frame) in frames.iter().enumerate() {
        let Some((low, extent)) = frame else { continue };
        let changed = fresh
            .iter()
            .any(|(i, _, _)| position_mesh[*i] == Some(m) && reader[*i] == Some("POSITION"));
        if !changed {
            continue;
        }
        report.meshes += 1;
        let scale = f64::from(*extent) / f64::from(top);
        let offset = low.map(f64::from);
        // M · (the scale, then the offset), column-major.
        let restored = |mat: &[f64]| -> Vec<f64> {
            let mut out = mat.to_vec();
            for r in 0..4 {
                out[12 + r] = mat[r] * offset[0]
                    + mat[4 + r] * offset[1]
                    + mat[8 + r] * offset[2]
                    + mat[12 + r];
                for c in 0..3 {
                    out[c * 4 + r] = mat[c * 4 + r] * scale;
                }
            }
            out
        };
        let users: Vec<usize> = (0..nodes.len())
            .filter(|&n| nodes[n].get("mesh").index() == Some(m))
            .collect();
        for n in users {
            if let Some(s) = nodes[n]
                .get("skin")
                .index()
                .filter(|&s| s < new_skins.len())
            {
                let at = match skin_for.get(&(s, m)) {
                    Some(&at) => at,
                    None => {
                        let mut skin = new_skins[s].clone();
                        let joints = skin.get("joints").items().len();
                        let had = skin
                            .get("inverseBindMatrices")
                            .index()
                            .and_then(mats_of)
                            .filter(|v| v.len() == joints * 16);
                        let mut bytes = Vec::with_capacity(joints * 64);
                        for j in 0..joints {
                            let mat: Vec<f64> = match &had {
                                Some(v) => v[j * 16..j * 16 + 16]
                                    .iter()
                                    .map(|&x| f64::from(x))
                                    .collect(),
                                None => (0..16)
                                    .map(|k| if k % 5 == 0 { 1.0 } else { 0.0 })
                                    .collect(),
                            };
                            for x in restored(&mat) {
                                bytes.extend_from_slice(&(x as f32).to_le_bytes());
                            }
                        }
                        let accessor = new_accessors.len();
                        new_accessors.push(obj(vec![
                            ("componentType", num(5126)),
                            ("count", num(joints)),
                            ("type", Value::Str("MAT4".into())),
                        ]));
                        fresh.push((accessor, bytes, 0));
                        set(&mut skin, "inverseBindMatrices", num(accessor));
                        new_skins.push(skin);
                        skin_for.insert((s, m), new_skins.len() - 1);
                        new_skins.len() - 1
                    }
                };
                set(&mut new_nodes[n], "skin", num(at));
                continue;
            }
            if !nodes[n].get("children").items().is_empty()
                || animated[n]
                || !nodes[n].get("camera").is_null()
            {
                let child = new_nodes.len();
                let mut fields = vec![
                    ("mesh", num(m)),
                    (
                        "translation",
                        Value::Arr(offset.iter().map(|&v| Value::Num(v)).collect()),
                    ),
                    ("scale", Value::Arr(vec![Value::Num(scale); 3])),
                ];
                let weights = nodes[n].get("weights").clone();
                if !weights.is_null() {
                    fields.push(("weights", weights));
                }
                let node = &mut new_nodes[n];
                let mut children = node.get("children").items().to_vec();
                children.push(num(child));
                if let Value::Obj(own) = node {
                    own.retain(|(k, _)| k != "mesh" && k != "weights");
                }
                set(node, "children", Value::Arr(children));
                new_nodes.push(obj(fields));
                // A clip that drives the targets drives them there.
                for anim in &mut new_animations {
                    let mut channels = anim.get("channels").items().to_vec();
                    for c in &mut channels {
                        let target = c.get("target");
                        if target.get("node").index() == Some(n)
                            && target.get("path").str() == Some("weights")
                        {
                            let mut target = target.clone();
                            set(&mut target, "node", num(child));
                            set(c, "target", target);
                        }
                    }
                    set(anim, "channels", Value::Arr(channels));
                }
                continue;
            }
            let node = &mut new_nodes[n];
            if let Some(mat) = node.get("matrix").nums().filter(|v| v.len() == 16) {
                // Column-major: the new last column is M · (offset, 1).
                let mut out = mat.clone();
                for r in 0..4 {
                    out[12 + r] = mat[r] * offset[0]
                        + mat[4 + r] * offset[1]
                        + mat[8 + r] * offset[2]
                        + mat[12 + r];
                    for c in 0..3 {
                        out[c * 4 + r] = mat[c * 4 + r] * scale;
                    }
                }
                set(
                    node,
                    "matrix",
                    Value::Arr(out.into_iter().map(Value::Num).collect()),
                );
                continue;
            }
            let t = node
                .get("translation")
                .nums()
                .filter(|v| v.len() == 3)
                .unwrap_or(vec![0.0; 3]);
            let q = node
                .get("rotation")
                .nums()
                .filter(|v| v.len() == 4)
                .unwrap_or(vec![0.0, 0.0, 0.0, 1.0]);
            let s = node
                .get("scale")
                .nums()
                .filter(|v| v.len() == 3)
                .unwrap_or(vec![1.0; 3]);
            // T R S · (offset, then scale): the offset scaled and turned.
            let v = [s[0] * offset[0], s[1] * offset[1], s[2] * offset[2]];
            let (x, y, z, w) = (q[0], q[1], q[2], q[3]);
            let cross = [
                y * v[2] - z * v[1],
                z * v[0] - x * v[2],
                x * v[1] - y * v[0],
            ];
            let twice = [
                y * cross[2] - z * cross[1],
                z * cross[0] - x * cross[2],
                x * cross[1] - y * cross[0],
            ];
            let turned = [0, 1, 2].map(|c| v[c] + 2.0 * (w * cross[c] + twice[c]));
            set(
                node,
                "translation",
                Value::Arr((0..3).map(|c| Value::Num(t[c] + turned[c])).collect()),
            );
            set(
                node,
                "scale",
                Value::Arr(s.iter().map(|v| Value::Num(v * scale)).collect()),
            );
        }
    }
    // The buffer again: views something still reads, then the new ones.
    let old_views = doc.get("bufferViews").items().to_vec();
    let rewritten: Vec<usize> = fresh.iter().map(|(i, _, _)| *i).collect();
    let mut kept = vec![false; old_views.len()];
    for (i, a) in new_accessors.iter().enumerate() {
        if !rewritten.contains(&i)
            && let Some(v) = a.get("bufferView").index().filter(|&v| v < kept.len())
        {
            kept[v] = true;
        }
    }
    for image in doc.get("images").items() {
        if let Some(v) = image.get("bufferView").index().filter(|&v| v < kept.len()) {
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
    for (i, a) in new_accessors.iter_mut().enumerate() {
        if !rewritten.contains(&i)
            && let Some(v) = a.get("bufferView").index()
        {
            set(a, "bufferView", num(renumber[v]));
        }
    }
    for (i, bytes, size) in &fresh {
        out_bin.resize(out_bin.len().next_multiple_of(4), 0);
        set(&mut new_accessors[*i], "bufferView", num(views.len()));
        let mut view = vec![
            ("buffer", num(0)),
            ("byteOffset", num(out_bin.len())),
            ("byteLength", num(bytes.len())),
        ];
        // Vertex data says its stride; matrices are a plain view.
        if *size > 0 {
            view.push(("byteStride", num(*size)));
            view.push(("target", num(34962)));
        }
        views.push(obj(view));
        out_bin.extend_from_slice(bytes);
    }
    out_bin.resize(out_bin.len().next_multiple_of(4), 0);
    let mut images = doc.get("images").items().to_vec();
    for image in &mut images {
        if let Some(v) = image.get("bufferView").index() {
            set(image, "bufferView", num(renumber[v]));
        }
    }
    set(&mut doc, "accessors", Value::Arr(new_accessors));
    set(&mut doc, "nodes", Value::Arr(new_nodes));
    if !new_skins.is_empty() {
        set(&mut doc, "skins", Value::Arr(new_skins));
    }
    if !new_animations.is_empty() {
        set(&mut doc, "animations", Value::Arr(new_animations));
    }
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
        names.push(Value::Str("KHR_mesh_quantization".into()));
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

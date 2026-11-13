//! Compresses a GLB's geometry and animation with meshopt
//! (`EXT_meshopt_compression`), losslessly: every buffer view an
//! accessor reads is written as an attribute or triangle stream where
//! that is smaller, and reads back to the same bytes
//! (`docs/research.md` §51).
//!
//! Entry: [`compress`]. Images are left as they are. The streams are
//! version 0, the one every reader of the extension takes.

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
            (a, b, c) = (t[0], t[1], t[2]);
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
            // Smaller, and the same bytes when read back.
            bytes.len() < data.len()
                && decode(bytes, *count, *size, *mode, Filter::None).as_deref() == Ok(data)
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

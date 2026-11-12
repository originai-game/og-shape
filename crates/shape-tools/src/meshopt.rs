//! Meshopt-compressed buffer views, decoded (`docs/research.md` §42):
//! the three bitstreams and the four filters of glTF's
//! `EXT_meshopt_compression` and `KHR_meshopt_compression`, written from
//! the two specifications' text.
//!
//! Entry: [`decode`]. Integer work throughout but the filters, which are
//! f64 `+ - * /` and `sqrt` and a rounding cast, so the bytes are the
//! same on every host.

/// What a compressed view holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Attributes,
    Triangles,
    Indices,
}

/// What is done to a decoded attribute view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Filter {
    None,
    Octahedral,
    Quaternion,
    Exponential,
    Color,
}

/// The `count` elements of `stride` bytes that `data` holds.
pub(crate) fn decode(
    data: &[u8],
    count: usize,
    stride: usize,
    mode: Mode,
    filter: Filter,
) -> Result<Vec<u8>, String> {
    let index = matches!(mode, Mode::Triangles | Mode::Indices);
    if index && stride != 2 && stride != 4 {
        return Err(format!("indices of {stride} bytes"));
    }
    if index && filter != Filter::None {
        return Err("a filter on indices".into());
    }
    // The least a stream can take for what it claims, so a count is
    // believed only as far as the bytes behind it go.
    let least = match mode {
        Mode::Attributes => count.saturating_mul(stride) / 1024,
        Mode::Triangles => count / 3,
        Mode::Indices => count,
    };
    if least > data.len() {
        return Err(format!("{count} elements claimed of {} bytes", data.len()));
    }
    let mut out = match mode {
        Mode::Attributes => attributes(data, count, stride)?,
        Mode::Triangles => triangles(data, count, stride)?,
        Mode::Indices => indices(data, count, stride)?,
    };
    match filter {
        Filter::None => {}
        Filter::Octahedral => octahedral(&mut out, stride)?,
        Filter::Quaternion => quaternion(&mut out, stride)?,
        Filter::Exponential => exponential(&mut out),
        Filter::Color => color(&mut out, stride)?,
    }
    Ok(out)
}

fn unzigzag8(v: u8) -> u8 {
    if v & 1 != 0 { !(v >> 1) } else { v >> 1 }
}

/// Mode 0: per byte of the element, deltas from the element before, in
/// groups of sixteen. Version 0 (`0xa0`) is `EXT_meshopt_compression`'s;
/// version 1 (`0xa1`, `KHR_meshopt_compression`) adds a control mode a
/// byte and a block, and a delta kind for every four bytes.
fn attributes(data: &[u8], count: usize, stride: usize) -> Result<Vec<u8>, String> {
    if stride == 0 || stride > 256 || !stride.is_multiple_of(4) {
        return Err(format!("attributes of {stride} bytes"));
    }
    let v1 = match data.first() {
        Some(0xa0) => false,
        Some(0xa1) => true,
        _ => return Err("not an attribute stream".into()),
    };
    let channels = if v1 { stride / 4 } else { 0 };
    let tail = (stride + channels).max(if v1 { 24 } else { 32 });
    if data.len() < 1 + tail {
        return Err("not an attribute stream".into());
    }
    let end = data.len() - tail;
    let short = || "the attribute stream is cut short".to_string();
    let base = data.len() - stride - channels;
    let mut last = data[base..base + stride].to_vec();
    let modes = &data[base + stride..];
    for m in modes {
        if m & 15 > 2 || (m & 15 < 2 && m >> 4 != 0) {
            return Err(format!("channel mode {m:#x}"));
        }
    }
    let mut out = vec![0u8; count * stride];
    let block_max = ((8192 / stride) & !15).min(256);
    let mut at = 1;
    let mut done = 0;
    // The block's deltas, a row of 256 for each byte of the element.
    let mut deltas = vec![0u8; stride * 256];
    while done < count {
        let n = (count - done).min(block_max);
        let groups = n.div_ceil(16);
        let control = at;
        at += channels;
        if at > end {
            return Err(short());
        }
        for k in 0..stride {
            let row = &mut deltas[k * 256..k * 256 + 256];
            // The bits a delta takes under each of a group's four
            // headers: 0 for none, 8 for a whole byte.
            let widths: [usize; 4] = if v1 {
                match (data[control + k / 4] >> ((k % 4) * 2)) & 3 {
                    0 => [0, 1, 2, 4],
                    1 => [1, 2, 4, 8],
                    2 => {
                        row[..n].fill(0);
                        continue;
                    }
                    _ => {
                        row[..n].copy_from_slice(
                            data.get(at..at + n)
                                .filter(|_| at + n <= end)
                                .ok_or_else(short)?,
                        );
                        at += n;
                        continue;
                    }
                }
            } else {
                [0, 2, 4, 8]
            };
            let header = at;
            at += groups.div_ceil(4);
            if at > end {
                return Err(short());
            }
            for g in 0..groups {
                let width = widths[usize::from((data[header + g / 4] >> ((g % 4) * 2)) & 3)];
                let d = &mut row[g * 16..g * 16 + 16];
                match width {
                    0 => d.fill(0),
                    8 => {
                        d.copy_from_slice(
                            data.get(at..at + 16)
                                .filter(|_| at + 16 <= end)
                                .ok_or_else(short)?,
                        );
                        at += 16;
                    }
                    _ => {
                        let sentinel = (1u8 << width) - 1;
                        let per = 8 / width;
                        let mut extra = at + 16 / per;
                        if extra > end {
                            return Err(short());
                        }
                        for (i, v) in d.iter_mut().enumerate() {
                            let byte = data[at + i / per];
                            // One bit a delta fills a byte from its low
                            // end; two and four from its high end.
                            let shift = if width == 1 {
                                i % per
                            } else {
                                8 - width * (i % per + 1)
                            };
                            let small = (byte >> shift) & sentinel;
                            *v = if small == sentinel {
                                let b =
                                    *data.get(extra).filter(|_| extra < end).ok_or_else(short)?;
                                extra += 1;
                                b
                            } else {
                                small
                            };
                        }
                        at = extra;
                    }
                }
            }
        }
        for c in 0..stride / 4 {
            let mode = modes.get(c).copied().unwrap_or(0);
            let k = c * 4;
            let row = |j: usize, i: usize| deltas[(k + j) * 256 + i];
            let slot = |i: usize| (done + i) * stride + k;
            match mode & 15 {
                0 => {
                    for j in 0..4 {
                        let mut p = last[k + j];
                        for i in 0..n {
                            p = p.wrapping_add(unzigzag8(row(j, i)));
                            out[slot(i) + j] = p;
                        }
                        last[k + j] = p;
                    }
                }
                1 => {
                    for j in [0, 2] {
                        let mut p = u16::from_le_bytes([last[k + j], last[k + j + 1]]);
                        for i in 0..n {
                            let d = u16::from_le_bytes([row(j, i), row(j + 1, i)]);
                            p = p.wrapping_add(if d & 1 != 0 { !(d >> 1) } else { d >> 1 });
                            out[slot(i) + j..slot(i) + j + 2].copy_from_slice(&p.to_le_bytes());
                        }
                        last[k + j..k + j + 2].copy_from_slice(&p.to_le_bytes());
                    }
                }
                _ => {
                    let turn = u32::from(mode >> 4);
                    let mut p =
                        u32::from_le_bytes([last[k], last[k + 1], last[k + 2], last[k + 3]]);
                    for i in 0..n {
                        let d = u32::from_le_bytes([row(0, i), row(1, i), row(2, i), row(3, i)]);
                        p ^= d.rotate_right(turn);
                        out[slot(i)..slot(i) + 4].copy_from_slice(&p.to_le_bytes());
                    }
                    last[k..k + 4].copy_from_slice(&p.to_le_bytes());
                }
            }
        }
        done += n;
    }
    if at != end {
        return Err("the attribute stream holds more than its elements".into());
    }
    Ok(out)
}

/// An unsigned LEB128 value of at most five bytes.
fn varint(data: &[u8], at: &mut usize, end: usize) -> Result<u32, String> {
    let mut v = 0u32;
    for i in 0..5 {
        let b = *data
            .get(*at)
            .filter(|_| *at < end)
            .ok_or("the index stream is cut short")?;
        *at += 1;
        v |= u32::from(b & 0x7f) << (7 * i);
        if b & 0x80 == 0 {
            return Ok(v);
        }
    }
    Ok(v)
}

fn put(out: &mut [u8], i: usize, stride: usize, v: u32) {
    if stride == 2 {
        out[i * 2..i * 2 + 2].copy_from_slice(&(v as u16).to_le_bytes());
    } else {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
}

/// Mode 1: a byte a triangle, naming an edge and a vertex seen lately.
fn triangles(data: &[u8], count: usize, stride: usize) -> Result<Vec<u8>, String> {
    if !count.is_multiple_of(3) {
        return Err(format!("{count} indices are not whole triangles"));
    }
    let tris = count / 3;
    if data.len() < 1 + tris + 16 || data[0] != 0xe1 {
        return Err("not a triangle stream".into());
    }
    let end = data.len() - 16;
    let aux = &data[end..];
    let mut at = 1 + tris;
    let mut out = vec![0u8; count * stride];
    let (mut next, mut last) = (0u32, 0u32);
    let mut edges = [(0u32, 0u32); 16];
    let mut verts = [0u32; 16];
    let (mut eo, mut vo) = (0usize, 0usize);
    let index = |at: &mut usize, last: &mut u32| -> Result<u32, String> {
        let v = varint(data, at, end)?;
        let d = if v & 1 != 0 { !(v >> 1) } else { v >> 1 };
        *last = last.wrapping_add(d);
        Ok(*last)
    };
    for t in 0..tris {
        let code = data[1 + t];
        let (a, b, c);
        if code < 0xf0 {
            (a, b) = edges[(eo.wrapping_sub(1 + usize::from(code >> 4))) & 15];
            let y = code & 15;
            let fresh;
            match y {
                0 => {
                    c = next;
                    next = next.wrapping_add(1);
                    fresh = true;
                }
                1..=12 => {
                    c = verts[(vo.wrapping_sub(1 + usize::from(y))) & 15];
                    fresh = false;
                }
                13 | 14 => {
                    c = if y == 13 {
                        last.wrapping_sub(1)
                    } else {
                        last.wrapping_add(1)
                    };
                    last = c;
                    fresh = true;
                }
                _ => {
                    c = index(&mut at, &mut last)?;
                    fresh = true;
                }
            }
            if fresh {
                verts[vo & 15] = c;
                vo = vo.wrapping_add(1);
            }
        } else {
            let explicit = code >= 0xfe;
            let zw = if explicit {
                let zw = *data
                    .get(at)
                    .filter(|_| at < end)
                    .ok_or("the triangle stream is cut short")?;
                at += 1;
                if zw == 0 {
                    next = 0;
                }
                zw
            } else {
                aux[usize::from(code & 15)]
            };
            let (z, w) = (zw >> 4, zw & 15);
            let seen = |k: u8| verts[(vo.wrapping_sub(usize::from(k))) & 15];
            let mut ta = 0;
            if code != 0xff {
                ta = next;
                next = next.wrapping_add(1);
            }
            let mut tb = if z == 0 {
                next = next.wrapping_add(1);
                next.wrapping_sub(1)
            } else {
                seen(z)
            };
            let mut tc = if w == 0 {
                next = next.wrapping_add(1);
                next.wrapping_sub(1)
            } else {
                seen(w)
            };
            if code == 0xff {
                ta = index(&mut at, &mut last)?;
            }
            if explicit && z == 15 {
                tb = index(&mut at, &mut last)?;
            }
            if explicit && w == 15 {
                tc = index(&mut at, &mut last)?;
            }
            (a, b, c) = (ta, tb, tc);
            let mut push = |v: u32| {
                verts[vo & 15] = v;
                vo = vo.wrapping_add(1);
            };
            push(a);
            if z == 0 || (explicit && z == 15) {
                push(b);
            }
            if w == 0 || (explicit && w == 15) {
                push(c);
            }
            edges[eo & 15] = (b, a);
            eo = eo.wrapping_add(1);
        }
        edges[eo & 15] = (c, b);
        edges[(eo + 1) & 15] = (a, c);
        eo = eo.wrapping_add(2);
        put(&mut out, t * 3, stride, a);
        put(&mut out, t * 3 + 1, stride, b);
        put(&mut out, t * 3 + 2, stride, c);
    }
    if at != end {
        return Err("the triangle stream holds more than its triangles".into());
    }
    Ok(out)
}

/// Mode 2: each index a delta from one of two running values.
fn indices(data: &[u8], count: usize, stride: usize) -> Result<Vec<u8>, String> {
    if data.len() < 1 + count + 4 || data[0] != 0xd1 {
        return Err("not an index stream".into());
    }
    let end = data.len() - 4;
    let mut at = 1;
    let mut last = [0u32; 2];
    let mut out = vec![0u8; count * stride];
    for i in 0..count {
        let v = varint(data, &mut at, end)?;
        let which = (v & 1) as usize;
        let d = if v & 2 != 0 { !(v >> 2) } else { v >> 2 };
        last[which] = last[which].wrapping_add(d);
        put(&mut out, i, stride, last[which]);
    }
    if at != end {
        return Err("the index stream holds more than its indices".into());
    }
    Ok(out)
}

/// `x` to the nearest integer, halves away from zero.
fn round(x: f64) -> i32 {
    (if x >= 0.0 { x + 0.5 } else { x - 0.5 }) as i32
}

/// Filter 1: unit vectors, from two octahedral components and the
/// integer that stands for one.
fn octahedral(out: &mut [u8], stride: usize) -> Result<(), String> {
    if stride != 4 && stride != 8 {
        return Err(format!("octahedral vectors of {stride} bytes"));
    }
    let wide = stride == 8;
    let max = if wide { 32767.0 } else { 127.0 };
    for e in out.chunks_exact_mut(stride) {
        let read = |i: usize| {
            if wide {
                f64::from(i16::from_le_bytes([e[i * 2], e[i * 2 + 1]]))
            } else {
                f64::from(e[i] as i8)
            }
        };
        let one = read(2);
        let (mut x, mut y, mut z) = (0.0, 0.0, 0.0);
        if one != 0.0 {
            x = read(0) / one;
            y = read(1) / one;
            z = 1.0 - x.abs() - y.abs();
            let t = z.min(0.0);
            x += if x >= 0.0 { t } else { -t };
            y += if y >= 0.0 { t } else { -t };
            let len = (x * x + y * y + z * z).sqrt();
            if len > 0.0 {
                (x, y, z) = (x / len, y / len, z / len);
            }
        }
        for (i, v) in [x, y, z].into_iter().enumerate() {
            let r = round(v * max);
            if wide {
                e[i * 2..i * 2 + 2].copy_from_slice(&(r as i16).to_le_bytes());
            } else {
                e[i] = r as i8 as u8;
            }
        }
    }
    Ok(())
}

/// Filter 2: unit quaternions, from the three smaller components.
fn quaternion(out: &mut [u8], stride: usize) -> Result<(), String> {
    if stride != 8 {
        return Err(format!("quaternions of {stride} bytes"));
    }
    let range = 1.0 / 2f64.sqrt();
    for e in out.as_chunks_mut::<8>().0 {
        let read = |i: usize| i16::from_le_bytes([e[i * 2], e[i * 2 + 1]]);
        let last = read(3);
        let one = f64::from(last | 3);
        let v = [0, 1, 2].map(|i| f64::from(read(i)) / one * range);
        let w = (1.0 - v[0] * v[0] - v[1] * v[1] - v[2] * v[2])
            .max(0.0)
            .sqrt();
        let top = (last & 3) as usize;
        for (k, x) in [w, v[0], v[1], v[2]].into_iter().enumerate() {
            let i = (top + k) % 4;
            e[i * 2..i * 2 + 2].copy_from_slice(&(round(x * 32767.0) as i16).to_le_bytes());
        }
    }
    Ok(())
}

/// Filter 3: floats, from an exponent in the top byte and a mantissa in
/// the other three.
fn exponential(out: &mut [u8]) {
    for e in out.as_chunks_mut::<4>().0 {
        let v = i32::from_le_bytes([e[0], e[1], e[2], e[3]]);
        let exp = v >> 24;
        let mut x = f64::from((v << 8) >> 8);
        // Powers of two multiply exactly.
        for _ in 0..exp.unsigned_abs() {
            x = if exp > 0 { x * 2.0 } else { x * 0.5 };
        }
        e.copy_from_slice(&(x as f32).to_le_bytes());
    }
}

/// Filter 4: colours, from luma, two chroma differences and an alpha
/// whose top bit says how many bits all four have.
fn color(out: &mut [u8], stride: usize) -> Result<(), String> {
    if stride != 4 && stride != 8 {
        return Err(format!("colours of {stride} bytes"));
    }
    let wide = stride == 8;
    let max = if wide { 65535.0 } else { 255.0 };
    for e in out.chunks_exact_mut(stride) {
        let raw = |i: usize| {
            if wide {
                u16::from_le_bytes([e[i * 2], e[i * 2 + 1]])
            } else {
                u16::from(e[i])
            }
        };
        let signed = |i: usize| {
            if wide {
                i32::from(raw(i) as i16)
            } else {
                i32::from(raw(i) as u8 as i8)
            }
        };
        let alpha = i32::from(raw(3));
        // All ones up to the alpha's top bit: the scale of the others.
        let scale = if alpha == 0 {
            0
        } else {
            (1i32 << (32 - alpha.leading_zeros())) - 1
        };
        let (y, co, cg) = (i32::from(raw(0)), signed(1), signed(2));
        let a = alpha & (scale >> 1);
        let rgba = [y + co - cg, y + cg, y - co - cg, (a << 1) | (a & 1)];
        for (i, v) in rgba.into_iter().enumerate() {
            let r = if scale == 0 {
                0
            } else {
                round(f64::from(v) * max / f64::from(scale)).clamp(0, max as i32)
            };
            if wide {
                e[i * 2..i * 2 + 2].copy_from_slice(&(r as u16).to_le_bytes());
            } else {
                e[i] = r as u8;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_group_of_four_bit_deltas_is_the_specification_s() {
        // The specification's example group, as the one byte of sixteen
        // one-byte elements from a baseline of 0.
        let group = [0x17, 0x5f, 0xf0, 0xbc, 0x77, 0xa9, 0x21, 0x00, 0x34, 0xb5];
        let mut data = vec![0xa0, 0x02, 0, 0, 0];
        // Four byte planes: the first holds the group, the others rest.
        data.splice(2..2, group);
        data.extend([0u8; 32]);
        let out = attributes(&data, 16, 4).unwrap();
        let deltas = [-1, -4, -3, 26, -91, 0, -6, 6, -4, -4, 5, -5, 1, -1, 0, 0];
        let mut at = 0i32;
        for (i, d) in deltas.iter().enumerate() {
            at += d;
            assert_eq!(out[i * 4], at as u8, "element {i}");
            assert_eq!(out[i * 4 + 1..i * 4 + 4], [0, 0, 0]);
        }
    }

    #[test]
    fn varints_are_the_specification_s() {
        // Low bits first (LEB128). The specification's third example,
        // `ff a0 05 => 0x1fd005`, reads high bits first and disagrees
        // with its second and with the files; it is not kept.
        assert_eq!(varint(&[0x7f], &mut 0, 1), Ok(0x7f));
        assert_eq!(varint(&[0x81, 0x04], &mut 0, 2), Ok(0x201));
        assert_eq!(varint(&[0xff, 0xa0, 0x05], &mut 0, 3), Ok(0x1_507f));
        assert!(varint(&[0xff, 0xa0, 0x05], &mut 0, 2).is_err());
    }

    #[test]
    fn filtered_vectors_and_turns_are_unit_ones() {
        // Octahedral, 8 bits: +z, +x, and (−½, −½) on the far side.
        let mut v = vec![0, 0, 127, 9, 127, 0, 127, 0, 0xc0, 0xc0, 127, 0];
        octahedral(&mut v, 4).unwrap();
        assert_eq!(v[..8], [0, 0, 127, 9, 127, 0, 0, 0]);
        let far = [v[8] as i8, v[9] as i8, v[10] as i8];
        assert!(far[0] < 0 && far[0] == far[1] && far[2] < 0, "{far:?}");
        let len: i32 = far.iter().map(|c| i32::from(*c) * i32::from(*c)).sum();
        assert!((len - 127 * 127).abs() < 2 * 127, "{far:?}");
        // A quaternion at 12 bits whose largest part is y, the others
        // none; and one with x a half.
        let mut q = Vec::new();
        for c in [0i16, 0, 0, 2044 | 1, 1447, 0, 0, 2044 | 3] {
            q.extend(c.to_le_bytes());
        }
        quaternion(&mut q, 8).unwrap();
        let at = |i: usize| i16::from_le_bytes([q[i * 2], q[i * 2 + 1]]);
        assert_eq!([at(0), at(1), at(2), at(3)], [0, 32767, 0, 0]);
        // (x, y, z, w) = (½, 0, 0, √¾): the largest is w, x comes next.
        assert!(
            (at(4) - 16379).abs() <= 1 && (at(7) - 28379).abs() <= 2,
            "{} {}",
            at(4),
            at(7)
        );
        assert_eq!([at(5), at(6)], [0, 0]);
    }
}

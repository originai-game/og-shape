//! Test support: a meshopt encoder written from the glTF extensions'
//! text, small rather than tight, to build compressed files by hand.

fn group(out: &mut Vec<u8>, d: &[u8], width: usize) {
    if width == 8 {
        out.extend(d);
        return;
    }
    let sentinel = (1u8 << width) - 1;
    let per = 8 / width;
    let mut packed = vec![0u8; 16 / per];
    for (i, v) in d.iter().enumerate() {
        let small = (*v).min(sentinel);
        let shift = if width == 1 {
            i % per
        } else {
            8 - width * (i % per + 1)
        };
        packed[i / per] |= small << shift;
    }
    out.extend(packed);
    out.extend(d.iter().filter(|v| **v >= sentinel));
}

/// `data`, elements of `stride` bytes, as an attribute stream: version
/// 0 with no `channels`, else version 1 with those channel modes.
pub fn attributes(data: &[u8], stride: usize, channels: Option<&[u8]>) -> Vec<u8> {
    let count = data.len() / stride;
    let mut out = vec![if channels.is_some() { 0xa1 } else { 0xa0 }];
    let block_max = ((8192 / stride) & !15).min(256);
    // The baseline is the first element but for one byte, so the first
    // delta is not always nothing.
    let mut base = data[..stride].to_vec();
    base[0] = base[0].wrapping_add(7);
    let mut last = base.clone();
    let mut done = 0;
    while done < count {
        let n = (count - done).min(block_max);
        let groups = n.div_ceil(16);
        let mut rows = vec![vec![0u8; groups * 16]; stride];
        for c in 0..stride / 4 {
            let mode = channels.map_or(0, |m| m[c]);
            let k = c * 4;
            for i in 0..n {
                let e = &data[(done + i) * stride + k..][..4];
                match mode & 15 {
                    0 => {
                        for j in 0..4 {
                            let d = e[j].wrapping_sub(last[k + j]);
                            rows[k + j][i] = if d & 0x80 != 0 { !(d << 1) } else { d << 1 };
                        }
                    }
                    1 => {
                        for j in [0, 2] {
                            let d = u16::from_le_bytes([e[j], e[j + 1]])
                                .wrapping_sub(u16::from_le_bytes([last[k + j], last[k + j + 1]]));
                            let z = if d & 0x8000 != 0 { !(d << 1) } else { d << 1 };
                            rows[k + j][i] = z as u8;
                            rows[k + j + 1][i] = (z >> 8) as u8;
                        }
                    }
                    _ => {
                        let p =
                            u32::from_le_bytes([last[k], last[k + 1], last[k + 2], last[k + 3]]);
                        let d = (u32::from_le_bytes([e[0], e[1], e[2], e[3]]) ^ p)
                            .rotate_left(u32::from(mode >> 4));
                        for (j, b) in d.to_le_bytes().into_iter().enumerate() {
                            rows[k + j][i] = b;
                        }
                    }
                }
                last[k..k + 4].copy_from_slice(e);
            }
        }
        // Version 1: a control mode a byte, every kind taken in turn.
        let controls: Vec<u8> = (0..stride)
            .map(|k| match rows[k].iter().all(|d| *d == 0) {
                true => 2,
                false => [0, 1, 3][k % 3],
            })
            .collect();
        if channels.is_some() {
            for c in controls.chunks(4) {
                out.push(c[0] | (c[1] << 2) | (c[2] << 4) | (c[3] << 6));
            }
        }
        for k in 0..stride {
            let widths: [usize; 4] = match (channels.is_some(), controls[k]) {
                (false, _) => [0, 2, 4, 8],
                (true, 0) => [0, 1, 2, 4],
                (true, 1) => [1, 2, 4, 8],
                (true, 2) => continue,
                _ => {
                    out.extend(&rows[k][..n]);
                    continue;
                }
            };
            let cost = |d: &[u8], w: usize| match w {
                0 if d.iter().all(|v| *v == 0) => 0,
                0 => usize::MAX,
                8 => 16,
                w => 2 * w + d.iter().filter(|v| **v >= (1 << w) - 1).count(),
            };
            let picks: Vec<usize> = rows[k]
                .chunks(16)
                .map(|d| (0..4).min_by_key(|i| cost(d, widths[*i])).unwrap())
                .collect();
            for p in picks.chunks(4) {
                out.push(
                    p.iter()
                        .enumerate()
                        .map(|(i, v)| (*v as u8) << (i * 2))
                        .sum(),
                );
            }
            for (d, p) in rows[k].chunks(16).zip(&picks) {
                if widths[*p] > 0 {
                    group(&mut out, d, widths[*p]);
                }
            }
        }
        done += n;
    }
    let tail = stride + channels.map_or(0, <[u8]>::len);
    let least: usize = if channels.is_some() { 24 } else { 32 };
    out.extend(vec![0u8; least.saturating_sub(tail)]);
    out.extend(base);
    out.extend(channels.unwrap_or(&[]));
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

/// A triangle list as a triangle stream.
pub fn triangles(tris: &[[u32; 3]]) -> Vec<u8> {
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

/// Indices as an index stream, the two running values taken in turn.
pub fn indices(values: &[u32]) -> Vec<u8> {
    let mut out = vec![0xd1];
    let mut last = [0u32; 2];
    for (i, v) in values.iter().enumerate() {
        let which = i & 1;
        varint(
            &mut out,
            (zigzag(v.wrapping_sub(last[which])) << 1) | which as u32,
        );
        last[which] = *v;
    }
    out.extend([0u8; 4]);
    out
}

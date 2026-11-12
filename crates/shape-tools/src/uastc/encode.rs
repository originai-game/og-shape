//! Encodes 4×4 texels to a UASTC block (`docs/research.md` §50).
//!
//! Entry: [`encode_block`]. The texels are fitted in each of a set of
//! modes (one, two or three subsets; one or two planes of weights) and
//! the fit with the least squared error against the texels is packed.
//! A fit is a line through the texels' colours: the direction they vary
//! most along, its ends rounded to what the mode's endpoints hold, each
//! texel given the nearest weight, and the ends then solved again for
//! those weights. Errors are whole numbers measured with the decoder's
//! own arithmetic, so the choice is the same on every host.
//!
//! The hints a transcoder to ETC1 and ETC2 alpha reads are found by
//! trying what it would do with each. The BC1 hints are left off (the
//! transcoder then works the block out itself).

use super::{
    COMPONENTS, PATTERNS2, PATTERNS3, PATTERNS7, PLANES, RANGES, SOLID, SUBSETS, WEIGHT_BITS,
    WEIGHTS1, WEIGHTS2, WEIGHTS3, WEIGHTS4, WEIGHTS5, mix, range_parts, widened,
};

/// Each mode's prefix code and its length.
const CODES: [(u8, u8); 19] = [
    (0x1, 4),
    (0x35, 6),
    (0x1D, 5),
    (0x3, 5),
    (0x13, 5),
    (0xB, 5),
    (0x1B, 5),
    (0x7, 5),
    (0x17, 5),
    (0xF, 5),
    (0x2, 3),
    (0x0, 2),
    (0x6, 3),
    (0x1F, 5),
    (0xD, 5),
    (0x5, 7),
    (0x15, 6),
    (0x25, 6),
    (0x9, 4),
];

/// ETC1's intensity tables, least to most.
const ETC1_TABLES: [[i32; 4]; 8] = [
    [-8, -2, 2, 8],
    [-17, -5, 5, 17],
    [-29, -9, 9, 29],
    [-42, -13, 13, 42],
    [-60, -18, 18, 60],
    [-80, -24, 24, 80],
    [-106, -33, 33, 106],
    [-183, -47, 47, 183],
];

/// ETC2's alpha modifier tables.
const EAC_TABLES: [[i32; 8]; 16] = [
    [-3, -6, -9, -15, 2, 5, 8, 14],
    [-3, -7, -10, -13, 2, 6, 9, 12],
    [-2, -5, -8, -13, 1, 4, 7, 12],
    [-2, -4, -6, -13, 1, 3, 5, 12],
    [-3, -6, -8, -12, 2, 5, 7, 11],
    [-3, -7, -9, -11, 2, 6, 8, 10],
    [-4, -7, -8, -11, 3, 6, 7, 10],
    [-3, -5, -8, -11, 2, 4, 7, 10],
    [-2, -6, -8, -10, 1, 5, 7, 9],
    [-2, -5, -8, -10, 1, 4, 7, 9],
    [-2, -4, -8, -10, 1, 3, 7, 9],
    [-2, -5, -7, -10, 1, 4, 6, 9],
    [-3, -4, -7, -10, 2, 3, 6, 9],
    [-1, -2, -3, -10, 0, 1, 2, 9],
    [-4, -6, -8, -9, 3, 5, 7, 8],
    [-3, -5, -7, -9, 2, 4, 6, 8],
];

/// What an endpoint range can hold: for each 8-bit value the nearest
/// one it holds, and that one's code.
struct Range {
    near: [u8; 256],
    code: [u8; 256],
}

impl Range {
    fn new(range: u8) -> Self {
        let (plain, base) = range_parts(range);
        let levels = (1usize << plain) * if base == 0 { 1 } else { base as usize };
        let mut held = [None::<u8>; 256];
        for code in 0..levels {
            if let Some(v) = widened(range, code as u32) {
                held[v as usize] = Some(code as u8);
            }
        }
        let (mut near, mut code) = ([0u8; 256], [0u8; 256]);
        for v in 0..256usize {
            // The nearest held value, the lower of two as near.
            let mut best = (usize::MAX, 0usize);
            for (h, c) in held.iter().enumerate() {
                if c.is_some() && h.abs_diff(v) < best.0 {
                    best = (h.abs_diff(v), h);
                }
            }
            near[v] = best.1 as u8;
            code[v] = held[best.1].unwrap_or(0);
        }
        Range { near, code }
    }

    fn round(&self, v: f64) -> u8 {
        self.near[(v.clamp(0.0, 255.0) + 0.5) as usize]
    }
}

/// The ranges the modes use, made once for an image.
pub struct Ranges {
    by_range: Vec<(u8, Range)>,
}

impl Ranges {
    pub fn new() -> Self {
        Ranges {
            by_range: [7, 8, 11, 12, 13, 18, 19, 20]
                .into_iter()
                .map(|r| (r, Range::new(r)))
                .collect(),
        }
    }

    fn of(&self, range: u8) -> &Range {
        let i = self
            .by_range
            .iter()
            .position(|(r, _)| *r == range)
            .unwrap_or(0);
        &self.by_range[i].1
    }
}

fn weights_of(bits: u8) -> &'static [u8] {
    match bits {
        1 => &WEIGHTS1,
        2 => &WEIGHTS2,
        3 => &WEIGHTS3,
        4 => &WEIGHTS4,
        _ => &WEIGHTS5,
    }
}

/// A line fitted to some texels in some components.
struct Fit {
    lo: [u8; 4],
    hi: [u8; 4],
    /// Each texel's weight index (texels outside the fit: 0).
    weights: [u8; 16],
    err: u32,
}

/// The line through `members`' colours in `comps`, with ends the range
/// holds and each member's weight out of `table`.
fn fit(px: &[[u8; 4]; 16], members: &[usize], comps: &[usize], range: &Range, table: &[u8]) -> Fit {
    let n = members.len() as f64;
    let mut mean = [0.0f64; 4];
    for &i in members {
        for &c in comps {
            mean[c] += f64::from(px[i][c]);
        }
    }
    for &c in comps {
        mean[c] /= n;
    }
    // The direction of most variance: the covariance matrix applied a
    // few times to its own column of the component that varies most
    // (never square to that direction, as the box's diagonal can be).
    let mut axis = [0.0f64; 4];
    let mut cov = [[0.0f64; 4]; 4];
    for &i in members {
        for &a in comps {
            for &b in comps {
                cov[a][b] += (f64::from(px[i][a]) - mean[a]) * (f64::from(px[i][b]) - mean[b]);
            }
        }
    }
    let widest = comps
        .iter()
        .copied()
        .fold(comps[0], |w, c| if cov[c][c] > cov[w][w] { c } else { w });
    for &c in comps {
        axis[c] = cov[c][widest];
    }
    if comps.len() > 1 {
        for _ in 0..6 {
            let mut next = [0.0f64; 4];
            for &a in comps {
                for &b in comps {
                    next[a] += cov[a][b] * axis[b];
                }
            }
            let big = comps.iter().map(|&c| next[c].abs()).fold(0.0, f64::max);
            if big == 0.0 {
                break;
            }
            for &c in comps {
                axis[c] = next[c] / big;
            }
        }
    }
    let length: f64 = comps.iter().map(|&c| axis[c] * axis[c]).sum();
    let (mut lo, mut hi) = (mean, mean);
    if length > 0.0 {
        let (mut first, mut last) = (f64::INFINITY, f64::NEG_INFINITY);
        for &i in members {
            let t: f64 = comps
                .iter()
                .map(|&c| (f64::from(px[i][c]) - mean[c]) * axis[c])
                .sum::<f64>()
                / length;
            first = first.min(t);
            last = last.max(t);
        }
        for &c in comps {
            lo[c] = mean[c] + axis[c] * first;
            hi[c] = mean[c] + axis[c] * last;
        }
    }
    let mut best = Fit {
        lo: [0; 4],
        hi: [0; 4],
        weights: [0; 16],
        err: u32::MAX,
    };
    for _ in 0..3 {
        let (mut ql, mut qh) = ([0u8; 4], [0u8; 4]);
        for &c in comps {
            ql[c] = range.round(lo[c]);
            qh[c] = range.round(hi[c]);
        }
        // What each weight gives, and each member's nearest.
        let mut shades = [[0u8; 4]; 32];
        for (k, &w) in table.iter().enumerate() {
            for &c in comps {
                shades[k][c] = mix(ql[c], qh[c], w);
            }
        }
        let mut weights = [0u8; 16];
        let mut err = 0u32;
        for &i in members {
            let mut least = u32::MAX;
            for (k, shade) in shades[..table.len()].iter().enumerate() {
                let mut e = 0u32;
                for &c in comps {
                    let d = u32::from(shade[c].abs_diff(px[i][c]));
                    e += d * d;
                }
                if e < least {
                    least = e;
                    weights[i] = k as u8;
                }
            }
            err += least;
        }
        if err < best.err {
            best = Fit {
                lo: ql,
                hi: qh,
                weights,
                err,
            };
        }
        if err == 0 {
            break;
        }
        // The ends that suit those weights best (least squares).
        let (mut s00, mut s01, mut s11) = (0.0f64, 0.0f64, 0.0f64);
        let (mut t0, mut t1) = ([0.0f64; 4], [0.0f64; 4]);
        for &i in members {
            let a = f64::from(table[weights[i] as usize]) / 64.0;
            s00 += (1.0 - a) * (1.0 - a);
            s01 += a * (1.0 - a);
            s11 += a * a;
            for &c in comps {
                t0[c] += (1.0 - a) * f64::from(px[i][c]);
                t1[c] += a * f64::from(px[i][c]);
            }
        }
        let det = s00 * s11 - s01 * s01;
        if det.abs() < 1e-9 {
            break;
        }
        for &c in comps {
            lo[c] = (t0[c] * s11 - t1[c] * s01) / det;
            hi[c] = (t1[c] * s00 - t0[c] * s01) / det;
        }
    }
    best
}

/// A block fitted in one mode.
struct Fitted {
    mode: usize,
    pattern: usize,
    /// The component with weights of its own (two planes), else 4.
    apart: usize,
    /// Each subset's ends: `[subset][component]`.
    lo: [[u8; 4]; 3],
    hi: [[u8; 4]; 3],
    /// Each texel's weight index, and its second plane's.
    weights: [[u8; 16]; 2],
    err: u32,
}

fn pattern_of(mode: usize, pattern: usize) -> &'static [u8; 16] {
    const NONE: [u8; 16] = [0; 16];
    match mode {
        2 | 4 | 9 | 16 => &PATTERNS2[pattern],
        7 => &PATTERNS7[pattern],
        3 => &PATTERNS3[pattern],
        _ => &NONE,
    }
}

/// The block fitted in `mode` (not the solid one, nor luminance-alpha).
fn fitted(
    px: &[[u8; 4]; 16],
    ranges: &Ranges,
    mode: usize,
    pattern: usize,
    apart: usize,
) -> Fitted {
    let range = ranges.of(RANGES[mode]);
    let table = weights_of(WEIGHT_BITS[mode]);
    let all: &[usize] = if COMPONENTS[mode] == 4 {
        &[0, 1, 2, 3]
    } else {
        &[0, 1, 2]
    };
    let mut out = Fitted {
        mode,
        pattern,
        apart,
        lo: [[0; 4]; 3],
        hi: [[0; 4]; 3],
        weights: [[0; 16]; 2],
        err: 0,
    };
    if PLANES[mode] == 2 {
        let members: Vec<usize> = (0..16).collect();
        let rest: Vec<usize> = all.iter().copied().filter(|&c| c != apart).collect();
        let a = fit(px, &members, &rest, range, table);
        let b = fit(px, &members, &[apart], range, table);
        for &c in &rest {
            out.lo[0][c] = a.lo[c];
            out.hi[0][c] = a.hi[c];
        }
        out.lo[0][apart] = b.lo[apart];
        out.hi[0][apart] = b.hi[apart];
        out.weights = [a.weights, b.weights];
        out.err = a.err + b.err;
        return out;
    }
    let subsets = pattern_of(mode, pattern);
    for s in 0..SUBSETS[mode] as usize {
        let members: Vec<usize> = (0..16).filter(|&i| subsets[i] as usize == s).collect();
        let f = fit(px, &members, all, range, table);
        out.lo[s] = f.lo;
        out.hi[s] = f.hi;
        for &i in &members {
            out.weights[0][i] = f.weights[i];
        }
        out.err += f.err;
    }
    out
}

/// How far a pattern's subsets are from lines of their own: the squared
/// distance of each texel from the diagonal of its subset's box.
fn spread(px: &[[u8; 4]; 16], pattern: &[u8; 16], subsets: usize, comps: usize) -> f64 {
    let mut total = 0.0;
    for s in 0..subsets {
        let (mut least, mut most, mut mean, mut n) = ([255.0f64; 4], [0.0f64; 4], [0.0f64; 4], 0.0);
        for i in (0..16).filter(|&i| pattern[i] as usize == s) {
            for c in 0..comps {
                let v = f64::from(px[i][c]);
                least[c] = least[c].min(v);
                most[c] = most[c].max(v);
                mean[c] += v;
            }
            n += 1.0;
        }
        let mut length = 0.0;
        for c in 0..comps {
            mean[c] /= n;
            length += (most[c] - least[c]) * (most[c] - least[c]);
        }
        for i in (0..16).filter(|&i| pattern[i] as usize == s) {
            let (mut along, mut whole) = (0.0, 0.0);
            for c in 0..comps {
                let d = f64::from(px[i][c]) - mean[c];
                along += d * (most[c] - least[c]);
                whole += d * d;
            }
            total += whole
                - if length > 0.0 {
                    along * along / length
                } else {
                    0.0
                };
        }
    }
    total
}

/// The `keep` patterns of a mode whose subsets lie nearest lines.
fn likely(px: &[[u8; 4]; 16], mode: usize, count: usize, keep: usize) -> Vec<usize> {
    let mut by: Vec<(f64, usize)> = (0..count)
        .map(|p| {
            let s = spread(
                px,
                pattern_of(mode, p),
                SUBSETS[mode] as usize,
                COMPONENTS[mode] as usize,
            );
            (s, p)
        })
        .collect();
    by.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    by.truncate(keep);
    by.into_iter().map(|(_, p)| p).collect()
}

/// Bits written to a block, the first the lowest of byte 0.
struct Out {
    bytes: [u8; 16],
    at: usize,
}

impl Out {
    fn put(&mut self, v: u32, n: u8) {
        for k in 0..n as usize {
            if (v >> k) & 1 == 1 && self.at + k < 128 {
                self.bytes[(self.at + k) / 8] |= 1 << ((self.at + k) % 8);
            }
        }
        self.at += n as usize;
    }
}

/// ETC1's 5-bit or 4-bit colour as 8 bits.
fn etc1_wide(v: i32, diff: bool) -> i32 {
    if diff { (v << 3) | (v >> 2) } else { v * 17 }
}

/// The least squared error of 8 texels drawn from one base colour, and
/// the table that gives it.
fn etc1_half(px: &[[u8; 4]; 16], texels: &[usize; 8], base: [i32; 3]) -> (u32, u8) {
    let mut best = (u32::MAX, 0u8);
    for (t, table) in ETC1_TABLES.iter().enumerate() {
        let mut err = 0u32;
        for &i in texels {
            let mut least = u32::MAX;
            for &m in table {
                let mut e = 0u32;
                for c in 0..3 {
                    let d = (base[c] + m).clamp(0, 255) - i32::from(px[i][c]);
                    e += (d * d) as u32;
                }
                least = least.min(e);
            }
            err += least;
        }
        if err < best.0 {
            best = (err, t as u8);
        }
    }
    best
}

/// The specification's `apply_etc1_bias`: how a bias moves one half's
/// quantised colour.
fn etc1_biased(mut color: [i32; 3], bias: u32, limit: i32, half: usize) -> [i32; 3] {
    const DIVS: [u32; 3] = [1, 3, 9];
    let second = half == 1;
    for c in 0..3 {
        let only = |on_second: bool, channel: usize, d: i32| {
            if second == on_second && c == channel {
                d
            } else {
                0
            }
        };
        let delta = match bias {
            2 => only(false, 0, -1),
            5 => only(false, 1, -1),
            6 => only(false, 2, -1),
            7 => only(false, 0, 1),
            11 => only(false, 1, 1),
            15 => only(false, 2, 1),
            18 => only(true, 0, -1),
            19 => only(true, 1, -1),
            20 => only(true, 2, -1),
            21 => only(true, 0, 1),
            24 => only(true, 1, 1),
            8 => only(true, 2, 1),
            10 => -2,
            27 => {
                if second {
                    0
                } else {
                    -1
                }
            }
            28 => {
                if second {
                    -1
                } else {
                    1
                }
            }
            29 => i32::from(second),
            30 => {
                if second {
                    -1
                } else {
                    0
                }
            }
            31 => i32::from(!second),
            _ => ((bias / DIVS[c]) % 3) as i32 - 1,
        };
        let mut v = color[c];
        if v == 0 {
            v += if delta == -2 { 3 } else { delta + 1 };
        } else if v == limit {
            v += delta - 1;
        } else {
            v += delta;
            if v < 0 || v > limit {
                v = (v - delta) - delta;
            }
        }
        color[c] = v.clamp(0, limit);
    }
    color
}

/// ETC1's hints for a block's decoded texels: (flip, differential, the
/// two halves' tables, the bias).
fn etc1_hints(px: &[[u8; 4]; 16], biased: bool) -> (bool, bool, u8, u8, u8) {
    // The halves: side by side, or (flipped) one over the other.
    const HALVES: [[[usize; 8]; 2]; 2] = [
        [[0, 1, 4, 5, 8, 9, 12, 13], [2, 3, 6, 7, 10, 11, 14, 15]],
        [[0, 1, 2, 3, 4, 5, 6, 7], [8, 9, 10, 11, 12, 13, 14, 15]],
    ];
    // What the transcoder would draw with these hints, and its error.
    let tried = |flip: usize,
                 diff: bool,
                 bias: Option<u32>,
                 seen: &mut Vec<(usize, [i32; 3], (u32, u8))>| {
        let limit = if diff { 31 } else { 15 };
        let mut colors = [[0i32; 3]; 2];
        for half in 0..2 {
            let mut sum = [0u32; 3];
            for &i in &HALVES[flip][half] {
                for c in 0..3 {
                    sum[c] += u32::from(px[i][c]);
                }
            }
            for c in 0..3 {
                colors[half][c] = ((sum[c] * limit as u32 + 1020) / (8 * 255)) as i32;
            }
            if let Some(b) = bias {
                colors[half] = etc1_biased(colors[half], b, limit, half);
            }
        }
        if diff {
            for c in 0..3 {
                colors[1][c] = colors[0][c] + (colors[1][c] - colors[0][c]).clamp(-4, 3);
            }
        }
        let mut err = 0u32;
        let mut tables = [0u8; 2];
        for half in 0..2 {
            let base = colors[half].map(|v| etc1_wide(v, diff));
            let key = flip * 2 + half;
            let found = match seen.iter().find(|(k, b, _)| *k == key && *b == base) {
                Some((_, _, f)) => *f,
                None => {
                    let f = etc1_half(px, &HALVES[flip][half], base);
                    seen.push((key, base, f));
                    f
                }
            };
            err = err.saturating_add(found.0);
            tables[half] = found.1;
        }
        (err, tables)
    };
    let mut seen = Vec::new();
    let plain = if biased { Some(13) } else { None };
    let mut best = (u32::MAX, false, false, [0u8; 2], 13u8);
    for flip in 0..2 {
        for diff in [false, true] {
            let (err, tables) = tried(flip, diff, plain, &mut seen);
            if err < best.0 {
                best = (err, flip == 1, diff, tables, 13);
            }
        }
    }
    if biased {
        let (flip, diff) = (usize::from(best.1), best.2);
        for bias in 0..32u32 {
            let (err, tables) = tried(flip, diff, Some(bias), &mut seen);
            if err < best.0 {
                best = (err, best.1, diff, tables, bias as u8);
            }
        }
    }
    (best.1, best.2, best.3[0], best.3[1], best.4)
}

/// ETC2's alpha hints (table, multiplier) for a block's decoded alphas.
fn eac_hints(px: &[[u8; 4]; 16]) -> u8 {
    let least = px.iter().map(|p| i32::from(p[3])).min().unwrap_or(255);
    let most = px.iter().map(|p| i32::from(p[3])).max().unwrap_or(255);
    if least == most {
        return 13 | (1 << 4);
    }
    let mut best = (u32::MAX, 13u8, 1u8);
    for (t, table) in EAC_TABLES.iter().enumerate() {
        // The base the transcoder puts between the least and the most.
        let span = table[7] - table[3];
        let center = (f64::from(least)
            + f64::from(most - least) * f64::from(-table[3]) / f64::from(span)
            + 0.5) as i32;
        for mult in 1..16i32 {
            let mut err = 0u32;
            for p in px {
                let a = i32::from(p[3]);
                let d = table
                    .iter()
                    .map(|m| ((center + m * mult).clamp(0, 255) - a).unsigned_abs())
                    .min()
                    .unwrap_or(0);
                err += d * d;
            }
            if err < best.0 {
                best = (err, t as u8, mult as u8);
            }
        }
    }
    best.1 | (best.2 << 4)
}

/// A block of one colour: the colour, and how ETC1 draws it nearest.
fn solid(c: [u8; 4]) -> [u8; 16] {
    let mut best = (u32::MAX, false, 0u32, 0u32, [0u32; 3]);
    for diff in [true, false] {
        let limit = if diff { 32 } else { 16 };
        for (t, table) in ETC1_TABLES.iter().enumerate() {
            for (s, &m) in table.iter().enumerate() {
                let mut err = 0u32;
                let mut color = [0u32; 3];
                for ch in 0..3 {
                    let mut least = u32::MAX;
                    for v in 0..limit {
                        let d = (etc1_wide(v, diff) + m).clamp(0, 255) - i32::from(c[ch]);
                        if ((d * d) as u32) < least {
                            least = (d * d) as u32;
                            color[ch] = v as u32;
                        }
                    }
                    err += least;
                }
                if err < best.0 {
                    best = (err, diff, t as u32, s as u32, color);
                }
            }
        }
    }
    let mut out = Out {
        bytes: [0; 16],
        at: 0,
    };
    out.put(u32::from(CODES[SOLID].0), CODES[SOLID].1);
    for v in c {
        out.put(u32::from(v), 8);
    }
    out.put(u32::from(best.1), 1);
    out.put(best.2, 3);
    out.put(best.3, 2);
    for v in best.4 {
        out.put(v, 5);
    }
    out.bytes
}

/// Packs a fitted block.
fn packed(f: &Fitted, ranges: &Ranges) -> [u8; 16] {
    let mode = f.mode;
    let components = COMPONENTS[mode] as usize;
    let subsets = SUBSETS[mode] as usize;
    let planes = PLANES[mode] as usize;
    let bits = WEIGHT_BITS[mode];
    let top = (1u8 << bits) - 1;
    let pattern = pattern_of(mode, f.pattern);
    let (mut lo, mut hi, mut weights) = (f.lo, f.hi, f.weights);
    // A subset's first weight is stored without its top bit: where it
    // is set, the subset's ends change places and its weights turn
    // round. With two planes each plane's components do so apart.
    let mut first = [false; 16];
    for s in 0..subsets {
        let Some(anchor) = pattern.iter().position(|&p| p as usize == s) else {
            continue;
        };
        first[anchor] = true;
        for (plane, weights) in weights.iter_mut().enumerate().take(planes) {
            if weights[anchor] <= top / 2 {
                continue;
            }
            for i in (0..16).filter(|&i| pattern[i] as usize == s) {
                weights[i] = top - weights[i];
            }
            for c in 0..components {
                if planes == 1 || (c == f.apart) == (plane == 1) {
                    std::mem::swap(&mut lo[s][c], &mut hi[s][c]);
                }
            }
        }
    }
    let mut out = Out {
        bytes: [0; 16],
        at: 0,
    };
    // What the block decodes to, for the hints: the fit's own texels.
    let table = weights_of(bits);
    let mut drawn = [[0u8, 0, 0, 255]; 16];
    for (i, texel) in drawn.iter_mut().enumerate() {
        let s = pattern[i] as usize;
        for c in 0..components {
            let plane = usize::from(planes == 2 && c == f.apart);
            texel[c] = mix(lo[s][c], hi[s][c], table[weights[plane][i] as usize]);
        }
    }
    out.put(u32::from(CODES[mode].0), CODES[mode].1);
    let biased = !matches!(mode, 10..=12);
    let (flip, diff, t0, t1, bias) = etc1_hints(&drawn, biased);
    out.put(0, 1);
    if biased {
        out.put(0, 1);
    }
    out.put(u32::from(flip), 1);
    out.put(u32::from(diff), 1);
    out.put(u32::from(t0), 3);
    out.put(u32::from(t1), 3);
    if biased {
        out.put(u32::from(bias), 5);
    }
    if components == 4 {
        out.put(u32::from(eac_hints(&drawn)), 8);
    }
    match mode {
        2 | 4 | 7 | 9 | 16 => out.put(f.pattern as u32, 5),
        3 => out.put(f.pattern as u32, 4),
        _ => {}
    }
    if matches!(mode, 6 | 11 | 13) {
        out.put(f.apart as u32, 2);
    }
    // The ends as codes: the trits or quints of all first, then the
    // plain bits of each.
    let range = ranges.of(RANGES[mode]);
    let (plain, base) = range_parts(RANGES[mode]);
    let mut codes = Vec::with_capacity(18);
    for s in 0..subsets {
        for c in 0..components {
            codes.push(u32::from(range.code[lo[s][c] as usize]));
            codes.push(u32::from(range.code[hi[s][c] as usize]));
        }
    }
    if base != 0 {
        let group = if base == 3 { 5 } else { 3 };
        for chunk in codes.chunks(group) {
            let width = match (base, chunk.len()) {
                (3, 1) => 2,
                (3, 2) => 4,
                (3, 3) => 5,
                (3, 4) => 7,
                (3, _) => 8,
                (_, 1) => 3,
                (_, 2) => 5,
                _ => 7,
            };
            let mut code = 0;
            for &v in chunk.iter().rev() {
                code = code * base + (v >> plain);
            }
            out.put(code, width);
        }
    }
    for &v in &codes {
        out.put(v & ((1 << plain) - 1), plain);
    }
    for i in 0..16 {
        let short = u8::from(first[i]);
        for plane in weights.iter().take(planes) {
            out.put(u32::from(plane[i]), bits - short);
        }
    }
    out.bytes
}

/// The 16 texels (RGBA, rows from the top left) as a UASTC block.
pub fn encode_block(px: &[[u8; 4]; 16], ranges: &Ranges) -> [u8; 16] {
    if px.iter().all(|p| *p == px[0]) {
        return solid(px[0]);
    }
    let opaque = px.iter().all(|p| p[3] == 255);
    let mut best: Option<Fitted> = None;
    let offer = |best: &mut Option<Fitted>, f: Fitted| {
        if best.as_ref().is_none_or(|b| f.err < b.err) {
            *best = Some(f);
        }
    };
    // One subset, then a component apart; where those leave an error,
    // the patterns of two and three subsets most like the block.
    type Modes<'a> = (&'a [usize], &'a [(usize, usize)], &'a [usize], &'a [usize]);
    let (single, apart, pairs, triples): Modes = if opaque {
        (&[0, 18, 5, 1], &[(6, 0), (6, 1), (6, 2)], &[2, 4, 7], &[3])
    } else {
        (&[10, 12, 14], &[(11, 3), (13, 3)], &[9], &[])
    };
    for &mode in single {
        offer(&mut best, fitted(px, ranges, mode, 0, 4));
    }
    for &(mode, c) in apart {
        offer(&mut best, fitted(px, ranges, mode, 0, c));
    }
    for (modes, keep) in [(pairs, 3), (triples, 2)] {
        for &mode in modes {
            if best.as_ref().is_some_and(|b| b.err <= 16) {
                break;
            }
            let count = match mode {
                3 => PATTERNS3.len(),
                7 => PATTERNS7.len(),
                _ => PATTERNS2.len(),
            };
            for p in likely(px, mode, count, keep) {
                offer(&mut best, fitted(px, ranges, mode, p, 4));
            }
        }
    }
    match best {
        Some(f) => packed(&f, ranges),
        None => solid(px[0]),
    }
}

#[cfg(test)]
mod tests {
    use super::super::decode_block;
    use super::*;

    fn error(a: &[[u8; 4]; 16], b: &[[u8; 4]; 16]) -> u32 {
        a.iter()
            .zip(b)
            .flat_map(|(p, q)| (0..4).map(move |c| u32::from(p[c].abs_diff(q[c])).pow(2)))
            .sum()
    }

    /// Every mode the encoder writes decodes to the texels its fit
    /// promised: the packing is the decoder's reading turned round.
    #[test]
    fn a_fit_in_each_mode_decodes_to_its_own_error() {
        let ranges = Ranges::new();
        // Texels that vary in every component, with a seam down the
        // block so the patterns have something to find.
        let mut px = [[0u8; 4]; 16];
        let mut state = 12345u32;
        for (i, p) in px.iter_mut().enumerate() {
            for (c, v) in p.iter_mut().enumerate() {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12345);
                let noise = (state >> 16) % 40;
                *v = (if i % 4 < 2 {
                    40 + c as u32 * 30
                } else {
                    200 - c as u32 * 20
                } + noise) as u8;
            }
        }
        let cases: [(usize, usize, usize); 20] = [
            (0, 0, 4),
            (1, 0, 4),
            (2, 7, 4),
            (2, 29, 4),
            (3, 5, 4),
            (3, 10, 4),
            (4, 12, 4),
            (5, 0, 4),
            (6, 0, 0),
            (6, 0, 2),
            (7, 18, 4),
            (9, 3, 4),
            (10, 0, 4),
            (11, 0, 3),
            (11, 0, 1),
            (12, 0, 4),
            (13, 0, 3),
            (14, 0, 4),
            (18, 0, 4),
            (7, 0, 4),
        ];
        for (mode, pattern, apart) in cases {
            let f = fitted(&px, &ranges, mode, pattern, apart);
            let back = decode_block(&packed(&f, &ranges)).expect("a valid block");
            // An opaque mode's alpha comes back 255, whatever it was.
            let mut want = px;
            if COMPONENTS[mode] == 3 {
                for p in &mut want {
                    p[3] = 255;
                }
            }
            assert_eq!(error(&back, &want), f.err, "mode {mode} pattern {pattern}");
        }
    }

    /// Flat, smooth, two-coloured and translucent blocks come back
    /// close: within a squared error a wrong fit or a wrong bit would
    /// pass many times over.
    #[test]
    fn blocks_come_back_close() {
        let ranges = Ranges::new();
        let flat = [[17u8, 99, 203, 255]; 16];
        assert_eq!(decode_block(&encode_block(&flat, &ranges)), Some(flat));
        let clear = [[17u8, 99, 203, 80]; 16];
        assert_eq!(decode_block(&encode_block(&clear, &ranges)), Some(clear));
        let ramp: [[u8; 4]; 16] = std::array::from_fn(|i| {
            let v = (i % 4 * 60 + i / 4 * 5) as u8;
            [v, 255 - v, v / 2, 255]
        });
        let halves: [[u8; 4]; 16] = std::array::from_fn(|i| {
            if i % 4 < 2 {
                [250, 10, 10, 255]
            } else {
                [10, 10, 250, 255]
            }
        });
        let glass: [[u8; 4]; 16] = std::array::from_fn(|i| {
            let v = (i * 13) as u8;
            [v, 100, 255 - v, (255 - i * 16) as u8]
        });
        for (name, px, most) in [
            ("ramp", ramp, 150),
            ("halves", halves, 0),
            ("glass", glass, 400),
        ] {
            let back = decode_block(&encode_block(&px, &ranges)).expect("a valid block");
            let err = error(&back, &px);
            assert!(err <= most, "{name}: {err}");
        }
    }
}

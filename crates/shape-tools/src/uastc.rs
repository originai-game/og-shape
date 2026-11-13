//! UASTC LDR 4×4 blocks: the texture format of `KHR_texture_basisu`'s
//! high-quality mode, each 4×4 texels in 16 bytes.
//!
//! Entry: [`decode_block`] (one block to its 16 RGBA texels) and
//! [`decode`] (a level of blocks to an RGBA image). Written from the
//! format's specification, which is in the public domain
//! (`docs/research.md` §50); its 64 example blocks are the test.
//!
//! A block is a mode (a prefix code), hints for transcoders (skipped
//! here), a partition pattern for the modes with two or three subsets,
//! the endpoints (trits or quints first, then each value's bits) and the
//! weights (plain bits; the first weight of each subset is stored one
//! bit short). Texels are ASTC's: endpoints widened to 16 bits and mixed
//! by a weight out of 64.

/// Bits of a mode's prefix code.
const CODE_BITS: [u8; 19] = [4, 6, 5, 5, 5, 5, 5, 5, 5, 5, 3, 2, 3, 5, 5, 7, 6, 6, 4];
/// Bits per weight.
const WEIGHT_BITS: [u8; 19] = [4, 2, 3, 2, 2, 3, 2, 2, 0, 2, 4, 2, 3, 1, 2, 4, 2, 2, 5];
/// The endpoint range (ASTC's numbering).
const RANGES: [u8; 19] = [
    19, 20, 8, 7, 12, 20, 18, 12, 0, 8, 13, 13, 19, 20, 20, 20, 20, 20, 11,
];
const SUBSETS: [u8; 19] = [1, 1, 2, 3, 2, 1, 1, 2, 0, 2, 1, 1, 1, 1, 1, 1, 2, 1, 1];
const PLANES: [u8; 19] = [1, 1, 1, 1, 1, 1, 2, 1, 0, 1, 1, 2, 1, 2, 1, 1, 1, 2, 1];
/// Components: 3 is RGB, 4 RGBA, 2 luminance and alpha.
const COMPONENTS: [u8; 19] = [3, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 2, 2, 2, 3];
/// Hint bits after the mode: BC1 (1 or 2), ETC1 (8), its bias (5 or 0),
/// ETC2 alpha (8, alpha modes).
const HINT_BITS: [u8; 19] = [
    15, 15, 15, 15, 15, 15, 15, 15, 0, 23, 17, 17, 17, 23, 23, 23, 23, 23, 15,
];
/// The solid-colour mode.
const SOLID: usize = 8;

const WEIGHTS1: [u8; 2] = [0, 64];
const WEIGHTS2: [u8; 4] = [0, 21, 43, 64];
const WEIGHTS3: [u8; 8] = [0, 9, 18, 27, 37, 46, 55, 64];
const WEIGHTS4: [u8; 16] = [0, 4, 8, 12, 17, 21, 25, 29, 35, 39, 43, 47, 52, 56, 60, 64];
#[rustfmt::skip]
const WEIGHTS5: [u8; 32] = [
    0, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24, 26, 28, 30,
    34, 36, 38, 40, 42, 44, 46, 48, 50, 52, 54, 56, 58, 60, 62, 64,
];

/// Mode by the low seven bits of a block's first byte (19: reserved).
#[rustfmt::skip]
const MODES: [u8; 128] = [11, 0, 10, 3, 11, 15, 12, 7, 11, 18, 10, 5, 11, 14, 12, 9, 11, 0, 10, 4, 11, 16, 12, 8, 11, 18, 10, 6, 11, 2, 12, 13, 11, 0, 10, 3, 11, 17, 12, 7, 11, 18, 10, 5, 11, 14, 12, 9, 11, 0, 10, 4, 11, 1, 12, 8, 11, 18, 10, 6, 11, 2, 12, 13, 11, 0, 10, 3, 11, 19, 12, 7, 11, 18, 10, 5, 11, 14, 12, 9, 11, 0, 10, 4, 11, 16, 12, 8, 11, 18, 10, 6, 11, 2, 12, 13, 11, 0, 10, 3, 11, 17, 12, 7, 11, 18, 10, 5, 11, 14, 12, 9, 11, 0, 10, 4, 11, 1, 12, 8, 11, 18, 10, 6, 11, 2, 12, 13];

/// Each texel's subset, two-subset patterns (modes 2, 4, 9, 16).
#[rustfmt::skip]
const PATTERNS2: [[u8; 16]; 30] = [
    [0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1],
    [0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1],
    [1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0],
    [0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 1, 1, 1],
    [1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 0, 0],
    [0, 0, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1],
    [1, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0],
    [1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1],
    [1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1],
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 0],
    [1, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0],
    [1, 0, 0, 0, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1],
    [1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 0, 0, 0, 1],
    [0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0],
    [0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0],
    [1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 0, 0, 1, 1],
    [1, 0, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 1, 0],
    [0, 0, 1, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 0],
    [1, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 0, 0, 1, 1],
    [0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0],
    [1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1],
    [1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0],
    [1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0],
    [1, 0, 0, 1, 0, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 0],
];

/// Each texel's subset, three-subset patterns (mode 3).
#[rustfmt::skip]
const PATTERNS3: [[u8; 16]; 11] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 2, 2, 1, 1, 2, 2],
    [1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 2, 2, 2, 2],
    [1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 2, 2, 2, 2],
    [1, 1, 1, 1, 2, 2, 2, 2, 0, 0, 0, 0, 0, 0, 0, 0],
    [1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2, 0],
    [0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2, 0, 1, 1, 2],
    [0, 2, 1, 1, 0, 2, 1, 1, 0, 2, 1, 1, 0, 2, 1, 1],
    [2, 0, 0, 0, 2, 0, 0, 0, 2, 1, 1, 1, 2, 1, 1, 1],
    [2, 0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2, 2, 0, 1, 2],
    [1, 1, 1, 1, 0, 0, 0, 0, 2, 2, 2, 2, 1, 1, 1, 1],
    [0, 0, 2, 2, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 2, 2],
];

/// Each texel's subset, the two-subset patterns of mode 7.
#[rustfmt::skip]
const PATTERNS7: [[u8; 16]; 19] = [
    [0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0],
    [1, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 1],
    [1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1],
    [0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0],
    [0, 0, 0, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    [0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 1],
    [1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0],
    [0, 1, 1, 1, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 1, 1, 1, 0],
    [1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0],
    [0, 1, 1, 1, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 0],
    [1, 1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1, 0, 0, 0],
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 0],
    [0, 0, 1, 1, 0, 1, 1, 0, 1, 1, 0, 0, 1, 0, 0, 0],
    [1, 1, 1, 1, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0],
];

/// Endpoint range 7: the 8-bit value of each coded integer.
#[rustfmt::skip]
const RANGE7: [u8; 12] = [0, 255, 69, 186, 23, 232, 92, 163, 46, 209, 116, 139];

/// Endpoint range 12: the 8-bit value of each coded integer.
#[rustfmt::skip]
const RANGE12: [u8; 40] = [0, 255, 32, 223, 65, 190, 97, 158, 6, 249, 39, 216, 71, 184, 104, 151, 13, 242, 45, 210, 78, 177, 110, 145, 19, 236, 52, 203, 84, 171, 117, 138, 26, 229, 58, 197, 91, 164, 123, 132];

/// Endpoint range 13: the 8-bit value of each coded integer.
#[rustfmt::skip]
const RANGE13: [u8; 48] = [0, 255, 16, 239, 32, 223, 48, 207, 65, 190, 81, 174, 97, 158, 113, 142, 5, 250, 21, 234, 38, 217, 54, 201, 70, 185, 86, 169, 103, 152, 119, 136, 11, 244, 27, 228, 43, 212, 59, 196, 76, 179, 92, 163, 108, 147, 124, 131];

/// Endpoint range 18: the 8-bit value of each coded integer.
#[rustfmt::skip]
const RANGE18: [u8; 160] = [0, 255, 8, 247, 16, 239, 24, 231, 32, 223, 40, 215, 48, 207, 56, 199, 64, 191, 72, 183, 80, 175, 88, 167, 96, 159, 104, 151, 112, 143, 120, 135, 1, 254, 9, 246, 17, 238, 25, 230, 33, 222, 41, 214, 49, 206, 57, 198, 65, 190, 73, 182, 81, 174, 89, 166, 97, 158, 105, 150, 113, 142, 121, 134, 3, 252, 11, 244, 19, 236, 27, 228, 35, 220, 43, 212, 51, 204, 59, 196, 67, 188, 75, 180, 83, 172, 91, 164, 99, 156, 107, 148, 115, 140, 123, 132, 4, 251, 12, 243, 20, 235, 28, 227, 36, 219, 44, 211, 52, 203, 60, 195, 68, 187, 76, 179, 84, 171, 92, 163, 100, 155, 108, 147, 116, 139, 124, 131, 6, 249, 14, 241, 22, 233, 30, 225, 38, 217, 46, 209, 54, 201, 62, 193, 70, 185, 78, 177, 86, 169, 94, 161, 102, 153, 110, 145, 118, 137, 126, 129];

/// Endpoint range 19: the 8-bit value of each coded integer.
#[rustfmt::skip]
const RANGE19: [u8; 192] = [0, 255, 4, 251, 8, 247, 12, 243, 16, 239, 20, 235, 24, 231, 28, 227, 32, 223, 36, 219, 40, 215, 44, 211, 48, 207, 52, 203, 56, 199, 60, 195, 64, 191, 68, 187, 72, 183, 76, 179, 80, 175, 84, 171, 88, 167, 92, 163, 96, 159, 100, 155, 104, 151, 108, 147, 112, 143, 116, 139, 120, 135, 124, 131, 1, 254, 5, 250, 9, 246, 13, 242, 17, 238, 21, 234, 25, 230, 29, 226, 33, 222, 37, 218, 41, 214, 45, 210, 49, 206, 53, 202, 57, 198, 61, 194, 65, 190, 69, 186, 73, 182, 77, 178, 81, 174, 85, 170, 89, 166, 93, 162, 97, 158, 101, 154, 105, 150, 109, 146, 113, 142, 117, 138, 121, 134, 125, 130, 2, 253, 6, 249, 10, 245, 14, 241, 18, 237, 22, 233, 26, 229, 30, 225, 34, 221, 38, 217, 42, 213, 46, 209, 50, 205, 54, 201, 58, 197, 62, 193, 66, 189, 70, 185, 74, 181, 78, 177, 82, 173, 86, 169, 90, 165, 94, 161, 98, 157, 102, 153, 106, 149, 110, 145, 114, 141, 118, 137, 122, 133, 126, 129];

/// Bits read from the block, first bit the lowest of byte 0.
struct Bits<'a> {
    bytes: &'a [u8; 16],
    at: usize,
}

impl Bits<'_> {
    /// The next `n` bits (at most 8); bits past the block are 0.
    fn take(&mut self, n: u8) -> u32 {
        let mut v = 0;
        for k in 0..n as usize {
            let i = self.at + k;
            if i < 128 {
                v |= u32::from((self.bytes[i / 8] >> (i % 8)) & 1) << k;
            }
        }
        self.at += n as usize;
        v
    }
}

/// A coded endpoint integer as its 8-bit value.
fn widened(range: u8, v: u32) -> Option<u8> {
    let v = v as usize;
    Some(match range {
        7 => *RANGE7.get(v)?,
        8 => (v * 17) as u8,
        11 => ((v << 3) | (v >> 2)) as u8,
        12 => *RANGE12.get(v)?,
        13 => *RANGE13.get(v)?,
        18 => *RANGE18.get(v)?,
        19 => *RANGE19.get(v)?,
        _ => v as u8,
    })
}

/// (plain bits, 3 for trits or 5 for quints or 0) of an endpoint range.
fn range_parts(range: u8) -> (u8, u32) {
    match range {
        7 => (2, 3),
        8 => (4, 0),
        11 => (5, 0),
        12 => (3, 5),
        13 => (4, 3),
        18 => (5, 5),
        19 => (6, 3),
        _ => (8, 0),
    }
}

/// ASTC's mix of two 8-bit endpoints by a weight out of 64.
fn mix(l: u8, h: u8, w: u8) -> u8 {
    let (l, h, w) = (u32::from(l) * 257, u32::from(h) * 257, u32::from(w));
    (((l * (64 - w) + h * w + 32) >> 6) >> 8) as u8
}

/// A block's 16 texels, RGBA, in rows from the top left; `None` when
/// the block is not a valid one.
pub fn decode_block(bytes: &[u8; 16]) -> Option<[[u8; 4]; 16]> {
    let mode = MODES[(bytes[0] & 127) as usize] as usize;
    if mode >= 19 {
        return None;
    }
    let mut bits = Bits {
        bytes,
        at: CODE_BITS[mode] as usize,
    };
    if mode == SOLID {
        let c = [
            bits.take(8) as u8,
            bits.take(8) as u8,
            bits.take(8) as u8,
            bits.take(8) as u8,
        ];
        return Some([c; 16]);
    }
    bits.at += HINT_BITS[mode] as usize;
    let subsets = SUBSETS[mode] as usize;
    let none = [0u8; 16];
    let pattern: &[u8; 16] = match mode {
        2 | 4 | 9 | 16 => PATTERNS2.get(bits.take(5) as usize)?,
        7 => PATTERNS7.get(bits.take(5) as usize)?,
        3 => PATTERNS3.get(bits.take(4) as usize)?,
        _ => &none,
    };
    let planes = PLANES[mode] as usize;
    // The component the second plane's weights mix.
    let apart = match mode {
        6 | 11 | 13 => bits.take(2) as usize,
        17 => 3,
        _ => 4,
    };
    let components = COMPONENTS[mode] as usize;
    let count = components * 2 * subsets;
    let range = RANGES[mode];
    let (plain, base) = range_parts(range);
    // The trits or quints, a group of five or three to a code, the last
    // code as short as its group allows.
    let mut high = [0u32; 18];
    if base != 0 {
        let group = if base == 3 { 5 } else { 3 };
        let mut done = 0;
        while done < count {
            let n = (count - done).min(group);
            let width = match (base, n) {
                (3, 1) => 2,
                (3, 2) => 4,
                (3, 3) => 5,
                (3, 4) => 7,
                (3, _) => 8,
                (_, 1) => 3,
                (_, 2) => 5,
                _ => 7,
            };
            let mut code = bits.take(width);
            for slot in &mut high[done..done + n] {
                *slot = code % base;
                code /= base;
            }
            done += n;
        }
    }
    let mut ends = [0u8; 18];
    for i in 0..count {
        let v = bits.take(plain) | (high[i] << plain);
        ends[i] = widened(range, v)?;
    }
    let width = WEIGHT_BITS[mode];
    let table: &[u8] = match width {
        1 => &WEIGHTS1,
        2 => &WEIGHTS2,
        3 => &WEIGHTS3,
        4 => &WEIGHTS4,
        _ => &WEIGHTS5,
    };
    // The first texel of each subset holds its weights one bit short.
    let mut first = [false; 16];
    for s in 0..subsets {
        if let Some(i) = pattern.iter().position(|&p| p as usize == s) {
            first[i] = true;
        }
    }
    let mut out = [[0u8, 0, 0, 255]; 16];
    for (i, texel) in out.iter_mut().enumerate() {
        let short = u8::from(first[i]);
        let w0 = table[bits.take(width - short) as usize];
        let w1 = if planes == 2 {
            table[bits.take(width - short) as usize]
        } else {
            w0
        };
        let ends = &ends[pattern[i] as usize * components * 2..];
        let at = |c: usize, own: usize| {
            mix(
                ends[c * 2],
                ends[c * 2 + 1],
                if own == apart { w1 } else { w0 },
            )
        };
        *texel = match components {
            // Luminance and alpha: the second plane, if any, is alpha's.
            2 => {
                let l = at(0, 0);
                [l, l, l, at(1, 3)]
            }
            3 => [at(0, 0), at(1, 1), at(2, 2), 255],
            _ => [at(0, 0), at(1, 1), at(2, 2), at(3, 3)],
        };
    }
    Some(out)
}

/// A level of blocks (rows of ⌈width/4⌉, 16 bytes each) as RGBA, 4
/// bytes a texel, rows from the top.
pub fn decode(data: &[u8], width: usize, height: usize) -> Result<Vec<u8>, String> {
    let (across, down) = (width.div_ceil(4), height.div_ceil(4));
    let need = across
        .checked_mul(down)
        .and_then(|n| n.checked_mul(16))
        .ok_or("the image is too large")?;
    if width == 0 || height == 0 || data.len() < need {
        return Err(format!(
            "a {width}×{height} image needs {need} bytes of blocks, and there are {}",
            data.len()
        ));
    }
    let mut out = vec![0u8; width * height * 4];
    for (b, bytes) in data[..need].as_chunks::<16>().0.iter().enumerate() {
        let texels =
            decode_block(bytes).ok_or_else(|| format!("block {b} is not a valid UASTC block"))?;
        let (bx, by) = (b % across * 4, b / across * 4);
        for (i, t) in texels.iter().enumerate() {
            let (x, y) = (bx + i % 4, by + i / 4);
            if x < width && y < height {
                out[(y * width + x) * 4..][..4].copy_from_slice(t);
            }
        }
    }
    Ok(out)
}

mod encode;
pub use encode::{Ranges, encode_block};

/// An RGBA image (4 bytes a texel, rows from the top) as blocks, rows
/// of ⌈width/4⌉; texels past an edge repeat the edge's. A block may be
/// written as a copy of one written shortly before it where that costs
/// it no more than `rdo` in squared error a channel of a texel over
/// its own best (0: never): equal blocks are what the compression
/// after this finds.
pub fn encode_rdo(rgba: &[u8], width: usize, height: usize, rdo: f64) -> Vec<u8> {
    let ranges = Ranges::new();
    let (across, down) = (width.div_ceil(4), height.div_ceil(4));
    let mut out = Vec::with_capacity(across * down * 16);
    for by in 0..down {
        for bx in 0..across {
            let px: [[u8; 4]; 16] = std::array::from_fn(|i| {
                let x = (bx * 4 + i % 4).min(width - 1);
                let y = (by * 4 + i / 4).min(height - 1);
                let at = (y * width + x) * 4;
                [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
            });
            let own = encode_block(&px, &ranges);
            let mut block = own;
            if rdo > 0.0 {
                let error = |b: &[u8; 16]| -> u32 {
                    decode_block(b).map_or(u32::MAX, |t| {
                        t.iter()
                            .zip(&px)
                            .flat_map(|(p, q)| {
                                (0..4).map(move |c| u32::from(p[c].abs_diff(q[c])).pow(2))
                            })
                            .sum()
                    })
                };
                // The blocks to the left, and above with its neighbours.
                let allowed = f64::from(error(&own)) + rdo * 64.0;
                let at = out.len() / 16;
                let mut best = allowed;
                for back in [
                    1,
                    2,
                    3,
                    4,
                    across,
                    across + 1,
                    across.saturating_sub(1),
                    2 * across,
                ] {
                    if back == 0 || back > at {
                        continue;
                    }
                    let from = (at - back) * 16;
                    let Ok(candidate) = <[u8; 16]>::try_from(&out[from..from + 16]) else {
                        continue;
                    };
                    let e = f64::from(error(&candidate));
                    if e <= best {
                        // The nearest that is good enough; among
                        // equals, the first tried.
                        if e < best || block == own {
                            block = candidate;
                            best = e;
                        }
                    }
                }
            }
            out.extend_from_slice(&block);
        }
    }
    out
}

#[cfg(test)]
mod vectors;

#[cfg(test)]
mod tests {
    use super::*;

    /// The specification's 64 blocks decode to exactly its pixels, and
    /// its three invalid ones to nothing.
    #[test]
    fn the_specification_s_blocks_decode_to_its_pixels() {
        let mut invalid = 0;
        for (i, (block, pixels)) in vectors::BLOCKS.iter().zip(&vectors::PIXELS).enumerate() {
            match decode_block(block) {
                Some(texels) => {
                    let got = texels.map(u32::from_le_bytes);
                    assert_eq!(
                        &got,
                        pixels,
                        "block {i}, mode {}",
                        MODES[(block[0] & 127) as usize]
                    );
                }
                None => {
                    assert!(pixels.iter().all(|&p| p == 0xFFFF_00FF), "block {i}");
                    invalid += 1;
                }
            }
        }
        assert_eq!(invalid, 3);
    }
}

//! A texture budget: a map whose longest side is over a limit is decoded,
//! shrunk by a box filter and written again as PNG.
//!
//! Entry: [`downsize`]. Decoding reads PNG (every colour type and bit
//! depth, interlaced or not) and JPEG, baseline, extended sequential
//! or progressive (Huffman, 8-bit, any sampling; a lossless, hierarchical
//! or arithmetic-coded JPEG is [`TextureError::Unsupported`]), and KTX2
//! of UASTC blocks ([`crate::ktx2`]). [`to_ktx2`] writes a map as one.
//! Everything after the file's bytes is
//! integer arithmetic: the JPEG's inverse DCT in fixed point, the box
//! filter with integer area weights, so the output is the same bytes on
//! every host. The one table computed in f64 (sRGB to linear, by
//! `shape_core::trig::pow`) is rounded to integers before use.
//!
//! The filter averages each output texel over the input area it covers
//! (exact rational weights). Colour maps (base colour, emissive) are
//! averaged in linear light and their alpha as it is; data maps
//! (metallic-roughness, normal, occlusion) channel by channel as stored,
//! so a normal map's averaged vectors are not renormalised.

use std::fmt;

use crate::image::Texture;
use crate::preview::Image;

/// How far to shrink. The zero value changes nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Downsize {
    /// The longest side allowed, in texels; 0 is no limit. A larger map is
    /// scaled so its longest side is exactly this, keeping its aspect.
    pub max: u32,
    /// The map holds colour (sRGB: averaged in linear light); otherwise
    /// data, averaged as stored.
    pub color: bool,
}

/// Why a texture could not be resized.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureError {
    /// The image is broken or not an image (what).
    Format(String),
    /// A kind of image this decoder does not read (which).
    Unsupported(String),
}

impl fmt::Display for TextureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextureError::Format(w) => write!(f, "not a readable image: {w}"),
            TextureError::Unsupported(w) => write!(f, "not decoded: {w}"),
        }
    }
}

impl std::error::Error for TextureError {}

fn bad(what: impl Into<String>) -> TextureError {
    TextureError::Format(what.into())
}

/// `t` shrunk so its longest side is at most `d.max`, as PNG; `None` when
/// it already fits (or `d.max` is 0), so the original bytes stay.
pub fn downsize(t: &Texture, d: &Downsize) -> Result<Option<Texture>, TextureError> {
    if d.max == 0 {
        return Ok(None);
    }
    let image = decode(&t.bytes)?;
    let longest = image.width.max(image.height);
    if longest <= d.max {
        return Ok(None);
    }
    let scale = |side: u32| {
        let v = (u64::from(side) * u64::from(d.max) + u64::from(longest) / 2) / u64::from(longest);
        (v as u32).max(1)
    };
    let small = resize(&image, scale(image.width), scale(image.height), d.color);
    let name = match t.name.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}.png"),
        None => t.name.clone(),
    };
    Ok(Some(Texture {
        name,
        mime: "image/png".into(),
        bytes: crate::png(&small),
    }))
}

/// `t` as a KTX2 file of UASTC blocks with its mip levels
/// (`KHR_texture_basisu`), compressed with Zstandard; `None` when it is
/// a KTX2 file already. `color` as in [`Downsize::color`]: the texels
/// are sRGB, and the levels are averaged in linear light.
pub fn to_ktx2(t: &Texture, color: bool) -> Result<Option<Texture>, TextureError> {
    ktx2_of(t, color, false, 0.0)
}

/// As [`to_ktx2`], trading quality for bytes: a block may be written
/// as a copy of one just before it or above it where that costs it at
/// most `rdo` in squared error a channel of a texel over its own best,
/// and the Zstandard pass then stores the copy as a reference. 0 is
/// [`to_ktx2`]; 1 to 4 are useful.
pub fn to_ktx2_rdo(t: &Texture, color: bool, rdo: f64) -> Result<Option<Texture>, TextureError> {
    if !(rdo >= 0.0 && rdo.is_finite()) {
        return Err(bad("the rate-distortion allowance must be 0 or more"));
    }
    ktx2_of(t, color, false, rdo)
}

/// As [`to_ktx2`] with ETC1S blocks under BasisLZ: the small mode of
/// `KHR_texture_basisu`, a fraction of UASTC's bytes at a lower
/// quality (one colour and one intensity table a block, from codebooks
/// the whole file shares).
pub fn to_ktx2_etc1s(t: &Texture, color: bool) -> Result<Option<Texture>, TextureError> {
    ktx2_of(t, color, true, 0.0)
}

fn ktx2_of(
    t: &Texture,
    color: bool,
    small: bool,
    rdo: f64,
) -> Result<Option<Texture>, TextureError> {
    if t.mime == "image/ktx2" {
        return Ok(None);
    }
    let image = decode(&t.bytes)?;
    let alpha = image.rgba.as_chunks::<4>().0.iter().any(|p| p[3] != 255);
    let mut levels = Vec::new();
    let (mut w, mut h) = (image.width, image.height);
    while w > 1 || h > 1 {
        (w, h) = ((w / 2).max(1), (h / 2).max(1));
        levels.push(resize(&image, w, h, color));
    }
    levels.insert(0, image);
    let name = match t.name.rsplit_once('.') {
        Some((stem, _)) => format!("{stem}.ktx2"),
        None => format!("{}.ktx2", t.name),
    };
    let bytes = if small {
        // Codebooks by how many blocks there are, as the reference
        // tool's default comes out on a 1024² image (about 2 000
        // endpoints and 1 300 selectors).
        let blocks: usize = levels
            .iter()
            .map(|l| (l.width as usize).div_ceil(4) * (l.height as usize).div_ceil(4))
            .sum::<usize>()
            * if alpha { 2 } else { 1 };
        crate::ktx2::encode_etc1s(
            &levels,
            color,
            alpha,
            (blocks / 40).clamp(256, 8192),
            (blocks / 60).clamp(256, 8192),
        )
    } else {
        crate::ktx2::encode(&levels, color, alpha, rdo)
    };
    Ok(Some(Texture {
        name,
        mime: "image/ktx2".into(),
        bytes,
    }))
}

/// Decodes a PNG, a JPEG, a KTX2 file (UASTC, or ETC1S under BasisLZ) or a
/// WebP image to RGBA8.
pub fn decode(bytes: &[u8]) -> Result<Image, TextureError> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        decode_png(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        decode_jpeg(bytes)
    } else if bytes.starts_with(b"\xabKTX 20") {
        crate::ktx2::decode(bytes).map_err(TextureError::Unsupported)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        decode_webp(bytes)
    } else {
        Err(TextureError::Unsupported(
            "only PNG, JPEG, KTX2 and WebP are decoded".into(),
        ))
    }
}

/// A WebP image (lossy, lossless, with or without alpha; the first
/// frame of an animation is not taken: it is refused) by the
/// `image-webp` crate.
fn decode_webp(bytes: &[u8]) -> Result<Image, TextureError> {
    let webp = |e: image_webp::DecodingError| bad(format!("WebP: {e}"));
    let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).map_err(webp)?;
    if d.is_animated() {
        return Err(TextureError::Unsupported("an animated WebP".into()));
    }
    let (width, height) = d.dimensions();
    if u64::from(width) * u64::from(height) > crate::decode::MAX_PIXELS {
        return Err(TextureError::Unsupported(format!(
            "a {width} × {height} WebP is over {} pixels",
            crate::decode::MAX_PIXELS
        )));
    }
    let size = d
        .output_buffer_size()
        .ok_or_else(|| bad("a WebP too large to hold"))?;
    let mut buf = vec![0u8; size];
    d.read_image(&mut buf).map_err(webp)?;
    let rgba = if d.has_alpha() {
        buf
    } else {
        buf.as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect()
    };
    Ok(Image {
        width,
        height,
        rgba,
    })
}

// ── Box filter ──────────────────────────────────────────────────────────────

/// Fixed-point scale of linear values.
const LIN_ONE: u32 = 1 << 20;

/// sRGB byte to linear, × [`LIN_ONE`], rounded.
fn lin_table() -> [u32; 256] {
    core::array::from_fn(|i| {
        let l = crate::export::linear(i as f64 / 255.0);
        (l * f64::from(LIN_ONE)).round() as u32
    })
}

/// The sRGB byte whose linear value is nearest `v` (ties to the lower).
fn to_srgb(table: &[u32; 256], v: u32) -> u8 {
    let i = table.partition_point(|&x| x < v);
    if i == 0 {
        return 0;
    }
    if i == 256 {
        return 255;
    }
    if v - table[i - 1] <= table[i] - v {
        (i - 1) as u8
    } else {
        i as u8
    }
}

/// The weights of one axis: for each output texel, the input texels it
/// covers and how much of each (integer lengths, summing to `from`).
fn weights(from: u32, to: u32) -> Vec<Vec<(usize, u64)>> {
    let (from, to) = (u64::from(from), u64::from(to));
    (0..to)
        .map(|i| {
            // Output i spans [i·from, (i+1)·from); input j spans
            // [j·to, (j+1)·to), in the same units.
            let (lo, hi) = (i * from, (i + 1) * from);
            let mut w = Vec::new();
            let mut j = lo / to;
            while j * to < hi {
                let overlap = hi.min((j + 1) * to) - lo.max(j * to);
                if overlap > 0 {
                    w.push((j as usize, overlap));
                }
                j += 1;
            }
            w
        })
        .collect()
}

fn resize(img: &Image, tw: u32, th: u32, color: bool) -> Image {
    let (w, h) = (img.width as usize, img.height as usize);
    let table = lin_table();
    // Each channel in fixed point: linear × 2²⁰ for colour, × 2¹² else.
    let plain_shift = 12;
    let value = |c: usize, b: u8| -> u64 {
        if color && c < 3 {
            u64::from(table[b as usize])
        } else {
            u64::from(b) << plain_shift
        }
    };
    let wx = weights(img.width, tw);
    let wy = weights(img.height, th);
    let (fx, fy) = (u64::from(img.width), u64::from(img.height));
    // Horizontal pass: h rows of tw texels.
    let mut mid = vec![0u64; h * tw as usize * 4];
    for y in 0..h {
        let row = &img.rgba[y * w * 4..(y + 1) * w * 4];
        for (x, ws) in wx.iter().enumerate() {
            for c in 0..4 {
                let sum: u64 = ws.iter().map(|&(j, k)| value(c, row[j * 4 + c]) * k).sum();
                mid[(y * tw as usize + x) * 4 + c] = (sum + fx / 2) / fx;
            }
        }
    }
    // Vertical pass.
    let mut rgba = vec![0u8; tw as usize * th as usize * 4];
    for (y, ws) in wy.iter().enumerate() {
        for x in 0..tw as usize {
            for c in 0..4 {
                let sum: u64 = ws
                    .iter()
                    .map(|&(j, k)| mid[(j * tw as usize + x) * 4 + c] * k)
                    .sum();
                let v = (sum + fy / 2) / fy;
                rgba[(y * tw as usize + x) * 4 + c] = if color && c < 3 {
                    to_srgb(&table, v as u32)
                } else {
                    ((v + (1 << (plain_shift - 1))) >> plain_shift).min(255) as u8
                };
            }
        }
    }
    Image {
        width: tw,
        height: th,
        rgba,
    }
}

// ── PNG ─────────────────────────────────────────────────────────────────────

fn be32(b: &[u8], at: usize) -> Result<u32, TextureError> {
    b.get(at..at + 4)
        .map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
        .ok_or_else(|| bad("PNG is cut short"))
}

fn decode_png(b: &[u8]) -> Result<Image, TextureError> {
    let mut at = 8;
    let mut header: Option<(u32, u32, u8, u8, u8)> = None;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut trns: Vec<u8> = Vec::new();
    let mut data: Vec<u8> = Vec::new();
    while at + 8 <= b.len() {
        let len = be32(b, at)? as usize;
        let kind = &b[at + 4..at + 8];
        let body = b
            .get(at + 8..at + 8 + len)
            .ok_or_else(|| bad("a PNG chunk runs past the end"))?;
        match kind {
            b"IHDR" if body.len() >= 13 => {
                if body[10] != 0 || body[11] != 0 {
                    return Err(bad("PNG compression or filter method"));
                }
                header = Some((be32(body, 0)?, be32(body, 4)?, body[8], body[9], body[12]));
            }
            b"PLTE" => palette = body.as_chunks::<3>().0.to_vec(),
            b"tRNS" => trns = body.to_vec(),
            b"IDAT" => data.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    let (w, h, depth, kind, interlace) = header.ok_or_else(|| bad("PNG has no IHDR"))?;
    if w == 0 || h == 0 || u64::from(w) * u64::from(h) > 1 << 28 {
        return Err(bad("PNG size"));
    }
    let channels: usize = match kind {
        0 | 3 => 1,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return Err(bad("PNG colour type")),
    };
    let ok_depth = match kind {
        0 => [1, 2, 4, 8, 16].contains(&depth),
        3 => [1, 2, 4, 8].contains(&depth),
        _ => [8, 16].contains(&depth),
    };
    if !ok_depth {
        return Err(bad("PNG bit depth"));
    }
    let bits = channels * depth as usize;
    let bpp = bits.div_ceil(8).max(1);
    // Passes: (x0, y0, dx, dy); one for a plain image, Adam7's seven else.
    let passes: &[(usize, usize, usize, usize)] = if interlace == 0 {
        &[(0, 0, 1, 1)]
    } else {
        &[
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ]
    };
    let (w, h) = (w as usize, h as usize);
    let mut limit = 0usize;
    for &(x0, y0, dx, dy) in passes {
        let (pw, ph) = ((w + dx - 1 - x0) / dx, (h + dy - 1 - y0) / dy);
        if pw > 0 && ph > 0 {
            limit += ph * (1 + (pw * bits).div_ceil(8));
        }
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&data, limit)
        .map_err(|_| bad("PNG data does not inflate"))?;
    let mut rgba = vec![0u8; w * h * 4];
    let sample = |row: &[u8], x: usize, c: usize| -> u16 {
        let i = x * channels + c;
        match depth {
            16 => u16::from_be_bytes([row[i * 2], row[i * 2 + 1]]),
            8 => u16::from(row[i]),
            d => {
                let d = d as usize;
                let bit = i * d;
                let byte = row[bit / 8];
                let shift = 8 - d - bit % 8;
                u16::from((byte >> shift) & ((1u8 << d) - 1))
            }
        }
    };
    let to8 = |v: u16| -> u8 {
        match depth {
            16 => ((u32::from(v) * 255 + 32767) / 65535) as u8,
            8 => v as u8,
            d => (u32::from(v) * 255 / ((1 << d) - 1)) as u8,
        }
    };
    let key = |c: usize| -> Option<u16> {
        trns.get(c * 2..c * 2 + 2)
            .map(|x| u16::from_be_bytes([x[0], x[1]]))
    };
    let mut at = 0;
    for &(x0, y0, dx, dy) in passes {
        let (pw, ph) = ((w + dx - 1 - x0) / dx, (h + dy - 1 - y0) / dy);
        if pw == 0 || ph == 0 {
            continue;
        }
        let stride = (pw * bits).div_ceil(8);
        let mut prev = vec![0u8; stride];
        for py in 0..ph {
            let filter = *raw.get(at).ok_or_else(|| bad("PNG data is short"))?;
            let mut row = raw
                .get(at + 1..at + 1 + stride)
                .ok_or_else(|| bad("PNG data is short"))?
                .to_vec();
            at += 1 + stride;
            for i in 0..stride {
                let a = if i >= bpp { row[i - bpp] } else { 0 };
                let up = prev[i];
                let c = if i >= bpp { prev[i - bpp] } else { 0 };
                row[i] = row[i].wrapping_add(match filter {
                    0 => 0,
                    1 => a,
                    2 => up,
                    3 => ((u16::from(a) + u16::from(up)) / 2) as u8,
                    4 => {
                        let p = i16::from(a) + i16::from(up) - i16::from(c);
                        let (pa, pb, pc) = (
                            (p - i16::from(a)).abs(),
                            (p - i16::from(up)).abs(),
                            (p - i16::from(c)).abs(),
                        );
                        if pa <= pb && pa <= pc {
                            a
                        } else if pb <= pc {
                            up
                        } else {
                            c
                        }
                    }
                    _ => return Err(bad("PNG filter type")),
                });
            }
            let y = y0 + py * dy;
            for px in 0..pw {
                let x = x0 + px * dx;
                let o = (y * w + x) * 4;
                let px_rgba: [u8; 4] = match kind {
                    0 => {
                        let v = sample(&row, px, 0);
                        let g = to8(v);
                        [g, g, g, if key(0) == Some(v) { 0 } else { 255 }]
                    }
                    2 => {
                        let v = [0, 1, 2].map(|c| sample(&row, px, c));
                        let transparent = (0..3).all(|c| key(c) == Some(v[c]));
                        [
                            to8(v[0]),
                            to8(v[1]),
                            to8(v[2]),
                            if transparent { 0 } else { 255 },
                        ]
                    }
                    3 => {
                        let i = sample(&row, px, 0) as usize;
                        let p = palette.get(i).ok_or_else(|| bad("PNG palette index"))?;
                        [p[0], p[1], p[2], trns.get(i).copied().unwrap_or(255)]
                    }
                    4 => {
                        let g = to8(sample(&row, px, 0));
                        [g, g, g, to8(sample(&row, px, 1))]
                    }
                    _ => [0, 1, 2, 3].map(|c| to8(sample(&row, px, c))),
                };
                rgba[o..o + 4].copy_from_slice(&px_rgba);
            }
            prev = row;
        }
    }
    Ok(Image {
        width: w as u32,
        height: h as u32,
        rgba,
    })
}

// ── JPEG (baseline) ─────────────────────────────────────────────────────────

const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// A Huffman table as the JPEG specification decodes it (F.2.2.3): per
/// code length, the largest code, and where its symbols start.
#[derive(Clone, Default)]
struct Huffman {
    maxcode: [i32; 17],
    valptr: [i32; 17],
    mincode: [i32; 17],
    symbols: Vec<u8>,
}

impl Huffman {
    fn new(counts: &[u8; 16], symbols: Vec<u8>) -> Huffman {
        let mut h = Huffman {
            maxcode: [-1; 17],
            symbols,
            ..Default::default()
        };
        let (mut code, mut k) = (0i32, 0i32);
        for l in 1..=16 {
            let n = i32::from(counts[l - 1]);
            if n > 0 {
                h.valptr[l] = k;
                h.mincode[l] = code;
                code += n;
                k += n;
                h.maxcode[l] = code - 1;
            }
            code <<= 1;
        }
        h
    }
}

/// Bits of one entropy-coded segment (stuffing removed).
struct Bits<'a> {
    data: &'a [u8],
    at: usize,
    acc: u32,
    n: u32,
}

impl Bits<'_> {
    fn bit(&mut self) -> u32 {
        if self.n == 0 {
            // Past the end: zeros, as decoders pad.
            self.acc = u32::from(self.data.get(self.at).copied().unwrap_or(0));
            self.at += 1;
            self.n = 8;
        }
        self.n -= 1;
        (self.acc >> self.n) & 1
    }

    fn bits(&mut self, n: u32) -> u32 {
        (0..n).fold(0, |v, _| (v << 1) | self.bit())
    }

    fn decode(&mut self, h: &Huffman) -> Result<u8, TextureError> {
        let mut code = 0i32;
        for l in 1..=16 {
            code = (code << 1) | self.bit() as i32;
            if h.maxcode[l] >= 0 && code <= h.maxcode[l] {
                let i = h.valptr[l] + code - h.mincode[l];
                return h
                    .symbols
                    .get(i as usize)
                    .copied()
                    .ok_or_else(|| bad("JPEG Huffman code"));
            }
        }
        Err(bad("JPEG Huffman code"))
    }

    /// A coefficient of `s` bits, sign-extended (F.2.2.1).
    fn extend(&mut self, s: u32) -> i32 {
        if s == 0 {
            return 0;
        }
        let v = self.bits(s) as i32;
        if v < 1 << (s - 1) {
            v - (1 << s) + 1
        } else {
            v
        }
    }
}

struct Component {
    id: u8,
    h: usize,
    v: usize,
    tq: usize,
    /// Blocks across and down, padded to whole MCUs.
    bw: usize,
    bh: usize,
    /// Decoded samples, `bw·8` × `bh·8`.
    samples: Vec<u8>,
    /// A progressive frame's coefficients as its scans build them, 64 a
    /// block (not yet scaled by the quantization table); empty otherwise.
    coef: Vec<i32>,
}

/// The inverse DCT basis, × 2¹³: `T[x][u] = c(u)/2 · cos((2x+1)uπ/16)`.
fn idct_table() -> [[i64; 8]; 8] {
    let pi = core::f64::consts::PI;
    core::array::from_fn(|x| {
        core::array::from_fn(|u| {
            let c = if u == 0 {
                core::f64::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            let v = c / 2.0 * shape_core::trig::cos((2 * x + 1) as f64 * u as f64 * pi / 16.0);
            (v * 8192.0).round() as i64
        })
    })
}

/// The 8×8 block of dequantized coefficients (natural order) to samples.
fn idct(t: &[[i64; 8]; 8], f: &[i32; 64], out: &mut [u8], stride: usize) {
    // Rows: tmp[v][x] = Σu T[x][u]·F[v][u], kept × 2³.
    let mut tmp = [[0i64; 8]; 8];
    for v in 0..8 {
        let row = &f[v * 8..v * 8 + 8];
        if row.iter().all(|&c| c == 0) {
            continue;
        }
        for x in 0..8 {
            let s: i64 = (0..8).map(|u| t[x][u] * i64::from(row[u])).sum();
            tmp[v][x] = (s + (1 << 9)) >> 10;
        }
    }
    // Columns: out[y][x] = Σv T[y][v]·tmp[v][x], × 2¹⁶ in all.
    for y in 0..8 {
        for x in 0..8 {
            let s: i64 = (0..8).map(|v| t[y][v] * tmp[v][x]).sum();
            let p = ((s + (1 << 15)) >> 16) + 128;
            out[y * stride + x] = p.clamp(0, 255) as u8;
        }
    }
}

fn decode_jpeg(b: &[u8]) -> Result<Image, TextureError> {
    let be16 = |at: usize| -> Result<usize, TextureError> {
        b.get(at..at + 2)
            .map(|x| usize::from(u16::from_be_bytes([x[0], x[1]])))
            .ok_or_else(|| bad("JPEG is cut short"))
    };
    let mut quant = [[0i32; 64]; 4];
    let mut dc: Vec<Huffman> = vec![Huffman::default(); 4];
    let mut ac: Vec<Huffman> = vec![Huffman::default(); 4];
    let mut comps: Vec<Component> = Vec::new();
    let (mut width, mut height) = (0usize, 0usize);
    let (mut hmax, mut vmax) = (1usize, 1usize);
    let mut restart = 0usize;
    let mut progressive = false;
    let mut adobe: Option<u8> = None;
    let mut jfif = false;
    let mut at = 2;
    let table = idct_table();
    loop {
        // Next marker.
        while b.get(at) == Some(&0xFF) && b.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        if b.get(at) != Some(&0xFF) {
            return Err(bad("JPEG marker expected"));
        }
        let marker = *b.get(at + 1).ok_or_else(|| bad("JPEG is cut short"))?;
        at += 2;
        if marker == 0xD9 {
            break;
        }
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            continue;
        }
        let len = be16(at)?;
        let seg = b
            .get(at + 2..at + len)
            .ok_or_else(|| bad("a JPEG segment runs past the end"))?;
        match marker {
            0xC0..=0xC2 => {
                progressive = marker == 0xC2;
                if seg.first() != Some(&8) {
                    return Err(TextureError::Unsupported(
                        "a JPEG of other than 8-bit precision".into(),
                    ));
                }
                height = usize::from(u16::from_be_bytes([seg[1], seg[2]]));
                width = usize::from(u16::from_be_bytes([seg[3], seg[4]]));
                let n = usize::from(seg[5]);
                if width == 0 || height == 0 || !(n == 1 || n == 3) || seg.len() < 6 + 3 * n {
                    return Err(TextureError::Unsupported(format!(
                        "a JPEG of {n} components or no height"
                    )));
                }
                for k in 0..n {
                    let c = &seg[6 + 3 * k..9 + 3 * k];
                    let (h, v) = (usize::from(c[1] >> 4), usize::from(c[1] & 15));
                    if !(1..=4).contains(&h) || !(1..=4).contains(&v) || c[2] > 3 {
                        return Err(bad("JPEG sampling factors"));
                    }
                    comps.push(Component {
                        id: c[0],
                        h,
                        v,
                        tq: usize::from(c[2]),
                        bw: 0,
                        bh: 0,
                        samples: Vec::new(),
                        coef: Vec::new(),
                    });
                }
                hmax = comps.iter().map(|c| c.h).max().unwrap_or(1);
                vmax = comps.iter().map(|c| c.v).max().unwrap_or(1);
                let (mx, my) = (width.div_ceil(8 * hmax), height.div_ceil(8 * vmax));
                for c in &mut comps {
                    c.bw = mx * c.h;
                    c.bh = my * c.v;
                    c.samples = vec![0; c.bw * 8 * c.bh * 8];
                    if progressive {
                        c.coef = vec![0; c.bw * c.bh * 64];
                    }
                }
            }
            0xC3..=0xCF if marker != 0xC4 && marker != 0xC8 && marker != 0xCC => {
                return Err(TextureError::Unsupported(
                    "a lossless, hierarchical or arithmetic-coded JPEG".into(),
                ));
            }
            0xC4 => {
                let mut i = 0;
                while i + 17 <= seg.len() {
                    let (class, id) = (seg[i] >> 4, usize::from(seg[i] & 15));
                    let counts: [u8; 16] = seg[i + 1..i + 17].try_into().unwrap_or([0; 16]);
                    let n: usize = counts.iter().map(|&c| usize::from(c)).sum();
                    let symbols = seg
                        .get(i + 17..i + 17 + n)
                        .ok_or_else(|| bad("JPEG Huffman table"))?
                        .to_vec();
                    if id > 3 {
                        return Err(bad("JPEG Huffman table id"));
                    }
                    let h = Huffman::new(&counts, symbols);
                    if class == 0 {
                        dc[id] = h;
                    } else {
                        ac[id] = h;
                    }
                    i += 17 + n;
                }
            }
            0xDB => {
                let mut i = 0;
                while i < seg.len() {
                    let (p, id) = (seg[i] >> 4, usize::from(seg[i] & 15));
                    if id > 3 {
                        return Err(bad("JPEG quantization table id"));
                    }
                    for k in 0..64 {
                        quant[id][ZIGZAG[k]] = if p == 0 {
                            i32::from(*seg.get(i + 1 + k).ok_or_else(|| bad("JPEG DQT"))?)
                        } else {
                            let at = i + 1 + 2 * k;
                            i32::from(u16::from_be_bytes([
                                *seg.get(at).ok_or_else(|| bad("JPEG DQT"))?,
                                *seg.get(at + 1).ok_or_else(|| bad("JPEG DQT"))?,
                            ]))
                        };
                    }
                    i += 1 + if p == 0 { 64 } else { 128 };
                }
            }
            0xDD => restart = usize::from(u16::from_be_bytes([seg[0], seg[1]])),
            0xE0 if seg.starts_with(b"JFIF\0") => jfif = true,
            0xEE if seg.starts_with(b"Adobe") && seg.len() >= 12 => adobe = Some(seg[11]),
            0xDA => {
                if comps.is_empty() {
                    return Err(bad("JPEG scan before its frame"));
                }
                let n = usize::from(seg[0]);
                let mut scan: Vec<(usize, usize, usize)> = Vec::new();
                for k in 0..n {
                    let id = seg[1 + 2 * k];
                    let t = seg[2 + 2 * k];
                    let ci = comps
                        .iter()
                        .position(|c| c.id == id)
                        .ok_or_else(|| bad("JPEG scan names no component"))?;
                    scan.push((ci, usize::from(t >> 4).min(3), usize::from(t & 15).min(3)));
                }
                // The entropy-coded data, split at restart markers, stuffing
                // removed.
                at += len;
                let mut chunks: Vec<Vec<u8>> = vec![Vec::new()];
                while at < b.len() {
                    if b[at] == 0xFF {
                        let next = *b.get(at + 1).unwrap_or(&0xD9);
                        if next == 0 {
                            if let Some(c) = chunks.last_mut() {
                                c.push(0xFF);
                            }
                            at += 2;
                            continue;
                        }
                        if (0xD0..=0xD7).contains(&next) {
                            chunks.push(Vec::new());
                            at += 2;
                            continue;
                        }
                        if next == 0xFF {
                            at += 1;
                            continue;
                        }
                        break;
                    }
                    if let Some(c) = chunks.last_mut() {
                        c.push(b[at]);
                    }
                    at += 1;
                }
                if progressive {
                    // The band of coefficients this scan carries, and
                    // which bit of them (G.1.1).
                    let band = seg
                        .get(1 + 2 * n..4 + 2 * n)
                        .ok_or_else(|| bad("JPEG scan header"))?;
                    scan_progressive(
                        &mut comps,
                        &scan,
                        &chunks,
                        restart,
                        &dc,
                        &ac,
                        (
                            usize::from(band[0]),
                            usize::from(band[1]),
                            u32::from(band[2] >> 4),
                            u32::from(band[2] & 15),
                        ),
                        (hmax, vmax, width, height),
                    )?;
                } else {
                    scan_data(
                        &mut comps,
                        &scan,
                        &chunks,
                        restart,
                        &quant,
                        &dc,
                        &ac,
                        &table,
                        (hmax, vmax, width, height),
                    )?;
                }
                continue;
            }
            _ => {}
        }
        at += len;
    }
    if comps.is_empty() {
        return Err(bad("JPEG has no frame"));
    }
    if progressive {
        // Every scan is in: scale each block by its table and transform it.
        let mut f = [0i32; 64];
        for c in &mut comps {
            let q = &quant[c.tq];
            let stride = c.bw * 8;
            for by in 0..c.bh {
                for bx in 0..c.bw {
                    let block = &c.coef[(by * c.bw + bx) * 64..][..64];
                    for i in 0..64 {
                        f[i] = block[i] * q[i];
                    }
                    idct(
                        &table,
                        &f,
                        &mut c.samples[by * 8 * stride + bx * 8..],
                        stride,
                    );
                }
            }
        }
    }
    let rgb_ids = comps.len() == 3 && comps.iter().map(|c| c.id).eq(*b"RGB");
    let ycc = comps.len() == 3 && !rgb_ids && (jfif || adobe != Some(0));
    let planes: Vec<Vec<i32>> = comps
        .iter()
        .map(|c| upsample(c, hmax, vmax, width, height))
        .collect();
    let mut rgba = vec![255u8; width * height * 4];
    for i in 0..width * height {
        let o = i * 4;
        if comps.len() == 1 {
            let g = planes[0][i] as u8;
            rgba[o..o + 3].copy_from_slice(&[g, g, g]);
        } else if ycc {
            let (yy, cb, cr) = (planes[0][i], planes[1][i] - 128, planes[2][i] - 128);
            let r = yy + ((91_881 * cr + 32_768) >> 16);
            let g = yy + ((-22_554 * cb - 46_802 * cr + 32_768) >> 16);
            let bl = yy + ((116_130 * cb + 32_768) >> 16);
            rgba[o..o + 3].copy_from_slice(&[r, g, bl].map(|v| v.clamp(0, 255) as u8));
        } else {
            rgba[o..o + 3].copy_from_slice(&[0, 1, 2].map(|k| planes[k][i] as u8));
        }
    }
    Ok(Image {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

/// A component's samples at the image's size. Half resolution in either
/// direction is filled by the triangle filter (each output sample ¾ the
/// nearer input sample and ¼ the next, clamped at the edges, the weights
/// of both directions multiplied); any other ratio by the nearest sample.
fn upsample(c: &Component, hmax: usize, vmax: usize, width: usize, height: usize) -> Vec<i32> {
    let stride = c.bw * 8;
    let (cw, ch) = (
        (width * c.h).div_ceil(hmax).max(1),
        (height * c.v).div_ceil(vmax).max(1),
    );
    let at = |x: usize, y: usize| i32::from(c.samples[y.min(ch - 1) * stride + x.min(cw - 1)]);
    // Per axis: for output i, the two input samples and their weights
    // out of 4 (one sample with weight 4 when not halved).
    let taps = |n: usize, m: usize, cn: usize| -> Vec<(usize, usize, i32)> {
        (0..n)
            .map(|i| {
                if m == 2 {
                    let s = i / 2;
                    let other = if i % 2 == 0 {
                        s.saturating_sub(1)
                    } else {
                        (s + 1).min(cn - 1)
                    };
                    (s, other, 3)
                } else {
                    let s = (i / m).min(cn - 1);
                    (s, s, 4)
                }
            })
            .collect()
    };
    let tx = taps(width, hmax / c.h, cw);
    let ty = taps(height, vmax / c.v, ch);
    let mut out = Vec::with_capacity(width * height);
    for &(y0, y1, wy) in &ty {
        for &(x0, x1, wx) in &tx {
            let v = wy * (wx * at(x0, y0) + (4 - wx) * at(x1, y0))
                + (4 - wy) * (wx * at(x0, y1) + (4 - wx) * at(x1, y1));
            out.push((v + 8) >> 4);
        }
    }
    out
}

/// Decodes one scan's blocks into the components' samples.
/// The blocks of a scan in coding order, as (scan component, block
/// across, block down), and how many of them a restart interval holds:
/// per MCU, each scan component's h × v blocks; a scan of one component
/// goes block by block over its own size (A.2.2).
fn block_order(
    comps: &[Component],
    scan: &[(usize, usize, usize)],
    restart: usize,
    (hmax, vmax, width, height): (usize, usize, usize, usize),
) -> (Vec<(usize, usize, usize)>, usize) {
    let mut order: Vec<(usize, usize, usize)> = Vec::new();
    if scan.len() == 1 {
        let c = &comps[scan[0].0];
        let (cw, ch) = (
            (width * c.h).div_ceil(hmax).div_ceil(8),
            (height * c.v).div_ceil(vmax).div_ceil(8),
        );
        for by in 0..ch {
            for bx in 0..cw {
                order.push((0, bx, by));
            }
        }
    } else {
        let (mx, my) = (width.div_ceil(8 * hmax), height.div_ceil(8 * vmax));
        for my_ in 0..my {
            for mx_ in 0..mx {
                for (si, &(ci, _, _)) in scan.iter().enumerate() {
                    let c = &comps[ci];
                    for v in 0..c.v {
                        for h in 0..c.h {
                            order.push((si, mx_ * c.h + h, my_ * c.v + v));
                        }
                    }
                }
            }
        }
    }
    let per_mcu = if scan.len() == 1 {
        1
    } else {
        scan.iter()
            .map(|&(ci, _, _)| comps[ci].h * comps[ci].v)
            .sum()
    };
    let interval = if restart > 0 {
        restart * per_mcu
    } else {
        usize::MAX
    };
    (order, interval)
}

/// One scan of a progressive frame (G.1.2): a band `ss..=se` of each
/// block's coefficients in zigzag order, either their bits from `al` up
/// (`ah` 0) or one more bit, `al`, of what earlier scans sent. The DC
/// coefficient has scans of its own, over any of the components; the
/// others come one component a scan, with runs of blocks that end early
/// counted across blocks.
#[allow(clippy::too_many_arguments)]
fn scan_progressive(
    comps: &mut [Component],
    scan: &[(usize, usize, usize)],
    chunks: &[Vec<u8>],
    restart: usize,
    dc: &[Huffman],
    ac: &[Huffman],
    (ss, se, ah, al): (usize, usize, u32, u32),
    dims: (usize, usize, usize, usize),
) -> Result<(), TextureError> {
    if ss > se || se > 63 || al > 13 || (ss == 0 && se != 0) || (ss > 0 && scan.len() != 1) {
        return Err(bad("JPEG progressive scan"));
    }
    let (order, interval) = block_order(comps, scan, restart, dims);
    let mut pred = vec![0i32; scan.len()];
    let mut eobrun = 0u32;
    let mut chunk = 0;
    let mut bits = Bits {
        data: chunks.first().map_or(&[][..], |c| c.as_slice()),
        at: 0,
        acc: 0,
        n: 0,
    };
    let one = 1i32 << al;
    for (k, &(si, bx, by)) in order.iter().enumerate() {
        if k > 0 && k % interval == 0 {
            chunk += 1;
            bits = Bits {
                data: chunks.get(chunk).map_or(&[][..], |c| c.as_slice()),
                at: 0,
                acc: 0,
                n: 0,
            };
            pred.iter_mut().for_each(|p| *p = 0);
            eobrun = 0;
        }
        let (ci, td, ta) = scan[si];
        let c = &mut comps[ci];
        if bx >= c.bw || by >= c.bh {
            return Err(bad("JPEG block outside its component"));
        }
        let block = &mut c.coef[(by * c.bw + bx) * 64..][..64];
        // One more bit of a coefficient already sent, towards its sign.
        let refine = |v: &mut i32, bits: &mut Bits| {
            if bits.bit() == 1 && (*v & one) == 0 {
                *v += if *v >= 0 { one } else { -one };
            }
        };
        if ss == 0 {
            if ah == 0 {
                let s = bits.decode(&dc[td])?;
                pred[si] += bits.extend(u32::from(s));
                block[0] = pred[si] << al;
            } else if bits.bit() == 1 {
                block[0] |= one;
            }
            continue;
        }
        if ah == 0 {
            if eobrun > 0 {
                eobrun -= 1;
                continue;
            }
            let mut i = ss;
            while i <= se {
                let rs = bits.decode(&ac[ta])?;
                let (r, s) = (u32::from(rs >> 4), u32::from(rs & 15));
                if s == 0 {
                    if r < 15 {
                        // This block ends here, and 2^r − 1 + more after it.
                        eobrun = (1 << r) - 1;
                        if r > 0 {
                            eobrun += bits.bits(r);
                        }
                        break;
                    }
                    i += 16;
                    continue;
                }
                i += r as usize;
                if i > se {
                    return Err(bad("JPEG coefficient run past the band"));
                }
                block[ZIGZAG[i]] = bits.extend(s) << al;
                i += 1;
            }
            continue;
        }
        // Refinement of the band: new coefficients of size one, each
        // after a run of still-zero ones; every coefficient passed that
        // is not zero gets its next bit.
        let mut i = ss;
        if eobrun == 0 {
            while i <= se {
                let rs = bits.decode(&ac[ta])?;
                let (mut r, s) = (u32::from(rs >> 4), u32::from(rs & 15));
                let mut new = 0;
                match s {
                    0 if r < 15 => {
                        eobrun = 1 << r;
                        if r > 0 {
                            eobrun += bits.bits(r);
                        }
                        break;
                    }
                    0 => {}
                    1 => new = if bits.bit() == 1 { one } else { -one },
                    _ => return Err(bad("JPEG refinement of a size other than one")),
                }
                while i <= se {
                    let v = &mut block[ZIGZAG[i]];
                    if *v != 0 {
                        refine(v, &mut bits);
                    } else {
                        if r == 0 {
                            break;
                        }
                        r -= 1;
                    }
                    i += 1;
                }
                if new != 0 {
                    if i > se {
                        return Err(bad("JPEG coefficient run past the band"));
                    }
                    block[ZIGZAG[i]] = new;
                }
                i += 1;
            }
        }
        if eobrun > 0 {
            // The rest of the band has nothing new: only next bits.
            while i <= se {
                let v = &mut block[ZIGZAG[i]];
                if *v != 0 {
                    refine(v, &mut bits);
                }
                i += 1;
            }
            eobrun -= 1;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn scan_data(
    comps: &mut [Component],
    scan: &[(usize, usize, usize)],
    chunks: &[Vec<u8>],
    restart: usize,
    quant: &[[i32; 64]; 4],
    dc: &[Huffman],
    ac: &[Huffman],
    table: &[[i64; 8]; 8],
    (hmax, vmax, width, height): (usize, usize, usize, usize),
) -> Result<(), TextureError> {
    let (order, interval) = block_order(comps, scan, restart, (hmax, vmax, width, height));
    let mut pred = vec![0i32; scan.len()];
    let mut chunk = 0;
    let mut bits = Bits {
        data: chunks.first().map_or(&[][..], |c| c.as_slice()),
        at: 0,
        acc: 0,
        n: 0,
    };
    let mut f = [0i32; 64];
    for (k, &(si, bx, by)) in order.iter().enumerate() {
        if k > 0 && k % interval == 0 {
            chunk += 1;
            bits = Bits {
                data: chunks.get(chunk).map_or(&[][..], |c| c.as_slice()),
                at: 0,
                acc: 0,
                n: 0,
            };
            pred.iter_mut().for_each(|p| *p = 0);
        }
        let (ci, td, ta) = scan[si];
        let q = &quant[comps[ci].tq];
        f.fill(0);
        let s = bits.decode(&dc[td])?;
        pred[si] += bits.extend(u32::from(s));
        f[0] = pred[si] * q[0];
        let mut i = 1;
        while i < 64 {
            let rs = bits.decode(&ac[ta])?;
            let (r, s) = (usize::from(rs >> 4), u32::from(rs & 15));
            if s == 0 {
                if r == 15 {
                    i += 16;
                    continue;
                }
                break;
            }
            i += r;
            if i > 63 {
                return Err(bad("JPEG coefficient run past the block"));
            }
            f[ZIGZAG[i]] = bits.extend(s) * q[ZIGZAG[i]];
            i += 1;
        }
        let c = &mut comps[ci];
        if bx < c.bw && by < c.bh {
            let stride = c.bw * 8;
            idct(
                table,
                &f,
                &mut c.samples[by * 8 * stride + bx * 8..],
                stride,
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: u32, h: u32) -> Image {
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgba.extend([(x * 255 / w) as u8, (y * 255 / h) as u8, 128, 255]);
            }
        }
        Image {
            width: w,
            height: h,
            rgba,
        }
    }

    #[test]
    fn png_round_trips_and_shrinks_by_area() {
        let img = gradient(10, 6);
        assert_eq!(decode(&crate::png(&img)).unwrap(), img);
        // Halving a 2×2 checker of 0 and 255: plain gives the mean byte,
        // colour the byte of the mean light (188), not 128.
        let checker = Image {
            width: 2,
            height: 2,
            rgba: [0u8, 255, 255, 0]
                .iter()
                .flat_map(|&v| [v, v, v, 255])
                .collect(),
        };
        let t = Texture::from_bytes("c.jpg", crate::png(&checker));
        for (color, want) in [(false, 128), (true, 188)] {
            let small = downsize(&t, &Downsize { max: 1, color }).unwrap().unwrap();
            assert_eq!(
                (small.name.as_str(), small.mime.as_str()),
                ("c.png", "image/png")
            );
            let d = decode(&small.bytes).unwrap();
            assert_eq!(d.rgba, vec![want, want, want, 255]);
        }
        // Within the limit: kept as it is.
        assert_eq!(
            downsize(
                &t,
                &Downsize {
                    max: 2,
                    color: true
                }
            )
            .unwrap(),
            None
        );
        // 10 × 6 to 5 × 3: the aspect kept, each texel the mean of four.
        let small = resize(&img, 5, 3, false);
        // Red 0 and 25 in each row: 12.5, rounded up.
        assert_eq!(small.rgba[0], 13);
    }

    #[test]
    fn png_bit_depths_and_palettes_decode() {
        // 1-bit grey: 0b1010_0000, 3 wide, 1 high.
        let mut f = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let chunk = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
            out.extend((data.len() as u32).to_be_bytes());
            out.extend(kind);
            out.extend(data);
            out.extend([0, 0, 0, 0]);
        };
        chunk(&mut f, b"IHDR", &[0, 0, 0, 3, 0, 0, 0, 1, 1, 0, 0, 0, 0]);
        let z = miniz_oxide::deflate::compress_to_vec_zlib(&[0, 0b1010_0000], 6);
        chunk(&mut f, b"IDAT", &z);
        chunk(&mut f, b"IEND", &[]);
        let d = decode(&f).unwrap();
        assert_eq!(
            d.rgba,
            vec![255, 255, 255, 255, 0, 0, 0, 255, 255, 255, 255, 255]
        );
    }
}

#[cfg(test)]
mod webp_tests {
    use super::*;

    /// A WebP image decodes to the texels it was made from (a lossless
    /// one, with alpha and without); an animation and a broken file
    /// are refused with the reason.
    #[test]
    fn a_webp_image_is_decoded() {
        let (w, h) = (23u32, 17u32);
        let rgba: Vec<u8> = (0..w * h * 4)
            .map(|i| ((i * 37 + 11) % 256) as u8)
            .collect();
        let pack = |data: &[u8], kind| {
            let mut out = Vec::new();
            image_webp::WebPEncoder::new(&mut out)
                .encode(data, w, h, kind)
                .unwrap();
            out
        };
        let with_alpha = pack(&rgba, image_webp::ColorType::Rgba8);
        // The same file is kept for the gate.
        assert_eq!(
            with_alpha,
            include_bytes!("../../../golden/meshes/pattern.webp")
        );
        let im = decode(&with_alpha).unwrap();
        assert_eq!((im.width, im.height), (w, h));
        assert_eq!(im.rgba, rgba);
        let rgb: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        let opaque = decode(&pack(&rgb, image_webp::ColorType::Rgb8)).unwrap();
        for (p, q) in opaque
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(rgba.as_chunks::<4>().0)
        {
            assert_eq!(*p, [q[0], q[1], q[2], 255]);
        }
        for cut in [12, 20, with_alpha.len() / 2] {
            assert!(decode(&with_alpha[..cut]).is_err(), "cut at {cut}");
        }
    }
}

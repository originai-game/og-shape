//! Decodes a texture's PNG or JPEG bytes to RGBA8, for the preview to
//! sample (`docs/research.md` §30).
//!
//! - PNG: every colour type and bit depth of the specification, Adam7
//!   interlacing, a palette's `tRNS`; zlib by `miniz_oxide`. 16-bit
//!   samples round to 8 bits (`(v + 128) / 257`). Colour-space chunks
//!   (`gAMA`, `iCCP`, `sRGB`) are ignored: the bytes are taken as sRGB,
//!   as glTF says a base-colour image is.
//! - JPEG: `jpeg-decoder`, built without its SIMD paths
//!   (`platform_independent`): integer IDCT, upsampling and colour
//!   conversion, so the bytes are the same on every host. Grey becomes
//!   grey RGB; CMYK becomes `(255 − c)(255 − k) / 255`.
//! - Other formats (WebP, KTX2) are refused with the reason, as is an
//!   image over [`MAX_PIXELS`].
//!
//! Integer arithmetic only.

use crate::preview::Image;

/// The most pixels an image may have: 64 Mi, a 8192² texture.
pub const MAX_PIXELS: u64 = 1 << 26;

/// The image an image file's bytes hold, as RGBA8, rows top to bottom.
pub(crate) fn decode(bytes: &[u8]) -> Result<Image, String> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(bytes)
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        jpeg(bytes)
    } else {
        Err("not a PNG or JPEG image (the preview decodes those two)".into())
    }
}

fn too_big(w: u32, h: u32) -> Result<(), String> {
    if w == 0 || h == 0 {
        return Err("an image with no pixels".into());
    }
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err(format!(
            "a {w} × {h} image is over the preview's {MAX_PIXELS} pixels"
        ));
    }
    Ok(())
}

fn jpeg(bytes: &[u8]) -> Result<Image, String> {
    use jpeg_decoder::{Decoder, PixelFormat};
    let mut d = Decoder::new(bytes);
    d.read_info().map_err(|e| format!("JPEG: {e}"))?;
    let info = d.info().ok_or("JPEG: no frame header")?;
    let (w, h) = (u32::from(info.width), u32::from(info.height));
    too_big(w, h)?;
    let px = d.decode().map_err(|e| format!("JPEG: {e}"))?;
    let n = (w * h) as usize;
    let mut rgba = Vec::with_capacity(n * 4);
    match info.pixel_format {
        PixelFormat::L8 => {
            for &g in px.iter().take(n) {
                rgba.extend_from_slice(&[g, g, g, 255]);
            }
        }
        PixelFormat::L16 => {
            for g in px.as_chunks::<2>().0.iter().take(n) {
                let v = round16(u16::from_be_bytes([g[0], g[1]]));
                rgba.extend_from_slice(&[v, v, v, 255]);
            }
        }
        PixelFormat::RGB24 => {
            for c in px.as_chunks::<3>().0.iter().take(n) {
                rgba.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        PixelFormat::CMYK32 => {
            for c in px.as_chunks::<4>().0.iter().take(n) {
                let k = 255 - u32::from(c[3]);
                let ch = |v: u8| (((255 - u32::from(v)) * k + 127) / 255) as u8;
                rgba.extend_from_slice(&[ch(c[0]), ch(c[1]), ch(c[2]), 255]);
            }
        }
    }
    if rgba.len() != n * 4 {
        return Err("JPEG: fewer pixels than its header says".into());
    }
    Ok(Image {
        width: w,
        height: h,
        rgba,
    })
}

/// A 16-bit sample to 8 bits, rounded to nearest.
fn round16(v: u16) -> u8 {
    ((u32::from(v) + 128) / 257) as u8
}

fn be32(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4)
        .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| "PNG: cut short".to_string())
}

struct Header {
    width: u32,
    height: u32,
    depth: u8,
    colour: u8,
    interlaced: bool,
}

impl Header {
    /// Samples per pixel.
    fn channels(&self) -> usize {
        match self.colour {
            0 | 3 => 1,
            2 => 3,
            4 => 2,
            _ => 4,
        }
    }

    /// Bytes in a row of `w` pixels, and bytes per whole pixel (at least
    /// 1), for the filters.
    fn row(&self, w: usize) -> (usize, usize) {
        let bits = self.channels() * usize::from(self.depth);
        ((w * bits).div_ceil(8), bits.div_ceil(8).max(1))
    }
}

fn png(bytes: &[u8]) -> Result<Image, String> {
    let mut at = 8;
    let mut header: Option<Header> = None;
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut trns: Vec<u8> = Vec::new();
    let mut data: Vec<u8> = Vec::new();
    loop {
        let len = be32(bytes, at)? as usize;
        let kind = bytes.get(at + 4..at + 8).ok_or("PNG: cut short")?;
        let body = bytes
            .get(at + 8..at + 8 + len)
            .ok_or("PNG: a chunk runs past the end")?;
        match kind {
            b"IHDR" => {
                if body.len() < 13 {
                    return Err("PNG: a short IHDR".into());
                }
                let h = Header {
                    width: be32(body, 0)?,
                    height: be32(body, 4)?,
                    depth: body[8],
                    colour: body[9],
                    interlaced: body[12] == 1,
                };
                let ok = matches!(
                    (h.colour, h.depth),
                    (0, 1 | 2 | 4 | 8 | 16) | (3, 1 | 2 | 4 | 8) | (2 | 4 | 6, 8 | 16)
                );
                if !ok || body[10] != 0 || body[11] != 0 || body[12] > 1 {
                    return Err(format!(
                        "PNG: colour type {} at {} bits is not in the specification",
                        h.colour, h.depth
                    ));
                }
                too_big(h.width, h.height)?;
                header = Some(h);
            }
            b"PLTE" => {
                palette = body.as_chunks::<3>().0.to_vec();
            }
            b"tRNS" => trns = body.to_vec(),
            b"IDAT" => data.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    let h = header.ok_or("PNG: no IHDR")?;
    let (w, ht) = (h.width as usize, h.height as usize);
    // The bytes the passes need, filter bytes included.
    let passes: Vec<(usize, usize, usize, usize, usize, usize)> = if h.interlaced {
        // Adam7: (x0, y0, dx, dy) per pass.
        [
            (0, 0, 8, 8),
            (4, 0, 8, 8),
            (0, 4, 4, 8),
            (2, 0, 4, 4),
            (0, 2, 2, 4),
            (1, 0, 2, 2),
            (0, 1, 1, 2),
        ]
        .iter()
        .map(|&(x0, y0, dx, dy)| {
            let pw = (w + dx - 1 - x0) / dx;
            let ph = (ht + dy - 1 - y0) / dy;
            (
                x0,
                y0,
                dx,
                dy,
                if x0 < w { pw } else { 0 },
                if y0 < ht { ph } else { 0 },
            )
        })
        .collect()
    } else {
        vec![(0, 0, 1, 1, w, ht)]
    };
    let need: usize = passes
        .iter()
        .filter(|p| p.4 > 0 && p.5 > 0)
        .map(|p| (h.row(p.4).0 + 1) * p.5)
        .sum();
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&data, need)
        .map_err(|_| "PNG: the image data does not inflate".to_string())?;
    if raw.len() < need {
        return Err("PNG: less image data than its size needs".into());
    }
    let mut rgba = vec![0u8; w * ht * 4];
    let mut off = 0;
    for &(x0, y0, dx, dy, pw, ph) in &passes {
        if pw == 0 || ph == 0 {
            continue;
        }
        let (stride, bpp) = h.row(pw);
        let mut prev = vec![0u8; stride];
        let mut cur = vec![0u8; stride];
        for py in 0..ph {
            let filter = raw[off];
            cur.copy_from_slice(&raw[off + 1..off + 1 + stride]);
            off += stride + 1;
            unfilter(filter, &mut cur, &prev, bpp)?;
            for px in 0..pw {
                let p = pixel(&h, &cur, px, &palette, &trns);
                let (x, y) = (x0 + px * dx, y0 + py * dy);
                rgba[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&p);
            }
            std::mem::swap(&mut prev, &mut cur);
        }
    }
    Ok(Image {
        width: h.width,
        height: h.height,
        rgba,
    })
}

fn unfilter(filter: u8, cur: &mut [u8], prev: &[u8], bpp: usize) -> Result<(), String> {
    match filter {
        0 => {}
        1 => {
            for i in bpp..cur.len() {
                cur[i] = cur[i].wrapping_add(cur[i - bpp]);
            }
        }
        2 => {
            for (c, p) in cur.iter_mut().zip(prev) {
                *c = c.wrapping_add(*p);
            }
        }
        3 => {
            for i in 0..cur.len() {
                let left = if i >= bpp { u16::from(cur[i - bpp]) } else { 0 };
                cur[i] = cur[i].wrapping_add(((left + u16::from(prev[i])) / 2) as u8);
            }
        }
        4 => {
            for i in 0..cur.len() {
                let a = if i >= bpp { i16::from(cur[i - bpp]) } else { 0 };
                let b = i16::from(prev[i]);
                let c = if i >= bpp {
                    i16::from(prev[i - bpp])
                } else {
                    0
                };
                let p = a + b - c;
                let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                let pred = if pa <= pb && pa <= pc {
                    a
                } else if pb <= pc {
                    b
                } else {
                    c
                };
                cur[i] = cur[i].wrapping_add(pred as u8);
            }
        }
        f => return Err(format!("PNG: filter type {f} is not in the specification")),
    }
    Ok(())
}

/// Sample `k` of pixel `x` in an unfiltered row, at the image's depth.
fn sample(h: &Header, row: &[u8], x: usize, k: usize) -> u16 {
    let d = usize::from(h.depth);
    let i = x * h.channels() + k;
    match d {
        16 => u16::from_be_bytes([row[2 * i], row[2 * i + 1]]),
        8 => u16::from(row[i]),
        _ => {
            let bit = i * d;
            let byte = row[bit / 8];
            let shift = 8 - d - bit % 8;
            u16::from((byte >> shift) & ((1u8 << d) - 1))
        }
    }
}

fn pixel(h: &Header, row: &[u8], x: usize, palette: &[[u8; 3]], trns: &[u8]) -> [u8; 4] {
    let s = |k| sample(h, row, x, k);
    // A sample at the image's depth to 8 bits.
    let to8 = |v: u16| -> u8 {
        match h.depth {
            16 => round16(v),
            8 => v as u8,
            d => (u32::from(v) * 255 / ((1u32 << d) - 1)) as u8,
        }
    };
    // The tRNS colour key (grey or RGB) at the image's depth.
    let key = |k: usize| -> Option<u16> {
        trns.get(2 * k..2 * k + 2)
            .map(|b| u16::from_be_bytes([b[0], b[1]]))
    };
    match h.colour {
        0 => {
            let g = s(0);
            let a = if key(0) == Some(g) { 0 } else { 255 };
            let g = to8(g);
            [g, g, g, a]
        }
        2 => {
            let (r, g, b) = (s(0), s(1), s(2));
            let keyed =
                trns.len() >= 6 && key(0) == Some(r) && key(1) == Some(g) && key(2) == Some(b);
            [to8(r), to8(g), to8(b), if keyed { 0 } else { 255 }]
        }
        3 => {
            let i = usize::from(s(0));
            let c = palette.get(i).copied().unwrap_or([0, 0, 0]);
            [c[0], c[1], c[2], trns.get(i).copied().unwrap_or(255)]
        }
        4 => {
            let g = to8(s(0));
            [g, g, g, to8(s(1))]
        }
        _ => [to8(s(0)), to8(s(1)), to8(s(2)), to8(s(3))],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trips_the_encoder() {
        // Every filter but Up is exercised by the other tests' files; this
        // one checks the encoder's output comes back as it went in.
        let im = Image {
            width: 5,
            height: 3,
            rgba: (0..60).map(|i| (i * 53 % 256) as u8).collect(),
        };
        assert_eq!(decode(&crate::png(&im)).unwrap(), im);
    }

    /// A PNG of `raw` (filter bytes included) with the given header.
    fn file(
        w: u32,
        h: u32,
        depth: u8,
        colour: u8,
        interlace: u8,
        raw: &[u8],
        extra: &[(&[u8; 4], &[u8])],
    ) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let chunk = |out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]| {
            out.extend_from_slice(&(body.len() as u32).to_be_bytes());
            out.extend_from_slice(kind);
            out.extend_from_slice(body);
            out.extend_from_slice(&[0; 4]); // the CRC is not checked
        };
        let mut ihdr = w.to_be_bytes().to_vec();
        ihdr.extend_from_slice(&h.to_be_bytes());
        ihdr.extend_from_slice(&[depth, colour, 0, 0, interlace]);
        chunk(&mut out, b"IHDR", &ihdr);
        for (k, b) in extra {
            chunk(&mut out, k, b);
        }
        chunk(
            &mut out,
            b"IDAT",
            &miniz_oxide::deflate::compress_to_vec_zlib(raw, 6),
        );
        chunk(&mut out, b"IEND", &[]);
        out
    }

    #[test]
    fn png_filters_palettes_and_low_depths() {
        // 2-bit palette, 3 × 2, Sub then Paeth rows; index 1 transparent.
        let plte: &[u8] = &[10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120];
        let raw = [1, 0b00_01_10_00, 4, 0b11_10_01_00];
        let im = decode(&file(
            3,
            2,
            2,
            3,
            0,
            &raw,
            &[(b"PLTE", plte), (b"tRNS", &[255, 0])],
        ))
        .unwrap();
        // Row 0, Sub on a 1-byte pixel: one byte, unchanged (no left).
        assert_eq!(
            &im.rgba[0..12],
            &[10, 20, 30, 255, 40, 50, 60, 0, 70, 80, 90, 255]
        );
        // Row 1, Paeth with the row above: 0b11_10_01_00 + 0b00_01_10_00.
        let idx: [usize; 3] = [3, 3, 3];
        let want: Vec<u8> = idx
            .iter()
            .flat_map(|&i| [plte[3 * i], plte[3 * i + 1], plte[3 * i + 2], 255])
            .collect();
        assert_eq!(&im.rgba[12..24], &want[..]);
        // 16-bit grey with alpha, Average filter: the second pixel adds
        // half the first, 0x1234 / 2 = 0x091A and 0xFFFF / 2 + 0x80 00.
        let raw = [3, 0x12, 0x34, 0xFF, 0xFF, 0x00, 0x00, 0x80, 0x00];
        let im = decode(&file(2, 1, 16, 4, 0, &raw, &[])).unwrap();
        // 0x1234 → 18, 0x091A → 9, 0xFF7F → 255 (rounded).
        assert_eq!(im.rgba, vec![18, 18, 18, 255, 9, 9, 9, 255]);
    }

    #[test]
    fn png_adam7_places_every_pass() {
        // 3 × 3 RGB, interlaced: passes 1 (0,0), 4 (2,0), 5 (0,2),(2,2),
        // 6 (1,0),(1,2), 7 rows 1: (0,1),(1,1),(2,1). Pixel value = index.
        let mut raw = Vec::new();
        let px = |i: u8| [i, i, i];
        let rows: [&[u8]; 6] = [&[0], &[2], &[6, 8], &[1], &[7], &[3, 4, 5]];
        for r in rows {
            raw.push(0);
            for &i in r {
                raw.extend_from_slice(&px(i));
            }
        }
        let im = decode(&file(3, 3, 8, 2, 1, &raw, &[])).unwrap();
        let got: Vec<u8> = im.rgba.chunks(4).map(|c| c[0]).collect();
        assert_eq!(got, (0..9).collect::<Vec<u8>>());
    }

    #[test]
    fn jpeg_baseline_with_halved_chroma() {
        // 64 × 64, 4:2:0, quality 80: red grows to the right, green
        // downward, blue 40 or 160 in 16-pixel squares.
        let im = decode(include_bytes!("../../../golden/programs/label.jpg")).unwrap();
        assert_eq!((im.width, im.height), (64, 64));
        let near = |x: u32, y: u32, want: [i32; 3]| {
            let i = ((y * 64 + x) * 4) as usize;
            let got = [0, 1, 2].map(|k| i32::from(im.rgba[i + k]));
            assert!(
                got.iter().zip(want).all(|(g, w)| (g - w).abs() <= 16),
                "({x}, {y}): {got:?}, not near {want:?}"
            );
            assert_eq!(im.rgba[i + 3], 255);
        };
        near(56, 6, [227, 24, 160]);
        near(6, 56, [24, 227, 160]);
        near(8, 40, [32, 162, 40]);
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        assert!(decode(b"RIFF....WEBP").is_err());
        assert!(decode(&[0xFF, 0xD8, 0xFF, 0xD9]).is_err());
        let huge = file(1 << 14, 1 << 14, 8, 0, 0, &[0], &[]);
        assert!(decode(&huge).unwrap_err().contains("over"));
    }
}

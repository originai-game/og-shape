//! Decodes a texture's PNG, JPEG or KTX2 bytes to RGBA8, for the preview to
//! sample (`docs/research.md` §30, §33).
//!
//! The decoding is the texture budget's ([`crate::texture::decode`],
//! `docs/research.md` §29.1): PNG of every colour type and bit depth,
//! Adam7, a palette's `tRNS` (16-bit samples rounded to 8 bits); baseline,
//! extended sequential and progressive JPEG, one or three components,
//! integer inverse DCT. Colour-space chunks (`gAMA`, `iCCP`, `sRGB`) are ignored: the
//! bytes are taken as sRGB, as glTF says a base-colour image is.
//! Arithmetic-coded and lossless JPEG and WebP are refused with
//! the reason, as is an image over [`MAX_PIXELS`] (read from its header,
//! before anything is decoded).
//!
//! Integer arithmetic only.

use crate::image::ImageInfo;
use crate::preview::Image;

/// The most pixels an image may have: 64 Mi, a 8192² texture.
pub const MAX_PIXELS: u64 = 1 << 26;

/// The image an image file's bytes hold, as RGBA8, rows top to bottom.
pub(crate) fn decode(bytes: &[u8]) -> Result<Image, String> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        && !bytes.starts_with(&[0xFF, 0xD8])
        && !bytes.starts_with(b"\xabKTX 20")
    {
        return Err("not a PNG, JPEG or KTX2 image (the preview decodes those)".into());
    }
    if let (Some(w), Some(h)) = {
        let i = ImageInfo::from_bytes("", bytes);
        (i.width, i.height)
    } {
        too_big(w, h)?;
    }
    let im = crate::texture::decode(bytes).map_err(|e| e.to_string())?;
    too_big(im.width, im.height)?;
    Ok(im)
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
    fn jpeg_progressive_in_ten_scans() {
        // The picture of the baseline file drawn again (red 4x, green 4y,
        // blue 160 or 40 in 16-pixel squares) and saved progressive, 4:2:0:
        // DC first, then bands of AC, then their refinements.
        let bytes = include_bytes!("../../../golden/programs/label_progressive.jpg");
        assert!(bytes.windows(2).any(|w| w == [0xFF, 0xC2]));
        let scans = bytes.windows(2).filter(|w| *w == [0xFF, 0xDA]).count();
        assert!(scans >= 8, "{scans}");
        let im = decode(bytes).unwrap();
        assert_eq!((im.width, im.height), (64, 64));
        // Within the file's own loss of the picture everywhere away from
        // the squares' borders (where 4:2:0 blurs the blue).
        for y in 0..64u32 {
            for x in 0..64u32 {
                if x % 16 < 3 || x % 16 > 12 || y % 16 < 3 || y % 16 > 12 {
                    continue;
                }
                let i = ((y * 64 + x) * 4) as usize;
                let blue = if (x / 16 + y / 16) % 2 == 0 { 160 } else { 40 };
                let want = [(x * 4) as i32, (y * 4) as i32, blue];
                let got = [0, 1, 2].map(|k| i32::from(im.rgba[i + k]));
                assert!(
                    got.iter().zip(want).all(|(g, w)| (g - w).abs() <= 12),
                    "({x}, {y}): {got:?}, not near {want:?}"
                );
            }
        }
    }

    #[test]
    fn refuses_what_it_cannot_read() {
        assert!(decode(b"RIFF....WEBP").is_err());
        assert!(decode(&[0xFF, 0xD8, 0xFF, 0xD9]).is_err());
        let huge = file(1 << 14, 1 << 14, 8, 0, 0, &[0], &[]);
        assert!(decode(&huge).unwrap_err().contains("over"));
    }
}

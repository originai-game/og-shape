//! Reads a KTX2 texture whose blocks are UASTC (`KHR_texture_basisu`'s
//! high-quality mode) to RGBA8: the largest level, as stored, after
//! Zstandard or zlib where the file uses one (`docs/research.md` §50).
//!
//! Entry: [`decode`]. A file of ETC1S blocks (BasisLZ), of another block
//! format, a cube map, an array or a volume is refused with the reason.

use crate::preview::Image;

const MAGIC: [u8; 12] = [
    0xAB, b'K', b'T', b'X', b' ', b'2', b'0', 0xBB, b'\r', b'\n', 0x1A, b'\n',
];
/// The data format descriptor's colour models.
const MODEL_ETC1S: u8 = 163;
const MODEL_UASTC: u8 = 166;

/// What a KTX2 file's header says of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Header {
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    /// 0 none, 1 BasisLZ, 2 Zstandard, 3 zlib.
    pub scheme: u32,
    /// The descriptor's colour model: 163 ETC1S, 166 UASTC.
    pub model: u8,
    /// Whether the transfer function is sRGB.
    pub srgb: bool,
    /// The largest level: where its bytes are, and how many once
    /// decompressed.
    level: (usize, usize, usize),
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?)).ok()
}

/// The header of a KTX2 file of one flat image (with or without mip
/// levels).
pub(crate) fn header(b: &[u8]) -> Result<Header, String> {
    if !b.starts_with(&MAGIC) {
        return Err("not a KTX2 file".into());
    }
    let short = || "the KTX2 header is cut short".to_string();
    let field = |i: usize| u32_at(b, 12 + i * 4).ok_or_else(short);
    let (width, height, depth) = (field(2)?, field(3)?, field(4)?);
    let (layers, faces, levels, scheme) = (field(5)?, field(6)?, field(7)?, field(8)?);
    if width == 0 || height == 0 {
        return Err("a KTX2 image with no pixels".into());
    }
    if depth != 0 || layers != 0 || faces != 1 {
        return Err("a KTX2 volume, array or cube map (one flat image is read)".into());
    }
    let dfd = field(9)? as usize;
    let model = *b.get(dfd + 12).ok_or_else(short)?;
    let srgb = *b.get(dfd + 14).ok_or_else(short)? == 2;
    let level = (
        u64_at(b, 80).ok_or_else(short)?,
        u64_at(b, 88).ok_or_else(short)?,
        u64_at(b, 96).ok_or_else(short)?,
    );
    Ok(Header {
        width,
        height,
        levels: levels.max(1),
        scheme,
        model,
        srgb,
        level,
    })
}

/// The file's largest level as RGBA8, rows top to bottom.
pub(crate) fn decode(b: &[u8]) -> Result<Image, String> {
    let h = header(b)?;
    if h.model == MODEL_ETC1S || h.scheme == 1 {
        return Err("a KTX2 file of ETC1S blocks (BasisLZ); UASTC ones are decoded".into());
    }
    if h.model != MODEL_UASTC || u32_at(b, 12) != Some(0) {
        return Err(format!(
            "a KTX2 file of colour model {} (UASTC, 166, is decoded)",
            h.model
        ));
    }
    let (at, stored, full) = h.level;
    let data = at
        .checked_add(stored)
        .and_then(|end| b.get(at..end))
        .ok_or("the KTX2 level lies outside the file")?;
    let (w, hgt) = (h.width as usize, h.height as usize);
    let need = w.div_ceil(4) * hgt.div_ceil(4) * 16;
    if full != need {
        return Err(format!(
            "a {w}×{hgt} UASTC level is {need} bytes, and the file says {full}"
        ));
    }
    let blocks: std::borrow::Cow<[u8]> = match h.scheme {
        0 => data.into(),
        2 => {
            use std::io::Read;
            let mut out = Vec::new();
            let mut d = ruzstd::decoding::StreamingDecoder::new(data)
                .map_err(|e| format!("the KTX2 level's Zstandard data: {e}"))?;
            // No more than the level holds: a frame that says otherwise
            // is not read to its end.
            (&mut d)
                .take(need as u64 + 1)
                .read_to_end(&mut out)
                .map_err(|e| format!("the KTX2 level's Zstandard data: {e}"))?;
            out.into()
        }
        3 => miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(data, need)
            .map_err(|e| format!("the KTX2 level's zlib data: {e}"))?
            .into(),
        s => return Err(format!("KTX2 supercompression scheme {s}")),
    };
    if blocks.len() != need {
        return Err(format!(
            "the KTX2 level decompressed to {} bytes of {need}",
            blocks.len()
        ));
    }
    let rgba = crate::uastc::decode(&blocks, w, hgt)?;
    Ok(Image {
        width: h.width,
        height: h.height,
        rgba,
    })
}

/// A KTX2 file of UASTC blocks: `levels` from the largest down (each
/// its width, height and RGBA texels), compressed with Zstandard.
/// `srgb` says the texels are colour; `alpha` that they are not all
/// opaque.
pub(crate) fn encode(levels: &[Image], srgb: bool, alpha: bool) -> Vec<u8> {
    let Some(first) = levels.first() else {
        return Vec::new();
    };
    let data: Vec<(Vec<u8>, usize)> = levels
        .iter()
        .map(|l| {
            let blocks = crate::uastc::encode(&l.rgba, l.width as usize, l.height as usize);
            let full = blocks.len();
            let packed = ruzstd::encoding::compress_to_vec(
                blocks.as_slice(),
                ruzstd::encoding::CompressionLevel::Fastest,
            );
            (packed, full)
        })
        .collect();
    // The data format descriptor: one block of 16 bytes, one sample.
    let mut dfd = Vec::with_capacity(44);
    dfd.extend_from_slice(&44u32.to_le_bytes());
    dfd.extend_from_slice(&0u32.to_le_bytes());
    dfd.extend_from_slice(&2u16.to_le_bytes());
    dfd.extend_from_slice(&40u16.to_le_bytes());
    dfd.extend_from_slice(&[MODEL_UASTC, 1, if srgb { 2 } else { 1 }, 0]);
    dfd.extend_from_slice(&[3, 3, 0, 0]);
    dfd.extend_from_slice(&[0; 8]);
    dfd.extend_from_slice(&0u16.to_le_bytes());
    dfd.extend_from_slice(&[127, if alpha { 3 } else { 0 }]);
    dfd.extend_from_slice(&[0; 8]);
    dfd.extend_from_slice(&u32::MAX.to_le_bytes());
    let mut kvd = Vec::new();
    for (key, value) in [("KTXorientation", "rd"), ("KTXwriter", "og-shape")] {
        let length = key.len() + 1 + value.len() + 1;
        kvd.extend_from_slice(&(length as u32).to_le_bytes());
        kvd.extend_from_slice(key.as_bytes());
        kvd.push(0);
        kvd.extend_from_slice(value.as_bytes());
        kvd.push(0);
        kvd.resize(kvd.len().next_multiple_of(4), 0);
    }
    let dfd_at = 80 + 24 * levels.len();
    let kvd_at = dfd_at + dfd.len();
    // The levels' bytes, the smallest first.
    let mut at = kvd_at + kvd.len();
    let mut places = vec![(0usize, 0usize, 0usize); levels.len()];
    for (i, (packed, full)) in data.iter().enumerate().rev() {
        places[i] = (at, packed.len(), *full);
        at += packed.len();
    }
    let mut out = Vec::with_capacity(at);
    out.extend_from_slice(&MAGIC);
    for v in [
        0,
        1,
        first.width,
        first.height,
        0,
        0,
        1,
        levels.len() as u32,
        2,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in [dfd_at, dfd.len(), kvd_at, kvd.len()] {
        out.extend_from_slice(&(v as u32).to_le_bytes());
    }
    out.extend_from_slice(&[0; 16]);
    for (at, stored, full) in &places {
        for v in [at, stored, full] {
            out.extend_from_slice(&(*v as u64).to_le_bytes());
        }
    }
    out.extend_from_slice(&dfd);
    out.extend_from_slice(&kvd);
    for (packed, _) in data.iter().rev() {
        out.extend_from_slice(packed);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::image::Texture;

    /// A 37×22 image with a seam, a ramp and alpha that varies.
    fn image() -> Image {
        let (w, h) = (37u32, 22u32);
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let v = (x * 6) as u8;
                rgba.extend_from_slice(&if x < 20 {
                    [v, 200 - v, (y * 10) as u8, 255]
                } else {
                    [30, v, 255 - v, (y * 11) as u8]
                });
            }
        }
        Image {
            width: w,
            height: h,
            rgba,
        }
    }

    /// A texture written as KTX2 reads back as the file says it is: its
    /// size, its levels down to one texel, colour and alpha, and texels
    /// within what UASTC keeps of them.
    #[test]
    fn a_written_file_reads_back() {
        let im = image();
        let t = Texture::from_bytes("map.png", crate::png(&im));
        let k = crate::texture::to_ktx2(&t, true).unwrap().unwrap();
        assert_eq!(
            (k.name.as_str(), k.mime.as_str()),
            ("map.ktx2", "image/ktx2")
        );
        let h = header(&k.bytes).unwrap();
        assert_eq!((h.width, h.height, h.levels, h.scheme), (37, 22, 6, 2));
        assert!(h.srgb && h.model == MODEL_UASTC);
        // The sample's channels: RGBA, for the alpha.
        assert_eq!(k.bytes[80 + 24 * 6 + 4 + 24 + 3], 3);
        let back = decode(&k.bytes).unwrap();
        assert_eq!((back.width, back.height), (37, 22));
        let worst = back
            .rgba
            .iter()
            .zip(&im.rgba)
            .map(|(a, b)| a.abs_diff(*b))
            .max();
        assert!(worst <= Some(6), "{worst:?}");
        // Already KTX2: kept as it is.
        assert_eq!(crate::texture::to_ktx2(&k, true), Ok(None));
        // Data, not colour, and opaque: linear, RGB.
        let mut opaque = im.clone();
        opaque
            .rgba
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|p| p[3] = 255);
        let d = crate::texture::to_ktx2(&Texture::from_bytes("n.png", crate::png(&opaque)), false);
        let d = d.unwrap().unwrap();
        assert!(!header(&d.bytes).unwrap().srgb);
        assert_eq!(d.bytes[80 + 24 * 6 + 4 + 24 + 3], 0);
    }

    /// Files this does not read say why, and a file cut anywhere is an
    /// error, never a panic.
    #[test]
    fn what_is_not_read_says_why() {
        let t = Texture::from_bytes("map.png", crate::png(&image()));
        let k = crate::texture::to_ktx2(&t, true).unwrap().unwrap().bytes;
        for cut in 0..k.len() {
            assert!(decode(&k[..cut]).is_err(), "cut at {cut}");
        }
        // ETC1S (BasisLZ), a cube map, a block that is not one.
        let mut etc1s = k.clone();
        etc1s[80 + 24 * 6 + 12] = MODEL_ETC1S;
        assert!(decode(&etc1s).unwrap_err().contains("ETC1S"));
        let mut cube = k.clone();
        cube[12 + 6 * 4] = 6;
        assert!(decode(&cube).unwrap_err().contains("cube"));
        let mut plain = k.clone();
        plain[12 + 8 * 4] = 0;
        assert!(decode(&plain).is_err());
    }
}

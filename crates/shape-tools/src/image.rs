//! Texture images found by import: their size read from the file header,
//! never decoded, for the measure of texture memory.

/// One image a model references.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageInfo {
    /// The image's name, or its URI, or `image<i>`.
    pub name: String,
    /// The container format: `png`, `jpeg`, `webp`, `ktx2`, or `unknown`.
    pub format: String,
    /// Pixels across, if the header could be read.
    pub width: Option<u32>,
    /// Pixels down, if the header could be read.
    pub height: Option<u32>,
    /// Bytes in the file (compressed).
    pub file_bytes: u64,
    /// Why the image could not be read, if it could not.
    pub error: Option<String>,
}

impl ImageInfo {
    /// Reads the size from an image file's header.
    pub fn from_bytes(name: &str, bytes: &[u8]) -> ImageInfo {
        let (format, size) = header(bytes);
        ImageInfo {
            name: name.to_string(),
            format: format.to_string(),
            width: size.map(|s| s.0),
            height: size.map(|s| s.1),
            file_bytes: bytes.len() as u64,
            error: if size.is_none() {
                Some(format!("cannot read the size of a {format} image"))
            } else {
                None
            },
        }
    }

    /// An image whose bytes could not be had.
    pub fn missing(name: &str, why: &str) -> ImageInfo {
        ImageInfo {
            name: name.to_string(),
            format: "unknown".into(),
            error: Some(why.to_string()),
            ..Default::default()
        }
    }

    /// Bytes once decoded to RGBA8, without and with a full mip chain
    /// (each level half the last, rounded down, to 1×1).
    pub fn decoded_bytes(&self) -> Option<(u64, u64)> {
        let (mut w, mut h) = (u64::from(self.width?), u64::from(self.height?));
        let base = w * h * 4;
        let mut all = base;
        while w > 1 || h > 1 {
            w = (w / 2).max(1);
            h = (h / 2).max(1);
            all += w * h * 4;
        }
        Some((base, all))
    }
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn be16(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes(
        b.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn le16(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes(
        b.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn header(b: &[u8]) -> (&'static str, Option<(u32, u32)>) {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        // IHDR is the first chunk.
        let size = (|| Some((be32(b, 16)?, be32(b, 20)?)))();
        return ("png", size);
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        return ("jpeg", jpeg(b));
    }
    if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        return ("webp", webp(b));
    }
    if b.starts_with(b"\xABKTX 20\xBB\r\n\x1A\n") {
        let size = (|| Some((le32(b, 20)?, le32(b, 24)?.max(1))))();
        return ("ktx2", size);
    }
    ("unknown", None)
}

/// The first start-of-frame marker's size.
fn jpeg(b: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    loop {
        while *b.get(i)? != 0xFF {
            i += 1;
        }
        while *b.get(i)? == 0xFF {
            i += 1;
        }
        let marker = *b.get(i)?;
        i += 1;
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            continue;
        }
        let len = be16(b, i)? as usize;
        let sof = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if sof {
            let (h, w) = (be16(b, i + 3)?, be16(b, i + 5)?);
            return Some((w, h));
        }
        if marker == 0xD9 || len < 2 {
            return None;
        }
        i += len;
    }
}

fn webp(b: &[u8]) -> Option<(u32, u32)> {
    match b.get(12..16)? {
        b"VP8 " => Some((le16(b, 26)? & 0x3FFF, le16(b, 28)? & 0x3FFF)),
        b"VP8L" => {
            let v = le32(b, 21)?;
            Some(((v & 0x3FFF) + 1, ((v >> 14) & 0x3FFF) + 1))
        }
        b"VP8X" => {
            let w = le32(b, 24)? & 0xFF_FFFF;
            let h = le32(b, 27)? & 0xFF_FFFF;
            Some((w + 1, h + 1))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_sizes_and_counts_the_mip_chain() {
        let png = crate::png(&crate::Image {
            width: 6,
            height: 3,
            rgba: vec![0; 6 * 3 * 4],
        });
        let info = ImageInfo::from_bytes("a", &png);
        assert_eq!(
            (info.format.as_str(), info.width, info.height),
            ("png", Some(6), Some(3))
        );
        // 6×3, 3×1, 1×1.
        assert_eq!(info.decoded_bytes(), Some((72, 72 + 12 + 4)));
        // A baseline JPEG header: SOI, an APP0 segment, SOF0 with 640×480.
        let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0];
        jpg.extend([
            0xFF, 0xC0, 0, 11, 8, 0x01, 0xE0, 0x02, 0x80, 3, 0, 0, 0, 0, 0,
        ]);
        let info = ImageInfo::from_bytes("b", &jpg);
        assert_eq!((info.width, info.height), (Some(640), Some(480)));
        assert!(ImageInfo::from_bytes("c", b"nope").error.is_some());
    }
}

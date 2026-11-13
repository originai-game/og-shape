//! A PNG encoder for an RGBA [`Image`]: 8 bits per channel, every row
//! filtered `Up`, zlib by `miniz_oxide` at a fixed level. The bytes depend
//! only on the image (the gate hashes them on every target).

use crate::preview::Image;

const CRC_TABLE: [u32; 256] = crc_table();

const fn crc_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[n] = c;
        n += 1;
    }
    table
}

pub(crate) fn crc32(chunks: &[&[u8]]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for bytes in chunks {
        for &b in *bytes {
            c = CRC_TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
        }
    }
    c ^ 0xFFFF_FFFF
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[kind, data]).to_be_bytes());
}

/// The image as PNG bytes (RGBA, 8 bits per channel).
pub fn png(image: &Image) -> Vec<u8> {
    let (w, h) = (image.width as usize, image.height as usize);
    let stride = w * 4;
    let mut raw = Vec::with_capacity((stride + 1) * h);
    for y in 0..h {
        // Filter 2 (Up): each byte minus the one above it.
        raw.push(2);
        let row = &image.rgba[y * stride..(y + 1) * stride];
        if y == 0 {
            raw.extend_from_slice(row);
        } else {
            let above = &image.rgba[(y - 1) * stride..y * stride];
            raw.extend(row.iter().zip(above).map(|(a, b)| a.wrapping_sub(*b)));
        }
    }
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&image.width.to_be_bytes());
    ihdr.extend_from_slice(&image.height.to_be_bytes());
    // 8 bits, colour type 6 (RGBA), deflate, adaptive filtering, no
    // interlace.
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(
        &mut out,
        b"IDAT",
        &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 9),
    );
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_the_png_reference() {
        // The CRC of an empty IEND chunk, as every PNG file ends with it.
        assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082);
    }

    #[test]
    fn round_trips_through_inflate() {
        let image = Image {
            width: 3,
            height: 2,
            rgba: (0..24).map(|i| (i * 37 % 256) as u8).collect(),
        };
        let bytes = png(&image);
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        // IHDR is 8 + 13 + 4 bytes; IDAT's length follows it.
        let at = 8 + 25;
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize;
        assert_eq!(&bytes[at + 4..at + 8], b"IDAT");
        let raw =
            miniz_oxide::inflate::decompress_to_vec_zlib(&bytes[at + 8..at + 8 + len]).unwrap();
        // Undo the Up filter.
        let mut rows: Vec<Vec<u8>> = Vec::new();
        for y in 0..2 {
            let line = &raw[y * 13..(y + 1) * 13];
            assert_eq!(line[0], 2);
            let row: Vec<u8> = line[1..]
                .iter()
                .enumerate()
                .map(|(i, &b)| b.wrapping_add(if y > 0 { rows[y - 1][i] } else { 0 }))
                .collect();
            rows.push(row);
        }
        assert_eq!(rows.concat(), image.rgba);
    }
}

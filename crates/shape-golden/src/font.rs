//! A minimal TrueType font built here, for the `text` golden case: no font
//! ships with the kernel, and this one is test data written by hand (the
//! repository's licence). Units per em 1 000; glyphs:
//!
//! - `O`: a square 600 × 700 with a square hole (the outer contour
//!   clockwise, the inner counter-clockwise, as TrueType draws them);
//! - `D`: a bar and a bowl drawn with quadratic off-curve points;
//! - `I`: a bar 200 × 700.
//!
//! Tables: `cmap` (format 6 for U+0044 to U+004F), `glyf`, `head`,
//! `hhea`, `hmtx`, `loca` (short), `maxp` (version 0.5).

/// A glyph: contours of `(x, y, on_curve)` points, and its advance.
struct Glyph {
    contours: Vec<Vec<(i16, i16, bool)>>,
    advance: u16,
}

fn glyphs() -> Vec<Glyph> {
    let on = |x, y| (x, y, true);
    let off = |x, y| (x, y, false);
    vec![
        // .notdef: empty.
        Glyph {
            contours: Vec::new(),
            advance: 500,
        },
        // O
        Glyph {
            contours: vec![
                vec![on(0, 0), on(0, 700), on(600, 700), on(600, 0)],
                vec![on(150, 150), on(450, 150), on(450, 550), on(150, 550)],
            ],
            advance: 700,
        },
        // D: a straight back and a bowl of two quadratic arcs.
        Glyph {
            contours: vec![vec![
                on(0, 0),
                on(0, 700),
                on(250, 700),
                off(550, 700),
                on(550, 350),
                off(550, 0),
                on(250, 0),
            ]],
            advance: 650,
        },
        // I
        Glyph {
            contours: vec![vec![on(0, 0), on(0, 700), on(200, 700), on(200, 0)]],
            advance: 300,
        },
    ]
}

fn u16b(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn i16b(v: &mut Vec<u8>, x: i16) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn u32b(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_be_bytes());
}

fn glyf(g: &Glyph) -> Vec<u8> {
    let mut v = Vec::new();
    if g.contours.is_empty() {
        return v;
    }
    let pts: Vec<(i16, i16, bool)> = g.contours.iter().flatten().copied().collect();
    i16b(&mut v, g.contours.len() as i16);
    i16b(&mut v, pts.iter().map(|p| p.0).min().unwrap_or(0));
    i16b(&mut v, pts.iter().map(|p| p.1).min().unwrap_or(0));
    i16b(&mut v, pts.iter().map(|p| p.0).max().unwrap_or(0));
    i16b(&mut v, pts.iter().map(|p| p.1).max().unwrap_or(0));
    let mut end = 0u16;
    for c in &g.contours {
        end += c.len() as u16;
        u16b(&mut v, end - 1);
    }
    u16b(&mut v, 0); // no instructions
    for p in &pts {
        v.push(u8::from(p.2)); // on-curve bit; coordinates as i16 deltas
    }
    let (mut x, mut y) = (0i16, 0i16);
    for p in &pts {
        i16b(&mut v, p.0 - x);
        x = p.0;
    }
    for p in &pts {
        i16b(&mut v, p.1 - y);
        y = p.1;
    }
    if v.len() % 2 == 1 {
        v.push(0);
    }
    v
}

/// The font's bytes.
pub fn font() -> Vec<u8> {
    let gs = glyphs();
    let n = gs.len() as u16;
    // glyf and loca.
    let mut glyf_t = Vec::new();
    let mut loca = Vec::new();
    for g in &gs {
        u16b(&mut loca, (glyf_t.len() / 2) as u16);
        glyf_t.extend(glyf(g));
    }
    u16b(&mut loca, (glyf_t.len() / 2) as u16);
    // cmap: format 6 from 'D' (0x44) to 'O' (0x4F).
    let mut sub = Vec::new();
    let ids: Vec<u16> = (0x44u16..=0x4F)
        .map(|c| match c {
            0x4F => 1,
            0x44 => 2,
            0x49 => 3,
            _ => 0,
        })
        .collect();
    u16b(&mut sub, 6);
    u16b(&mut sub, (10 + 2 * ids.len()) as u16);
    u16b(&mut sub, 0);
    u16b(&mut sub, 0x44);
    u16b(&mut sub, ids.len() as u16);
    for id in ids {
        u16b(&mut sub, id);
    }
    let mut cmap = Vec::new();
    u16b(&mut cmap, 0);
    u16b(&mut cmap, 1);
    u16b(&mut cmap, 3); // Windows
    u16b(&mut cmap, 1); // Unicode BMP
    u32b(&mut cmap, 12);
    cmap.extend(sub);
    // head.
    let mut head = Vec::new();
    u32b(&mut head, 0x0001_0000);
    u32b(&mut head, 0x0001_0000);
    u32b(&mut head, 0);
    u32b(&mut head, 0x5F0F_3CF5);
    u16b(&mut head, 0);
    u16b(&mut head, 1000);
    head.extend([0u8; 16]);
    for x in [0i16, 0, 600, 700] {
        i16b(&mut head, x);
    }
    u16b(&mut head, 0);
    u16b(&mut head, 8);
    i16b(&mut head, 2);
    i16b(&mut head, 0); // short loca
    i16b(&mut head, 0);
    // hhea.
    let mut hhea = Vec::new();
    u32b(&mut hhea, 0x0001_0000);
    for x in [800i16, -200, 0] {
        i16b(&mut hhea, x);
    }
    u16b(&mut hhea, 700);
    for x in [0i16, 0, 600, 1, 0, 0, 0, 0, 0, 0, 0] {
        i16b(&mut hhea, x);
    }
    u16b(&mut hhea, n);
    // hmtx.
    let mut hmtx = Vec::new();
    for g in &gs {
        u16b(&mut hmtx, g.advance);
        i16b(&mut hmtx, 0);
    }
    // maxp 0.5.
    let mut maxp = Vec::new();
    u32b(&mut maxp, 0x0000_5000);
    u16b(&mut maxp, n);

    let tables: [(&[u8; 4], Vec<u8>); 7] = [
        (b"cmap", cmap),
        (b"glyf", glyf_t),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"loca", loca),
        (b"maxp", maxp),
    ];
    let mut out = Vec::new();
    u32b(&mut out, 0x0001_0000);
    u16b(&mut out, tables.len() as u16);
    for x in [64u16, 2, 48] {
        u16b(&mut out, x);
    }
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    for (tag, t) in &tables {
        out.extend_from_slice(*tag);
        u32b(&mut out, 0); // checksums are not checked
        u32b(&mut out, offset as u32);
        u32b(&mut out, t.len() as u32);
        body.extend(t);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    out.extend(body);
    out
}

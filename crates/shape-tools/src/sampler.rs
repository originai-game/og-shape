//! A decoded base-colour texture as the preview samples it
//! (`docs/research.md` §30).
//!
//! - **Colour space.** Texels are sRGB bytes. Filtering and every product
//!   with a factor or a vertex colour happen in linear light:
//!   [`LINEAR`] maps a byte to linear light in 0..=65535 (the sRGB curve,
//!   from `trig::pow`, rounded), and [`encode`] maps back to the nearest
//!   byte by an exact search of that table.
//! - **Mip chain.** Level 0 is the image. Level k + 1 is level k halved,
//!   each size rounded down and at least 1; texel (x, y) is the mean of
//!   texels (2x, 2y), (2x + 1, 2y), (2x, 2y + 1), (2x + 1, 2y + 1) of
//!   level k (a coordinate past the last texel is the last), colour in
//!   linear light, alpha as is, rounded half up, back to sRGB bytes. An
//!   odd last row or column of level k is dropped. To 1 × 1.
//! - **Level.** ρ is the pixel's footprint in level-0 texels: the longer
//!   of the UV steps one pixel right and one pixel down, each scaled by
//!   the image's width and height. The level is the nearest to log₂ ρ:
//!   the largest k with 4^k ≤ 2ρ², and at least 0, at most the last.
//! - **Sampling.** Bilinear within that one level, with repeat wrap (UVs
//!   are taken modulo 1): the texel grid's centres at (i + ½) / width,
//!   v = 0 at the image's bottom row (as [`shape_mesh::Solid::uvs`] say).
//!   Weights are 8-bit fixed point, `floor(256 × fraction)`; the sum is
//!   integer.
//!
//! Integer arithmetic except the position in the grid (f64 `*`, `-`,
//! `floor`), so the result is the same on every host.

use shape_core::{DVec2, trig};

use crate::preview::Image;

/// An sRGB byte as linear light in 0..=65535.
pub(crate) struct Linear([u16; 256]);

/// The table, from the sRGB curve.
pub(crate) fn linear_table() -> Linear {
    let mut t = [0u16; 256];
    for (b, v) in t.iter_mut().enumerate() {
        *v = (to_linear(b as f64 / 255.0) * 65535.0 + 0.5).floor() as u16;
    }
    Linear(t)
}

/// An sRGB component in 0..=1 to linear light.
pub(crate) fn to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        trig::pow((c + 0.055) / 1.055, 2.4)
    }
}

/// Linear light in 0..=1 to an sRGB component.
pub(crate) fn to_srgb(l: f64) -> f64 {
    if l <= 0.003_130_8 {
        l * 12.92
    } else {
        1.055 * trig::pow(l, 1.0 / 2.4) - 0.055
    }
}

impl Linear {
    fn get(&self, b: u8) -> u32 {
        u32::from(self.0[usize::from(b)])
    }

    /// The byte whose linear value is nearest `l`; ties to the lower.
    fn encode(&self, l: u32) -> u8 {
        // The last byte whose value is at most l.
        let k = self.0.partition_point(|&v| u32::from(v) <= l);
        if k == 0 {
            return 0;
        }
        let lo = k - 1;
        if lo == 255 {
            return 255;
        }
        let (a, b) = (u32::from(self.0[lo]), u32::from(self.0[lo + 1]));
        if l - a <= b - l {
            lo as u8
        } else {
            (lo + 1) as u8
        }
    }
}

/// A texture's mip chain.
pub(crate) struct Mips {
    levels: Vec<Image>,
    lin: Linear,
}

impl Mips {
    /// The chain of an image (§ the module's rule).
    pub(crate) fn new(image: Image) -> Mips {
        let lin = linear_table();
        let mut levels = vec![image];
        loop {
            let last = &levels[levels.len() - 1];
            let (w, h) = (last.width, last.height);
            if w <= 1 && h <= 1 {
                break;
            }
            let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
            let mut rgba = Vec::with_capacity((nw * nh * 4) as usize);
            let at = |x: u32, y: u32| (((y.min(h - 1)) * w + x.min(w - 1)) * 4) as usize;
            for y in 0..nh {
                for x in 0..nw {
                    let q = [
                        at(2 * x, 2 * y),
                        at(2 * x + 1, 2 * y),
                        at(2 * x, 2 * y + 1),
                        at(2 * x + 1, 2 * y + 1),
                    ];
                    for c in 0..3 {
                        let s: u32 = q.iter().map(|&i| lin.get(last.rgba[i + c])).sum();
                        rgba.push(lin.encode((s + 2) / 4));
                    }
                    let a: u32 = q.iter().map(|&i| u32::from(last.rgba[i + 3])).sum();
                    rgba.push(((a + 2) / 4) as u8);
                }
            }
            levels.push(Image {
                width: nw,
                height: nh,
                rgba,
            });
        }
        Mips { levels, lin }
    }

    /// Level 0's size.
    pub(crate) fn size(&self) -> (f64, f64) {
        let l = &self.levels[0];
        (f64::from(l.width), f64::from(l.height))
    }

    /// The level for a footprint of `rho2` = ρ² level-0 texels.
    pub(crate) fn level(&self, rho2: f64) -> usize {
        let mut k = 0;
        let mut four = 4.0;
        while k + 1 < self.levels.len() && four <= 2.0 * rho2 {
            k += 1;
            four *= 4.0;
        }
        k
    }

    /// Linear RGBA in 0..=1 at `uv` on level `k`.
    pub(crate) fn sample(&self, uv: DVec2, k: usize) -> [f64; 4] {
        let l = &self.levels[k];
        let (w, h) = (i64::from(l.width), i64::from(l.height));
        // Finite and far from i64's ends; a guard, not a clamp of real UVs.
        let fin = |v: f64| {
            if v.is_finite() {
                v.clamp(-1e9, 1e9)
            } else {
                0.0
            }
        };
        let x = fin(uv.x) * w as f64 - 0.5;
        let y = (1.0 - fin(uv.y)) * h as f64 - 0.5;
        let (fx0, fy0) = (x.floor(), y.floor());
        let wx = ((x - fx0) * 256.0).floor() as u64;
        let wy = ((y - fy0) * 256.0).floor() as u64;
        let (x0, y0) = (fx0 as i64, fy0 as i64);
        let tex = |xx: i64, yy: i64| -> usize {
            let (xx, yy) = (xx.rem_euclid(w), yy.rem_euclid(h));
            ((yy * w + xx) * 4) as usize
        };
        let q = [
            (tex(x0, y0), (256 - wx) * (256 - wy)),
            (tex(x0 + 1, y0), wx * (256 - wy)),
            (tex(x0, y0 + 1), (256 - wx) * wy),
            (tex(x0 + 1, y0 + 1), wx * wy),
        ];
        let mut out = [0.0; 4];
        for (c, o) in out.iter_mut().enumerate() {
            let s: u64 = q
                .iter()
                .map(|&(i, wt)| {
                    let b = l.rgba[i + c];
                    let v = if c < 3 {
                        u64::from(self.lin.get(b))
                    } else {
                        u64::from(b) * 257
                    };
                    v * wt
                })
                .sum();
            // Weights sum to 65536.
            *o = (s / 65536) as f64 / 65535.0;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_round_trips_every_byte() {
        let t = linear_table();
        for b in 0..=255u8 {
            assert_eq!(t.encode(t.get(b)), b);
        }
        // Linear middle grey is sRGB 188, not 128.
        assert_eq!(t.encode(32768), 188);
    }

    #[test]
    fn a_fine_checker_averages_in_linear_light() {
        // 4 × 4 black and white checker: level 1 is mid grey in linear
        // light (sRGB 188), level 2 the same; 4 × 4, 2 × 2, 1 × 1.
        let rgba = (0..16)
            .flat_map(|i| {
                let v = if (i % 4 + i / 4) % 2 == 0 { 0 } else { 255 };
                [v, v, v, 255]
            })
            .collect();
        let m = Mips::new(Image {
            width: 4,
            height: 4,
            rgba,
        });
        assert_eq!(m.levels.len(), 3);
        assert_eq!(&m.levels[1].rgba[0..4], &[188, 188, 188, 255]);
        assert_eq!(&m.levels[2].rgba[..], &[188, 188, 188, 255]);
        // Footprints: 1 texel → 0, 2 → 1, 1.5 → 1 (log₂ 1.5 = 0.58), 100 → last.
        assert_eq!([1.0, 4.0, 2.25, 1e4].map(|r2| m.level(r2)), [0, 1, 1, 2]);
        // At level 0, a texel's centre gives the texel; v = 0 is the
        // bottom row; u wraps.
        let at = |u: f64, v: f64| m.sample(DVec2::new(u, v), 0)[0];
        assert_eq!(at(0.125, 0.875), 0.0); // texel (0, 0): black
        assert_eq!(at(0.375, 0.875), 1.0); // texel (1, 0): white
        assert_eq!(at(1.125, 0.875), at(0.125, 0.875));
        assert_eq!(at(0.125, 0.625), 1.0); // texel (0, 1): white
    }
}

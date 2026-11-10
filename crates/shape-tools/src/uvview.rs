//! The UV layout of a scene's parts, one image per material
//! (`docs/research.md` §30): the chart outlines over the material's map,
//! or over a checker when it has none. For an agent to look at, and the
//! layout a texturing step paints into.
//!
//! - **Which parts.** Parts with UVs, grouped by material: parts whose
//!   [`Material`]s are equal share an image, in the order of their first
//!   part. Parts with no UVs are left out.
//! - **Pixel to UV.** The image covers the whole UV tiles its UVs touch:
//!   a square from `uv_min` = (⌊min u⌋, ⌊min v⌋), `tiles` units each way
//!   (at least 1; UVs in 0..=1 give the unit square). Pixel (x, y), rows
//!   top to bottom, covers u in `uv_min.u + tiles × [x, x + 1] / size`
//!   and v in `uv_min.v + tiles × [size − y − 1, size − y] / size`: v up,
//!   as [`shape_mesh::Solid::uvs`] are.
//! - **Under the charts.** The map sampled at each pixel's centre as the
//!   preview samples it (repeat wrap, so it repeats past one tile), or a
//!   checker of 8 × 8 squares a tile. Texels no triangle covers are
//!   dimmed to 40 %.
//! - **Lines.** Each chart's outline, one pixel wide and dark: every
//!   triangle edge no other triangle of the part shares with the same two
//!   UVs (a seam or an open edge). Edges inside a chart are not drawn: on
//!   a generated atlas of tens of thousands of triangles they hide the
//!   map.
//! - A tiling map's UVs (a program's `mapSize`) can span many tiles; the
//!   image shows all of them at one scale.
//!
//! Integer coverage and lines on the preview's fixed-point grid; the
//! bytes are the same on every host.

use std::cmp::Ordering;

use shape_core::DVec2;

use crate::export::{ExportError, Material, Scene};
use crate::preview::{Image, Maps};

/// What [`uv_view`] draws. The zero value is 512 × 512 images.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UvView {
    /// Pixels across and down; 0 is 512.
    pub size: u32,
}

/// One material's UV layout.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UvImage {
    /// The material's name, or its first part's when it has none.
    pub material: String,
    /// The parts drawn, in scene order.
    pub parts: Vec<String>,
    /// The layout.
    pub image: Image,
    /// The UV at the image's bottom-left corner: whole numbers.
    pub uv_min: [f64; 2],
    /// UV units the image spans each way: a whole number, at least 1.
    pub tiles: u32,
    /// Whether the map is under the charts (else the checker).
    pub textured: bool,
    /// Why the material's map is not under the charts, if it has one
    /// that could not be decoded.
    pub map_error: Option<String>,
}

const SUB: i64 = 256;
const LIGHT: [u8; 3] = [214, 214, 210];
const DARK: [u8; 3] = [150, 150, 146];
const OUTLINE: [u8; 3] = [16, 16, 16];
/// The most tiles an image spans: past this the charts are specks.
const MAX_TILES: f64 = 4096.0;

/// The UV layout of each material's parts.
///
/// Errors as [`crate::glb`] does on an invalid scene.
pub fn uv_view(scene: &Scene, d: &UvView) -> Result<Vec<UvImage>, ExportError> {
    crate::export::check(scene)?;
    let size = if d.size == 0 { 512 } else { d.size };
    // Material groups: (material, part indices).
    let mut groups: Vec<(&Material, Vec<usize>)> = Vec::new();
    for (i, p) in scene.parts.iter().enumerate() {
        let Some(uvs) = &p.solid.uvs else { continue };
        if uvs.len() != p.solid.triangles.len() {
            continue;
        }
        match groups.iter_mut().find(|(m, _)| **m == p.material) {
            Some((_, list)) => list.push(i),
            None => groups.push((&p.material, vec![i])),
        }
    }
    let mut maps = Maps::new();
    let mut out = Vec::with_capacity(groups.len());
    for (material, list) in groups {
        let uvs: Vec<&Vec<[DVec2; 3]>> = list
            .iter()
            .filter_map(|&i| scene.parts[i].solid.uvs.as_ref())
            .collect();
        let finite = |q: &DVec2| q.x.is_finite() && q.y.is_finite();
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for q in uvs
            .iter()
            .flat_map(|u| u.iter().flatten())
            .filter(|q| finite(q))
        {
            lo = lo.min(*q);
            hi = hi.max(*q);
        }
        if lo.x > hi.x {
            lo = DVec2::ZERO;
            hi = DVec2::ONE;
        }
        let min = DVec2::new(lo.x.max(-MAX_TILES).floor(), lo.y.max(-MAX_TILES).floor());
        let span = (hi.x.min(MAX_TILES).ceil() - min.x)
            .max(hi.y.min(MAX_TILES).ceil() - min.y)
            .clamp(1.0, MAX_TILES);
        let tiles = span as u32;
        let n = f64::from(tiles);
        let s = f64::from(size);
        // Under the charts.
        let (mips, map_error) = match &material.map {
            None => (None, None),
            Some(t) => match maps.get(t) {
                Ok(k) => (maps.mips(k), None),
                Err(e) => (None, Some(e)),
            },
        };
        let px = (size * size) as usize;
        let mut rgb = vec![[0u8; 3]; px];
        match mips {
            Some(m) => {
                let (tw, th) = m.size();
                let step = n / s;
                let level = m.level((step * tw).max(step * th) * (step * tw).max(step * th));
                for y in 0..size {
                    for x in 0..size {
                        let uv = DVec2::new(
                            min.x + n * (f64::from(x) + 0.5) / s,
                            min.y + n * (s - f64::from(y) - 0.5) / s,
                        );
                        let c = m.sample(uv, level);
                        rgb[(y * size + x) as usize] =
                            [0, 1, 2].map(|k| byte(crate::sampler::to_srgb(c[k])));
                    }
                }
            }
            None => {
                for y in 0..size {
                    for x in 0..size {
                        // Checker squares of 1/8 tile.
                        let cx = (u64::from(x) * 8 * u64::from(tiles)) / u64::from(size);
                        let cy = (u64::from(y) * 8 * u64::from(tiles)) / u64::from(size);
                        rgb[(y * size + x) as usize] =
                            if (cx + cy) % 2 == 0 { LIGHT } else { DARK };
                    }
                }
            }
        }
        // Fixed-point pixel position of a UV.
        let to_px = |q: DVec2| -> (i64, i64) {
            let q = if finite(&q) { q } else { min };
            let x = (q.x - min.x) / n * s;
            let y = (1.0 - (q.y - min.y) / n) * s;
            let fix = |v: f64| (v.clamp(-1e12, 1e12) * SUB as f64).round() as i64;
            (fix(x), fix(y))
        };
        // Coverage.
        let mut covered = vec![false; px];
        for u in &uvs {
            for t in u.iter() {
                cover(size, t.map(to_px), &mut covered);
            }
        }
        for (c, &k) in rgb.iter_mut().zip(&covered) {
            if !k {
                *c = c.map(|v| (u32::from(v) * 2 / 5) as u8);
            }
        }
        for (&i, u) in list.iter().zip(&uvs) {
            for (a, b) in outlines(&scene.parts[i].solid.triangles, u) {
                line(size, to_px(a), to_px(b), &mut rgb, OUTLINE);
            }
        }
        let mut rgba = Vec::with_capacity(px * 4);
        for c in &rgb {
            rgba.extend_from_slice(c);
            rgba.push(255);
        }
        let parts: Vec<String> = list.iter().map(|&i| scene.parts[i].name.clone()).collect();
        out.push(UvImage {
            material: if material.name.is_empty() {
                parts[0].clone()
            } else {
                material.name.clone()
            },
            parts,
            image: Image {
                width: size,
                height: size,
                rgba,
            },
            uv_min: [min.x, min.y],
            tiles,
            textured: mips.is_some(),
            map_error,
        });
    }
    Ok(out)
}

fn byte(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8
}

fn cmp_uv(a: DVec2, b: DVec2) -> Ordering {
    a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y))
}

/// A part's chart outlines in UV: the triangle edges no other triangle
/// shares with the same UVs at both ends. Sorted, so the drawing order is
/// fixed.
fn outlines(tris: &[[u32; 3]], uvs: &[[DVec2; 3]]) -> Vec<(DVec2, DVec2)> {
    // (lower vertex, higher vertex, its UV, the other's UV)
    let mut all: Vec<(u32, u32, DVec2, DVec2)> = Vec::with_capacity(tris.len() * 3);
    for (t, uv) in tris.iter().zip(uvs) {
        for k in 0..3 {
            let (a, b) = (k, (k + 1) % 3);
            let (va, vb) = (t[a], t[b]);
            if va == vb {
                continue;
            }
            all.push(if va < vb {
                (va, vb, uv[a], uv[b])
            } else {
                (vb, va, uv[b], uv[a])
            });
        }
    }
    let key = |x: &(u32, u32, DVec2, DVec2), y: &(u32, u32, DVec2, DVec2)| {
        x.0.cmp(&y.0)
            .then(x.1.cmp(&y.1))
            .then(cmp_uv(x.2, y.2))
            .then(cmp_uv(x.3, y.3))
    };
    all.sort_by(key);
    let mut outline = Vec::new();
    let mut i = 0;
    while i < all.len() {
        let mut j = i + 1;
        while j < all.len() && key(&all[i], &all[j]) == Ordering::Equal {
            j += 1;
        }
        if j - i == 1 {
            outline.push((all[i].2, all[i].3));
        }
        i = j;
    }
    outline
}

/// Marks the pixels whose centre a triangle covers (both windings).
fn cover(size: u32, c: [(i64, i64); 3], covered: &mut [bool]) {
    let e = |a: (i64, i64), b: (i64, i64), p: (i64, i64)| -> i128 {
        i128::from(b.0 - a.0) * i128::from(p.1 - a.1)
            - i128::from(b.1 - a.1) * i128::from(p.0 - a.0)
    };
    let area = e(c[0], c[1], c[2]);
    if area == 0 {
        return;
    }
    let s = i64::from(size);
    let lo_x = c
        .iter()
        .map(|p| p.0)
        .min()
        .unwrap_or(0)
        .div_euclid(SUB)
        .max(0);
    let hi_x = c
        .iter()
        .map(|p| p.0)
        .max()
        .unwrap_or(0)
        .div_euclid(SUB)
        .min(s - 1);
    let lo_y = c
        .iter()
        .map(|p| p.1)
        .min()
        .unwrap_or(0)
        .div_euclid(SUB)
        .max(0);
    let hi_y = c
        .iter()
        .map(|p| p.1)
        .max()
        .unwrap_or(0)
        .div_euclid(SUB)
        .min(s - 1);
    for y in lo_y..=hi_y {
        for x in lo_x..=hi_x {
            let p = (x * SUB + SUB / 2, y * SUB + SUB / 2);
            let w = [e(c[1], c[2], p), e(c[2], c[0], p), e(c[0], c[1], p)];
            let inside = if area > 0 {
                w.iter().all(|&v| v >= 0)
            } else {
                w.iter().all(|&v| v <= 0)
            };
            if inside {
                covered[(y * s + x) as usize] = true;
            }
        }
    }
}

/// A one-pixel line in fixed point, in `colour`.
fn line(size: u32, a: (i64, i64), b: (i64, i64), rgb: &mut [[u8; 3]], colour: [u8; 3]) {
    let s = i64::from(size);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let steps = (dx.abs().max(dy.abs()) / SUB).max(1);
    // A line far outside the image is not walked pixel by pixel.
    if steps > 64 * s {
        return;
    }
    for i in 0..=steps {
        let x = (a.0 + dx * i / steps).div_euclid(SUB);
        let y = (a.1 + dy * i / steps).div_euclid(SUB);
        if x < 0 || y < 0 || x >= s || y >= s {
            continue;
        }
        let k = (y * s + x) as usize;
        rgb[k] = colour;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::Part;
    use shape_core::DVec3;
    use shape_mesh::Solid;

    #[test]
    fn a_seam_is_an_outline_and_a_shared_edge_is_not() {
        // A square of two triangles in one chart: its diagonal is inside,
        // until the second triangle's UVs move off it (a seam).
        let mut s = Solid::default();
        for (x, y) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)] {
            s.push_vertex(DVec3::new(x, y, 0.0));
        }
        let uv = |x: f64, y: f64| DVec2::new(x, y);
        s.push_triangle_uv(
            [0, 1, 2],
            0,
            &[],
            [uv(0.1, 0.1), uv(0.4, 0.1), uv(0.4, 0.4)],
        );
        s.push_triangle_uv(
            [0, 2, 3],
            0,
            &[],
            [uv(0.1, 0.1), uv(0.4, 0.4), uv(0.1, 0.4)],
        );
        let outline = outlines(&s.triangles, s.uvs.as_ref().unwrap());
        assert_eq!(outline.len(), 4);
        assert!(!outline.contains(&(uv(0.1, 0.1), uv(0.4, 0.4))));
        let mut seam = s.clone();
        seam.uvs.as_mut().unwrap()[1] = [uv(0.6, 0.1), uv(0.9, 0.4), uv(0.6, 0.4)];
        assert_eq!(
            outlines(&seam.triangles, seam.uvs.as_ref().unwrap()).len(),
            6
        );
        // UVs in 0..1: the unit square; out to 2.3: three tiles.
        let scene = Scene {
            name: "t".into(),
            parts: vec![Part::new("p", &s)],
        };
        let v = uv_view(&scene, &UvView { size: 64 }).unwrap();
        assert_eq!((v.len(), v[0].tiles, v[0].uv_min), (1, 1, [0.0, 0.0]));
        assert!(!v[0].textured);
        // The pixel at UV (0.25, 0.25) is covered (not dimmed), at (0.8,
        // 0.8) not.
        let at = |u: f64, w: f64| {
            let (x, y) = ((u * 64.0) as usize, ((1.0 - w) * 64.0) as usize);
            v[0].image.rgba[(y * 64 + x) * 4]
        };
        assert!(at(0.27, 0.23) >= DARK[0]);
        assert!(at(0.8, 0.8) < DARK[0]);
        s.uvs.as_mut().unwrap()[0][2] = uv(2.3, 0.4);
        let scene = Scene {
            name: "t".into(),
            parts: vec![Part::new("p", &s)],
        };
        assert_eq!(uv_view(&scene, &UvView { size: 64 }).unwrap()[0].tiles, 3);
    }
}

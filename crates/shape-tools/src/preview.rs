//! A deterministic CPU turntable preview.
//!
//! - Orthographic views from `views` azimuths around the scene's vertical
//!   (kernel +Z) axis, all at one scale, so a part keeps its size from view
//!   to view.
//! - Triangles are snapped to a fixed-point grid (1/256 pixel) and covered
//!   by integer edge functions with a top-left rule: a pixel centre on an
//!   edge two triangles share belongs to exactly one of them.
//! - Lambert shading from one world light, smooth within a surface (the
//!   GLB's normals), interpolated in f64 from the integer barycentrics.
//! - Real edges are drawn as one-pixel lines, hidden behind nearer faces.
//! - A ground square under the scene, with the scene's shadow cast on it
//!   along the light.
//!
//! No GPU, threads or SIMD; every step is integer arithmetic or f64
//! `+ - * /` and `sqrt` in a fixed order, so the framebuffer's bytes are
//! the same on every host.

use shape_core::{DVec3, trig};

use crate::export::{ExportError, Scene};
use crate::surface::{face_normal, split};

/// An RGBA image, 8 bits per channel, rows top to bottom.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 4` bytes.
    pub rgba: Vec<u8>,
}

/// What [`preview`] draws. The zero value is four 256 × 256 views.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Preview {
    /// Pixels across; 0 is 256.
    pub width: u32,
    /// Pixels down; 0 is the width.
    pub height: u32,
    /// Views evenly around the turntable; 0 is 4.
    pub views: u32,
    /// Camera height above the horizon, radians; `None` is 25°. Must lie
    /// in (0, π/2]: the ground is seen from above.
    pub elevation: Option<f64>,
    /// Azimuth of the first view, radians from the side the kernel's −Y
    /// looks at (the front), turning counter-clockwise seen from above.
    pub azimuth: f64,
    /// Leave the real edges out.
    pub no_edges: bool,
    /// Leave the ground and its shadow out.
    pub no_ground: bool,
}

const SUB: i64 = 256; // fixed-point steps per pixel
const BACKGROUND: [u8; 3] = [246, 246, 244];
const GROUND: [u8; 3] = [222, 222, 218];
const SHADOW: [u8; 3] = [178, 178, 174];
/// Shading: ambient plus diffuse, of the base colour.
const AMBIENT: f64 = 0.38;
const DIFFUSE: f64 = 0.62;

struct Camera {
    centre: DVec3,
    right: DVec3,
    up: DVec3,
    /// Into the screen: depth grows along it.
    forward: DVec3,
    scale: f64,
    width: u32,
    height: u32,
}

impl Camera {
    /// Screen position in fixed point, and depth.
    fn project(&self, p: DVec3) -> (i64, i64, f64) {
        let d = p - self.centre;
        let sx = f64::from(self.width) * 0.5 + d.dot(self.right) * self.scale;
        let sy = f64::from(self.height) * 0.5 - d.dot(self.up) * self.scale;
        (
            (sx * SUB as f64).round() as i64,
            (sy * SUB as f64).round() as i64,
            d.dot(self.forward),
        )
    }
}

struct Target {
    width: u32,
    height: u32,
    rgb: Vec<[u8; 3]>,
    depth: Vec<f64>,
    ground: Vec<bool>,
}

/// One projected corner: fixed-point screen position, depth, and a value
/// to interpolate (light).
#[derive(Clone, Copy)]
struct Corner {
    x: i64,
    y: i64,
    z: f64,
    light: f64,
}

/// Whether pixel centres exactly on the edge `a → b` belong to the
/// triangle (the top-left rule, for counter-clockwise area > 0 in y-down
/// screen space). Opposite directions disagree, so a shared edge's pixels
/// go to one side only.
fn owns(dx: i64, dy: i64) -> bool {
    dy < 0 || (dy == 0 && dx > 0)
}

fn edge(a: &Corner, b: &Corner, px: i64, py: i64) -> i64 {
    (b.x - a.x) * (py - a.y) - (b.y - a.y) * (px - a.x)
}

/// Covers a triangle; `plot(pixel index, depth, light)` decides what to do.
fn raster(size: (u32, u32), mut c: [Corner; 3], mut plot: impl FnMut(usize, f64, f64)) {
    let mut area = edge(&c[0], &c[1], c[2].x, c[2].y);
    if area == 0 {
        return;
    }
    if area < 0 {
        c.swap(1, 2);
        area = -area;
    }
    let (w, h) = (i64::from(size.0), i64::from(size.1));
    let lo_x = c.iter().map(|v| v.x).min().unwrap_or(0);
    let hi_x = c.iter().map(|v| v.x).max().unwrap_or(0);
    let lo_y = c.iter().map(|v| v.y).min().unwrap_or(0);
    let hi_y = c.iter().map(|v| v.y).max().unwrap_or(0);
    // Pixels whose centre (px + 1/2) can lie inside.
    let x0 = (lo_x.div_euclid(SUB)).max(0);
    let x1 = (hi_x.div_euclid(SUB)).min(w - 1);
    let y0 = (lo_y.div_euclid(SUB)).max(0);
    let y1 = (hi_y.div_euclid(SUB)).min(h - 1);
    // Edge k is opposite corner k.
    let bias: [i64; 3] = [(c[1], c[2]), (c[2], c[0]), (c[0], c[1])]
        .map(|(a, b)| if owns(b.x - a.x, b.y - a.y) { 0 } else { 1 });
    let inv = 1.0 / area as f64;
    for py in y0..=y1 {
        let cy = py * SUB + SUB / 2;
        for px in x0..=x1 {
            let cx = px * SUB + SUB / 2;
            let e0 = edge(&c[1], &c[2], cx, cy);
            let e1 = edge(&c[2], &c[0], cx, cy);
            let e2 = edge(&c[0], &c[1], cx, cy);
            if e0 - bias[0] < 0 || e1 - bias[1] < 0 || e2 - bias[2] < 0 {
                continue;
            }
            let (l0, l1, l2) = (e0 as f64 * inv, e1 as f64 * inv, e2 as f64 * inv);
            let z = l0 * c[0].z + l1 * c[1].z + l2 * c[2].z;
            let light = l0 * c[0].light + l1 * c[1].light + l2 * c[2].light;
            plot((py * w + px) as usize, z, light);
        }
    }
}

/// `v` in 0..=1 to a byte, rounding half up.
fn byte(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8
}

fn scale(rgb: [u8; 3], k: u32, d: u32) -> [u8; 3] {
    rgb.map(|c| (u32::from(c) * k / d) as u8)
}

/// The scene from `views` directions around its vertical axis.
///
/// Errors as [`crate::glb`] does on an invalid scene.
pub fn preview(scene: &Scene, d: &Preview) -> Result<Vec<Image>, ExportError> {
    // The same checks as export, and each part's pose.
    let placed = crate::export::check(scene)?;
    let width = if d.width == 0 { 256 } else { d.width };
    let height = if d.height == 0 { width } else { d.height };
    let views = if d.views == 0 { 4 } else { d.views };
    let elevation = d.elevation.unwrap_or(trig::to_radians(25.0));
    if !(elevation > 0.0 && elevation <= trig::PI / 2.0) || !d.azimuth.is_finite() {
        return Err(ExportError::NotFinite("the preview's elevation".into()));
    }
    let parts: Vec<Drawn> = scene
        .parts
        .iter()
        .zip(&placed)
        .map(|(p, m)| {
            let mut s = split(p.solid);
            for v in &mut s.positions {
                *v = m.apply(*v);
            }
            for n in &mut s.normals {
                *n = m.rotate(*n);
            }
            let edges = p
                .solid
                .real_edges()
                .into_iter()
                .map(|(a, b)| {
                    (
                        m.apply(p.solid.positions[a as usize]),
                        m.apply(p.solid.positions[b as usize]),
                    )
                })
                .collect();
            let rgb = p.material.color.unwrap_or([1.0; 3]);
            (s, edges, rgb)
        })
        .collect();
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for (s, _, _) in &parts {
        for &p in &s.positions {
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    if lo.x > hi.x {
        // Nothing to draw: the background only.
        lo = DVec3::ZERO;
        hi = DVec3::ZERO;
    }
    let centre = (lo + hi) * 0.5;
    let radius = ((hi - lo) * 0.5).length().max(1e-9);
    let fit = f64::from(width.min(height)) * 0.5 * 0.92 / radius;
    // One world light, from above, front left; it also casts the shadow.
    let light = DVec3::new(-0.45, -0.6, 1.0).normalize();
    let ground_z = lo.z;
    let half = radius * 1.15;
    let ground = [
        DVec3::new(centre.x - half, centre.y - half, ground_z),
        DVec3::new(centre.x + half, centre.y - half, ground_z),
        DVec3::new(centre.x + half, centre.y + half, ground_z),
        DVec3::new(centre.x - half, centre.y + half, ground_z),
    ];
    let (se, ce) = trig::sin_cos(elevation);
    let mut out = Vec::with_capacity(views as usize);
    for k in 0..views {
        let az = d.azimuth + trig::TAU * (f64::from(k) / f64::from(views));
        let (sa, ca) = trig::sin_cos(az);
        // At azimuth 0 the camera stands at −Y looking at +Y.
        let toward = DVec3::new(sa * ce, -ca * ce, se); // from the scene to the camera
        let right = DVec3::new(ca, sa, 0.0);
        // Right-handed: right × up = toward; up leans to +Z.
        let up = toward.cross(right);
        let cam = Camera {
            centre,
            right,
            up,
            forward: -toward,
            scale: fit,
            width,
            height,
        };
        let n = (width * height) as usize;
        let mut t = Target {
            width,
            height,
            rgb: vec![BACKGROUND; n],
            depth: vec![f64::INFINITY; n],
            ground: vec![false; n],
        };
        if !d.no_ground {
            draw_ground(&mut t, &cam, &ground, &parts, light, ground_z);
        }
        for (s, _, rgb) in &parts {
            draw_solid(&mut t, &cam, s, *rgb, light, toward);
        }
        if !d.no_edges {
            for (_, edges, _) in &parts {
                draw_edges(&mut t, &cam, edges, radius);
            }
        }
        let mut rgba = Vec::with_capacity(n * 4);
        for px in &t.rgb {
            rgba.extend_from_slice(px);
            rgba.push(255);
        }
        out.push(Image {
            width,
            height,
            rgba,
        });
    }
    Ok(out)
}

/// A part as drawn: its posed vertices, its posed real edges, its colour.
type Drawn = (crate::surface::Split, Vec<(DVec3, DVec3)>, [f64; 3]);

fn draw_ground(
    t: &mut Target,
    cam: &Camera,
    quad: &[DVec3; 4],
    parts: &[Drawn],
    light: DVec3,
    ground_z: f64,
) {
    let corner = |p: DVec3| {
        let (x, y, z) = cam.project(p);
        Corner {
            x,
            y,
            z,
            light: 0.0,
        }
    };
    let q = quad.map(corner);
    for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
        let (depth, ground) = (&mut t.depth, &mut t.ground);
        let rgb = &mut t.rgb;
        raster((t.width, t.height), tri, |i, z, _| {
            if z < depth[i] {
                depth[i] = z;
                ground[i] = true;
                rgb[i] = GROUND;
            }
        });
    }
    // The shadow: every triangle pressed onto the ground along the light.
    let mut shade = vec![false; t.rgb.len()];
    let dims = (t.width, t.height);
    for (s, _, _) in parts {
        for tri in s.indices.as_chunks::<3>().0 {
            let c = [0, 1, 2].map(|k| {
                let p = s.positions[tri[k] as usize];
                let on = p - light * ((p.z - ground_z) / light.z);
                corner(on)
            });
            raster(dims, c, |i, _, _| {
                if t.ground[i] {
                    shade[i] = true;
                }
            });
        }
    }
    for (px, s) in t.rgb.iter_mut().zip(&shade) {
        if *s {
            *px = SHADOW;
        }
    }
}

fn draw_solid(
    t: &mut Target,
    cam: &Camera,
    s: &crate::surface::Split,
    rgb: [f64; 3],
    light: DVec3,
    toward: DVec3,
) {
    let dims = (t.width, t.height);
    for tri in s.indices.as_chunks::<3>().0 {
        let p = [0, 1, 2].map(|k| s.positions[tri[k] as usize]);
        // Lit from the side the camera sees.
        let flip = if face_normal(p[0], p[1], p[2]).dot(toward) < 0.0 {
            -1.0
        } else {
            1.0
        };
        let c = [0, 1, 2].map(|k| {
            let (x, y, z) = cam.project(p[k]);
            let n = s.normals[tri[k] as usize] * flip;
            Corner {
                x,
                y,
                z,
                light: AMBIENT + DIFFUSE * n.dot(light).max(0.0),
            }
        });
        let (depth, ground, out) = (&mut t.depth, &mut t.ground, &mut t.rgb);
        raster(dims, c, |i, z, l| {
            if z < depth[i] {
                depth[i] = z;
                ground[i] = false;
                out[i] = [byte(rgb[0] * l), byte(rgb[1] * l), byte(rgb[2] * l)];
            }
        });
    }
}

/// Real edges, one pixel wide, where nothing nearer hides them.
fn draw_edges(t: &mut Target, cam: &Camera, edges: &[(DVec3, DVec3)], radius: f64) {
    // A line on a face is as near as the face: allow a little depth.
    let bias = radius * 0.006;
    let (w, h) = (i64::from(t.width), i64::from(t.height));
    for &(a, b) in edges {
        let (ax, ay, az) = cam.project(a);
        let (bx, by, bz) = cam.project(b);
        let (dx, dy) = (bx - ax, by - ay);
        let steps = (dx.abs().max(dy.abs()) / SUB).max(1);
        for i in 0..=steps {
            let x = (ax + dx * i / steps).div_euclid(SUB);
            let y = (ay + dy * i / steps).div_euclid(SUB);
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let k = (y * w + x) as usize;
            let z = az + (bz - az) * (i as f64 / steps as f64);
            if z - bias <= t.depth[k] {
                t.rgb[k] = scale(t.rgb[k], 2, 5);
            }
        }
    }
}

/// Images of one size side by side, `columns` to a row, left to right
/// then top to bottom; empty cells are the background colour.
pub fn sheet(images: &[Image], columns: u32) -> Image {
    let Some(first) = images.first() else {
        return Image::default();
    };
    let (w, h) = (first.width, first.height);
    let columns = columns.clamp(1, images.len() as u32);
    let rows = (images.len() as u32).div_ceil(columns);
    let (sw, sh) = (w * columns, h * rows);
    let mut rgba = Vec::with_capacity((sw * sh * 4) as usize);
    for _ in 0..sw * sh {
        rgba.extend_from_slice(&BACKGROUND);
        rgba.push(255);
    }
    for (i, im) in images.iter().enumerate() {
        if im.width != w || im.height != h {
            continue;
        }
        let (cx, cy) = (i as u32 % columns, i as u32 / columns);
        for y in 0..h {
            let src = (y * w * 4) as usize;
            let dst = (((cy * h + y) * sw + cx * w) * 4) as usize;
            rgba[dst..dst + (w * 4) as usize]
                .copy_from_slice(&im.rgba[src..src + (w * 4) as usize]);
        }
    }
    Image {
        width: sw,
        height: sh,
        rgba,
    }
}

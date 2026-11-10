//! A deterministic CPU turntable preview.
//!
//! - Orthographic views from `views` azimuths around the scene's vertical
//!   (kernel +Z) axis, all at one scale, so a part keeps its size from view
//!   to view.
//! - Triangles are snapped to a fixed-point grid (1/256 pixel) and covered
//!   by integer edge functions with a top-left rule: a pixel centre on an
//!   edge two triangles share belongs to exactly one of them.
//! - Lambert shading from one world light, interpolated in f64 from the
//!   integer barycentrics. Smooth only across facets of one surface that
//!   meet at less than [`CREASE_DEGREES`]: a corner's normal is the
//!   area-weighted mean of the triangles around its vertex on its surface
//!   whose normals lie within that angle of its own triangle's. A
//!   hexagonal loft shows its facets; a 12-sided or finer turn is round.
//!   (The GLB's normals stay smooth per surface.)
//! - Opaque parts first, with a depth buffer. Then the triangles of parts
//!   whose material's `opacity` is below 1, back to front, blended over
//!   what is drawn (`opacity` × their colour + (1 − `opacity`) × the
//!   pixel), tested against the opaque depth and not writing it. The
//!   order is the sum of a triangle's three corner depths, farthest
//!   first, ties to the earlier part and then the earlier triangle: an
//!   exact comparison of values computed the same way on every host. A
//!   part with `opacity` 0 is not drawn.
//! - Real edges are drawn as one-pixel lines, hidden behind nearer faces.
//! - A ground square under the scene, with the scene's shadow cast on it
//!   along the light; where only transparent parts cast it, it is lighter
//!   by their opacity (the most opaque caster at the pixel).
//!
//! No GPU, threads or SIMD; every step is integer arithmetic or f64
//! `+ - * /` and `sqrt` in a fixed order, so the framebuffer's bytes are
//! the same on every host.

use shape_core::{DVec3, trig};

use shape_mesh::Solid;

use crate::export::{ExportError, Scene};
use crate::surface::face_normal;

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
/// Facets of one surface meeting at less than this many degrees are
/// shaded smooth; at this angle or more the edge between them shows.
pub const CREASE_DEGREES: f64 = 35.0;

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
    let crease = trig::cos(trig::to_radians(CREASE_DEGREES));
    let mut parts: Vec<Drawn> = Vec::with_capacity(scene.parts.len());
    for (p, m) in scene.parts.iter().zip(&placed) {
        let opacity = p.material.opacity.unwrap_or(1.0);
        if !opacity.is_finite() {
            return Err(ExportError::NotFinite(format!(
                "the opacity of part {:?}",
                p.name
            )));
        }
        let tris = p
            .solid
            .triangles
            .iter()
            .map(|t| t.map(|v| m.apply(p.solid.positions[v as usize])))
            .collect();
        let normals = corner_normals(p.solid, crease)
            .into_iter()
            .map(|c| c.map(|n| m.rotate(n)))
            .collect();
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
        parts.push(Drawn {
            tris,
            normals,
            edges,
            rgb: p.material.color.unwrap_or([1.0; 3]),
            opacity: opacity.clamp(0.0, 1.0),
        });
    }
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for p in &parts {
        for &q in p.tris.iter().flatten() {
            lo = lo.min(q);
            hi = hi.max(q);
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
        for p in parts.iter().filter(|p| p.opacity == 1.0) {
            for k in 0..p.tris.len() {
                draw_triangle(&mut t, &cam, p, k, light, toward);
            }
        }
        for (part, k) in back_to_front(&cam, &parts) {
            draw_triangle(&mut t, &cam, &parts[part], k, light, toward);
        }
        if !d.no_edges {
            for p in parts.iter().filter(|p| p.opacity > 0.0) {
                draw_edges(&mut t, &cam, &p.edges, radius);
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

/// A part as drawn, posed.
struct Drawn {
    /// Each triangle's corners.
    tris: Vec<[DVec3; 3]>,
    /// Each triangle's corner normals ([`corner_normals`]).
    normals: Vec<[DVec3; 3]>,
    /// The real edges.
    edges: Vec<(DVec3, DVec3)>,
    /// sRGB colour.
    rgb: [f64; 3],
    /// In 0..=1; 1 is opaque.
    opacity: f64,
}

/// Per triangle, each corner's shading normal: the area-weighted sum of
/// the normals of the triangles around its vertex that share the
/// triangle's surface and whose unit normals have a dot product of at
/// least `crease` (the cosine of [`CREASE_DEGREES`]) with its own,
/// normalized. The triangle itself always counts unless it is degenerate;
/// a zero sum gives +Z (only degenerate triangles have one).
pub(crate) fn corner_normals(s: &Solid, crease: f64) -> Vec<[DVec3; 3]> {
    let weighted: Vec<DVec3> = s
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|v| s.positions[v as usize]);
            (b - a).cross(c - a)
        })
        .collect();
    let unit: Vec<DVec3> = s
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|v| s.positions[v as usize]);
            face_normal(a, b, c)
        })
        .collect();
    let mut around: Vec<Vec<u32>> = vec![Vec::new(); s.positions.len()];
    for (i, t) in s.triangles.iter().enumerate() {
        for &v in t {
            around[v as usize].push(i as u32);
        }
    }
    s.triangles
        .iter()
        .enumerate()
        .map(|(i, t)| {
            t.map(|v| {
                let mut n = DVec3::ZERO;
                for &j in &around[v as usize] {
                    let j = j as usize;
                    if s.surfaces[j] == s.surfaces[i] && unit[j].dot(unit[i]) >= crease {
                        n += weighted[j];
                    }
                }
                let len = n.length();
                if len > 0.0 { n / len } else { DVec3::Z }
            })
        })
        .collect()
}

/// The triangles of transparent parts (`0 < opacity < 1`) in drawing
/// order: the sum of the corners' depths, largest (farthest) first; ties
/// to the lower part index, then the lower triangle index. Depths are
/// finite (the scene's positions are checked), so `total_cmp` is the
/// plain order.
fn back_to_front(cam: &Camera, parts: &[Drawn]) -> Vec<(usize, usize)> {
    let mut order: Vec<(f64, usize, usize)> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        if p.opacity == 1.0 || p.opacity == 0.0 {
            continue;
        }
        for (k, t) in p.tris.iter().enumerate() {
            let z = t.iter().map(|&q| cam.project(q).2).fold(0.0, |a, b| a + b);
            order.push((z, i, k));
        }
    }
    order.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    order.into_iter().map(|(_, i, k)| (i, k)).collect()
}

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
    // The shadow: every triangle pressed onto the ground along the light,
    // as dark as the most opaque part casting it there.
    let mut shade = vec![0.0f64; t.rgb.len()];
    let dims = (t.width, t.height);
    for p in parts.iter().filter(|p| p.opacity > 0.0) {
        for tri in &p.tris {
            let c = tri.map(|q| corner(q - light * ((q.z - ground_z) / light.z)));
            raster(dims, c, |i, _, _| {
                if t.ground[i] && p.opacity > shade[i] {
                    shade[i] = p.opacity;
                }
            });
        }
    }
    for (px, &s) in t.rgb.iter_mut().zip(&shade) {
        if s == 1.0 {
            *px = SHADOW;
        } else if s > 0.0 {
            *px = [0, 1, 2]
                .map(|k| byte((f64::from(px[k]) * (1.0 - s) + f64::from(SHADOW[k]) * s) / 255.0));
        }
    }
}

/// Triangle `k` of part `p`: opaque into the depth buffer, or blended
/// over what is drawn, tested against the depth but not writing it.
fn draw_triangle(t: &mut Target, cam: &Camera, p: &Drawn, k: usize, light: DVec3, toward: DVec3) {
    let dims = (t.width, t.height);
    let q = p.tris[k];
    // Lit from the side the camera sees.
    let flip = if face_normal(q[0], q[1], q[2]).dot(toward) < 0.0 {
        -1.0
    } else {
        1.0
    };
    let c = [0, 1, 2].map(|i| {
        let (x, y, z) = cam.project(q[i]);
        let n = p.normals[k][i] * flip;
        Corner {
            x,
            y,
            z,
            light: AMBIENT + DIFFUSE * n.dot(light).max(0.0),
        }
    });
    let (rgb, alpha) = (p.rgb, p.opacity);
    let (depth, ground, out) = (&mut t.depth, &mut t.ground, &mut t.rgb);
    raster(dims, c, |i, z, l| {
        if z >= depth[i] {
            return;
        }
        if alpha == 1.0 {
            depth[i] = z;
            ground[i] = false;
            out[i] = [byte(rgb[0] * l), byte(rgb[1] * l), byte(rgb[2] * l)];
        } else {
            let under = out[i];
            out[i] = [0, 1, 2]
                .map(|c| byte(rgb[c] * l * alpha + f64::from(under[c]) / 255.0 * (1.0 - alpha)));
        }
    });
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::{Material, Part};

    /// An `n`-sided prism of radius 10 and height 10 whose sides are one
    /// surface (as a loft's are), its caps two more.
    fn prism(n: u32, at: DVec3) -> Solid {
        let mut s = Solid::default();
        for z in [0.0, 10.0] {
            for k in 0..n {
                let (sn, cs) = trig::sin_cos(trig::TAU * f64::from(k) / f64::from(n));
                s.push_vertex(at + DVec3::new(10.0 * cs, 10.0 * sn, z));
            }
        }
        for k in 0..n {
            let (a, b) = (k, (k + 1) % n);
            s.push_triangle([a, b, b + n], 0, &[]);
            s.push_triangle([a, b + n, a + n], 0, &[]);
        }
        for k in 1..n - 1 {
            s.push_triangle([0, k + 1, k], 1, &[]);
            s.push_triangle([n, n + k, n + k + 1], 2, &[]);
        }
        s
    }

    fn sides_flat(n: u32) -> bool {
        let s = prism(n, DVec3::ZERO);
        let crease = trig::cos(trig::to_radians(CREASE_DEGREES));
        let normals = corner_normals(&s, crease);
        s.triangles.iter().zip(&normals).all(|(t, c)| {
            let [a, b, d] = t.map(|v| s.positions[v as usize]);
            let f = face_normal(a, b, d);
            c.iter().all(|n| (*n - f).length() < 1e-12)
        })
    }

    #[test]
    fn facets_past_the_crease_are_flat_and_finer_ones_smooth() {
        // A hexagon's facets meet at 60°: each shaded flat. A 24-gon's
        // meet at 15°: shaded smooth across them.
        assert!(sides_flat(6));
        assert!(sides_flat(8));
        assert!(!sides_flat(12));
        assert!(!sides_flat(24));
    }

    /// A square 20 across, flat at height `z`.
    fn quad(z: f64) -> Solid {
        let mut s = Solid::default();
        for (x, y) in [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)] {
            s.push_vertex(DVec3::new(x, y, z));
        }
        s.push_triangle([0, 1, 2], 0, &[]);
        s.push_triangle([0, 2, 3], 0, &[]);
        s
    }

    fn part<'a>(name: &str, s: &'a Solid, rgb: [f64; 3], opacity: f64) -> Part<'a> {
        Part {
            material: Material {
                color: Some(rgb),
                opacity: Some(opacity),
                ..Default::default()
            },
            ..Part::new(name, s)
        }
    }

    fn centre(parts: Vec<Part>) -> [u8; 3] {
        let scene = Scene {
            name: "t".into(),
            parts,
        };
        let d = Preview {
            width: 32,
            views: 1,
            elevation: Some(trig::PI / 2.0 - 1e-3),
            no_edges: true,
            no_ground: true,
            ..Default::default()
        };
        let im = &preview(&scene, &d).unwrap()[0];
        let i = ((16 * 32 + 16) * 4) as usize;
        [im.rgba[i], im.rgba[i + 1], im.rgba[i + 2]]
    }

    #[test]
    fn transparent_parts_blend_back_to_front_whatever_their_order() {
        // Seen from above: a red quad at z 0, blue at 5 (opacity 0.5),
        // green at 10 (opacity 0.5).
        let (red, blue, green) = (quad(0.0), quad(5.0), quad(10.0));
        let one = centre(vec![
            part("red", &red, [1.0, 0.0, 0.0], 1.0),
            part("blue", &blue, [0.0, 0.0, 1.0], 0.5),
            part("green", &green, [0.0, 1.0, 0.0], 0.5),
        ]);
        let other = centre(vec![
            part("green", &green, [0.0, 1.0, 0.0], 0.5),
            part("blue", &blue, [0.0, 0.0, 1.0], 0.5),
            part("red", &red, [1.0, 0.0, 0.0], 1.0),
        ]);
        assert_eq!(one, other);
        // Every colour shows through: green over blue over red.
        assert!(one[0] > 0 && one[1] > 0 && one[2] > 0, "{one:?}");
        // Blue drawn over green (the wrong order) would leave more blue.
        assert!(one[1] > one[2], "{one:?}");
        // Opaque, the nearest hides the rest.
        let solid = centre(vec![
            part("red", &red, [1.0, 0.0, 0.0], 1.0),
            part("green", &green, [0.0, 1.0, 0.0], 1.0),
        ]);
        assert_eq!((solid[0], solid[2]), (0, 0));
    }
}

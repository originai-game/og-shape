//! A deterministic CPU turntable preview.
//!
//! - Views from `views` azimuths around the scene's vertical (kernel +Z)
//!   axis, all at one scale, so a part keeps its size from view to view.
//!   Orthographic by default; with [`Preview::fov`], perspective from a
//!   distance that fits the scene's bounding sphere as the orthographic
//!   view does. [`views`] gives each view's camera ([`ViewCamera`]).
//! - Triangles are snapped to a fixed-point grid (1/256 pixel) and covered
//!   by integer edge functions with a top-left rule: a pixel centre on an
//!   edge two triangles share belongs to exactly one of them. In
//!   perspective, values are interpolated perspective-correctly (the
//!   barycentrics weighted by 1 / depth).
//! - Lambert shading from one world light, interpolated in f64 from the
//!   integer barycentrics. Smooth only across facets of one surface that
//!   meet at less than [`CREASE_DEGREES`]: a corner's normal is the
//!   area-weighted mean of the triangles around its vertex on its surface
//!   whose normals lie within that angle of its own triangle's. A
//!   hexagonal loft shows its facets; a 12-sided or finer turn is round.
//!   (The GLB's normals stay smooth per surface.) [`Preview::unlit`]
//!   leaves the light out: colour only.
//! - Colour. A part with neither a drawable map nor vertex colours is its
//!   material's `color`, shaded in sRGB as it always was. A part with a
//!   map (PNG or JPEG, on a part with UVs) or [`Solid::colors`] is
//!   coloured per pixel: the texture sampled at the pixel's UV (a
//!   box-filtered mip chain, the level by the pixel's UV footprint,
//!   bilinear within it, repeat wrap; `crate::sampler`), times the
//!   material's `color` (in `color_space`: an imported material's is
//!   linear), times the interpolated vertex colour, all in
//!   linear light, then back to sRGB and shaded as an untextured part is.
//!   A white map and white vertex colours give the untextured pixel. A
//!   material's `mapSize` is already in the UVs a model's scene carries.
//!   Each distinct map is decoded once per call; one that cannot be
//!   decoded is listed in [`Views::undrawn`] and the part drawn without
//!   it.
//! - Opaque parts first, with a depth buffer. Then the triangles of parts
//!   whose material's `opacity` is below 1, back to front, blended over
//!   what is drawn (alpha × their colour + (1 − alpha) × the pixel),
//!   tested against the opaque depth and not writing it. Alpha is the
//!   `opacity`, times the map's alpha and the vertex colour's alpha where
//!   the part has them; a part with `opacity` 1 is opaque whatever its
//!   map's alpha (glTF's `OPAQUE`, which export writes for it). The
//!   order is the sum of a triangle's three corner depths, farthest
//!   first, ties to the earlier part and then the earlier triangle: an
//!   exact comparison of values computed the same way on every host. A
//!   part with `opacity` 0 is not drawn.
//! - Real edges are drawn as one-pixel lines, hidden behind nearer faces.
//! - A ground square under the scene, with the scene's shadow cast on it
//!   along the light; where only transparent parts cast it, it is lighter
//!   by their opacity (the most opaque caster at the pixel).
//! - A material's `emissive` (times `emissive_intensity`, in its
//!   `color_space`) is added to the lit colour in linear light, before
//!   blending: it glows the same in shade and in light. A part without it
//!   is drawn as before, to the byte. The emissive map multiplies it.
//! - `metalness` and `roughness` (and the metallic-roughness map's blue
//!   and green) add what the surface reflects, in linear light: a
//!   highlight of the light, and a sky over a ground (lighter the more
//!   the reflection points up), blurred as the surface roughens; a
//!   metal's own colour tints both and it keeps no matte colour. A
//!   matte part that is no metal is drawn as before, to the byte. The
//!   occlusion map's red dims the light from all round by
//!   `ao_map_intensity` (`docs/research.md` §53).
//! - A material's `normal_map` (with the part's UVs) bends the light per
//!   pixel: the map is read as data, at its finest level, in the
//!   triangle's own tangent frame (u along the tangent, v along the
//!   bitangent, from its positions and UVs, round the interpolated
//!   normal), times `normal_scale`. A baked normal map and a file's own
//!   show their detail; a seam between charts shows as a line if the
//!   frames disagree. Unlit views do not read it.
//!
//! No GPU, threads or SIMD; every step is integer arithmetic or f64
//! `+ - * /`, `sqrt`, `floor` and `trig` in a fixed order, so the
//! framebuffer's bytes are the same on every host.

use shape_core::{DVec2, DVec3, trig};

use shape_mesh::Solid;

use crate::export::{ColorSpace, ExportError, Material, Scene};
use crate::image::Texture;
use crate::sampler::{Mips, to_linear, to_srgb};
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
    /// Perspective: the vertical field of view, radians, in (0, π/3];
    /// `None` is orthographic. The camera stands as far off as fits the
    /// scene's bounding sphere into 92 % of the smaller side, as the
    /// orthographic view's scale does.
    pub fov: Option<f64>,
    /// Colour only: no light, so every pixel is its material's colour
    /// times its map and vertex colour. Edges and the ground stay unless
    /// [`Preview::no_edges`] and [`Preview::no_ground`] leave them out.
    pub unlit: bool,
    /// The box the camera frames and the ground lies under, corner to
    /// corner; `None` is the scene's own. One box for every frame of a
    /// motion keeps the camera still while the scene moves in it.
    pub frame: Option<(DVec3, DVec3)>,
}

/// Where a view looks from, in the scene's frame (kernel units, +Z up).
/// For three.js: a camera at `position` with this `up`, looking at
/// `target`; `PerspectiveCamera(fov in degrees, width / height)`, or an
/// `OrthographicCamera` `ortho_height` tall and `ortho_height × width /
/// height` wide. Pixel (0, 0) is the image's top left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ViewCamera {
    /// The eye. Orthographic: on the view's axis, three bounding-sphere
    /// radii from the target (any point on that line sees the same).
    pub position: DVec3,
    /// The point at the image's centre: the centre of the scene's box.
    pub target: DVec3,
    /// The image's up, a unit vector square to the view.
    pub up: DVec3,
    /// The vertical field of view, radians; `None` is orthographic.
    pub fov: Option<f64>,
    /// Orthographic: the scene units the image's height spans; 0 in
    /// perspective.
    pub ortho_height: f64,
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
}

/// What [`views`] draws: the images, their cameras, and the maps it could
/// not draw.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Views {
    /// One image a view.
    pub images: Vec<Image>,
    /// The camera of each image.
    pub cameras: Vec<ViewCamera>,
    /// Parts whose map or vertex colours were not drawn, and why (a map
    /// that is not PNG or JPEG, or does not decode; a part with no UVs).
    pub undrawn: Vec<(String, String)>,
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
    /// Perspective: the eye and the focal length in pixels.
    eye: Option<(DVec3, f64)>,
}

impl Camera {
    /// Screen position in fixed point, and depth (perspective: the
    /// distance from the eye along the view, always positive).
    fn project(&self, p: DVec3) -> (i64, i64, f64) {
        let (sx, sy, z) = match self.eye {
            None => {
                let d = p - self.centre;
                (
                    f64::from(self.width) * 0.5 + d.dot(self.right) * self.scale,
                    f64::from(self.height) * 0.5 - d.dot(self.up) * self.scale,
                    d.dot(self.forward),
                )
            }
            Some((eye, focal)) => {
                let d = p - eye;
                let z = d.dot(self.forward);
                let k = focal / z;
                (
                    f64::from(self.width) * 0.5 + d.dot(self.right) * k,
                    f64::from(self.height) * 0.5 - d.dot(self.up) * k,
                    z,
                )
            }
        };
        (
            (sx * SUB as f64).round() as i64,
            (sy * SUB as f64).round() as i64,
            z,
        )
    }

    fn corner(&self, p: DVec3, light: f64) -> Corner {
        let (x, y, z) = self.project(p);
        Corner {
            x,
            y,
            z,
            light,
            iz: 1.0 / z,
        }
    }
}

struct Target {
    width: u32,
    height: u32,
    rgb: Vec<[u8; 3]>,
    depth: Vec<f64>,
    ground: Vec<bool>,
}

/// One projected corner: fixed-point screen position, depth, a value to
/// interpolate (light), and 1 / depth for perspective.
#[derive(Clone, Copy)]
struct Corner {
    x: i64,
    y: i64,
    z: f64,
    light: f64,
    iz: f64,
}

/// A covered pixel.
struct Frag {
    /// Pixel index.
    i: usize,
    /// Depth.
    z: f64,
    /// Interpolated light.
    light: f64,
    /// The corners' weights, in the order the corners were given.
    w: [f64; 3],
    /// The weights one pixel right and one pixel down (past the edges
    /// too), when asked for; else zero.
    wx: [f64; 3],
    wy: [f64; 3],
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

/// Perspective-correct weights from screen barycentrics.
fn correct(l: [f64; 3], c: &[Corner; 3]) -> [f64; 3] {
    let a = [l[0] * c[0].iz, l[1] * c[1].iz, l[2] * c[2].iz];
    let s = a[0] + a[1] + a[2];
    if s == 0.0 {
        return l;
    }
    [a[0] / s, a[1] / s, a[2] / s]
}

/// Covers a triangle and calls `plot` for each pixel it covers.
/// `perspective` corrects the weights by 1 / depth; `derivs` fills
/// [`Frag::wx`] and [`Frag::wy`].
fn raster(
    size: (u32, u32),
    mut c: [Corner; 3],
    perspective: bool,
    derivs: bool,
    mut plot: impl FnMut(&Frag),
) {
    let mut area = edge(&c[0], &c[1], c[2].x, c[2].y);
    if area == 0 {
        return;
    }
    let swapped = area < 0;
    if swapped {
        c.swap(1, 2);
        area = -area;
    }
    // Back to the caller's corner order.
    let order = |w: [f64; 3]| if swapped { [w[0], w[2], w[1]] } else { w };
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
    let pairs = [(c[1], c[2]), (c[2], c[0]), (c[0], c[1])];
    let bias: [i64; 3] = pairs.map(|(a, b)| if owns(b.x - a.x, b.y - a.y) { 0 } else { 1 });
    // Each edge function's change one pixel right and one pixel down.
    let step_x: [i64; 3] = pairs.map(|(a, b)| -(b.y - a.y) * SUB);
    let step_y: [i64; 3] = pairs.map(|(a, b)| (b.x - a.x) * SUB);
    let inv = 1.0 / area as f64;
    let weights = |e: [i64; 3]| {
        let l = [e[0] as f64 * inv, e[1] as f64 * inv, e[2] as f64 * inv];
        order(if perspective { correct(l, &c) } else { l })
    };
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
            let (z, light, wt) = if perspective {
                let k = correct([l0, l1, l2], &c);
                (
                    k[0] * c[0].z + k[1] * c[1].z + k[2] * c[2].z,
                    k[0] * c[0].light + k[1] * c[1].light + k[2] * c[2].light,
                    order(k),
                )
            } else {
                (
                    l0 * c[0].z + l1 * c[1].z + l2 * c[2].z,
                    l0 * c[0].light + l1 * c[1].light + l2 * c[2].light,
                    order([l0, l1, l2]),
                )
            };
            let (wx, wy) = if derivs {
                let e = [e0, e1, e2];
                (
                    weights([0, 1, 2].map(|k| e[k] + step_x[k])),
                    weights([0, 1, 2].map(|k| e[k] + step_y[k])),
                )
            } else {
                ([0.0; 3], [0.0; 3])
            };
            plot(&Frag {
                i: (py * w + px) as usize,
                z,
                light,
                w: wt,
                wx,
                wy,
            });
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
/// Errors as [`crate::glb`] does on an invalid scene. [`views`] gives
/// the cameras and the maps not drawn too.
pub fn preview(scene: &Scene, d: &Preview) -> Result<Vec<Image>, ExportError> {
    Ok(views(scene, d)?.images)
}

/// The decoded maps of a scene's parts, each distinct one once: the
/// textures and their chains (or why not), in first-use order.
pub(crate) struct Maps<'a> {
    /// Each texture with whether it was read as data, and its chain.
    seen: Vec<(&'a Texture, bool, Result<Mips, String>)>,
}

impl<'a> Maps<'a> {
    pub(crate) fn new() -> Maps<'a> {
        Maps { seen: Vec::new() }
    }

    /// The index of `t`'s chain, decoding it the first time.
    pub(crate) fn get(&mut self, t: &'a Texture) -> Result<usize, String> {
        self.chain(t, false)
    }

    /// As [`Maps::get`], for a map of data (a normal map).
    pub(crate) fn get_data(&mut self, t: &'a Texture) -> Result<usize, String> {
        self.chain(t, true)
    }

    fn chain(&mut self, t: &'a Texture, data: bool) -> Result<usize, String> {
        let k = match self.seen.iter().position(|(s, d, _)| *s == t && *d == data) {
            Some(k) => k,
            None => {
                let m = crate::decode::decode(&t.bytes).map(if data {
                    Mips::new_data
                } else {
                    Mips::new
                });
                self.seen.push((t, data, m));
                self.seen.len() - 1
            }
        };
        match &self.seen[k].2 {
            Ok(_) => Ok(k),
            Err(e) => Err(format!("map {:?}: {e}", t.name)),
        }
    }

    pub(crate) fn mips(&self, k: usize) -> Option<&Mips> {
        self.seen.get(k).and_then(|(_, _, m)| m.as_ref().ok())
    }
}

/// The longest side of a preview image, in pixels.
pub const MAX_SIDE: u32 = 8192;
/// The most views of one preview.
pub const MAX_VIEWS: u32 = 720;
/// The most pixels of one preview, all its views together (2²⁸: a
/// gigabyte of RGBA).
pub const MAX_PIXELS: u64 = 1 << 28;

/// The scene from `views` directions around its vertical axis, with each
/// view's camera and the maps not drawn.
///
/// Refuses more than [`MAX_SIDE`] pixels a side, [`MAX_VIEWS`] views or
/// [`MAX_PIXELS`] pixels in all ([`ExportError::TooLarge`]).
/// Errors as [`crate::glb`] does on an invalid scene, and on an elevation
/// or field of view out of range.
pub fn views(scene: &Scene, d: &Preview) -> Result<Views, ExportError> {
    // The same checks as export, and each part's pose.
    let placed = crate::export::check(scene)?;
    let width = if d.width == 0 { 256 } else { d.width };
    let height = if d.height == 0 { width } else { d.height };
    let views = if d.views == 0 { 4 } else { d.views };
    // Bounds, so a caller that passes a request on need not check it:
    // an image side, the views, and the pixels of them all.
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(ExportError::TooLarge(format!(
            "{width} x {height} pixels; a side is at most {MAX_SIDE}"
        )));
    }
    if views > MAX_VIEWS {
        return Err(ExportError::TooLarge(format!(
            "{views} views; at most {MAX_VIEWS}"
        )));
    }
    let pixels = u64::from(width) * u64::from(height) * u64::from(views);
    if pixels > MAX_PIXELS {
        return Err(ExportError::TooLarge(format!(
            "{views} views of {width} x {height} are {pixels} pixels; at most {MAX_PIXELS} in all"
        )));
    }
    let elevation = d.elevation.unwrap_or(trig::to_radians(25.0));
    if !(elevation > 0.0 && elevation <= trig::PI / 2.0) || !d.azimuth.is_finite() {
        return Err(ExportError::NotFinite("the preview's elevation".into()));
    }
    if let Some(f) = d.fov
        && !(f > 0.0 && f <= trig::PI / 3.0)
    {
        return Err(ExportError::NotFinite(
            "the preview's field of view (over 0, at most pi/3)".into(),
        ));
    }
    let crease = trig::cos(trig::to_radians(CREASE_DEGREES));
    let mut maps = Maps::new();
    let mut undrawn = Vec::new();
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
        // The preview's colours are sRGB; an imported material's factors
        // are linear (`ColorSpace::Linear`).
        let rgb = p.material.color.unwrap_or([1.0; 3]);
        let rgb = match p.material.color_space {
            ColorSpace::Srgb => rgb,
            ColorSpace::Linear => rgb.map(|c| to_srgb(c.clamp(0.0, 1.0))),
        };
        let strength = p.material.emissive_intensity.unwrap_or(1.0);
        if !(strength.is_finite() && p.material.emissive.iter().all(|c| c.is_finite())) {
            return Err(ExportError::NotFinite(format!(
                "the emissive colour of part {:?}",
                p.name
            )));
        }
        let emissive = p.material.emissive.map(|c| {
            let c = c.clamp(0.0, 1.0);
            let c = match p.material.color_space {
                ColorSpace::Srgb => to_linear(c),
                ColorSpace::Linear => c,
            };
            c * strength.max(0.0)
        });
        let paint = paint(
            p.name.as_str(),
            p.solid,
            &p.material,
            rgb,
            &mut maps,
            &mut undrawn,
        );
        parts.push(Drawn {
            tris,
            normals,
            edges,
            rgb,
            emissive,
            opacity: opacity.clamp(0.0, 1.0),
            paint,
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
    if let Some((a, b)) = d.frame {
        if !(a.is_finite() && b.is_finite()) {
            return Err(ExportError::NotFinite("the preview's frame".into()));
        }
        lo = a.min(b);
        hi = a.max(b);
    }
    let centre = (lo + hi) * 0.5;
    let radius = ((hi - lo) * 0.5).length().max(1e-9);
    let fit = f64::from(width.min(height)) * 0.5 * 0.92 / radius;
    // Perspective: the focal length in pixels, and the distance at which
    // the bounding sphere's silhouette fills 92 % of the smaller side:
    // tan(half-angle) = t, so radius / distance = t / √(1 + t²).
    let lens = d.fov.map(|f| {
        let tan = trig::tan(f * 0.5);
        let focal = f64::from(height) * 0.5 / tan;
        let t = 0.92 * tan * f64::from(width.min(height)) / f64::from(height);
        (focal, radius * (1.0 + t * t).sqrt() / t)
    });
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
    let mut out = Views {
        undrawn,
        ..Default::default()
    };
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
            eye: lens.map(|(focal, dist)| (centre + toward * dist, focal)),
        };
        out.cameras.push(ViewCamera {
            position: match lens {
                Some((_, dist)) => centre + toward * dist,
                None => centre + toward * (radius * 3.0),
            },
            target: centre,
            up,
            fov: d.fov,
            ortho_height: if lens.is_some() {
                0.0
            } else {
                f64::from(height) / fit
            },
            width,
            height,
        });
        let n = (width * height) as usize;
        let mut t = Target {
            width,
            height,
            rgb: vec![BACKGROUND; n],
            depth: vec![f64::INFINITY; n],
            ground: vec![false; n],
        };
        let look = Look {
            cam: &cam,
            light,
            toward,
            unlit: d.unlit,
            maps: &maps,
        };
        if !d.no_ground {
            draw_ground(&mut t, &cam, &ground, &parts, light, ground_z);
        }
        for p in parts.iter().filter(|p| p.opacity == 1.0) {
            for k in 0..p.tris.len() {
                draw_triangle(&mut t, &look, p, k);
            }
        }
        for (part, k) in back_to_front(&cam, &parts) {
            draw_triangle(&mut t, &look, &parts[part], k);
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
        out.images.push(Image {
            width,
            height,
            rgba,
        });
    }
    Ok(out)
}

/// How a part is coloured per pixel, when it is.
struct Paint {
    /// The map's chain in [`Maps`], with the part's UVs.
    map: Option<(usize, Vec<[DVec2; 3]>)>,
    /// The normal map's chain, the part's UVs and the map's scale.
    normal: Option<(usize, Vec<[DVec2; 3]>, f64)>,
    /// Linear RGBA per triangle corner.
    colors: Option<Vec<[[f64; 4]; 3]>>,
    /// The material's colour in linear light.
    base: [f64; 3],
    /// What the surface does with light besides its colour, if anything.
    gloss: Option<Gloss>,
}

/// A material that is a metal, smoother than matte, or has maps for
/// those, for occlusion or for its glow. Each map is its chain in
/// [`Maps`]; they share the part's UVs.
struct Gloss {
    metal: f64,
    rough: f64,
    /// Roughness in green, metalness in blue, times the factors.
    metal_rough: Option<usize>,
    /// Occlusion in red, and how much of it counts.
    occlusion: Option<(usize, f64)>,
    /// The colour the emissive light is multiplied by.
    glow: Option<usize>,
    uvs: Option<Vec<[DVec2; 3]>>,
}

/// The light a mirror facing straight down and straight up reflects
/// (linear): the ground's and the sky's.
const ENV_DOWN: f64 = 0.15;
const ENV_UP: f64 = 0.9;

/// The part's [`Paint`], if it has a map it can draw or vertex colours;
/// what it cannot draw goes to `undrawn`.
fn paint<'a>(
    name: &str,
    s: &Solid,
    material: &'a Material,
    rgb: [f64; 3],
    maps: &mut Maps<'a>,
    undrawn: &mut Vec<(String, String)>,
) -> Option<Paint> {
    let n = s.triangles.len();
    let map = material.map.as_ref();
    let normal = material
        .normal_map
        .as_ref()
        .map(|t| (t, material.normal_scale.unwrap_or(1.0)));
    // The maps of the surface's gloss, each read with the part's UVs.
    let uvs = s.uvs.as_ref().filter(|uv| uv.len() == n);
    let mut chain = |t: Option<&'a Texture>, color: bool, what: &str| -> Option<usize> {
        let t = t?;
        let why = match uvs {
            None => format!("the part has no UVs to sample its {what} with"),
            Some(_) => match if color { maps.get(t) } else { maps.get_data(t) } {
                Ok(k) => return Some(k),
                Err(e) => e,
            },
        };
        undrawn.push((name.to_string(), why));
        None
    };
    let metal_rough = chain(material.metalness_map.as_ref(), false, "metalness map");
    let occlusion = chain(material.ao_map.as_ref(), false, "occlusion map")
        .map(|k| (k, material.ao_map_intensity.unwrap_or(1.0).clamp(0.0, 1.0)));
    let glow = chain(material.emissive_map.as_ref(), true, "emissive map");
    let metal = material.metalness.clamp(0.0, 1.0);
    let rough = material.roughness.unwrap_or(1.0).clamp(0.0, 1.0);
    let glossy = metal > 0.0 || rough < 1.0;
    let gloss =
        (glossy || metal_rough.is_some() || occlusion.is_some() || glow.is_some()).then(|| Gloss {
            metal,
            rough,
            metal_rough,
            occlusion,
            glow,
            uvs: uvs.cloned(),
        });
    let normal = normal.and_then(|(t, scale)| {
        let why = match &s.uvs {
            None => "the part has no UVs to sample its normal map with".to_string(),
            Some(uv) if uv.len() != n => "the part's UVs are not one per triangle".to_string(),
            Some(uv) => match maps.get_data(t) {
                Ok(k) => return Some((k, uv.clone(), scale)),
                Err(e) => e,
            },
        };
        undrawn.push((name.to_string(), why));
        None
    });
    let map = map.and_then(|t| {
        let why = match &s.uvs {
            None => "the part has no UVs to sample its map with".to_string(),
            Some(uv) if uv.len() != n => "the part's UVs are not one per triangle".to_string(),
            Some(uv) => match maps.get(t) {
                Ok(k) => return Some((k, uv.clone())),
                Err(e) => e,
            },
        };
        undrawn.push((name.to_string(), why));
        None
    });
    let colors = s.colors.as_ref().and_then(|c| {
        if c.len() == n && c.iter().flatten().flatten().all(|v| v.is_finite()) {
            Some(c.clone())
        } else {
            undrawn.push((
                name.to_string(),
                "the part's vertex colours are not one finite set per triangle".to_string(),
            ));
            None
        }
    });
    if map.is_none() && colors.is_none() && normal.is_none() && gloss.is_none() {
        return None;
    }
    Some(Paint {
        map,
        normal,
        colors,
        base: rgb.map(|c| to_linear(c.clamp(0.0, 1.0))),
        gloss,
    })
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
    /// Light given off, linear, added to the lit colour.
    emissive: [f64; 3],
    /// In 0..=1; 1 is opaque.
    opacity: f64,
    /// Per-pixel colour, if the part has a map or vertex colours.
    paint: Option<Paint>,
}

/// What one view's triangles are drawn with.
struct Look<'a> {
    cam: &'a Camera,
    light: DVec3,
    /// From the scene to the camera (orthographic); the view's axis.
    toward: DVec3,
    unlit: bool,
    maps: &'a Maps<'a>,
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

/// The part of a polygon in the plane z = const inside the axis-aligned
/// square `lo`..`hi` (x and y), by Sutherland–Hodgman.
fn clip_square(poly: &[DVec3], lo: DVec3, hi: DVec3) -> Vec<DVec3> {
    let mut out: Vec<DVec3> = poly.to_vec();
    // (axis, bound, keep below the bound)
    for (axis, bound, below) in [
        (0, lo.x, false),
        (0, hi.x, true),
        (1, lo.y, false),
        (1, hi.y, true),
    ] {
        let inside = |p: DVec3| {
            let v = if axis == 0 { p.x } else { p.y };
            if below { v <= bound } else { v >= bound }
        };
        let cut = |a: DVec3, b: DVec3| {
            let (va, vb) = if axis == 0 { (a.x, b.x) } else { (a.y, b.y) };
            a + (b - a) * ((bound - va) / (vb - va))
        };
        let input = std::mem::take(&mut out);
        for (i, &b) in input.iter().enumerate() {
            let a = input[(i + input.len() - 1) % input.len()];
            match (inside(a), inside(b)) {
                (true, true) => out.push(b),
                (true, false) => out.push(cut(a, b)),
                (false, true) => {
                    out.push(cut(a, b));
                    out.push(b);
                }
                (false, false) => {}
            }
        }
        if out.is_empty() {
            break;
        }
    }
    out
}

fn draw_ground(
    t: &mut Target,
    cam: &Camera,
    quad: &[DVec3; 4],
    parts: &[Drawn],
    light: DVec3,
    ground_z: f64,
) {
    let persp = cam.eye.is_some();
    let corner = |p: DVec3| cam.corner(p, 0.0);
    let q = quad.map(corner);
    for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
        let (depth, ground) = (&mut t.depth, &mut t.ground);
        let rgb = &mut t.rgb;
        raster((t.width, t.height), tri, persp, false, |f| {
            let i = f.i;
            if f.z < depth[i] {
                depth[i] = f.z;
                ground[i] = true;
                rgb[i] = GROUND;
            }
        });
    }
    // The shadow: every triangle pressed onto the ground along the light,
    // as dark as the most opaque part casting it there. In perspective it
    // is clipped to the ground square first: a shadow point past it can
    // lie behind the eye.
    let mut shade = vec![0.0f64; t.rgb.len()];
    let dims = (t.width, t.height);
    for p in parts.iter().filter(|p| p.opacity > 0.0) {
        for tri in &p.tris {
            let pressed = tri.map(|q| q - light * ((q.z - ground_z) / light.z));
            let mut cover = |c: [Corner; 3]| {
                raster(dims, c, persp, false, |f| {
                    let i = f.i;
                    if t.ground[i] && p.opacity > shade[i] {
                        shade[i] = p.opacity;
                    }
                });
            };
            if persp {
                let poly = clip_square(&pressed, quad[0], quad[2]);
                for k in 2..poly.len() {
                    cover([poly[0], poly[k - 1], poly[k]].map(corner));
                }
            } else {
                cover(pressed.map(corner));
            }
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

/// `w`-weighted sum of three 2-vectors.
fn mix2(w: [f64; 3], v: &[DVec2; 3]) -> DVec2 {
    v[0] * w[0] + v[1] * w[1] + v[2] * w[2]
}

/// Triangle `k` of part `p`: opaque into the depth buffer, or blended
/// over what is drawn, tested against the depth but not writing it.
fn draw_triangle(t: &mut Target, look: &Look, p: &Drawn, k: usize) {
    let dims = (t.width, t.height);
    let q = p.tris[k];
    // Lit from the side the camera sees.
    let toward = match look.cam.eye {
        None => look.toward,
        Some((eye, _)) => eye - q[0],
    };
    let flip = if face_normal(q[0], q[1], q[2]).dot(toward) < 0.0 {
        -1.0
    } else {
        1.0
    };
    let c = [0, 1, 2].map(|i| {
        let n = p.normals[k][i] * flip;
        let light = if look.unlit {
            1.0
        } else {
            AMBIENT + DIFFUSE * n.dot(look.light).max(0.0)
        };
        look.cam.corner(q[i], light)
    });
    let persp = look.cam.eye.is_some();
    let (rgb, alpha, e) = (p.rgb, p.opacity, p.emissive);
    // The lit sRGB component `c × l`, plus the emissive light (none: the
    // product as it was).
    let glow = |c: f64, l: f64, k: usize| {
        if e[k] == 0.0 {
            c * l
        } else {
            to_srgb((to_linear((c * l).clamp(0.0, 1.0)) + e[k]).min(1.0))
        }
    };
    let (depth, ground, out) = (&mut t.depth, &mut t.ground, &mut t.rgb);
    let Some(paint) = &p.paint else {
        raster(dims, c, persp, false, |f| {
            let (i, z, l) = (f.i, f.z, f.light);
            if z >= depth[i] {
                return;
            }
            if alpha == 1.0 {
                depth[i] = z;
                ground[i] = false;
                out[i] = [0, 1, 2].map(|c| byte(glow(rgb[c], l, c)));
            } else {
                let under = out[i];
                out[i] = [0, 1, 2].map(|c| {
                    byte(glow(rgb[c], l, c) * alpha + f64::from(under[c]) / 255.0 * (1.0 - alpha))
                });
            }
        });
        return;
    };
    let map = paint
        .map
        .as_ref()
        .and_then(|(m, uvs)| Some((look.maps.mips(*m)?, &uvs[k])));
    let colors = paint.colors.as_ref().map(|c| &c[k]);
    // A normal map bends the light per pixel: the triangle's tangent
    // frame from its positions and UVs (u along the tangent, v along the
    // bitangent, the image's green up), round the interpolated normal.
    let bump = paint.normal.as_ref().and_then(|(m, uvs, scale)| {
        if look.unlit {
            return None;
        }
        let uv = &uvs[k];
        let (e1, e2) = (q[1] - q[0], q[2] - q[0]);
        let (d1, d2) = (uv[1] - uv[0], uv[2] - uv[0]);
        let det = d1.x * d2.y - d2.x * d1.y;
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let tangent = (e1 * d2.y - e2 * d1.y) / det;
        let bitangent = (e2 * d1.x - e1 * d2.x) / det;
        Some((look.maps.mips(*m)?, uv, *scale, tangent, bitangent))
    });
    let corner_normals = p.normals[k].map(|n| n * flip);
    let gloss = paint.gloss.as_ref();
    let view = toward.try_normalize().unwrap_or(DVec3::Z);
    raster(dims, c, persp, map.is_some() || bump.is_some(), |f| {
        let (i, z, mut l) = (f.i, f.z, f.light);
        if z >= depth[i] {
            return;
        }
        // The normal the light meets here: the map's, where it bends it.
        let mut facing = None;
        if let Some((mips, uv, scale, tangent, bitangent)) = bump {
            let at = mix2(f.w, uv);
            let s = mips.sample(at, 0);
            let n = corner_normals[0] * f.w[0]
                + corner_normals[1] * f.w[1]
                + corner_normals[2] * f.w[2];
            if let (Some(n), true) = (n.try_normalize(), s.iter().all(|v| v.is_finite())) {
                let t = tangent - n * n.dot(tangent);
                if let Some(t) = t.try_normalize() {
                    let hand = if n.cross(t).dot(bitangent) < 0.0 {
                        -1.0
                    } else {
                        1.0
                    };
                    let b = n.cross(t) * hand;
                    let (x, y, zz) = (
                        (s[0] * 2.0 - 1.0) * scale,
                        (s[1] * 2.0 - 1.0) * scale,
                        s[2] * 2.0 - 1.0,
                    );
                    if let Some(bent) = (t * x + b * y + n * zz).try_normalize() {
                        l = AMBIENT + DIFFUSE * bent.dot(look.light).max(0.0);
                        facing = Some(bent);
                    }
                }
            }
        }
        let b = paint.base;
        let mut col = [b[0], b[1], b[2], 1.0];
        if let Some((mips, uv)) = map {
            let at = mix2(f.w, uv);
            let (tw, th) = mips.size();
            let footprint = |w: [f64; 3]| {
                let d = mix2(w, uv) - at;
                let (du, dv) = (d.x * tw, d.y * th);
                du * du + dv * dv
            };
            let rho2 = footprint(f.wx).max(footprint(f.wy));
            let s = mips.sample(at, mips.level(rho2));
            for (c, v) in col.iter_mut().zip(s) {
                *c *= v;
            }
        }
        if let Some(vc) = colors {
            for (ch, c) in col.iter_mut().enumerate() {
                *c *= f.w[0] * vc[0][ch] + f.w[1] * vc[1][ch] + f.w[2] * vc[2][ch];
            }
        }
        let lit: [f64; 3] = match gloss {
            None => {
                let srgb = [0, 1, 2].map(|c| to_srgb(col[c].clamp(0.0, 1.0)));
                [0, 1, 2].map(|c| glow(srgb[c], l, c))
            }
            Some(g) => {
                let at = g.uvs.as_ref().map(|uv| mix2(f.w, &uv[k]));
                let read = |m: Option<usize>| {
                    let s = look.maps.mips(m?)?.sample(at?, 0);
                    s.iter().all(|v| v.is_finite()).then_some(s)
                };
                let (mut metal, mut rough) = (g.metal, g.rough);
                if let Some(s) = read(g.metal_rough) {
                    rough *= s[1];
                    metal *= s[2];
                }
                let shade = match g.occlusion.and_then(|(m, by)| Some((read(Some(m))?, by))) {
                    Some((s, by)) => 1.0 + by * (s[0] - 1.0),
                    None => 1.0,
                };
                let tint = read(g.glow).unwrap_or([1.0; 4]);
                let n = facing.or_else(|| {
                    (corner_normals[0] * f.w[0]
                        + corner_normals[1] * f.w[1]
                        + corner_normals[2] * f.w[2])
                        .try_normalize()
                });
                let towards = n.map_or(0.0, |n| n.dot(look.light).max(0.0));
                if !look.unlit && shade != 1.0 {
                    // Occlusion dims the light from all round, not the sun.
                    l = AMBIENT * shade + DIFFUSE * towards;
                }
                // What is reflected: a highlight of the light, and the
                // sky over the ground, blurred as the surface roughens.
                let mut mirror = [0.0; 3];
                if let (Some(n), false, true) = (n, look.unlit, metal > 0.0 || rough < 1.0) {
                    let smooth = 1.0 - rough;
                    let alpha2 = (rough * rough * rough * rough).max(1e-6);
                    let power = (2.0 / alpha2 - 2.0).clamp(0.0, 2000.0);
                    let half = (look.light + view).try_normalize().unwrap_or(n);
                    let highlight = DIFFUSE
                        * ((power + 2.0) / 8.0 * trig::pow(n.dot(half).max(0.0), power)).min(8.0)
                        * towards
                        * smooth;
                    let back = n * (2.0 * n.dot(view)) - view;
                    let sharp = ENV_DOWN + (ENV_UP - ENV_DOWN) * (0.5 + 0.5 * back.z);
                    let flat = (ENV_DOWN + ENV_UP) / 2.0;
                    let around = (sharp + (flat - sharp) * rough) * shade;
                    let reach = metal + (1.0 - metal) * smooth;
                    for (c, m) in mirror.iter_mut().enumerate() {
                        let f0 = 0.04 * (1.0 - metal) + col[c].clamp(0.0, 1.0) * metal;
                        *m = f0 * (highlight + around * reach);
                    }
                }
                [0, 1, 2].map(|c| {
                    let matte = to_srgb((col[c] * (1.0 - metal)).clamp(0.0, 1.0));
                    let linear =
                        to_linear((matte * l).clamp(0.0, 1.0)) + mirror[c] + e[c] * tint[c];
                    to_srgb(linear.clamp(0.0, 1.0))
                })
            }
        };
        if alpha == 1.0 {
            depth[i] = z;
            ground[i] = false;
            out[i] = lit.map(byte);
        } else {
            let a = alpha * col[3].clamp(0.0, 1.0);
            let under = out[i];
            out[i] = [0, 1, 2].map(|c| byte(lit[c] * a + f64::from(under[c]) / 255.0 * (1.0 - a)));
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
            let s = i as f64 / steps as f64;
            // In perspective, 1 / depth is what varies linearly on screen.
            let z = if cam.eye.is_some() {
                1.0 / ((1.0 - s) / az + s / bz)
            } else {
                az + (bz - az) * s
            };
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
    use crate::image::Texture;

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

    /// A flat quad 40 × 100 in z = 0, x across and y away from a camera at
    /// −Y, UVs u = x, v = y over the whole quad.
    fn floor_quad() -> Solid {
        let mut s = Solid::default();
        for (x, y) in [(-20.0, 0.0), (20.0, 0.0), (20.0, 100.0), (-20.0, 100.0)] {
            s.push_vertex(DVec3::new(x, y, 0.0));
        }
        let uv = |x: f64, y: f64| DVec2::new(x, y);
        s.push_triangle_uv(
            [0, 1, 2],
            0,
            &[],
            [uv(0.0, 0.0), uv(1.0, 0.0), uv(1.0, 1.0)],
        );
        s.push_triangle_uv(
            [0, 2, 3],
            0,
            &[],
            [uv(0.0, 0.0), uv(1.0, 1.0), uv(0.0, 1.0)],
        );
        s
    }

    /// A PNG `w` × `h` from a function of the texel (x, y), rows top down.
    fn map(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Texture {
        let rgba: Vec<u8> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .flat_map(|(x, y)| f(x, y))
            .collect();
        Texture::from_bytes(
            "t.png",
            crate::png(&Image {
                width: w,
                height: h,
                rgba,
            }),
        )
    }

    fn one_view(s: &Solid, material: Material, d: Preview) -> Views {
        let scene = Scene {
            name: "t".into(),
            parts: vec![Part {
                material,
                ..Part::new("p", s)
            }],
        };
        views(
            &scene,
            &Preview {
                views: 1,
                no_edges: true,
                no_ground: true,
                ..d
            },
        )
        .unwrap()
    }

    fn rgb_at(im: &Image, x: u32, y: u32) -> [u8; 3] {
        let i = ((y * im.width + x) * 4) as usize;
        [im.rgba[i], im.rgba[i + 1], im.rgba[i + 2]]
    }

    #[test]
    fn a_map_is_sampled_the_right_way_up_and_perspective_correct() {
        // Near half (v < 0.5: the image's bottom rows) red, far half green.
        let halves = map(4, 256, |_, y| {
            if y >= 128 {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            }
        });
        let s = floor_quad();
        let m = Material {
            map: Some(halves),
            ..Default::default()
        };
        let d = Preview {
            width: 64,
            height: 128,
            elevation: Some(1.2),
            fov: Some(1.0),
            unlit: true,
            ..Default::default()
        };
        let v = one_view(&s, m, d);
        let (im, c) = (&v.images[0], &v.cameras[0]);
        // Where the quad's 3D midpoint (y = 50) lands on screen.
        let fwd = (c.target - c.position).normalize();
        let focal = 64.0 / trig::tan(0.5);
        let row = |p: DVec3| {
            let d = p - c.position;
            64.0 - d.dot(c.up) * focal / d.dot(fwd)
        };
        let (near, mid, far) = (
            row(DVec3::ZERO),
            row(DVec3::new(0.0, 50.0, 0.0)),
            row(DVec3::new(0.0, 100.0, 0.0)),
        );
        // Perspective puts the midpoint well above the screen midpoint.
        assert!(mid < (near + far) / 2.0 - 1.5, "{near} {mid} {far}");
        // The first row up from the near edge that is more green than red.
        let cross = (0..128)
            .rev()
            .find(|&y| {
                let [r, g, _] = rgb_at(im, 32, y);
                g > r
            })
            .unwrap() as f64
            + 1.0;
        // Rows below the crossing are red; an affine (screen-space)
        // interpolation would put it at the screen midpoint, 2 rows lower.
        assert!(
            (cross - mid).abs() < 0.75,
            "crossing at {cross}, midpoint {mid}"
        );
        assert_eq!(rgb_at(im, 32, near as u32 - 2), [255, 0, 0]);
        assert_eq!(rgb_at(im, 32, far as u32 + 2), [0, 255, 0]);
    }

    #[test]
    fn a_fine_map_far_off_is_its_average_not_noise() {
        // A 256² one-texel checker on a quad 40 pixels across: every pixel
        // is the linear mean, sRGB 188, not black or white.
        let fine = map(256, 256, |x, y| {
            if (x + y) % 2 == 0 {
                [0, 0, 0, 255]
            } else {
                [255; 4]
            }
        });
        let s = floor_quad();
        let m = Material {
            map: Some(fine),
            ..Default::default()
        };
        let d = Preview {
            width: 48,
            elevation: Some(trig::PI / 2.0),
            unlit: true,
            ..Default::default()
        };
        let im = &one_view(&s, m, d).images[0];
        for y in 10..38 {
            let [r, ..] = rgb_at(im, 24, y);
            assert!((180..=196).contains(&r), "row {y}: {r}");
        }
    }

    #[test]
    fn vertex_colours_tint_and_white_paint_is_the_plain_colour() {
        let mut s = floor_quad();
        let plain = one_view(&s, Material::default(), Preview::default()).images;
        // A white map and white vertex colours: the same bytes as neither.
        s.colors = Some(vec![[[1.0; 4]; 3]; 2]);
        let white = Material {
            map: Some(map(2, 2, |_, _| [255; 4])),
            ..Default::default()
        };
        assert_eq!(one_view(&s, white, Preview::default()).images, plain);
        // Red corners: red pixels.
        s.colors = Some(vec![[[1.0, 0.0, 0.0, 1.0]; 3]; 2]);
        let im = &one_view(&s, Material::default(), Preview::default()).images[0];
        let [r, g, b] = rgb_at(im, 128, 128);
        assert!(r > 100 && g == 0 && b == 0, "{r} {g} {b}");
        // A map that does not decode is reported and the part drawn plain.
        let bad = Material {
            map: Some(Texture::from_bytes("x.webp", b"RIFF0000WEBP".to_vec())),
            ..Default::default()
        };
        let v = one_view(&floor_quad(), bad, Preview::default());
        assert_eq!(v.images, plain);
        assert_eq!(v.undrawn.len(), 1);
    }

    /// Metal, gloss, occlusion and the emissive map each change the
    /// picture the way they should, and a matte part that is no metal
    /// is drawn as it always was.
    #[test]
    fn metal_gloss_occlusion_and_glow_maps_show() {
        let s = floor_quad();
        let at = |m: Material, d: Preview| rgb_at(&one_view(&s, m, d).images[0], 128, 128);
        let grey = Material {
            color: Some([0.5; 3]),
            ..Default::default()
        };
        let plain = at(grey.clone(), Preview::default());
        // Roughness 1 and no metal, said out loud: the same bytes.
        let said = Material {
            roughness: Some(1.0),
            ..grey.clone()
        };
        assert_eq!(at(said, Preview::default()), plain);
        // A metal is not black: it shows the sky, tinted by its colour.
        let gold = Material {
            color: Some([1.0, 0.8, 0.2]),
            metalness: 1.0,
            ..Default::default()
        };
        let [r, g, b] = at(gold, Preview::default());
        assert!(r > 150 && r > g && g > b && b > 20, "{r} {g} {b}");
        // The same metal polished shows more of the sky above it.
        let rough = Material {
            metalness: 1.0,
            ..grey.clone()
        };
        let polished = Material {
            roughness: Some(0.0),
            ..rough.clone()
        };
        assert!(at(polished, Preview::default())[0] > at(rough, Preview::default())[0]);
        // Occlusion darkens; with intensity 0 it does nothing.
        let dark = map(2, 2, |_, _| [0, 255, 255, 255]);
        let pit = Material {
            ao_map: Some(dark.clone()),
            ..grey.clone()
        };
        assert!(at(pit, Preview::default())[0] < plain[0]);
        let off = Material {
            ao_map: Some(dark),
            ao_map_intensity: Some(0.0),
            ..grey.clone()
        };
        assert_eq!(at(off, Preview::default()), plain);
        // The glow is its colour times its map's: a white glow under a
        // blue map is blue, and black where the map is black.
        let glowing = |texel: [u8; 4]| Material {
            color: Some([0.0; 3]),
            emissive: [1.0; 3],
            emissive_map: Some(map(2, 2, move |_, _| texel)),
            ..Default::default()
        };
        let unlit = Preview {
            unlit: true,
            ..Default::default()
        };
        assert_eq!(at(glowing([0, 0, 255, 255]), unlit), [0, 0, 255]);
        assert_eq!(at(glowing([0, 0, 0, 255]), unlit), [0, 0, 0]);
        // The metalness map's blue channel turns the metal off.
        let no_metal = Material {
            metalness: 1.0,
            metalness_map: Some(map(2, 2, |_, _| [0, 255, 0, 255])),
            ..grey
        };
        assert_eq!(at(no_metal, Preview::default()), plain);
    }

    #[test]
    fn emissive_adds_its_colour_in_shade_and_light() {
        // A black part glowing red is red whatever the light; a dim red
        // glow on a grey part reddens it; no glow is the plain bytes.
        let s = floor_quad();
        let black = Material {
            color: Some([0.0; 3]),
            emissive: [1.0, 0.0, 0.0],
            ..Default::default()
        };
        for unlit in [false, true] {
            let d = Preview {
                unlit,
                ..Default::default()
            };
            let im = &one_view(&s, black.clone(), d).images[0];
            assert_eq!(rgb_at(im, 128, 128), [255, 0, 0]);
        }
        let grey = Material {
            color: Some([0.5; 3]),
            ..Default::default()
        };
        let plain = rgb_at(
            &one_view(&s, grey.clone(), Preview::default()).images[0],
            128,
            128,
        );
        let warm = Material {
            emissive: [0.1, 0.0, 0.0],
            ..grey
        };
        let lit = rgb_at(&one_view(&s, warm, Preview::default()).images[0], 128, 128);
        assert!(lit[0] > plain[0] && lit[1] == plain[1] && lit[2] == plain[2]);
    }

    #[test]
    fn a_normal_map_bends_the_light_per_pixel() {
        // A flat floor whose map leans its left half toward −x and its
        // right half toward +x: the light comes from −x, so the left half
        // is brighter than a flat floor and the right half darker.
        let s = floor_quad();
        let grey = Material {
            color: Some([0.5; 3]),
            ..Default::default()
        };
        let lean = |x: u8| [x, 128, 200, 255];
        let bumped = Material {
            normal_map: Some(map(8, 8, |x, _| if x < 4 { lean(100) } else { lean(155) })),
            ..grey.clone()
        };
        let plain = &one_view(&s, grey.clone(), Preview::default()).images[0];
        let bent = &one_view(&s, bumped.clone(), Preview::default()).images[0];
        let centre = plain.width / 2;
        let row = plain.height / 2;
        let flat = rgb_at(plain, centre, row)[0];
        let (left, right) = (
            rgb_at(bent, centre - 12, row)[0],
            rgb_at(bent, centre + 12, row)[0],
        );
        assert!(left > flat && flat > right, "{left} {flat} {right}");
        // A flat map (128, 128, 255) changes nothing but rounding.
        let level = Material {
            normal_map: Some(map(8, 8, |_, _| [128, 128, 255, 255])),
            ..grey.clone()
        };
        let same = rgb_at(
            &one_view(&s, level, Preview::default()).images[0],
            centre,
            row,
        )[0];
        assert!(same.abs_diff(flat) <= 2, "{same} {flat}");
        // Unlit, the map is not read: colour only.
        let unlit = Preview {
            unlit: true,
            ..Default::default()
        };
        assert_eq!(
            rgb_at(&one_view(&s, bumped, unlit).images[0], centre, row),
            rgb_at(&one_view(&s, grey, unlit).images[0], centre, row)
        );
    }
}

//! Baking a source mesh's look into a low mesh's atlas
//! (`docs/research.md` §34).
//!
//! Entry: [`bake`]. The low mesh carries new UVs (an atlas from
//! [`crate::unwrap_atlas`]); the source carries its own UVs, maps and
//! vertex colours. For every texel of the atlas a low triangle covers:
//!
//! 1. **Coverage.** A texel whose centre lies in a low triangle's UV
//!    triangle belongs to it (exact `orient2d`; a centre on an edge goes
//!    to the triangle the edge runs up or flat-left in, as `measure`'s
//!    coverage rule; on two overlapping triangles, to the lower index). A
//!    texel whose square a triangle overlaps but whose centre no triangle
//!    holds takes the triangle whose UV triangle comes nearest its centre
//!    (conservative rasterisation), at that nearest point.
//! 2. **The low point.** Its position, and the frame a viewer shades it
//!    in: the normal and tangent interpolated from the corners' (as the
//!    plain GLB writes them: `TANGENT`, see below), normalised, the
//!    tangent made square to the normal, the bitangent `w · n × t`.
//! 3. **The source point.** Along the low normal, both ways, up to
//!    `distance`: every source triangle the segment crosses (exact
//!    `orient3d` sides). A crossing counts when the source face's normal
//!    agrees with the low normal (a positive dot), so the inside of a
//!    thin wall is not taken for its outside; the nearest agreeing one is
//!    taken (ties by the side, then the triangle). With none, the nearest
//!    point of the source within `distance`, whichever way it faces. With
//!    neither, the texel has no source: it is counted, and dilation fills
//!    it from its neighbours like the padding. A box tree over the source
//!    triangles finds both (no threads).
//! 4. **Sampling** at the source point, through the source's UVs,
//!    bilinear, repeat wrap: base colour in linear light times the vertex
//!    colour (or the vertex colour alone with no map); metallic-roughness
//!    and occlusion as stored; emissive in linear light (glTF stores it as
//!    sRGB). Where the source triangles' materials differ in their
//!    factors, each texel takes its material's factors too and the baked
//!    material's are neutral (1); otherwise the factors stay on the
//!    material and the maps are baked as they are.
//! 5. **Normals.** The source's shading normal (its smooth normal, with
//!    its normal map applied in its own frame, built the same way, at its
//!    `normal_scale`) in the low frame, `c = (s·t, s·b, s·n)`, written as
//!    the bytes `255 (c + 1) / 2` rounded. Baked from the source's normals
//!    alone when it has no normal map, so a dense mesh's shape survives
//!    decimation as shading.
//! 6. **Dilation.** Layer by layer, an empty texel next (8-neighbours) to
//!    filled ones takes their mean, until `dilate` layers or every texel is
//!    filled.
//!
//! **The tangent convention.** Per vertex of the plain GLB (one per
//! position, surface, UV and colour), the sum over its triangles of
//! ∂P/∂u and ∂P/∂v times their UV area, the tangent made square to the
//! normal and `w` the side ∂P/∂v falls on (`surface::corner_frames`). It
//! is the standard per-vertex accumulation, not MikkTSpace; the GLB
//! carries it as `TANGENT` ([`crate::PlainGlb::tangents`]), which a glTF
//! viewer uses instead of its own.
//!
//! f64 `+ − × /` and `sqrt` for positions, `trig::acos` for the angle
//! measures only, integer texel arithmetic and exact predicates for every
//! side: the PNG bytes are the same on every host.

use core::cmp::Ordering;
use std::fmt;

use shape_core::predicates::{orient2d, orient3d};
use shape_core::{DVec2, DVec3, trig};
use shape_mesh::Solid;

use crate::export::{ColorSpace, Material};
use crate::image::Texture;
use crate::intersect::tris_overlap_2d;
use crate::json::Json;
use crate::measure::takes;
use crate::preview::Image;
use crate::sampler::{Linear, linear_table, to_linear};
use crate::surface::{CornerFrame, corner_frames};

/// How to bake. The zero value bakes 1024² maps, looks 2 % of the
/// source's diagonal either way along the low normal, fills every empty
/// texel by dilation and bakes normals.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bake {
    /// Texels across each square map; 0 is 1024. The low mesh's UVs
    /// should be packed at this size ([`crate::Atlas::size`]).
    pub size: u32,
    /// How far from the low surface a source point may be, in mm; 0 is
    /// 2 % of the source's bounding-box diagonal.
    pub distance: f64,
    /// Layers of dilation past the covered texels; 0 fills every texel.
    pub dilate: u32,
    /// No normal map.
    pub no_normals: bool,
}

/// What to bake from: the source solid, its materials, and which
/// material each triangle has.
#[derive(Clone, Copy, Debug)]
pub struct BakeSource<'a> {
    /// The source, with its UVs (when it has maps) and vertex colours.
    pub solid: &'a Solid,
    /// Its materials; empty is three.js's default.
    pub materials: &'a [Material],
    /// Per triangle, its index into `materials`; empty is all 0.
    pub material_of: &'a [u32],
}

/// One map baked.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BakedMap {
    /// Its kind ([`crate::MAP_KINDS`]).
    pub kind: &'static str,
    /// Texels across and down.
    pub size: u32,
    /// Bytes of its PNG.
    pub file_bytes: usize,
}

/// The shading normals' difference, in degrees, over the covered texels
/// that found a source.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NormalMeasures {
    /// Between the source's shading normal and the low mesh's with the
    /// baked map (as a viewer decodes its bytes in the written frame).
    pub mean: f64,
    /// Its 95th percentile (to 0.01°).
    pub p95: f64,
    /// Between the source's shading normal and the low mesh's own
    /// normal, without a map.
    pub mean_without: f64,
    /// Its 95th percentile.
    pub p95_without: f64,
}

/// What a bake did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BakeReport {
    /// Texels across.
    pub size: u32,
    /// The search distance used, mm.
    pub distance: f64,
    /// Texels a low triangle covers (centre inside, or square overlapped).
    pub covered: usize,
    /// Of those, the ones whose centre no triangle holds (conservative).
    pub conservative: usize,
    /// Covered texels sampled where the normal line crossed the source.
    pub hits: usize,
    /// Covered texels sampled at the nearest source point instead (no
    /// agreeing crossing within the distance).
    pub nearest: usize,
    /// Covered texels with no source within the distance: filled by
    /// dilation, like the padding.
    pub no_source: usize,
    /// Texels where the nearest crossing faced away from the low normal,
    /// so a farther one (or the nearest point) was taken.
    pub facing_away: usize,
    /// Texels with agreeing crossings on both sides of the low surface
    /// (two sheets near): the nearer was taken.
    pub ambiguous: usize,
    /// The largest distance from a texel's low point to its source point,
    /// mm.
    pub largest_distance: f64,
    /// Texels filled by dilation (each map the same).
    pub dilated: usize,
    /// Texels left empty (dilation stopped short): transparent black.
    pub empty: usize,
    /// The maps written, in [`crate::MAP_KINDS`] order.
    pub maps: Vec<BakedMap>,
    /// The source's factors went into the texels (its materials differ).
    pub factors_baked: bool,
    /// The source's vertex colours went into the base-colour map: the low
    /// mesh must not carry them as well.
    pub colors_baked: bool,
    /// What could not be baked, and why.
    pub not_baked: Vec<String>,
    /// The normal map's measures, when one was baked.
    pub normals: Option<NormalMeasures>,
}

/// The baked material and what was done.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Baked {
    /// The material for the low mesh: the source's factors (or neutral
    /// ones when they were baked into the texels) and the baked maps, as
    /// PNG.
    pub material: Material,
    /// The report.
    pub report: BakeReport,
}

/// Why a bake could not run.
#[derive(Clone, Debug, PartialEq)]
pub enum BakeError {
    /// A size, distance or index out of range (which).
    BadDesc(String),
    /// The low mesh has no UVs to bake into.
    NoUvs,
    /// The source or the low mesh has no triangles.
    Empty,
}

impl fmt::Display for BakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BakeError::BadDesc(w) => write!(f, "bake: {w}"),
            BakeError::NoUvs => f.write_str("bake: the low mesh has no UVs to bake into"),
            BakeError::Empty => f.write_str("bake: no triangles"),
        }
    }
}

impl std::error::Error for BakeError {}

// ── The source's box tree ──────────────────────────────────────────────────

const LEAF: usize = 4;

struct Node {
    min: DVec3,
    max: DVec3,
    /// Triangles `order[lo..hi]` (a leaf), or children `kids`, `kids + 1`.
    lo: u32,
    hi: u32,
    kids: u32,
}

/// A bounding-box hierarchy over a solid's triangles: median splits along
/// the longest axis of the centroids (ties by index).
struct Tree<'a> {
    s: &'a Solid,
    order: Vec<u32>,
    nodes: Vec<Node>,
}

fn tri(s: &Solid, i: u32) -> [DVec3; 3] {
    s.triangles[i as usize].map(|v| s.positions[v as usize])
}

impl<'a> Tree<'a> {
    fn new(s: &'a Solid) -> Tree<'a> {
        let n = s.triangles.len();
        let cen: Vec<DVec3> = (0..n as u32)
            .map(|i| {
                let [a, b, c] = tri(s, i);
                (a + b + c) / 3.0
            })
            .collect();
        let mut order: Vec<u32> = (0..n as u32).collect();
        let mut nodes = vec![Node {
            min: DVec3::ZERO,
            max: DVec3::ZERO,
            lo: 0,
            hi: n as u32,
            kids: 0,
        }];
        let mut stack = vec![0usize];
        while let Some(k) = stack.pop() {
            let (lo, hi) = (nodes[k].lo as usize, nodes[k].hi as usize);
            let (mut mn, mut mx) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
            for &i in &order[lo..hi] {
                for p in tri(s, i) {
                    mn = mn.min(p);
                    mx = mx.max(p);
                }
            }
            nodes[k].min = mn;
            nodes[k].max = mx;
            if hi - lo <= LEAF {
                continue;
            }
            let (mut cmn, mut cmx) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
            for &i in &order[lo..hi] {
                cmn = cmn.min(cen[i as usize]);
                cmx = cmx.max(cen[i as usize]);
            }
            let e = cmx - cmn;
            let axis = if e.x >= e.y && e.x >= e.z {
                0
            } else if e.y >= e.z {
                1
            } else {
                2
            };
            shape_core::sort::sort_by(&mut order[lo..hi], |&a, &b| {
                cen[a as usize][axis]
                    .total_cmp(&cen[b as usize][axis])
                    .then(a.cmp(&b))
            });
            let mid = (lo + (hi - lo) / 2) as u32;
            let kids = nodes.len() as u32;
            nodes[k].kids = kids;
            for (a, b) in [(lo as u32, mid), (mid, hi as u32)] {
                nodes.push(Node {
                    min: DVec3::ZERO,
                    max: DVec3::ZERO,
                    lo: a,
                    hi: b,
                    kids: 0,
                });
            }
            stack.push(kids as usize + 1);
            stack.push(kids as usize);
        }
        Tree { s, order, nodes }
    }

    /// Every triangle the segment `a → b` crosses or touches (exact), with
    /// the crossing's parameter along it (0 at `a`, 1 at `b`).
    fn crossings(&self, a: DVec3, b: DVec3, out: &mut Vec<(u32, f64)>) {
        out.clear();
        let d = b - a;
        // The boxes only prune; they are widened so that rounding in the
        // slab test never drops a crossing the exact test would find.
        let pad = |n: &Node| (n.max - n.min).max_element() * 1e-9 + 1e-12;
        let mut stack = vec![0u32];
        while let Some(k) = stack.pop() {
            let n = &self.nodes[k as usize];
            let e = pad(n);
            if !segment_box(a, d, n.min - DVec3::splat(e), n.max + DVec3::splat(e)) {
                continue;
            }
            if n.kids == 0 {
                for &i in &self.order[n.lo as usize..n.hi as usize] {
                    if let Some(t) = crossing(a, b, tri(self.s, i)) {
                        out.push((i, t));
                    }
                }
            } else {
                stack.push(n.kids + 1);
                stack.push(n.kids);
            }
        }
    }

    /// The nearest point of the solid to `p` within `limit` (by squared
    /// distance, ties by triangle): its triangle, the point and the
    /// squared distance.
    fn nearest(&self, p: DVec3, limit: f64) -> Option<(u32, DVec3, f64)> {
        let mut best: Option<(u32, DVec3, f64)> = None;
        let mut bound = limit * limit;
        let mut stack = vec![0u32];
        while let Some(k) = stack.pop() {
            let n = &self.nodes[k as usize];
            if box_dist2(p, n.min, n.max) > bound {
                continue;
            }
            if n.kids == 0 {
                for &i in &self.order[n.lo as usize..n.hi as usize] {
                    let q = closest_on_triangle(p, tri(self.s, i));
                    let d2 = (q - p).length_squared();
                    let better = match best {
                        None => d2 <= bound,
                        Some((bi, _, bd)) => d2 < bd || (d2 == bd && i < bi),
                    };
                    if better {
                        best = Some((i, q, d2));
                        bound = d2;
                    }
                }
            } else {
                let (l, r) = (n.kids, n.kids + 1);
                let dl = box_dist2(p, self.nodes[l as usize].min, self.nodes[l as usize].max);
                let dr = box_dist2(p, self.nodes[r as usize].min, self.nodes[r as usize].max);
                // The nearer child is visited first.
                if dl <= dr {
                    stack.push(r);
                    stack.push(l);
                } else {
                    stack.push(l);
                    stack.push(r);
                }
            }
        }
        best
    }
}

fn box_dist2(p: DVec3, min: DVec3, max: DVec3) -> f64 {
    let q = p.clamp(min, max);
    (q - p).length_squared()
}

/// Whether the segment `a + t d`, t in [0, 1], meets the box (slabs).
fn segment_box(a: DVec3, d: DVec3, min: DVec3, max: DVec3) -> bool {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for k in 0..3 {
        if d[k] == 0.0 {
            if a[k] < min[k] || a[k] > max[k] {
                return false;
            }
            continue;
        }
        let (mut u, mut v) = ((min[k] - a[k]) / d[k], (max[k] - a[k]) / d[k]);
        if u > v {
            core::mem::swap(&mut u, &mut v);
        }
        t0 = t0.max(u);
        t1 = t1.min(v);
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// The segment `a → b` crosses or touches triangle `t` (exact sides by
/// `orient3d`): its parameter, or `None`. A segment in the triangle's
/// plane crosses nothing.
fn crossing(a: DVec3, b: DVec3, t: [DVec3; 3]) -> Option<f64> {
    let sa = orient3d(t[0], t[1], t[2], a);
    let sb = orient3d(t[0], t[1], t[2], b);
    if sa == sb {
        // Both on one side, or both in the plane.
        return None;
    }
    // The line a–b passes the triangle's edges all on one side.
    let e = [
        orient3d(a, b, t[0], t[1]),
        orient3d(a, b, t[1], t[2]),
        orient3d(a, b, t[2], t[0]),
    ];
    let pos = e.contains(&Ordering::Greater);
    let neg = e.contains(&Ordering::Less);
    if pos && neg {
        return None;
    }
    let n = (t[1] - t[0]).cross(t[2] - t[0]);
    let (da, db) = (n.dot(a - t[0]), n.dot(b - t[0]));
    let tt = if da == db { 0.5 } else { da / (da - db) };
    Some(tt.clamp(0.0, 1.0))
}

/// The point of triangle `t` nearest `p` (Ericson's regions).
fn closest_on_triangle(p: DVec3, t: [DVec3; 3]) -> DVec3 {
    let [a, b, c] = t;
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return a + ab * v;
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return a + ac * w;
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return b + (c - b) * w;
    }
    let denom = va + vb + vc;
    if denom == 0.0 {
        return a;
    }
    let (v, w) = (vb / denom, vc / denom);
    a + ab * v + ac * w
}

/// Barycentric weights of `p` (on or near the triangle's plane) in `t`,
/// clamped to the triangle; a degenerate triangle gives its first corner.
fn bary3(p: DVec3, t: [DVec3; 3]) -> [f64; 3] {
    let (v0, v1, v2) = (t[1] - t[0], t[2] - t[0], p - t[0]);
    let (d00, d01, d11) = (v0.dot(v0), v0.dot(v1), v1.dot(v1));
    let (d20, d21) = (v2.dot(v0), v2.dot(v1));
    let den = d00 * d11 - d01 * d01;
    if den <= 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let v = ((d11 * d20 - d01 * d21) / den).clamp(0.0, 1.0);
    let w = ((d00 * d21 - d01 * d20) / den).clamp(0.0, 1.0 - v);
    [1.0 - v - w, v, w]
}

/// Barycentric weights of `p` in the 2D triangle `t` (unclamped); a
/// degenerate triangle gives its first corner.
fn bary2(p: DVec2, t: [DVec2; 3]) -> [f64; 3] {
    let (v0, v1, v2) = (t[1] - t[0], t[2] - t[0], p - t[0]);
    let den = v0.perp_dot(v1);
    if den == 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let v = v2.perp_dot(v1) / den;
    let w = v0.perp_dot(v2) / den;
    [1.0 - v - w, v, w]
}

/// The point of the 2D triangle `t` nearest `p`.
fn closest_2d(p: DVec2, t: [DVec2; 3]) -> DVec2 {
    let lift = |q: DVec2| DVec3::new(q.x, q.y, 0.0);
    let q = closest_on_triangle(lift(p), t.map(lift));
    DVec2::new(q.x, q.y)
}

// ── Maps ──────────────────────────────────────────────────────────────────

/// A decoded map, sampled bilinear with repeat wrap; colour channels in
/// linear light.
struct Map {
    image: Image,
    color: bool,
}

impl Map {
    /// RGBA in 0..=1 at `uv` (colour linear when the map holds colour):
    /// the texel grid's centres at (i + ½) / width, v = 0 at the bottom
    /// row, weights `floor(256 × fraction)`, integer sums.
    fn sample(&self, lin: &Linear, uv: DVec2) -> [f64; 4] {
        let l = &self.image;
        let (w, h) = (i64::from(l.width), i64::from(l.height));
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
                    let v = if self.color && c < 3 {
                        u64::from(lin.get(b))
                    } else {
                        u64::from(b) * 257
                    };
                    v * wt
                })
                .sum();
            *o = (s / 65536) as f64 / 65535.0;
        }
        out
    }
}

/// Linear light in 0..=1 to an sRGB byte (nearest by the table).
fn srgb_byte(lin: &Linear, v: f64) -> u8 {
    let v = if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    };
    lin.encode((v * 65535.0 + 0.5).floor() as u32)
}

/// 0..=1 to a byte as stored.
fn data_byte(v: f64) -> u8 {
    let v = if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        0.0
    };
    (v * 255.0 + 0.5).floor() as u8
}

fn unit(v: DVec3) -> DVec3 {
    let l = v.length();
    if l > 0.0 { v / l } else { DVec3::ZERO }
}

/// A material's factors in linear light.
struct Factors {
    color: [f64; 4],
    metalness: f64,
    roughness: f64,
    emissive: [f64; 3],
    ao: f64,
    normal_scale: f64,
}

fn factors(m: &Material) -> Factors {
    let c = m.color.unwrap_or([1.0; 3]);
    let lin = |v: f64| match m.color_space {
        ColorSpace::Linear => v,
        ColorSpace::Srgb => to_linear(v.clamp(0.0, 1.0)),
    };
    let ei = m.emissive_intensity.unwrap_or(1.0);
    Factors {
        color: [lin(c[0]), lin(c[1]), lin(c[2]), m.opacity.unwrap_or(1.0)],
        metalness: m.metalness,
        roughness: m.roughness.unwrap_or(1.0),
        emissive: m.emissive.map(|e| lin(e) * ei),
        ao: m.ao_map_intensity.unwrap_or(1.0),
        normal_scale: m.normal_scale.unwrap_or(1.0),
    }
}

/// A material without its name and maps: what decides whether the
/// factors can stay on the material.
fn bare(m: &Material) -> Material {
    Material {
        name: String::new(),
        map: None,
        metalness_map: None,
        normal_map: None,
        emissive_map: None,
        ao_map: None,
        ..m.clone()
    }
}

/// Bakes `src`'s look into `low`'s UV atlas (the module's rule). The low
/// mesh is not changed: when the report says `colors_baked`, write it
/// without its vertex colours, and with `TANGENT` when a normal map was
/// baked.
pub fn bake(src: &BakeSource, low: &Solid, d: &Bake) -> Result<Baked, BakeError> {
    let size = if d.size == 0 { 1024 } else { d.size };
    if size > 8192 {
        return Err(BakeError::BadDesc("size must be at most 8192".into()));
    }
    if !(d.distance >= 0.0 && d.distance.is_finite()) {
        return Err(BakeError::BadDesc("distance must be 0 or positive".into()));
    }
    let s = src.solid;
    if s.triangles.is_empty() || low.triangles.is_empty() {
        return Err(BakeError::Empty);
    }
    let Some(low_uv) = low.uvs.as_ref() else {
        return Err(BakeError::NoUvs);
    };
    if low_uv.len() != low.triangles.len() {
        return Err(BakeError::BadDesc(
            "the low mesh's UVs are for another number of triangles".into(),
        ));
    }
    let default = [Material::default()];
    let materials: &[Material] = if src.materials.is_empty() {
        &default
    } else {
        src.materials
    };
    if !src.material_of.is_empty() && src.material_of.len() != s.triangles.len() {
        return Err(BakeError::BadDesc(
            "material_of must have one entry per source triangle".into(),
        ));
    }
    if src
        .material_of
        .iter()
        .any(|&m| m as usize >= materials.len())
    {
        return Err(BakeError::BadDesc(
            "a material index is out of range".into(),
        ));
    }
    let mat_of = |i: usize| src.material_of.get(i).map_or(0, |&m| m as usize);
    let used: Vec<usize> = {
        let mut u: Vec<usize> = (0..s.triangles.len()).map(mat_of).collect();
        u.sort_unstable();
        u.dedup();
        u
    };
    let fold = used
        .iter()
        .any(|&m| bare(&materials[m]) != bare(&materials[used[0]]));
    let first = &materials[used[0]];
    let mut report = BakeReport {
        size,
        factors_baked: fold,
        ..Default::default()
    };
    // Decode each material's maps; a map that cannot be decoded is left
    // out, and so is its kind (it is not guessed from the others).
    let src_uv = s.uvs.as_ref().filter(|u| u.len() == s.triangles.len());
    let mut decoded: Vec<[Option<Map>; 5]> = Vec::with_capacity(materials.len());
    let mut kind_failed = [false; 5];
    for (mi, m) in materials.iter().enumerate() {
        let slots = m.map_slots();
        let mut row: [Option<Map>; 5] = Default::default();
        for (k, (kind, t)) in slots.iter().enumerate() {
            let Some(t) = t else { continue };
            if !used.contains(&mi) {
                continue;
            }
            if src_uv.is_none() {
                report.not_baked.push(format!(
                    "{kind} {:?}: the source has no UVs to read it through",
                    t.name
                ));
                kind_failed[k] = true;
                continue;
            }
            match crate::texture::decode(&t.bytes) {
                Ok(image) => {
                    row[k] = Some(Map {
                        image,
                        color: Material::is_color_map(kind),
                    });
                }
                Err(e) => {
                    report.not_baked.push(format!("{kind} {:?}: {e}", t.name));
                    kind_failed[k] = true;
                }
            }
        }
        decoded.push(row);
    }
    let any_map = |k: usize| used.iter().any(|&m| decoded[m][k].is_some());
    let colors = s.colors.as_ref().filter(|c| c.len() == s.triangles.len());
    // Which maps to make: [base, metallic-roughness, normal, emissive, ao].
    let make = [
        !kind_failed[0] && (any_map(0) || colors.is_some() || fold),
        !kind_failed[1] && (any_map(1) || fold),
        !d.no_normals,
        !kind_failed[3]
            && (any_map(3) || (fold && used.iter().any(|&m| materials[m].emissive != [0.0; 3]))),
        !kind_failed[4] && any_map(4),
    ];
    for (k, failed) in kind_failed.iter().enumerate() {
        if *failed && k != 2 {
            report.not_baked.push(format!(
                "{}: not baked; the low mesh's material has none",
                crate::export::MAP_KINDS[k]
            ));
        }
    }
    if kind_failed[2] && make[2] {
        report.not_baked.push(
            "normal_map: the source's normal map is not applied; normals baked from its geometry"
                .into(),
        );
    }
    report.colors_baked = make[0] && colors.is_some();
    let fac: Vec<Factors> = materials.iter().map(factors).collect();
    // The search distance.
    let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
    for p in &s.positions {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let reach = if d.distance > 0.0 {
        d.distance
    } else {
        (hi - lo).length() * 0.02
    };
    report.distance = reach;
    let tree = Tree::new(s);
    let src_frames = corner_frames(s);
    let low_frames = corner_frames(low);
    let lin = linear_table();
    let n = size as usize;
    let mut images: Vec<Option<Vec<u8>>> = make
        .iter()
        .map(|&m| m.then(|| vec![0u8; n * n * 4]))
        .collect();
    // Owner of each texel, and its point in the owner's UV triangle.
    let (owner, at) = rasterize(low_uv, size);
    let mut filled = vec![false; n * n];
    let mut hist = vec![0u32; 18001];
    let mut hist_without = vec![0u32; 18001];
    let (mut sum, mut sum_without, mut count) = (0.0, 0.0, 0usize);
    let mut hits: Vec<(u32, f64)> = Vec::new();
    for texel in 0..n * n {
        let li = owner[texel];
        if li == u32::MAX {
            continue;
        }
        report.covered += 1;
        let (b, conservative) = at[texel];
        if conservative {
            report.conservative += 1;
        }
        let lt = tri(low, li);
        let p = lt[0] * b[0] + lt[1] * b[1] + lt[2] * b[2];
        let lf = frame_at(&low_frames[li as usize], b);
        if lf.normal == DVec3::ZERO {
            report.no_source += 1;
            continue;
        }
        // The source point.
        let (a, z) = (p - lf.normal * reach, p + lf.normal * reach);
        tree.crossings(a, z, &mut hits);
        let mut best: Option<(f64, f64, u32)> = None;
        let mut nearest_any: Option<(f64, bool)> = None;
        let (mut front, mut back) = (false, false);
        for &(j, t) in &hits {
            let sd = (t * 2.0 - 1.0) * reach;
            let [q0, q1, q2] = tri(s, j);
            let agree = (q1 - q0).cross(q2 - q0).dot(lf.normal) > 0.0;
            if nearest_any.is_none_or(|(d0, _)| sd.abs() < d0) {
                nearest_any = Some((sd.abs(), agree));
            }
            if !agree {
                continue;
            }
            if sd > 0.0 {
                front = true;
            } else if sd < 0.0 {
                back = true;
            }
            let key = (sd.abs(), sd, j);
            if best.is_none_or(|bb| {
                (key.0, key.1).partial_cmp(&(bb.0, bb.1)) == Some(Ordering::Less)
                    || (key.0 == bb.0 && key.1 == bb.1 && key.2 < bb.2)
            }) {
                best = Some(key);
            }
        }
        if nearest_any.is_some_and(|(d0, agree)| !agree && best.is_none_or(|bb| d0 < bb.0)) {
            report.facing_away += 1;
        }
        if front && back {
            report.ambiguous += 1;
        }
        let (j, q, dist) = if let Some((dd, sd, j)) = best {
            report.hits += 1;
            (j, p + lf.normal * sd, dd)
        } else if let Some((j, q, d2)) = tree.nearest(p, reach) {
            report.nearest += 1;
            (j, q, d2.sqrt())
        } else {
            report.no_source += 1;
            continue;
        };
        report.largest_distance = report.largest_distance.max(dist);
        filled[texel] = true;
        let st = tri(s, j);
        let w = bary3(q, st);
        let ji = j as usize;
        let mi = mat_of(ji);
        let f = &fac[mi];
        let uv = src_uv.map(|u| u[ji][0] * w[0] + u[ji][1] * w[1] + u[ji][2] * w[2]);
        let sample = |k: usize| -> Option<[f64; 4]> {
            let m = decoded[mi][k].as_ref()?;
            Some(m.sample(&lin, uv?))
        };
        let px = {
            let (x, y) = (texel % n, texel / n);
            ((n - 1 - y) * n + x) * 4
        };
        if let Some(img) = &mut images[0] {
            let mut c = sample(0).unwrap_or([1.0; 4]);
            if let Some(col) = colors {
                let cc = col[ji];
                for (k, v) in c.iter_mut().enumerate() {
                    *v *= cc[0][k] * w[0] + cc[1][k] * w[1] + cc[2][k] * w[2];
                }
            }
            if fold {
                for (k, v) in c.iter_mut().enumerate() {
                    *v *= f.color[k];
                }
            }
            for k in 0..3 {
                img[px + k] = srgb_byte(&lin, c[k]);
            }
            img[px + 3] = data_byte(c[3]);
        }
        if let Some(img) = &mut images[1] {
            let mut c = sample(1).unwrap_or([1.0; 4]);
            if fold {
                c[1] *= f.roughness;
                c[2] *= f.metalness;
            }
            for k in 0..4 {
                img[px + k] = data_byte(c[k]);
            }
        }
        if let Some(img) = &mut images[3] {
            let mut c = sample(3).unwrap_or([1.0; 4]);
            if fold {
                for (k, v) in c.iter_mut().take(3).enumerate() {
                    *v *= f.emissive[k];
                }
            }
            for k in 0..3 {
                img[px + k] = srgb_byte(&lin, c[k]);
            }
            img[px + 3] = 255;
        }
        if let Some(img) = &mut images[4] {
            let mut c = sample(4).unwrap_or([1.0; 4]);
            if fold {
                c[0] = 1.0 + f.ao * (c[0] - 1.0);
            }
            for k in 0..4 {
                img[px + k] = data_byte(c[k]);
            }
        }
        if let Some(img) = &mut images[2] {
            let sf = frame_at(&src_frames[ji], w);
            let mut shade = sf.normal;
            if let Some(m) = sample(2) {
                let x = (m[0] * 2.0 - 1.0) * f.normal_scale;
                let y = (m[1] * 2.0 - 1.0) * f.normal_scale;
                let z = m[2] * 2.0 - 1.0;
                let bt = sf.normal.cross(sf.tangent) * sf.w;
                let v = unit(sf.tangent * x + bt * y + sf.normal * z);
                if v != DVec3::ZERO {
                    shade = v;
                }
            }
            if shade == DVec3::ZERO {
                shade = lf.normal;
            }
            let lb = lf.normal.cross(lf.tangent) * lf.w;
            let c = [shade.dot(lf.tangent), shade.dot(lb), shade.dot(lf.normal)];
            let bytes = c.map(|v| data_byte((v + 1.0) / 2.0));
            img[px..px + 3].copy_from_slice(&bytes);
            img[px + 3] = 255;
            // As a viewer decodes it, in the written frame.
            let dec = bytes.map(|v| f64::from(v) * 2.0 / 255.0 - 1.0);
            let seen = unit(lf.tangent * dec[0] + lb * dec[1] + lf.normal * dec[2]);
            let ang = |a: DVec3, b: DVec3| trig::to_degrees(trig::acos(a.dot(b).clamp(-1.0, 1.0)));
            let (e, e0) = (ang(seen, shade), ang(lf.normal, shade));
            sum += e;
            sum_without += e0;
            count += 1;
            hist[((e * 100.0 + 0.5).floor() as usize).min(18000)] += 1;
            hist_without[((e0 * 100.0 + 0.5).floor() as usize).min(18000)] += 1;
        }
    }
    if make[2] && count > 0 {
        let p95 = |h: &[u32]| {
            let need = (count as u64 * 95).div_ceil(100);
            let mut acc = 0u64;
            for (k, &c) in h.iter().enumerate() {
                acc += u64::from(c);
                if acc >= need {
                    return k as f64 / 100.0;
                }
            }
            180.0
        };
        report.normals = Some(NormalMeasures {
            mean: sum / count as f64,
            p95: p95(&hist),
            mean_without: sum_without / count as f64,
            p95_without: p95(&hist_without),
        });
    }
    // Dilation, the same layers for every map.
    let (level, layers) = dilation(&filled, n, d.dilate);
    report.dilated = layers.iter().map(Vec::len).sum();
    report.empty = n * n - filled.iter().filter(|&&f| f).count() - report.dilated;
    let mut material = if fold {
        Material {
            color: None,
            color_space: ColorSpace::Linear,
            metalness: 1.0,
            roughness: Some(1.0),
            emissive: if make[3] { [1.0; 3] } else { [0.0; 3] },
            emissive_intensity: None,
            ao_map_intensity: None,
            ..bare(first)
        }
    } else {
        bare(first)
    };
    material.normal_scale = None;
    for (k, img) in images.into_iter().enumerate() {
        let Some(mut rgba) = img else { continue };
        dilate(&mut rgba, n, &level, &layers);
        let kind = crate::export::MAP_KINDS[k];
        let png = crate::png::png(&Image {
            width: size,
            height: size,
            rgba,
        });
        report.maps.push(BakedMap {
            kind,
            size,
            file_bytes: png.len(),
        });
        let t = Texture {
            name: format!("baked_{kind}.png"),
            mime: "image/png".into(),
            bytes: png,
        };
        match k {
            0 => material.map = Some(t),
            1 => material.metalness_map = Some(t),
            2 => material.normal_map = Some(t),
            3 => material.emissive_map = Some(t),
            _ => material.ao_map = Some(t),
        }
    }
    Ok(Baked { material, report })
}

/// The frame at barycentric `b` of a triangle's corner frames: normal and
/// tangent interpolated and normalised, the tangent made square to the
/// normal, `w` the sign of the interpolated handedness (+1 on a tie).
fn frame_at(f: &[CornerFrame; 3], b: [f64; 3]) -> CornerFrame {
    let n = unit(f[0].normal * b[0] + f[1].normal * b[1] + f[2].normal * b[2]);
    let t = f[0].tangent * b[0] + f[1].tangent * b[1] + f[2].tangent * b[2];
    let mut t = unit(t - n * n.dot(t));
    if t == DVec3::ZERO {
        t = f[0].tangent;
    }
    let w = f[0].w * b[0] + f[1].w * b[1] + f[2].w * b[2];
    CornerFrame {
        normal: n,
        tangent: t,
        w: if w < 0.0 { -1.0 } else { 1.0 },
    }
}

/// Each texel's owning triangle (`u32::MAX` for none) and its barycentric
/// weights there, with whether it was taken conservatively (by its square,
/// at the nearest point of the triangle to its centre). Texel `y · size +
/// x`, rows from the bottom (v up).
#[allow(clippy::type_complexity)]
fn rasterize(uv: &[[DVec2; 3]], size: u32) -> (Vec<u32>, Vec<([f64; 3], bool)>) {
    let n = size as usize;
    let sz = f64::from(size);
    let mut owner = vec![u32::MAX; n * n];
    let mut at = vec![([0.0; 3], false); n * n];
    let span = |a: f64, b: f64, grow: f64| {
        let x0 = (a * sz - 0.5 - grow).ceil().max(0.0) as i64;
        let x1 = (b * sz - 0.5 + grow).floor().min(sz - 1.0) as i64;
        (x0, x1)
    };
    // Centres inside, exact, the top-left rule on edges, the lower index
    // first on an overlap.
    for (i, t) in uv.iter().enumerate() {
        let t = match orient2d(t[0], t[1], t[2]) {
            Ordering::Greater => *t,
            Ordering::Less => [t[0], t[2], t[1]],
            Ordering::Equal => continue,
        };
        let lo = t[0].min(t[1]).min(t[2]);
        let hi = t[0].max(t[1]).max(t[2]);
        let (x0, x1) = span(lo.x, hi.x, 0.0);
        let (y0, y1) = span(lo.y, hi.y, 0.0);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let k = y as usize * n + x as usize;
                if owner[k] != u32::MAX {
                    continue;
                }
                let p = DVec2::new((x as f64 + 0.5) / sz, (y as f64 + 0.5) / sz);
                let inside = (0..3).all(|e| {
                    let (a, b) = (t[e], t[(e + 1) % 3]);
                    match orient2d(a, b, p) {
                        Ordering::Greater => true,
                        Ordering::Equal => takes(a, b),
                        Ordering::Less => false,
                    }
                });
                if inside {
                    owner[k] = i as u32;
                    at[k] = (bary2(p, uv[i]), false);
                }
            }
        }
    }
    // Squares overlapped, centres held by none: the nearest triangle.
    let mut near = vec![f64::INFINITY; n * n];
    let mut best: Vec<(usize, u32, DVec2)> = Vec::new();
    let mut slot = vec![u32::MAX; n * n];
    for (i, t) in uv.iter().enumerate() {
        if orient2d(t[0], t[1], t[2]) == Ordering::Equal {
            continue;
        }
        let lo = t[0].min(t[1]).min(t[2]);
        let hi = t[0].max(t[1]).max(t[2]);
        let (x0, x1) = span(lo.x, hi.x, 1.0);
        let (y0, y1) = span(lo.y, hi.y, 1.0);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let k = y as usize * n + x as usize;
                if owner[k] != u32::MAX {
                    continue;
                }
                let (fx, fy) = (x as f64 / sz, y as f64 / sz);
                let e = 1.0 / sz;
                let w = DVec2::new;
                let sq1 = [w(fx, fy), w(fx + e, fy), w(fx + e, fy + e)];
                let sq2 = [w(fx, fy), w(fx + e, fy + e), w(fx, fy + e)];
                if !(tris_overlap_2d(*t, sq1) || tris_overlap_2d(*t, sq2)) {
                    continue;
                }
                let c = w(fx + e / 2.0, fy + e / 2.0);
                let q = closest_2d(c, *t);
                let d2 = (q - c).length_squared();
                if d2 < near[k] {
                    near[k] = d2;
                    if slot[k] == u32::MAX {
                        slot[k] = best.len() as u32;
                        best.push((k, i as u32, q));
                    } else {
                        best[slot[k] as usize] = (k, i as u32, q);
                    }
                }
            }
        }
    }
    for (k, i, q) in best {
        owner[k] = i;
        let b = bary2(q, uv[i as usize]);
        at[k] = (b.map(|v| v.clamp(0.0, 1.0)), true);
    }
    (owner, at)
}

/// The dilation: each texel's layer (0 filled to begin with, `k` filled
/// by layer `k`, `u32::MAX` never) and the layers' texels in increasing
/// index. Layer `k` is the empty texels next (8-neighbours) to a texel of
/// a lower layer; `limit` layers (0: until none is left).
fn dilation(filled: &[bool], n: usize, limit: u32) -> (Vec<u32>, Vec<Vec<usize>>) {
    let mut level: Vec<u32> = filled
        .iter()
        .map(|&f| if f { 0 } else { u32::MAX })
        .collect();
    let mut layers: Vec<Vec<usize>> = Vec::new();
    let mut frontier: Vec<usize> = (0..n * n)
        .filter(|&k| level[k] == u32::MAX && neighbours(k, n).any(|j| level[j] == 0))
        .collect();
    while !frontier.is_empty() && (limit == 0 || (layers.len() as u32) < limit) {
        let l = layers.len() as u32 + 1;
        for &k in &frontier {
            level[k] = l;
        }
        let mut next: Vec<usize> = frontier
            .iter()
            .flat_map(|&k| neighbours(k, n))
            .filter(|&j| level[j] == u32::MAX)
            .collect();
        next.sort_unstable();
        next.dedup();
        layers.push(core::mem::replace(&mut frontier, next));
    }
    (level, layers)
}

/// The 8-neighbours of texel `k` inside the `n` × `n` grid.
fn neighbours(k: usize, n: usize) -> impl Iterator<Item = usize> {
    let (x, y) = ((k % n) as i64, (k / n) as i64);
    (-1..=1i64)
        .flat_map(|dy| (-1..=1i64).map(move |dx| (dx, dy)))
        .filter(|&(dx, dy)| dx != 0 || dy != 0)
        .filter_map(move |(dx, dy)| {
            let (xx, yy) = (x + dx, y + dy);
            (xx >= 0 && yy >= 0 && (xx as usize) < n && (yy as usize) < n)
                .then(|| yy as usize * n + xx as usize)
        })
}

/// Fills an image's texels layer by layer, each the rounded mean of its
/// neighbours of lower layers, channel by channel. Texel `y · n + x` is the
/// image's row `n − 1 − y`.
fn dilate(rgba: &mut [u8], n: usize, level: &[u32], layers: &[Vec<usize>]) {
    let px = |k: usize| ((n - 1 - k / n) * n + k % n) * 4;
    for (l, layer) in layers.iter().enumerate() {
        let l = l as u32 + 1;
        for &k in layer {
            let mut sum = [0u32; 4];
            let mut m = 0u32;
            for j in neighbours(k, n) {
                if level[j] < l {
                    m += 1;
                    for (c, s) in sum.iter_mut().enumerate() {
                        *s += u32::from(rgba[px(j) + c]);
                    }
                }
            }
            for (c, s) in sum.iter().enumerate() {
                rgba[px(k) + c] = ((s + m / 2) / m) as u8;
            }
        }
    }
}

impl BakeReport {
    /// The report as JSON with keys in a fixed order.
    pub fn to_json(&self) -> String {
        let mut s = String::new();
        self.json().write(&mut s);
        s
    }

    pub(crate) fn json(&self) -> Json {
        let maps = self
            .maps
            .iter()
            .map(|m| {
                Json::Obj(vec![
                    ("kind", Json::Str(m.kind.into())),
                    ("size", Json::Int(u64::from(m.size))),
                    ("file_bytes", Json::Int(m.file_bytes as u64)),
                ])
            })
            .collect();
        let normals = self.normals.map_or(Json::Null, |m| {
            Json::Obj(vec![
                ("mean_degrees", Json::Num(m.mean)),
                ("p95_degrees", Json::Num(m.p95)),
                ("mean_degrees_without_map", Json::Num(m.mean_without)),
                ("p95_degrees_without_map", Json::Num(m.p95_without)),
            ])
        });
        Json::Obj(vec![
            ("size", Json::Int(u64::from(self.size))),
            ("distance", Json::Num(self.distance)),
            ("texels_covered", Json::Int(self.covered as u64)),
            ("conservative", Json::Int(self.conservative as u64)),
            ("hits", Json::Int(self.hits as u64)),
            ("nearest", Json::Int(self.nearest as u64)),
            ("no_source", Json::Int(self.no_source as u64)),
            ("facing_away", Json::Int(self.facing_away as u64)),
            ("ambiguous", Json::Int(self.ambiguous as u64)),
            ("largest_distance", Json::Num(self.largest_distance)),
            ("dilated", Json::Int(self.dilated as u64)),
            ("empty", Json::Int(self.empty as u64)),
            ("maps", Json::Arr(maps)),
            ("factors_baked", Json::Bool(self.factors_baked)),
            ("colors_baked", Json::Bool(self.colors_baked)),
            (
                "not_baked",
                Json::Arr(
                    self.not_baked
                        .iter()
                        .map(|w| Json::Str(w.clone()))
                        .collect(),
                ),
            ),
            ("normals", normals),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Atlas, Unwrap, unwrap_atlas};

    /// A bump on a 40 mm square: z = h (1 − r²/R²)² inside R.
    fn height(x: f64, y: f64) -> (f64, DVec3) {
        let (h, r) = (4.0, 12.0);
        let (dx, dy) = (x - 20.0, y - 20.0);
        let q = 1.0 - (dx * dx + dy * dy) / (r * r);
        if q <= 0.0 {
            return (0.0, DVec3::Z);
        }
        // ∂z/∂x = h · 2q · (−2 dx / R²).
        let gx = h * 2.0 * q * (-2.0 * dx / (r * r));
        let gy = h * 2.0 * q * (-2.0 * dy / (r * r));
        (h * q * q, unit(DVec3::new(-gx, -gy, 1.0)))
    }

    /// The dense source: an n × n grid over the square with the bump,
    /// UVs over the unit square, a colour per corner from x, one surface.
    fn source(n: usize) -> Solid {
        let mut s = Solid::default();
        for j in 0..=n {
            for i in 0..=n {
                let (x, y) = (40.0 * i as f64 / n as f64, 40.0 * j as f64 / n as f64);
                s.positions.push(DVec3::new(x, y, height(x, y).0));
            }
        }
        let at = |i: usize, j: usize| (j * (n + 1) + i) as u32;
        let mut uvs = Vec::new();
        let mut cols = Vec::new();
        for j in 0..n {
            for i in 0..n {
                for t in [
                    [at(i, j), at(i + 1, j), at(i + 1, j + 1)],
                    [at(i, j), at(i + 1, j + 1), at(i, j + 1)],
                ] {
                    s.push_triangle(t, 0, &[]);
                    let p = t.map(|v| s.positions[v as usize]);
                    uvs.push(p.map(|q| DVec2::new(q.x / 40.0, q.y / 40.0)));
                    cols.push(p.map(|q| [q.x / 40.0, 1.0, 1.0, 1.0]));
                }
            }
        }
        s.uvs = Some(uvs);
        s.colors = Some(cols);
        s
    }

    /// A 4 × 4 checker of 16-texel squares, black and white, as PNG (row
    /// 0 at the top).
    fn checker() -> Texture {
        let rgba = (0..64 * 64)
            .flat_map(|k| {
                let (x, y) = ((k % 64) / 16, (k / 64) / 16);
                let v = if (x + y) % 2 == 0 { 0 } else { 255 };
                [v, v, v, 255]
            })
            .collect();
        Texture {
            name: "checker.png".into(),
            mime: "image/png".into(),
            bytes: crate::png::png(&Image {
                width: 64,
                height: 64,
                rgba,
            }),
        }
    }

    fn low() -> Solid {
        let mut s = Solid::default();
        for (x, y) in [(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)] {
            s.positions.push(DVec3::new(x, y, 0.0));
        }
        s.push_triangle([0, 1, 2], 0, &[]);
        s.push_triangle([0, 2, 3], 0, &[]);
        s
    }

    #[test]
    fn bakes_colour_and_bump_into_the_low_frame() {
        let src = source(64);
        let mut lo = low();
        let atlas = Atlas {
            size: 64,
            padding: 2,
            no_turns: false,
        };
        unwrap_atlas(&mut lo, &Unwrap::default(), &atlas).unwrap();
        let m = Material {
            map: Some(checker()),
            ..Default::default()
        };
        let b = bake(
            &BakeSource {
                solid: &src,
                materials: core::slice::from_ref(&m),
                material_of: &[],
            },
            &lo,
            &Bake {
                size: 64,
                distance: 6.0,
                ..Default::default()
            },
        )
        .unwrap();
        let r = &b.report;
        assert!(r.colors_baked && !r.factors_baked, "{r:?}");
        assert_eq!(r.no_source, 0);
        assert_eq!(r.hits, r.covered, "{r:?}");
        assert_eq!(r.empty, 0);
        let base = crate::texture::decode(&b.material.map.as_ref().unwrap().bytes).unwrap();
        let nrm = crate::texture::decode(&b.material.normal_map.as_ref().unwrap().bytes).unwrap();
        let (owner, at) = rasterize(lo.uvs.as_ref().unwrap(), 64);
        let frames = corner_frames(&lo);
        let lin = linear_table();
        let mut checked = 0;
        for texel in 0..64 * 64 {
            let li = owner[texel];
            if li == u32::MAX || at[texel].1 {
                continue;
            }
            let w = at[texel].0;
            let p = tri(&lo, li);
            let p = p[0] * w[0] + p[1] * w[1] + p[2] * w[2];
            let px = ((63 - texel / 64) * 64 + texel % 64) * 4;
            // The base colour: the checker's square × the colour x / 40.
            let (cx, cy) = ((p.x / 10.0).floor() as i64, (p.y / 10.0).floor() as i64);
            let (fx, fy) = (p.x / 10.0 - cx as f64, p.y / 10.0 - cy as f64);
            if (0.2..0.8).contains(&fx) && (0.2..0.8).contains(&fy) {
                // Row 0 of the image is v = 1: square (cx, 3 − cy).
                let white = (cx + (3 - cy)) % 2 == 1;
                let r_lin = f64::from(lin.get(base.rgba[px])) / 65535.0;
                let want = if white { p.x / 40.0 } else { 0.0 };
                assert!((r_lin - want).abs() < 0.01, "{p} {r_lin} {want}");
                assert_eq!(base.rgba[px + 1] == 0, !white);
                checked += 1;
            }
            // The normal, decoded in the low frame, against the bump's.
            let f = frame_at(&frames[li as usize], w);
            let bt = f.normal.cross(f.tangent) * f.w;
            let c = [0, 1, 2].map(|k| f64::from(nrm.rgba[px + k]) * 2.0 / 255.0 - 1.0);
            let seen = unit(f.tangent * c[0] + bt * c[1] + f.normal * c[2]);
            let (_, want) = height(p.x, p.y);
            let ang = trig::to_degrees(trig::acos(seen.dot(want).clamp(-1.0, 1.0)));
            assert!(ang < 3.0, "{p} {seen} {want} {ang}");
        }
        assert!(checked > 500, "{checked}");
        let nm = r.normals.unwrap();
        assert!(nm.mean < 1.0 && nm.mean_without > 5.0, "{nm:?}");
    }

    #[test]
    fn a_thin_wall_bakes_its_near_side() {
        // Two sheets 1 mm apart facing away from each other (a wall's
        // outside at z = 0.5 facing +z, its inside at z = −0.5 facing −z),
        // red outside, blue inside; the low sheet at z = 0 faces +z, so
        // every texel takes red though blue is as near.
        let mut src = Solid::default();
        for z in [0.5, -0.5] {
            for (x, y) in [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)] {
                src.positions.push(DVec3::new(x, y, z));
            }
        }
        src.push_triangle([0, 1, 2], 0, &[]);
        src.push_triangle([0, 2, 3], 0, &[]);
        src.push_triangle([4, 6, 5], 0, &[]);
        src.push_triangle([4, 7, 6], 0, &[]);
        let red = [1.0, 0.0, 0.0, 1.0];
        let blue = [0.0, 0.0, 1.0, 1.0];
        src.colors = Some(vec![[red; 3], [red; 3], [blue; 3], [blue; 3]]);
        let mut lo = Solid::default();
        for (x, y) in [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)] {
            lo.positions.push(DVec3::new(x, y, 0.0));
        }
        lo.push_triangle([0, 1, 2], 0, &[]);
        lo.push_triangle([0, 2, 3], 0, &[]);
        let atlas = Atlas {
            size: 32,
            ..Default::default()
        };
        unwrap_atlas(&mut lo, &Unwrap::default(), &atlas).unwrap();
        let b = bake(
            &BakeSource {
                solid: &src,
                materials: &[],
                material_of: &[],
            },
            &lo,
            &Bake {
                size: 32,
                distance: 2.0,
                no_normals: true,
                ..Default::default()
            },
        )
        .unwrap();
        let r = &b.report;
        assert_eq!(r.hits, r.covered);
        assert_eq!(r.ambiguous, 0);
        let img = crate::texture::decode(&b.material.map.as_ref().unwrap().bytes).unwrap();
        assert!(img.rgba.chunks(4).all(|p| p == [255, 0, 0, 255]));
    }
}

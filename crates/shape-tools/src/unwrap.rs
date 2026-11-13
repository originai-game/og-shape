//! UV unwrap for texturing: charts, a planar parametrization per chart,
//! and packing into the unit square. Written here (xatlas, MIT, could be
//! ported but is about ten thousand lines; `docs/research.md` §9).
//!
//! 1. **Charts.** Triangles are taken largest first as seeds. A chart
//!    grows from its seed across edges shared by two triangles, taking a
//!    neighbour whose normal is within `angle` of the seed's, and whose
//!    projection overlaps no triangle already in the chart (decided
//!    exactly with `orient2d`). So no chart folds over itself.
//! 2. **Parametrization.** Each chart is projected along its seed's
//!    normal: every triangle keeps its winding, and lengths shrink by at
//!    most cos `angle`.
//!    With [`Unwrap::conformal`], charts that bend are first flattened
//!    keeping their angles ([`conformal`]) where that works; the rest
//!    is projected.
//! 3. **Packing.** Each chart is turned to its smallest bounding rectangle
//!    (rotating calipers over its exact convex hull), laid wide, and
//!    packed tallest first by a bottom-left skyline. One scale (texels per
//!    mm) holds for every chart; the largest that fits is found by
//!    bisection. Charts sit `padding` texels apart at the given
//!    resolution.
//!
//! The result depends only on the solid and the desc. Geometry, names
//! and skin weights are unchanged; the UVs, per triangle corner, go into
//! the solid's [`Solid::uvs`] (replacing any it had), so chart borders are
//! seams, and a writer that splits a vertex at a seam gives each copy the
//! vertex's weights.

use core::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use shape_core::predicates::orient2d;
use shape_core::{DVec2, DVec3, trig};
use shape_mesh::{Solid, edge_key};

use crate::intersect::tris_overlap_2d;

/// How to unwrap. The zero value is a 66° chart angle, a 1024-texel
/// atlas and 2 texels between charts.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Unwrap {
    /// Largest angle in degrees between a chart's faces and its
    /// projection direction, below 90; 0 is 66 (Blender's smart project).
    pub angle: f64,
    /// Texels across the square atlas the padding is measured in; 0 is
    /// 1024.
    pub resolution: u32,
    /// Texels between charts (half that to the atlas edge); 0 is 2.
    pub padding: f64,
    /// Charts share projection directions chosen over the whole solid
    /// ([`directions`]), as Blender's Smart UV Project chooses them: all
    /// that faces one way is projected one way, wherever it is. Off, a
    /// chart is projected along its own seed's normal.
    pub shared: bool,
    /// Charts that bend are flattened keeping their angles (least
    /// squares conformal maps) where that neither folds nor stretches an
    /// area past twice or under half; what is left is projected as
    /// without it. A curved surface then takes fewer, larger charts.
    pub conformal: bool,
}

/// Texture coordinates for a solid.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Uvs {
    /// Per triangle, its three corners' coordinates in the unit square,
    /// (0, 0) at the bottom left (glTF flips v on export).
    pub corners: Vec<[DVec2; 3]>,
    /// Per triangle, its chart.
    pub charts: Vec<u32>,
    /// How many charts.
    pub chart_count: usize,
    /// UV units per millimetre on the surface (before the projection's
    /// shrinking): the texel density is this × the resolution.
    pub scale: f64,
}

/// Why a solid could not be unwrapped.
#[derive(Clone, Debug, PartialEq)]
pub enum UnwrapError {
    /// The angle, resolution or padding is out of range.
    BadDesc(String),
    /// The solid has no triangles.
    Empty,
    /// The charts' padding alone does not fit in the atlas: fewer charts
    /// (a larger angle), a finer resolution or less padding.
    TooManyCharts(usize),
}

impl fmt::Display for UnwrapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnwrapError::BadDesc(w) => write!(f, "unwrap: {w}"),
            UnwrapError::Empty => f.write_str("unwrap: the solid has no triangles"),
            UnwrapError::TooManyCharts(n) => write!(
                f,
                "unwrap: {n} charts do not fit in the atlas with their padding"
            ),
        }
    }
}

impl std::error::Error for UnwrapError {}

fn unit(v: DVec3) -> DVec3 {
    let l = v.length();
    if l > 0.0 { v / l } else { DVec3::ZERO }
}

/// A chart's projection plane: two axes perpendicular to `n`.
fn basis(n: DVec3) -> (DVec3, DVec3) {
    let a = n.abs();
    let helper = if a.x <= a.y && a.x <= a.z {
        DVec3::X
    } else if a.y <= a.z {
        DVec3::Y
    } else {
        DVec3::Z
    };
    let u = unit(n.cross(helper));
    (u, n.cross(u))
}

type Cell = (i64, i64);

fn cells(t: &[DVec2; 3], size: f64) -> impl Iterator<Item = Cell> {
    let lo = t[0].min(t[1]).min(t[2]);
    let hi = t[0].max(t[1]).max(t[2]);
    let (x0, y0) = ((lo.x / size).floor() as i64, (lo.y / size).floor() as i64);
    let (x1, y1) = ((hi.x / size).floor() as i64, (hi.y / size).floor() as i64);
    (x0..=x1).flat_map(move |x| (y0..=y1).map(move |y| (x, y)))
}

/// Unwraps `s` into charts packed in the unit square, sets its
/// [`Solid::uvs`] to the result and returns the charts. On an error the
/// solid is unchanged.
pub fn unwrap(s: &mut Solid, d: &Unwrap) -> Result<Uvs, UnwrapError> {
    let u = charts(s, d)?;
    s.uvs = Some(u.corners.clone());
    Ok(u)
}

/// The desc with its defaults (angle aside): resolution and padding; or
/// why it cannot unwrap `s`.
pub(crate) fn check(s: &Solid, d: &Unwrap) -> Result<(u32, f64), UnwrapError> {
    let angle = if d.angle == 0.0 { 66.0 } else { d.angle };
    if !(angle > 0.0 && angle < 90.0) {
        return Err(UnwrapError::BadDesc(
            "angle must be between 0 and 90 degrees".into(),
        ));
    }
    let resolution = if d.resolution == 0 {
        1024
    } else {
        d.resolution
    };
    let padding = if d.padding == 0.0 { 2.0 } else { d.padding };
    if !(padding > 0.0 && padding.is_finite()) {
        return Err(UnwrapError::BadDesc("padding must be positive".into()));
    }
    if s.triangles.is_empty() {
        return Err(UnwrapError::Empty);
    }
    Ok((resolution, padding))
}

/// The charts of `s` (largest triangle first as seeds, grown while the
/// normal stays within the angle and the projection does not fold): each
/// chart's triangles, each triangle's chart, and each triangle projected
/// on its chart's plane (millimetres).
pub(crate) fn form(s: &Solid, d: &Unwrap) -> (Vec<Vec<u32>>, Vec<u32>, Vec<[DVec2; 3]>) {
    let angle = if d.angle == 0.0 { 66.0 } else { d.angle };
    let n = s.triangles.len();
    let cos_limit = trig::cos(trig::to_radians(angle));
    let corner = |i: usize| s.triangles[i].map(|v| s.positions[v as usize]);
    let mut normals = Vec::with_capacity(n);
    let mut areas = Vec::with_capacity(n);
    let mut edge_sum = 0.0;
    for i in 0..n {
        let [a, b, c] = corner(i);
        let cr = (b - a).cross(c - a);
        normals.push(unit(cr));
        areas.push(cr.length());
        edge_sum += (b - a).length() + (c - b).length() + (a - c).length();
    }
    let cell = (edge_sum / (3 * n) as f64 * 2.0).max(1e-9);
    let mut by_edge: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
    for (i, t) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            by_edge
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(i as u32);
        }
    }
    let mut seeds: Vec<usize> = (0..n).collect();
    seeds.sort_by(|&a, &b| areas[b].total_cmp(&areas[a]).then(a.cmp(&b)));
    // With shared directions: each triangle's, by index, and the list.
    let shared = d
        .shared
        .then(|| directions(&normals, &areas, &seeds, angle));
    let mut chart_of = vec![u32::MAX; n];
    let mut flat: Vec<[DVec2; 3]> = vec![[DVec2::ZERO; 3]; n];
    let mut charts: Vec<Vec<u32>> = Vec::new();
    if d.conformal {
        // Charts over wider turns of the surface, flattened keeping
        // their angles; the triangles they leave are projected below.
        // A chart grows from its seed taking, of the triangles on its
        // rim, the one that faces most as the seed does, so that any
        // first part of it is the flattest part and in one piece; where
        // the whole does not flatten, its first half is tried, then a
        // quarter, then an eighth.
        let cos_wide = trig::cos(trig::to_radians((angle * 1.6).min(150.0)));
        let mut spent = 0usize;
        for &seed in &seeds {
            if spent > 6 * n {
                break;
            }
            if chart_of[seed] != u32::MAX || normals[seed] == DVec3::ZERO {
                continue;
            }
            let mut rim: std::collections::BTreeSet<(u64, u32)> = std::collections::BTreeSet::new();
            let mut seen: BTreeMap<u32, ()> = BTreeMap::new();
            rim.insert((0, seed as u32));
            seen.insert(seed as u32, ());
            let mut members: Vec<u32> = Vec::new();
            // Where the chart turns a corner: how many triangles it had
            // before one much more turned than any before it.
            let mut corners: Vec<usize> = Vec::new();
            let mut most = 0.0f64;
            while let Some((key, i)) = rim.pop_first() {
                let turned = f64::from_bits(key);
                if turned > most + 0.25 {
                    corners.push(members.len());
                }
                most = most.max(turned);
                members.push(i);
                let t = s.triangles[i as usize];
                for k in 0..3 {
                    let users = &by_edge[&edge_key(t[k], t[(k + 1) % 3])];
                    if users.len() != 2 {
                        continue;
                    }
                    let j = if users[0] == i { users[1] } else { users[0] };
                    let facing = normals[j as usize].dot(normals[seed]);
                    if chart_of[j as usize] != u32::MAX
                        || facing < cos_wide
                        || seen.insert(j, ()).is_some()
                    {
                        continue;
                    }
                    // Least turned from the seed first (1 − cos ≥ 0: its
                    // bits order as its value).
                    rim.insert(((1.0 - facing).max(0.0).to_bits(), j));
                }
            }
            // The whole; then up to each corner, the last first; then
            // halves.
            let mut sizes = vec![members.len()];
            sizes.extend(corners.iter().rev().take(3));
            sizes.extend([2, 4, 8].map(|part| members.len() / part));
            sizes.sort_unstable_by(|a, b| b.cmp(a));
            sizes.dedup();
            for take in sizes {
                if take < 2 {
                    break;
                }
                let some = &members[..take];
                if let Some(laid) = conformal(s, some, &normals, &areas, cell) {
                    let id = charts.len() as u32;
                    for (&i, f) in some.iter().zip(laid) {
                        chart_of[i as usize] = id;
                        flat[i as usize] = f;
                    }
                    charts.push(some.to_vec());
                    break;
                }
                spent += take;
            }
        }
    }
    for &seed in &seeds {
        if chart_of[seed] != u32::MAX {
            continue;
        }
        let id = charts.len() as u32;
        let axis = match &shared {
            Some((list, of)) => list.get(of[seed] as usize).copied().unwrap_or(DVec3::ZERO),
            None => normals[seed],
        };
        let (u, v) = if axis == DVec3::ZERO {
            (DVec3::X, DVec3::Y)
        } else {
            basis(axis)
        };
        let project = |i: usize| corner(i).map(|p| DVec2::new(p.dot(u), p.dot(v)));
        let mut grid: BTreeMap<Cell, Vec<u32>> = BTreeMap::new();
        let mut members = vec![seed as u32];
        chart_of[seed] = id;
        flat[seed] = project(seed);
        for c in cells(&flat[seed], cell) {
            grid.entry(c).or_default().push(seed as u32);
        }
        let mut head = 0;
        while head < members.len() && axis != DVec3::ZERO {
            let i = members[head] as usize;
            head += 1;
            let t = s.triangles[i];
            for k in 0..3 {
                let users = &by_edge[&edge_key(t[k], t[(k + 1) % 3])];
                if users.len() != 2 {
                    continue;
                }
                let j = if users[0] as usize == i {
                    users[1]
                } else {
                    users[0]
                } as usize;
                // Within the angle of the seed's normal; or, with shared
                // directions, of the same direction as the seed.
                let fits = match &shared {
                    Some((_, of)) => of[j] == of[seed],
                    None => normals[j].dot(axis) >= cos_limit,
                };
                if chart_of[j] != u32::MAX || !fits {
                    continue;
                }
                let fj = project(j);
                if orient2d(fj[0], fj[1], fj[2]) != Ordering::Greater {
                    continue;
                }
                let mut near: Vec<u32> = cells(&fj, cell)
                    .filter_map(|c| grid.get(&c))
                    .flatten()
                    .copied()
                    .collect();
                near.sort_unstable();
                near.dedup();
                if near.iter().any(|&o| tris_overlap_2d(fj, flat[o as usize])) {
                    continue;
                }
                chart_of[j] = id;
                flat[j] = fj;
                for c in cells(&fj, cell) {
                    grid.entry(c).or_default().push(j as u32);
                }
                members.push(j as u32);
            }
        }
        charts.push(members);
    }
    (charts, chart_of, flat)
}

/// The triangles `members` flattened as one chart that keeps its angles
/// (a least squares conformal map, Lévy et al. 2002; `docs/research.md`
/// §55), in millimetres, each triangle in `members`' order; `None`
/// where the chart folds, lies over itself, or stretches a triangle's
/// area past twice or under half.
///
/// Each triangle asks that its flat shape be its own shape turned and
/// scaled: two linear equations in its corners' coordinates, weighted
/// by the root of its area. Two vertices far apart are pinned at their
/// true distance, and the rest is the least squares answer, found by
/// conjugate gradients on the equations (CGLS) from the chart's
/// projection on its mean plane.
fn conformal(
    s: &Solid,
    members: &[u32],
    normals: &[DVec3],
    areas: &[f64],
    cell: f64,
) -> Option<Vec<[DVec2; 3]>> {
    // The chart's vertices, numbered in the order met.
    let mut number: BTreeMap<u32, usize> = BTreeMap::new();
    let mut points: Vec<DVec3> = Vec::new();
    let tris: Vec<[usize; 3]> = members
        .iter()
        .map(|&i| {
            s.triangles[i as usize].map(|v| {
                *number.entry(v).or_insert_with(|| {
                    points.push(s.positions[v as usize]);
                    points.len() - 1
                })
            })
        })
        .collect();
    let count = points.len();
    // The pins: the vertex furthest from the first, and the one
    // furthest from that.
    let furthest = |from: usize| {
        let mut best = (0.0, from);
        for (i, p) in points.iter().enumerate() {
            let d = (*p - points[from]).length_squared();
            if d > best.0 {
                best = (d, i);
            }
        }
        best.1
    };
    let a = furthest(0);
    let b = furthest(a);
    let apart = (points[b] - points[a]).length();
    if a == b || apart == 0.0 {
        return None;
    }
    // Where to start: the projection on the mean plane, turned and
    // scaled to put the pins where they are held.
    let mean = unit(
        members
            .iter()
            .map(|&i| normals[i as usize] * areas[i as usize])
            .sum::<DVec3>(),
    );
    let (eu, ev) = if mean == DVec3::ZERO {
        (DVec3::X, DVec3::Y)
    } else {
        basis(mean)
    };
    let mut x: Vec<DVec2> = points
        .iter()
        .map(|p| DVec2::new(p.dot(eu), p.dot(ev)))
        .collect();
    let span = x[b] - x[a];
    let length = span.length_squared();
    let origin = x[a];
    for q in &mut x {
        let d = *q - origin;
        *q = if length > 0.0 {
            // d / span as complex numbers, times the true distance.
            DVec2::new(d.x * span.x + d.y * span.y, d.y * span.x - d.x * span.y) * (apart / length)
        } else {
            DVec2::ZERO
        };
    }
    x[a] = DVec2::ZERO;
    x[b] = DVec2::new(apart, 0.0);
    // Unknowns: each free vertex's u, then its v.
    let mut slot = vec![usize::MAX; count];
    let mut free = 0;
    for (i, sl) in slot.iter_mut().enumerate() {
        if i != a && i != b {
            *sl = free;
            free += 1;
        }
    }
    if free == 0 {
        return None;
    }
    // Per triangle, its corners as complex weights (the opposite edge
    // in the triangle's own plane, over the root of twice its area).
    let mut weights: Vec<[DVec2; 3]> = Vec::with_capacity(tris.len());
    for t in &tris {
        let [p0, p1, p2] = t.map(|v| points[v]);
        let e = p1 - p0;
        let len = e.length();
        let normal = e.cross(p2 - p0);
        let double = normal.length();
        if len == 0.0 || double == 0.0 {
            weights.push([DVec2::ZERO; 3]);
            continue;
        }
        let ex = e / len;
        let ey = normal.cross(e) / (double * len);
        let q = [
            DVec2::ZERO,
            DVec2::new(len, 0.0),
            DVec2::new((p2 - p0).dot(ex), (p2 - p0).dot(ey)),
        ];
        let root = double.sqrt();
        weights.push([
            (q[2] - q[1]) / root,
            (q[0] - q[2]) / root,
            (q[1] - q[0]) / root,
        ]);
    }
    // A x (two rows a triangle) and its transpose, over the free
    // unknowns; `held` is what the pins put on the right-hand side.
    let apply = |x: &[f64], out: &mut [f64]| {
        for (k, (t, w)) in tris.iter().zip(&weights).enumerate() {
            let (mut re, mut im) = (0.0, 0.0);
            for j in 0..3 {
                let sl = slot[t[j]];
                if sl == usize::MAX {
                    continue;
                }
                let (u, v) = (x[sl], x[free + sl]);
                re += w[j].x * u - w[j].y * v;
                im += w[j].y * u + w[j].x * v;
            }
            out[2 * k] = re;
            out[2 * k + 1] = im;
        }
    };
    let transpose = |r: &[f64], out: &mut [f64]| {
        out.fill(0.0);
        for (k, (t, w)) in tris.iter().zip(&weights).enumerate() {
            let (re, im) = (r[2 * k], r[2 * k + 1]);
            for j in 0..3 {
                let sl = slot[t[j]];
                if sl == usize::MAX {
                    continue;
                }
                out[sl] += w[j].x * re + w[j].y * im;
                out[free + sl] += w[j].x * im - w[j].y * re;
            }
        }
    };
    let rows = 2 * tris.len();
    let mut held = vec![0.0; rows];
    for (k, (t, w)) in tris.iter().zip(&weights).enumerate() {
        for j in 0..3 {
            if t[j] == b {
                // The pin at (apart, 0); the one at the origin adds nothing.
                held[2 * k] -= w[j].x * apart;
                held[2 * k + 1] -= w[j].y * apart;
            }
        }
    }
    let mut sol = vec![0.0; 2 * free];
    for (i, &sl) in slot.iter().enumerate() {
        if sl != usize::MAX {
            sol[sl] = x[i].x;
            sol[free + sl] = x[i].y;
        }
    }
    let dot = |p: &[f64], q: &[f64]| p.iter().zip(q).map(|(a, b)| a * b).sum::<f64>();
    let mut r = vec![0.0; rows];
    apply(&sol, &mut r);
    for (ri, h) in r.iter_mut().zip(&held) {
        *ri = h - *ri;
    }
    let mut g = vec![0.0; 2 * free];
    transpose(&r, &mut g);
    let mut dir = g.clone();
    let mut gamma = dot(&g, &g);
    let first = gamma;
    let mut q = vec![0.0; rows];
    for _ in 0..(4 * free + 100).min(8000) {
        if gamma <= first * 1e-20 || gamma == 0.0 {
            break;
        }
        apply(&dir, &mut q);
        let qq = dot(&q, &q);
        if qq == 0.0 {
            break;
        }
        let step = gamma / qq;
        for (v, d) in sol.iter_mut().zip(&dir) {
            *v += step * d;
        }
        for (ri, qi) in r.iter_mut().zip(&q) {
            *ri -= step * qi;
        }
        transpose(&r, &mut g);
        let next = dot(&g, &g);
        let turn = next / gamma;
        gamma = next;
        for (d, gi) in dir.iter_mut().zip(&g) {
            *d = gi + turn * *d;
        }
    }
    for (i, &sl) in slot.iter().enumerate() {
        if sl != usize::MAX {
            x[i] = DVec2::new(sol[sl], sol[free + sl]);
        }
    }
    if x.iter().any(|q| !q.is_finite()) {
        return None;
    }
    // To the surface's own area, then: no fold, no stretch, no overlap.
    let mut laid: Vec<[DVec2; 3]> = tris.iter().map(|t| t.map(|v| x[v])).collect();
    let flat_area = |t: &[DVec2; 3]| (t[1] - t[0]).perp_dot(t[2] - t[0]);
    let (mut on_surface, mut on_plane) = (0.0, 0.0);
    for (&i, t) in members.iter().zip(&laid) {
        on_surface += areas[i as usize];
        on_plane += flat_area(t);
    }
    if !(on_plane > 0.0 && on_surface > 0.0) {
        return None;
    }
    let scale = (on_surface / on_plane).sqrt();
    for t in &mut laid {
        *t = t.map(|q| q * scale);
    }
    let mut grid: BTreeMap<Cell, Vec<u32>> = BTreeMap::new();
    for (k, (&i, t)) in members.iter().zip(&laid).enumerate() {
        if orient2d(t[0], t[1], t[2]) != Ordering::Greater {
            return None;
        }
        let ratio = flat_area(t) / areas[i as usize];
        if areas[i as usize] > 0.0 && !(0.5..=2.0).contains(&ratio) {
            return None;
        }
        let mut near: Vec<u32> = cells(t, cell)
            .filter_map(|c| grid.get(&c))
            .flatten()
            .copied()
            .collect();
        near.sort_unstable();
        near.dedup();
        if near.iter().any(|&o| tris_overlap_2d(*t, laid[o as usize])) {
            return None;
        }
        for c in cells(t, cell) {
            grid.entry(c).or_default().push(k as u32);
        }
    }
    Some(laid)
}

/// Projection directions for the whole solid, and each triangle's (an
/// index into them), chosen as Blender's Smart UV Project chooses its
/// own (`docs/research.md` §49): the largest triangle's normal first;
/// every triangle within half the angle of it gathered and their
/// normals, weighted by area, averaged into a direction; then the
/// triangle whose normal lies furthest from every direction so far
/// starts the next, until none lies further than the angle. A triangle
/// takes the direction nearest its normal.
fn directions(
    normals: &[DVec3],
    areas: &[f64],
    largest_first: &[usize],
    angle: f64,
) -> (Vec<DVec3>, Vec<u32>) {
    let cos_half = trig::cos(trig::to_radians(angle / 2.0));
    let cos_whole = trig::cos(trig::to_radians(angle));
    let faces: Vec<usize> = largest_first
        .iter()
        .copied()
        .filter(|&i| normals[i] != DVec3::ZERO)
        .collect();
    let mut list: Vec<DVec3> = Vec::new();
    let mut taken = vec![false; normals.len()];
    let mut start = faces.first().copied();
    while let Some(first) = start {
        let towards = normals[first];
        let mut sum = DVec3::ZERO;
        for &i in &faces {
            if !taken[i] && (i == first || normals[i].dot(towards) > cos_half) {
                taken[i] = true;
                sum += normals[i] * areas[i];
            }
        }
        let mean = unit(sum);
        if mean != DVec3::ZERO {
            list.push(mean);
        }
        // The triangle left whose normal is furthest from them all.
        let mut furthest: Option<(f64, usize)> = None;
        for &i in &faces {
            if taken[i] {
                continue;
            }
            let nearest = list.iter().map(|d| d.dot(normals[i])).fold(-1.0, f64::max);
            if furthest.is_none_or(|(best, _)| nearest < best) {
                furthest = Some((nearest, i));
            }
        }
        start = furthest
            .filter(|(nearest, _)| *nearest < cos_whole)
            .map(|(_, i)| i);
    }
    let of = normals
        .iter()
        .map(|n| {
            let mut best = (f64::NEG_INFINITY, 0u32);
            for (k, d) in list.iter().enumerate() {
                let dot = d.dot(*n);
                if dot > best.0 {
                    best = (dot, k as u32);
                }
            }
            best.1
        })
        .collect();
    (list, of)
}

fn charts(s: &Solid, d: &Unwrap) -> Result<Uvs, UnwrapError> {
    let (resolution, padding) = check(s, d)?;
    let (mut charts, mut chart_of, flat) = form(s, d);
    let n = s.triangles.len();
    // Each chart turned to its smallest box, laid wide, from the origin;
    // packed; then checked exactly in its final coordinates. Rounding in
    // the turn and the scale can push two triangles that only touched
    // into a sliver of overlap: such a triangle leaves for a chart of its
    // own, and the charts are packed again.
    let projected = flat.clone();
    let mut local = vec![[DVec2::ZERO; 3]; n];
    let mut sizes: Vec<DVec2> = charts
        .iter()
        .map(|m| lay(m, &projected, &mut local))
        .collect();
    let pad = padding / f64::from(resolution);
    let mut corners = vec![[DVec2::ZERO; 3]; n];
    let mut scale = 0.0;
    for round in 0..16 {
        let (at, sc) = pack_all(&sizes, pad).ok_or(UnwrapError::TooManyCharts(charts.len()))?;
        scale = sc;
        for (c, members) in charts.iter().enumerate() {
            let at = at[c] + DVec2::splat(pad / 2.0);
            for &i in members {
                corners[i as usize] = local[i as usize].map(|q| at + q * scale);
            }
        }
        let mut leaving: Vec<u32> = Vec::new();
        for members in &charts {
            for (i, j) in overlapping(members, &corners) {
                // The later leaves; after three rounds, both do, so this
                // ends (a chart of one triangle overlaps nothing).
                leaving.push(i.max(j));
                if round >= 3 {
                    leaving.push(i.min(j));
                }
            }
        }
        if leaving.is_empty() {
            break;
        }
        if round == 15 {
            return Err(UnwrapError::BadDesc(
                "charts still overlap after rounding; this is a bug".into(),
            ));
        }
        leaving.sort_unstable();
        leaving.dedup();
        for members in &mut charts {
            members.retain(|i| leaving.binary_search(i).is_err());
        }
        for &i in &leaving {
            charts.push(vec![i]);
        }
        charts.retain(|m| !m.is_empty());
        for (c, members) in charts.iter().enumerate() {
            for &i in members {
                chart_of[i as usize] = c as u32;
            }
        }
        sizes = charts
            .iter()
            .map(|m| lay(m, &projected, &mut local))
            .collect();
    }
    Ok(Uvs {
        corners,
        charts: chart_of,
        chart_count: charts.len(),
        scale,
    })
}

/// Turns a chart to its smallest box, laid wide, from the origin; writes
/// its triangles' coordinates into `local` and returns the box's size.
pub(crate) fn lay(members: &[u32], projected: &[[DVec2; 3]], local: &mut [[DVec2; 3]]) -> DVec2 {
    let pts: Vec<DVec2> = members
        .iter()
        .flat_map(|&i| projected[i as usize])
        .collect();
    let (e, p) = best_axes(&pts);
    let mut q: Vec<DVec2> = pts.iter().map(|x| DVec2::new(x.dot(e), x.dot(p))).collect();
    let (mut lo, mut hi) = (q[0], q[0]);
    for x in &q {
        lo = lo.min(*x);
        hi = hi.max(*x);
    }
    let size = hi - lo;
    let wide = size.x >= size.y;
    for x in &mut q {
        *x -= lo;
        if !wide {
            // A quarter turn: (x, y) ↦ (h − y, x), winding kept.
            *x = DVec2::new(size.y - x.y, x.x);
        }
    }
    for (k, &i) in members.iter().enumerate() {
        local[i as usize] = [q[k * 3], q[k * 3 + 1], q[k * 3 + 2]];
    }
    if wide {
        size
    } else {
        DVec2::new(size.y, size.x)
    }
}

/// Packs every chart at the largest scale that fits (bisection), tallest
/// first. Returns each chart's corner and the scale.
fn pack_all(sizes: &[DVec2], pad: f64) -> Option<(Vec<DVec2>, f64)> {
    let total: f64 = sizes.iter().map(|s| s.x * s.y).sum();
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| {
        sizes[b]
            .y
            .total_cmp(&sizes[a].y)
            .then(sizes[b].x.total_cmp(&sizes[a].x))
            .then(a.cmp(&b))
    });
    let fit = |scale: f64| pack(sizes, &order, scale, pad);
    let mut hi = if total > 0.0 {
        (1.0 / total).sqrt()
    } else {
        let longest = sizes.iter().map(|s| s.x.max(s.y)).fold(0.0, f64::max);
        if longest > 0.0 { 1.0 / longest } else { 1.0 }
    };
    let mut best = (fit(0.0)?, 0.0);
    if let Some(p) = fit(hi) {
        return Some((p, hi));
    }
    let mut lo = 0.0;
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        match fit(mid) {
            Some(p) => {
                best = (p, mid);
                lo = mid;
            }
            None => hi = mid,
        }
    }
    Some(best)
}

/// Pairs of triangles of one chart that overlap, by a sweep along u with
/// exact tests.
pub(crate) fn overlapping(members: &[u32], uv: &[[DVec2; 3]]) -> Vec<(u32, u32)> {
    let bounds = |i: u32| {
        let t = uv[i as usize];
        (t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2]))
    };
    let mut order: Vec<u32> = members.to_vec();
    order.sort_by(|&a, &b| bounds(a).0.x.total_cmp(&bounds(b).0.x).then(a.cmp(&b)));
    let mut active: Vec<u32> = Vec::new();
    let mut out = Vec::new();
    for &i in &order {
        let (lo, hi) = bounds(i);
        active.retain(|&j| bounds(j).1.x > lo.x);
        for &j in &active {
            let (lo2, hi2) = bounds(j);
            if lo2.y < hi.y && hi2.y > lo.y && tris_overlap_2d(uv[i as usize], uv[j as usize]) {
                out.push((i, j));
            }
        }
        active.push(i);
    }
    out
}

/// The axes (unit `e` and its left normal) along which the points' box is
/// smallest: one hull edge's direction, by rotating calipers.
fn best_axes(pts: &[DVec2]) -> (DVec2, DVec2) {
    let hull = hull(pts);
    let mut best = (f64::INFINITY, DVec2::X);
    for k in 0..hull.len() {
        let d = hull[(k + 1) % hull.len()] - hull[k];
        let len = d.length();
        if len == 0.0 {
            continue;
        }
        let e = d / len;
        let p = DVec2::new(-e.y, e.x);
        let (mut a0, mut a1, mut b0, mut b1) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for q in &hull {
            let (a, b) = (q.dot(e), q.dot(p));
            a0 = a0.min(a);
            a1 = a1.max(a);
            b0 = b0.min(b);
            b1 = b1.max(b);
        }
        let area = (a1 - a0) * (b1 - b0);
        if area < best.0 {
            best = (area, e);
        }
    }
    let e = best.1;
    (e, DVec2::new(-e.y, e.x))
}

/// The convex hull, counter-clockwise, by the monotone chain with exact
/// turns.
fn hull(pts: &[DVec2]) -> Vec<DVec2> {
    let mut p: Vec<DVec2> = pts.to_vec();
    p.sort_by(|a, b| shape_core::order::cmp_dvec2(*a, *b));
    p.dedup();
    if p.len() < 3 {
        return p;
    }
    let mut lower: Vec<DVec2> = Vec::new();
    for &q in &p {
        while lower.len() >= 2
            && orient2d(lower[lower.len() - 2], lower[lower.len() - 1], q) != Ordering::Greater
        {
            lower.pop();
        }
        lower.push(q);
    }
    let mut upper: Vec<DVec2> = Vec::new();
    for &q in p.iter().rev() {
        while upper.len() >= 2
            && orient2d(upper[upper.len() - 2], upper[upper.len() - 1], q) != Ordering::Greater
        {
            upper.pop();
        }
        upper.push(q);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Places every chart at `scale` by a bottom-left skyline in the unit
/// square; each takes its size × scale plus `pad`. `None` if they do not
/// all fit.
fn pack(sizes: &[DVec2], order: &[usize], scale: f64, pad: f64) -> Option<Vec<DVec2>> {
    // The skyline: (x, height) steps, each running to the next one's x.
    let mut sky: Vec<(f64, f64)> = vec![(0.0, 0.0)];
    let mut at = vec![DVec2::ZERO; sizes.len()];
    for &c in order {
        let (w, h) = (sizes[c].x * scale + pad, sizes[c].y * scale + pad);
        if w > 1.0 || h > 1.0 {
            return None;
        }
        let mut best: Option<(f64, f64, usize)> = None;
        for i in 0..sky.len() {
            let x = sky[i].0;
            if x + w > 1.0 {
                break;
            }
            let mut y: f64 = 0.0;
            let mut j = i;
            while j < sky.len() && sky[j].0 < x + w {
                y = y.max(sky[j].1);
                j += 1;
            }
            if y + h > 1.0 {
                continue;
            }
            if best.is_none_or(|(by, bx, _)| y < by || (y == by && x < bx)) {
                best = Some((y, x, i));
            }
        }
        let (y, x, i) = best?;
        at[c] = DVec2::new(x, y);
        // Raise [x, x + w) to y + h.
        let end = x + w;
        let mut j = i;
        let mut after = sky[i].1;
        while j < sky.len() && sky[j].0 < end {
            after = sky[j].1;
            j += 1;
        }
        let mut next: Vec<(f64, f64)> = sky[..i].to_vec();
        next.push((x, y + h));
        if j < sky.len() && sky[j].0 == end {
            // The next step starts exactly where this one ends.
        } else if end < 1.0 {
            next.push((end, after));
        }
        next.extend_from_slice(&sky[j..]);
        // Merge equal neighbours.
        next.dedup_by(|b, a| a.1 == b.1);
        sky = next;
    }
    Some(at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_gen::solids::{CylinderDesc, cylinder, sphere};

    fn check(s: &Solid) -> Uvs {
        check_with(s, &Unwrap::default())
    }

    fn check_with(s: &Solid, d: &Unwrap) -> Uvs {
        let mut s = s.clone();
        let uv = unwrap(&mut s, d).unwrap();
        assert_eq!(s.uvs.as_ref(), Some(&uv.corners));
        // Inside the square, wound as the faces, no two triangles over one
        // another.
        for (i, t) in uv.corners.iter().enumerate() {
            for q in t {
                assert!(q.x >= 0.0 && q.x <= 1.0 && q.y >= 0.0 && q.y <= 1.0);
            }
            assert_eq!(
                orient2d(t[0], t[1], t[2]),
                Ordering::Greater,
                "triangle {i}"
            );
        }
        for i in 0..uv.corners.len() {
            for j in i + 1..uv.corners.len() {
                assert!(!tris_overlap_2d(uv.corners[i], uv.corners[j]), "{i} {j}");
            }
        }
        uv
    }

    #[test]
    fn charts_never_overlap() {
        let s = sphere(10.0).unwrap();
        let uv = check(&s);
        assert!(
            uv.chart_count >= 6 && uv.chart_count < 40,
            "{}",
            uv.chart_count
        );
        let c = cylinder(&CylinderDesc {
            radius: 5.0,
            height: 20.0,
            ..Default::default()
        })
        .unwrap();
        check(&c);
    }

    /// With shared directions a torus is projected along a few
    /// directions chosen over all of it, and its charts are fewer than
    /// when each starts from its own seed's normal; a box's six sides
    /// are six charts either way.
    #[test]
    fn shared_directions_are_chosen_over_the_whole_solid() {
        let shared = Unwrap {
            shared: true,
            ..Default::default()
        };
        let t = shape_gen::solids::torus(20.0, 6.0).unwrap();
        assert_eq!(check(&t).chart_count, 14);
        assert_eq!(check_with(&t, &shared).chart_count, 10);
        let b = shape_gen::solids::cuboid(&shape_gen::solids::BoxDesc {
            width: 10.0,
            depth: 20.0,
            height: 30.0,
        })
        .unwrap();
        assert_eq!(check_with(&b, &shared).chart_count, 6);
    }

    /// Charts that bend are flattened keeping their angles: a dome is
    /// three charts where projecting takes six, and a cylinder's side
    /// unrolls at its true area (a projection loses a fifth of it), in
    /// two strips beside its two caps.
    #[test]
    fn conformal_charts_keep_angles_and_area() {
        let conformal = Unwrap {
            conformal: true,
            ..Default::default()
        };
        let dome = shape_gen::solids::dome(10.0).unwrap();
        assert_eq!(check(&dome).chart_count, 6);
        assert_eq!(check_with(&dome, &conformal).chart_count, 3);
        let c = cylinder(&CylinderDesc {
            radius: 5.0,
            height: 20.0,
            ..Default::default()
        })
        .unwrap();
        // The area the UVs cover, in the surface's own units, over the
        // surface's.
        let kept = |uv: &Uvs| {
            let flat: f64 = uv
                .corners
                .iter()
                .map(|t| (t[1] - t[0]).perp_dot(t[2] - t[0]) / 2.0)
                .sum();
            flat / (uv.scale * uv.scale) / c.measure().area
        };
        let (projected, unrolled) = (check(&c), check_with(&c, &conformal));
        assert_eq!(unrolled.chart_count, 4);
        assert!(kept(&projected) < 0.9, "{}", kept(&projected));
        assert!((kept(&unrolled) - 1.0).abs() < 1e-6, "{}", kept(&unrolled));
    }
}

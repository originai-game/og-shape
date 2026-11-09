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
//! 3. **Packing.** Each chart is turned to its smallest bounding rectangle
//!    (rotating calipers over its exact convex hull), laid wide, and
//!    packed tallest first by a bottom-left skyline. One scale (texels per
//!    mm) holds for every chart; the largest that fits is found by
//!    bisection. Charts sit `padding` texels apart at the given
//!    resolution.
//!
//! The result depends only on the solid and the desc. Geometry and names
//! are unchanged; UVs are per triangle corner, so chart borders are
//! seams.

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

/// Unwraps `s` into charts packed in the unit square.
pub fn unwrap(s: &Solid, d: &Unwrap) -> Result<Uvs, UnwrapError> {
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
    let n = s.triangles.len();
    if n == 0 {
        return Err(UnwrapError::Empty);
    }
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
    let mut chart_of = vec![u32::MAX; n];
    let mut flat: Vec<[DVec2; 3]> = vec![[DVec2::ZERO; 3]; n];
    let mut charts: Vec<Vec<u32>> = Vec::new();
    for &seed in &seeds {
        if chart_of[seed] != u32::MAX {
            continue;
        }
        let id = charts.len() as u32;
        let axis = normals[seed];
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
                if chart_of[j] != u32::MAX || normals[j].dot(axis) < cos_limit {
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
    // Each chart turned to its smallest box, laid wide, from the origin.
    let mut sizes: Vec<DVec2> = Vec::with_capacity(charts.len());
    for members in &charts {
        let pts: Vec<DVec2> = members.iter().flat_map(|&i| flat[i as usize]).collect();
        let (e, p) = best_axes(&pts);
        let mut local: Vec<DVec2> = pts.iter().map(|q| DVec2::new(q.dot(e), q.dot(p))).collect();
        let (mut lo, mut hi) = (local[0], local[0]);
        for q in &local {
            lo = lo.min(*q);
            hi = hi.max(*q);
        }
        let mut size = hi - lo;
        let wide = size.x >= size.y;
        for q in &mut local {
            *q -= lo;
            if !wide {
                // A quarter turn: (x, y) ↦ (h − y, x), winding kept.
                *q = DVec2::new(size.y - q.y, q.x);
            }
        }
        if !wide {
            size = DVec2::new(size.y, size.x);
        }
        for (k, &i) in members.iter().enumerate() {
            flat[i as usize] = [local[k * 3], local[k * 3 + 1], local[k * 3 + 2]];
        }
        sizes.push(size);
    }
    let pad = padding / f64::from(resolution);
    let total: f64 = sizes.iter().map(|s| s.x * s.y).sum();
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by(|&a, &b| {
        sizes[b]
            .y
            .total_cmp(&sizes[a].y)
            .then(sizes[b].x.total_cmp(&sizes[a].x))
            .then(a.cmp(&b))
    });
    let fit = |scale: f64| pack(&sizes, &order, scale, pad);
    let mut hi = if total > 0.0 {
        (1.0 / total).sqrt()
    } else {
        let longest = sizes.iter().map(|s| s.x.max(s.y)).fold(0.0, f64::max);
        if longest > 0.0 { 1.0 / longest } else { 1.0 }
    };
    let mut lo = 0.0;
    let mut best = fit(0.0).ok_or(UnwrapError::TooManyCharts(charts.len()))?;
    if let Some(p) = fit(hi) {
        best = p;
        lo = hi;
    } else {
        for _ in 0..40 {
            let mid = (lo + hi) / 2.0;
            match fit(mid) {
                Some(p) => {
                    best = p;
                    lo = mid;
                }
                None => hi = mid,
            }
        }
    }
    let scale = lo;
    let mut corners = vec![[DVec2::ZERO; 3]; n];
    for (c, members) in charts.iter().enumerate() {
        let at = best[c] + DVec2::splat(pad / 2.0);
        for &i in members {
            corners[i as usize] = flat[i as usize].map(|q| at + q * scale);
        }
    }
    Ok(Uvs {
        corners,
        charts: chart_of,
        chart_count: charts.len(),
        scale,
    })
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
        let uv = unwrap(s, &Unwrap::default()).unwrap();
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
}

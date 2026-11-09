//! Profile operations: booleans, offset, stroke, mirror, hull,
//! transforms and triangulation.

use std::collections::BTreeMap;

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::single::SingleFloatOverlay;
use i_overlay::mesh::float::outline::offset::OutlineOffset;
use i_overlay::mesh::float::stroke::offset::StrokeOffset;
use i_overlay::mesh::float::style::{LineCap, LineJoin, OutlineStyle, StrokeStyle};
use shape_core::order::cmp_dvec2;
use shape_core::predicates::orient2d;
use shape_core::{DVec2, trig};

use crate::curve::segments;
use crate::{Contour, Fill, Profile, ProfileError, Region};

type Shapes = Vec<Vec<Vec<[f64; 2]>>>;

fn to_shapes(p: &Profile) -> Shapes {
    p.regions
        .iter()
        .map(|r| {
            std::iter::once(&r.outer)
                .chain(&r.holes)
                .map(|c| c.points.iter().map(|q| [q.x, q.y]).collect())
                .collect()
        })
        .collect()
}

/// Corner flags of the inputs. A point that survives an operation keeps
/// its flag; a new one (an intersection) is a corner. i_overlay rounds
/// through its integer grid, so a survivor is matched to the nearest input
/// point within 1e-6 of the inputs' size (the grid moves points by about
/// 1e-9 of it) and snapped back onto it exactly.
struct Flags {
    /// Input points sorted by x, with their flags.
    points: Vec<(DVec2, bool)>,
    tolerance: f64,
}

impl Flags {
    fn new(inputs: &[&Profile]) -> Flags {
        let mut points: Vec<(DVec2, bool)> = inputs
            .iter()
            .flat_map(|p| p.contours())
            .flat_map(|c| c.points.iter().copied().zip(c.sharp.iter().copied()))
            .collect();
        points.sort_by(|a, b| cmp_dvec2(a.0, b.0));
        let size = points
            .iter()
            .fold(0.0f64, |m, (p, _)| m.max(p.x.abs()).max(p.y.abs()));
        Flags {
            points,
            tolerance: size.max(1.0) * 1e-6,
        }
    }

    /// The input point `q` came from and its flag, or `q` as a new corner.
    fn restore(&self, q: DVec2) -> (DVec2, bool) {
        let lo = self
            .points
            .partition_point(|(p, _)| p.x < q.x - self.tolerance);
        let mut best: Option<(f64, DVec2, bool)> = None;
        for &(p, s) in &self.points[lo..] {
            if p.x > q.x + self.tolerance {
                break;
            }
            let d = (p - q).length_squared();
            if d <= self.tolerance * self.tolerance
                && best.is_none_or(|(bd, _, bs)| d < bd || (d == bd && s && !bs))
            {
                best = Some((d, p, s));
            }
        }
        best.map_or((q, true), |(_, p, s)| (p, s))
    }
}

/// i_overlay's output back to a profile, with the flags of `inputs`.
pub(crate) fn from_overlay(shapes: Shapes, inputs: &[&Profile]) -> Profile {
    from_shapes(shapes, &Flags::new(inputs))
}

/// Back to a profile: flags restored, each contour starting at its
/// smallest point, regions sorted by their outer's smallest point.
fn from_shapes(shapes: Shapes, flags: &Flags) -> Profile {
    let contour = |pts: &Vec<[f64; 2]>| {
        let (points, sharp) = pts
            .iter()
            .map(|q| flags.restore(DVec2::new(q[0], q[1])))
            .unzip();
        rotate_to_min(Contour { points, sharp })
    };
    let mut regions: Vec<Region> = shapes
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| Region {
            outer: contour(&s[0]),
            holes: s[1..].iter().map(contour).collect(),
        })
        .collect();
    for r in &mut regions {
        r.holes.sort_by(|a, b| cmp_dvec2(a.points[0], b.points[0]));
    }
    regions.sort_by(|a, b| cmp_dvec2(a.outer.points[0], b.outer.points[0]));
    Profile { regions }
}

fn rotate_to_min(mut c: Contour) -> Contour {
    if let Some(i) = (0..c.points.len()).min_by(|&a, &b| cmp_dvec2(c.points[a], c.points[b])) {
        c.points.rotate_left(i);
        c.sharp.rotate_left(i);
    }
    c
}

/// How offsets and strokes join their segments.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Join {
    /// Rounded, flattened by the chord rule.
    #[default]
    Round,
    /// Sharp, extended to the miter point.
    Miter,
    /// Cut straight across.
    Bevel,
}

fn line_join(join: Join, radius: f64) -> LineJoin<f64> {
    match join {
        Join::Round => LineJoin::Round(trig::TAU / segments(radius.abs(), trig::TAU) as f64),
        Join::Miter => LineJoin::Miter(trig::PI / 36.0),
        Join::Bevel => LineJoin::Bevel,
    }
}

impl Profile {
    fn overlay(&self, other: &Profile, rule: OverlayRule) -> Profile {
        let f = Flags::new(&[self, other]);
        let out = to_shapes(self).overlay(&to_shapes(other), rule, FillRule::NonZero);
        from_shapes(out, &f)
    }

    /// The area in either.
    pub fn union(&self, other: &Profile) -> Profile {
        self.overlay(other, OverlayRule::Union)
    }

    /// The area in this one and not in `other`.
    pub fn subtract(&self, other: &Profile) -> Profile {
        self.overlay(other, OverlayRule::Difference)
    }

    /// The area in both.
    pub fn intersect(&self, other: &Profile) -> Profile {
        self.overlay(other, OverlayRule::Intersect)
    }

    /// The area in exactly one.
    pub fn xor(&self, other: &Profile) -> Profile {
        self.overlay(other, OverlayRule::Xor)
    }

    /// Grown by `distance` (shrunk where negative), corners joined by `join`.
    pub fn offset(&self, distance: f64, join: Join) -> Result<Profile, ProfileError> {
        if !distance.is_finite() {
            return Err(ProfileError::NotFinite("distance"));
        }
        if distance == 0.0 {
            return Ok(self.clone());
        }
        let style = OutlineStyle::new(distance).line_join(line_join(join, distance));
        let out = to_shapes(self).outline(&style);
        let p = from_shapes(out, &Flags::new(&[self]));
        if p.regions.is_empty() {
            Err(ProfileError::Empty)
        } else {
            Ok(p)
        }
    }

    /// Mirrored across the y axis (x → −x), still counter-clockwise.
    pub fn mirror(&self) -> Profile {
        self.map(|p| DVec2::new(-p.x, p.y), true)
    }

    /// Moved by `d`.
    pub fn translate(&self, d: DVec2) -> Profile {
        self.map(|p| p + d, false)
    }

    /// Rotated by `angle` radians about the origin.
    pub fn rotate(&self, angle: f64) -> Profile {
        let (s, c) = trig::sin_cos(angle);
        self.map(|p| DVec2::new(c * p.x - s * p.y, s * p.x + c * p.y), false)
    }

    /// Scaled by `k` (x) and `l` (y); a negative product mirrors.
    pub fn scale(&self, k: f64, l: f64) -> Profile {
        self.map(|p| DVec2::new(p.x * k, p.y * l), k * l < 0.0)
    }

    fn map(&self, f: impl Fn(DVec2) -> DVec2, mirrors: bool) -> Profile {
        let regions = self
            .regions
            .iter()
            .map(|r| {
                let m = |c: &Contour| {
                    let c = Contour {
                        points: c.points.iter().map(|p| f(*p)).collect(),
                        sharp: c.sharp.clone(),
                    };
                    rotate_to_min(if mirrors { c.reversed() } else { c })
                };
                Region {
                    outer: m(&r.outer),
                    holes: r.holes.iter().map(m).collect(),
                }
            })
            .collect();
        Profile { regions }
    }

    /// The convex hull of every point (Andrew's monotone chain, sides
    /// decided exactly). All its points are corners.
    pub fn hull(&self) -> Result<Profile, ProfileError> {
        let mut pts: Vec<DVec2> = self
            .contours()
            .flat_map(|c| c.points.iter().copied())
            .collect();
        pts.sort_by(|a, b| cmp_dvec2(*a, *b));
        pts.dedup();
        if pts.len() < 3 {
            return Err(ProfileError::TooFewPoints(pts.len()));
        }
        let mut lower: Vec<DVec2> = Vec::new();
        for &p in &pts {
            while lower.len() >= 2
                && !orient2d(lower[lower.len() - 2], lower[lower.len() - 1], p).is_gt()
            {
                lower.pop();
            }
            lower.push(p);
        }
        let mut upper: Vec<DVec2> = Vec::new();
        for &p in pts.iter().rev() {
            while upper.len() >= 2
                && !orient2d(upper[upper.len() - 2], upper[upper.len() - 1], p).is_gt()
            {
                upper.pop();
            }
            upper.push(p);
        }
        lower.pop();
        upper.pop();
        lower.extend(upper);
        if lower.len() < 3 {
            return Err(ProfileError::Empty);
        }
        Ok(Profile::from_contour(rotate_to_min(Contour::corners(
            lower,
        ))))
    }
}

/// How a stroke ends.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StrokeDesc {
    /// Full width.
    pub width: f64,
    /// Corner joins.
    pub join: Join,
    /// Round ends (true) or square-cut ends at the points (false).
    pub round_caps: bool,
    /// Join the last point back to the first.
    pub closed: bool,
}

/// The area within `width / 2` of an open (or closed) polyline.
pub fn stroke(points: &[DVec2], d: &StrokeDesc) -> Result<Profile, ProfileError> {
    if !(d.width.is_finite()) {
        return Err(ProfileError::NotFinite("width"));
    }
    if d.width <= 0.0 {
        return Err(ProfileError::NotPositive("width"));
    }
    if points.len() < 2 {
        return Err(ProfileError::TooFewPoints(points.len()));
    }
    let r = d.width / 2.0;
    let cap = if d.round_caps {
        LineCap::Round(trig::TAU / segments(r, trig::TAU) as f64)
    } else {
        LineCap::Butt
    };
    let style = StrokeStyle::new(d.width)
        .line_join(line_join(d.join, r))
        .start_cap(cap.clone())
        .end_cap(cap);
    let path: Vec<[f64; 2]> = points.iter().map(|p| [p.x, p.y]).collect();
    let out = path.stroke(style, d.closed);
    let p = from_shapes(out, &Flags::new(&[]));
    if p.regions.is_empty() {
        Err(ProfileError::Empty)
    } else {
        Ok(p)
    }
}

/// Constrained Delaunay triangulation of the profile (spade), with every
/// contour point kept at its exact position, so caps weld to walls.
/// Triangles are kept where their centroid lies inside the profile.
pub(crate) fn triangulate(p: &Profile) -> Result<Fill, ProfileError> {
    use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
    if p.regions.is_empty() {
        return Err(ProfileError::Empty);
    }
    let mut fill = Fill::default();
    for r in &p.regions {
        let region = Profile {
            regions: vec![r.clone()],
        };
        let mut cdt: ConstrainedDelaunayTriangulation<Point2<f64>> =
            ConstrainedDelaunayTriangulation::new();
        // spade vertex index -> fill point index.
        let mut fill_of: BTreeMap<usize, u32> = BTreeMap::new();
        for c in region.contours() {
            let mut handles = Vec::with_capacity(c.points.len());
            for q in &c.points {
                let h = cdt
                    .insert(Point2::new(q.x, q.y))
                    .map_err(|_| ProfileError::NotFinite("points"))?;
                fill_of.entry(h.index()).or_insert_with(|| {
                    fill.points.push(*q);
                    fill.points.len() as u32 - 1
                });
                handles.push(h);
            }
            for i in 0..handles.len() {
                let (a, b) = (handles[i], handles[(i + 1) % handles.len()]);
                if a != b {
                    cdt.try_add_constraint(a, b);
                }
            }
        }
        for f in cdt.inner_faces() {
            let ids = f.vertices().map(|v| v.fix().index());
            let Some(t) = ids
                .iter()
                .map(|i| fill_of.get(i).copied())
                .collect::<Option<Vec<u32>>>()
            else {
                continue;
            };
            let [a, b, c] = [t[0], t[1], t[2]];
            let (pa, pb, pc) = (
                fill.points[a as usize],
                fill.points[b as usize],
                fill.points[c as usize],
            );
            if !region.contains((pa + pb + pc) / 3.0) {
                continue;
            }
            match orient2d(pa, pb, pc) {
                core::cmp::Ordering::Greater => fill.triangles.push([a, b, c]),
                core::cmp::Ordering::Less => fill.triangles.push([a, c, b]),
                core::cmp::Ordering::Equal => {}
            }
        }
    }
    Ok(fill)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Circle, Rect, circle, rect};

    fn sq(w: f64) -> Profile {
        rect(&Rect {
            width: w,
            height: w,
            radius: 0.0,
        })
        .unwrap()
    }

    #[test]
    fn booleans_add_up() {
        let a = sq(10.0);
        let b = sq(10.0).translate(DVec2::new(5.0, 5.0));
        let (u, i, s) = (a.union(&b), a.intersect(&b), a.subtract(&b));
        assert!((u.area() - 175.0).abs() < 1e-9, "{}", u.area());
        assert!((i.area() - 25.0).abs() < 1e-9);
        assert!((s.area() - 75.0).abs() < 1e-9);
        // The intersection points are new corners; the originals keep theirs.
        assert!(u.regions[0].outer.sharp.iter().all(|s| *s));
    }

    #[test]
    fn a_hole_survives_and_triangulates() {
        let washer = circle(&Circle {
            radius: 10.0,
            segments: 0,
        })
        .unwrap()
        .subtract(
            &circle(&Circle {
                radius: 4.0,
                segments: 0,
            })
            .unwrap(),
        );
        assert_eq!(washer.regions.len(), 1);
        assert_eq!(washer.regions[0].holes.len(), 1);
        assert!(washer.regions[0].outer.sharp.iter().all(|s| !*s));
        let f = washer.triangulate().unwrap();
        let area: f64 = f
            .triangles
            .iter()
            .map(|t| {
                let [a, b, c] = t.map(|i| f.points[i as usize]);
                (b - a).perp_dot(c - a) / 2.0
            })
            .sum();
        assert!(
            (area - washer.area()).abs() < 1e-9,
            "{area} {}",
            washer.area()
        );
        // Every contour point is a fill point, exactly.
        for c in washer.contours() {
            assert!(c.points.iter().all(|q| f.points.contains(q)));
        }
    }

    #[test]
    fn offset_and_stroke_have_their_areas() {
        let grown = sq(10.0).offset(1.0, Join::Miter).unwrap();
        assert!((grown.area() - 144.0).abs() < 1e-6, "{}", grown.area());
        let rounded = sq(10.0).offset(1.0, Join::Round).unwrap();
        let want = 100.0 + 40.0 + trig::PI;
        assert!(
            (rounded.area() - want).abs() / want < 2e-3,
            "{}",
            rounded.area()
        );
        let bar = stroke(
            &[DVec2::ZERO, DVec2::new(10.0, 0.0)],
            &StrokeDesc {
                width: 2.0,
                ..Default::default()
            },
        )
        .unwrap();
        assert!((bar.area() - 20.0).abs() < 1e-6);
    }

    #[test]
    fn hull_and_mirror() {
        let l = sq(4.0).union(&sq(4.0).translate(DVec2::new(4.0, 4.0)));
        let h = l.hull().unwrap();
        assert!((h.area() - (64.0 - 16.0)).abs() < 1e-9, "{}", h.area());
        let m = l.mirror();
        assert!((m.area() - l.area()).abs() < 1e-9);
        assert!(m.regions[0].outer.signed_area2() > 0.0);
    }
}

//! 2D profiles: the outlines that solids are extruded, revolved and swept
//! from (`docs/design.md` §2).
//!
//! Entry: [`Profile`]. Make one with a constructor ([`rect`], [`circle`],
//! [`ellipse`], [`slot`], [`polygon`], [`poly`], [`superellipse`], or a
//! [`Path`]), combine with [`Profile::union`], [`Profile::subtract`],
//! [`Profile::intersect`], [`Profile::offset`], [`stroke`],
//! [`Profile::mirror`] or [`Profile::hull`], and fill with
//! [`Profile::triangulate`].
//!
//! Units are millimetres, angles radians. Curves are flattened by the rule
//! of [`curve::segments`], which depends only on the arguments. 2D booleans,
//! offsets and strokes run on i_overlay's integer core; triangulation is
//! spade's constrained Delaunay, on the exact contour points.

pub mod curve;
mod make;
mod ops;
mod path;

pub use make::{
    Circle, Ellipse, Poly, PolyPoint, Polygon, Rect, Slot, Superellipse, circle, ellipse, poly,
    polygon, rect, slot, superellipse,
};
pub use ops::{Join, StrokeDesc, stroke};
pub use path::Path;

use shape_core::DVec2;
use shape_core::predicates::orient2d;

/// A closed outline. `sharp[i]` says whether point `i` is a corner: walls
/// built from the outline break their surface there, and the vertical edge
/// is a real edge. Points along a curve are not sharp.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Contour {
    /// The points, without repeating the first.
    pub points: Vec<DVec2>,
    /// Per point, whether it is a corner.
    pub sharp: Vec<bool>,
}

impl Contour {
    /// A contour whose points are all corners.
    pub fn corners(points: Vec<DVec2>) -> Contour {
        let sharp = vec![true; points.len()];
        Contour { points, sharp }
    }

    /// A contour whose points are all smooth (a flattened curve).
    pub fn smooth(points: Vec<DVec2>) -> Contour {
        let sharp = vec![false; points.len()];
        Contour { points, sharp }
    }

    /// Twice the signed area: positive when counter-clockwise.
    pub fn signed_area2(&self) -> f64 {
        let n = self.points.len();
        (0..n)
            .map(|i| {
                let (a, b) = (self.points[i], self.points[(i + 1) % n]);
                a.x * b.y - a.y * b.x
            })
            .sum()
    }

    /// The same contour, wound the other way.
    pub fn reversed(&self) -> Contour {
        let mut c = self.clone();
        c.points.reverse();
        c.sharp.reverse();
        c
    }
}

/// One filled area: an outer contour (counter-clockwise) and its holes
/// (clockwise).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Region {
    /// The outer boundary.
    pub outer: Contour,
    /// Holes inside it.
    pub holes: Vec<Contour>,
}

/// Zero or more disjoint regions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    /// The regions, in a canonical order after every operation.
    pub regions: Vec<Region>,
}

/// Why a profile operation could not be honoured.
#[derive(Clone, Debug, PartialEq)]
pub enum ProfileError {
    /// A size that must be positive was not (the field named).
    NotPositive(&'static str),
    /// A value was NaN or infinite (the field named).
    NotFinite(&'static str),
    /// Fewer points than an outline needs.
    TooFewPoints(usize),
    /// A corner radius larger than its corner allows (the point index).
    RadiusTooLarge(usize),
    /// The result has no area.
    Empty,
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileError::NotPositive(n) => write!(f, "{n} must be positive"),
            ProfileError::NotFinite(n) => write!(f, "{n} must be finite"),
            ProfileError::TooFewPoints(n) => write!(f, "an outline needs 3 points, got {n}"),
            ProfileError::RadiusTooLarge(i) => write!(
                f,
                "the radius at point {i} is larger than its corner allows"
            ),
            ProfileError::Empty => f.write_str("the profile has no area"),
        }
    }
}

impl std::error::Error for ProfileError {}

/// A triangulated profile: points and counter-clockwise triangles.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fill {
    /// The points; every contour point appears exactly once.
    pub points: Vec<DVec2>,
    /// Triangles as point indices, counter-clockwise.
    pub triangles: Vec<[u32; 3]>,
}

impl Profile {
    /// A profile of one region with no holes.
    pub fn from_contour(c: Contour) -> Profile {
        let c = if c.signed_area2() < 0.0 {
            c.reversed()
        } else {
            c
        };
        Profile {
            regions: vec![Region {
                outer: c,
                holes: Vec::new(),
            }],
        }
    }

    /// The total area.
    pub fn area(&self) -> f64 {
        self.contours().map(|c| c.signed_area2()).sum::<f64>() / 2.0
    }

    /// Every contour: each region's outer, then its holes.
    pub fn contours(&self) -> impl Iterator<Item = &Contour> {
        self.regions
            .iter()
            .flat_map(|r| std::iter::once(&r.outer).chain(&r.holes))
    }

    /// Is `p` inside (holes excluded)? Points on the boundary count as
    /// inside. Decided with exact predicates.
    pub fn contains(&self, p: DVec2) -> bool {
        let mut winding = 0i32;
        for c in self.contours() {
            let n = c.points.len();
            for i in 0..n {
                let (a, b) = (c.points[i], c.points[(i + 1) % n]);
                let side = orient2d(a, b, p);
                if side.is_eq()
                    && p.x >= a.x.min(b.x)
                    && p.x <= a.x.max(b.x)
                    && p.y >= a.y.min(b.y)
                    && p.y <= a.y.max(b.y)
                {
                    return true;
                }
                if a.y <= p.y {
                    if b.y > p.y && side.is_gt() {
                        winding += 1;
                    }
                } else if b.y <= p.y && side.is_lt() {
                    winding -= 1;
                }
            }
        }
        winding != 0
    }

    /// Fills the profile with triangles (constrained Delaunay, spade).
    pub fn triangulate(&self) -> Result<Fill, ProfileError> {
        ops::triangulate(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_counts_holes_and_boundaries() {
        let washer = circle(&Circle {
            radius: 10.0,
            ..Default::default()
        })
        .unwrap()
        .subtract(
            &circle(&Circle {
                radius: 4.0,
                ..Default::default()
            })
            .unwrap(),
        );
        assert!(washer.contains(DVec2::new(7.0, 0.0)));
        assert!(!washer.contains(DVec2::new(0.0, 0.0)));
        assert!(!washer.contains(DVec2::new(11.0, 0.0)));
        let sq = rect(&Rect {
            width: 2.0,
            height: 2.0,
            ..Default::default()
        })
        .unwrap();
        assert!(sq.contains(DVec2::new(1.0, 0.0)));
    }
}

//! three.js `Path` and `Shape` (og.Path2 and og.Shape of the TypeScript
//! runtime): curves from a current point, flattened to points as three.js's
//! `getPoints(divisions)` and `extractPoints(divisions)` flatten them. The
//! TypeScript classes (`packages/engine/src/geom.ts`) flatten before a
//! shape crosses the wire; a `type model` part names the same calls in its
//! text, and the cook flattens them here. Both use the same formulas in
//! f64 (JavaScript's numbers) and the same series for the arcs
//! ([`crate::trig::sincos`]), so a shape gives the same points either way.

use crate::trig::{TAU, sincos};
use glam::DVec2;

/// three.js's `Number.EPSILON` (the ellipse's angle tests).
const EPSILON: f64 = f64::EPSILON;

/// One of three.js's 2D curves.
#[derive(Clone, Debug, PartialEq)]
enum Curve {
    Line(DVec2, DVec2),
    Quadratic(DVec2, DVec2, DVec2),
    Cubic(DVec2, DVec2, DVec2, DVec2),
    Spline(Vec<DVec2>),
    Ellipse {
        centre: DVec2,
        radius: DVec2,
        start: f64,
        end: f64,
        clockwise: bool,
        rotation: f64,
    },
}

// three.js's Interpolations.
fn catmull_rom(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let v0 = (p2 - p0) * 0.5;
    let v1 = (p3 - p1) * 0.5;
    let t2 = t * t;
    let t3 = t * t2;
    (2.0 * p1 - 2.0 * p2 + v0 + v1) * t3 + (-3.0 * p1 + 3.0 * p2 - 2.0 * v0 - v1) * t2 + v0 * t + p1
}

fn quadratic(t: f64, p0: f64, p1: f64, p2: f64) -> f64 {
    let k = 1.0 - t;
    k * k * p0 + 2.0 * (1.0 - t) * t * p1 + t * t * p2
}

fn cubic(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let k = 1.0 - t;
    k * k * k * p0 + 3.0 * k * k * t * p1 + 3.0 * (1.0 - t) * t * t * p2 + t * t * t * p3
}

impl Curve {
    /// three.js `Curve.getPoint(t)`.
    fn point(&self, t: f64) -> DVec2 {
        match self {
            // LineCurve: the end exactly at 1.
            Curve::Line(a, b) => {
                if t == 1.0 {
                    *b
                } else {
                    DVec2::new((b.x - a.x) * t + a.x, (b.y - a.y) * t + a.y)
                }
            }
            Curve::Quadratic(a, c, b) => {
                DVec2::new(quadratic(t, a.x, c.x, b.x), quadratic(t, a.y, c.y, b.y))
            }
            Curve::Cubic(a, c1, c2, b) => DVec2::new(
                cubic(t, a.x, c1.x, c2.x, b.x),
                cubic(t, a.y, c1.y, c2.y, b.y),
            ),
            Curve::Spline(points) => {
                let n = points.len();
                let p = (n - 1) as f64 * t;
                let i = p.floor() as usize;
                let w = p - i as f64;
                let p0 = points[if i == 0 { i } else { i - 1 }];
                let p1 = points[i];
                let p2 = points[if i + 2 > n { n - 1 } else { i + 1 }];
                let p3 = points[if i + 3 > n { n - 1 } else { i + 2 }];
                DVec2::new(
                    catmull_rom(w, p0.x, p1.x, p2.x, p3.x),
                    catmull_rom(w, p0.y, p1.y, p2.y, p3.y),
                )
            }
            Curve::Ellipse {
                centre,
                radius,
                start,
                end,
                clockwise,
                rotation,
            } => {
                let mut delta = end - start;
                let same = delta.abs() < EPSILON;
                while delta < 0.0 {
                    delta += TAU;
                }
                while delta > TAU {
                    delta -= TAU;
                }
                if delta < EPSILON {
                    delta = if same { 0.0 } else { TAU };
                }
                if *clockwise && !same {
                    delta = if delta == TAU { -TAU } else { delta - TAU };
                }
                let (s, c) = sincos(start + t * delta);
                let mut px = centre.x + radius.x * c;
                let mut py = centre.y + radius.y * s;
                if *rotation != 0.0 {
                    let (rs, rc) = sincos(*rotation);
                    let tx = px - centre.x;
                    let ty = py - centre.y;
                    px = tx * rc - ty * rs + centre.x;
                    py = tx * rs + ty * rc + centre.y;
                }
                DVec2::new(px, py)
            }
        }
    }

    /// How finely three.js's `CurvePath.getPoints` divides it.
    fn resolution(&self, divisions: u32) -> u32 {
        match self {
            Curve::Line(..) => 1,
            Curve::Quadratic(..) | Curve::Cubic(..) => divisions,
            Curve::Spline(p) => divisions * p.len() as u32,
            Curve::Ellipse { .. } => divisions * 2,
        }
    }
}

/// three.js's `Path`: curves from a current point (`moveTo` only moves
/// it). Every method but [`Path2::get_points`] returns the path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path2 {
    curves: Vec<Curve>,
    current: DVec2,
}

impl Path2 {
    pub fn new() -> Path2 {
        Path2::default()
    }

    /// three.js `Path.setFromPoints`: a move to the first, lines to the rest.
    pub fn set_from_points(&mut self, points: &[DVec2]) -> &mut Path2 {
        let Some(first) = points.first() else {
            return self;
        };
        self.move_to(first.x, first.y);
        for p in &points[1..] {
            self.line_to(p.x, p.y);
        }
        self
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> &mut Path2 {
        self.current = DVec2::new(x, y);
        self
    }

    pub fn line_to(&mut self, x: f64, y: f64) -> &mut Path2 {
        let to = DVec2::new(x, y);
        self.curves.push(Curve::Line(self.current, to));
        self.current = to;
        self
    }

    pub fn quadratic_curve_to(&mut self, cpx: f64, cpy: f64, x: f64, y: f64) -> &mut Path2 {
        let to = DVec2::new(x, y);
        self.curves
            .push(Curve::Quadratic(self.current, DVec2::new(cpx, cpy), to));
        self.current = to;
        self
    }

    pub fn bezier_curve_to(&mut self, cp1: DVec2, cp2: DVec2, x: f64, y: f64) -> &mut Path2 {
        let to = DVec2::new(x, y);
        self.curves.push(Curve::Cubic(self.current, cp1, cp2, to));
        self.current = to;
        self
    }

    pub fn spline_thru(&mut self, points: &[DVec2]) -> &mut Path2 {
        let Some(last) = points.last() else {
            return self;
        };
        let mut all = vec![self.current];
        all.extend_from_slice(points);
        self.curves.push(Curve::Spline(all));
        self.current = *last;
        self
    }

    /// three.js `Path.arc`: `absarc` about a centre relative to the current point.
    pub fn arc(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        start: f64,
        end: f64,
        clockwise: bool,
    ) -> &mut Path2 {
        let (cx, cy) = (x + self.current.x, y + self.current.y);
        self.absarc(cx, cy, radius, start, end, clockwise)
    }

    pub fn absarc(
        &mut self,
        x: f64,
        y: f64,
        radius: f64,
        start: f64,
        end: f64,
        clockwise: bool,
    ) -> &mut Path2 {
        self.absellipse(
            DVec2::new(x, y),
            DVec2::splat(radius),
            start,
            end,
            clockwise,
            0.0,
        )
    }

    /// three.js `Path.ellipse`: `absellipse` about a centre relative to the current point.
    pub fn ellipse(
        &mut self,
        centre: DVec2,
        radius: DVec2,
        start: f64,
        end: f64,
        clockwise: bool,
        rotation: f64,
    ) -> &mut Path2 {
        let c = centre + self.current;
        self.absellipse(c, radius, start, end, clockwise, rotation)
    }

    pub fn absellipse(
        &mut self,
        centre: DVec2,
        radius: DVec2,
        start: f64,
        end: f64,
        clockwise: bool,
        rotation: f64,
    ) -> &mut Path2 {
        let curve = Curve::Ellipse {
            centre,
            radius,
            start,
            end,
            clockwise,
            rotation,
        };
        if !self.curves.is_empty() {
            // A previous curve: join it with a line when the arc starts elsewhere.
            let first = curve.point(0.0);
            if first != self.current {
                self.line_to(first.x, first.y);
            }
        }
        self.current = curve.point(1.0);
        self.curves.push(curve);
        self
    }

    /// three.js `Path.closePath`: a line back to the first curve's start.
    pub fn close_path(&mut self) -> &mut Path2 {
        let (Some(first), Some(last)) = (self.curves.first(), self.curves.last()) else {
            return self;
        };
        let (start, end) = (first.point(0.0), last.point(1.0));
        if start != end {
            self.curves.push(Curve::Line(end, start));
        }
        self
    }

    /// three.js `Path.getPoints(divisions)` (`CurvePath.getPoints`): each
    /// curve at its resolution, a point equal to the one before dropped.
    pub fn get_points(&self, divisions: u32) -> Vec<DVec2> {
        let mut out: Vec<DVec2> = Vec::new();
        for curve in &self.curves {
            let resolution = curve.resolution(divisions);
            for d in 0..=resolution {
                let p = curve.point(d as f64 / resolution as f64);
                if out.last() == Some(&p) {
                    continue;
                }
                out.push(p);
            }
        }
        out
    }
}

/// three.js's `Shape`: a path with holes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    pub path: Path2,
    pub holes: Vec<Path2>,
}

impl Shape {
    /// three.js `Shape.extractPoints(divisions)`.
    pub fn extract_points(&self, divisions: u32) -> crate::ShapePoints {
        crate::ShapePoints {
            points: self.path.get_points(divisions),
            holes: self.holes.iter().map(|h| h.get_points(divisions)).collect(),
        }
    }
}

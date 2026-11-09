//! [`body`]: a solid along +y drawn from its views, as a fuselage, a hull,
//! a car or a fish is drawn (sand3d's `body`, `solids.py`).
//!
//! | faces | edges |
//! |---|---|
//! | `side`; `start` at −y and `end` at +y where an end is flat | `start`, `end` |
//!
//! From the first point of `plan` to its last, the top view gives the
//! width (centred on x = 0) and the side view the `top` and `bottom` lines
//! (z). Each section across is a superellipse of squareness `n` (2 round,
//! higher boxier) filling that width and height. A view runs straight
//! between its points, or on a natural cubic spline through them. There is
//! a section at every point a view gives; where a spline or a changing `n`
//! curves the body, 48 more, closer at the ends. Two points at one y are a
//! step, a shoulder: the body reaches it with the first and runs on from
//! the second, 0.05 mm later. An end where both views close is a point;
//! any other end is a flat cap.

use shape_2d::curve::segments;
use shape_2d::{Contour, Profile, Superellipse, superellipse};
use shape_core::{DVec2, DVec3, trig};
use shape_mesh::Solid;

use crate::solids::{GenError, finish};
use crate::spline;

/// One view of a [`Body`]: a value along the body's length.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    /// `(y, value)` points, y never decreasing; two at one y are a step.
    /// Empty: `value` everywhere.
    pub points: Vec<DVec2>,
    /// The value when there are no points.
    pub value: f64,
    /// Through the points on a natural cubic spline instead of straight.
    pub smooth: bool,
}

impl View {
    /// One value along the whole length.
    pub fn constant(value: f64) -> View {
        View {
            value,
            ..Default::default()
        }
    }

    /// Straight between `(y, value)` points.
    pub fn points(points: &[[f64; 2]]) -> View {
        View {
            points: points.iter().map(|p| DVec2::from(*p)).collect(),
            ..Default::default()
        }
    }

    /// On a natural cubic spline through `(y, value)` points.
    pub fn spline(points: &[[f64; 2]]) -> View {
        View {
            smooth: true,
            ..View::points(points)
        }
    }
}

/// A solid along +y from its views.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Body {
    /// Width seen from above, `(y, width)`; its first and last y are the
    /// body's ends. Needs two points at least.
    pub plan: View,
    /// The top line seen from the side, z.
    pub top: View,
    /// The bottom line seen from the side, z.
    pub bottom: View,
    /// Squareness of the sections; 0 (with no points) for 2, round.
    pub n: View,
}

/// Sections a curved body gets along its length besides its views' points.
const SECTIONS: u32 = 48;
/// The length of a step (mm).
const STEP: f64 = 0.05;
/// Below this a width or height is closed (mm).
const TINY: f64 = 1e-3;

/// A view read along y: straight pieces or spline pieces between steps.
struct Reader {
    /// Each piece: its y (strictly increasing) and values.
    pieces: Vec<(Vec<f64>, Vec<f64>, Vec<f64>)>,
    constant: Option<f64>,
    smooth: bool,
}

impl Reader {
    fn new(v: &View, what: &str) -> Result<Reader, GenError> {
        if v.points.is_empty() {
            if !v.value.is_finite() {
                return Err(GenError::NotFinite("body view"));
            }
            return Ok(Reader {
                pieces: Vec::new(),
                constant: Some(v.value),
                smooth: false,
            });
        }
        if v.points.len() < 2 {
            return Err(GenError::Invalid(format!(
                "body: {what} needs two points at least, (y, value)"
            )));
        }
        if v.points.iter().any(|p| !p.is_finite()) {
            return Err(GenError::NotFinite("body view"));
        }
        if v.points.windows(2).any(|w| w[1].x < w[0].x) {
            return Err(GenError::Invalid(format!(
                "body: {what} goes back along y: a view's points run one way"
            )));
        }
        let mut pieces = vec![(vec![v.points[0].x], vec![v.points[0].y], Vec::new())];
        for w in v.points.windows(2) {
            let q = w[1];
            if q.x == w[0].x {
                if q.y != w[0].y {
                    pieces.push((vec![q.x], vec![q.y], Vec::new()));
                }
            } else if let Some(last) = pieces.last_mut() {
                last.0.push(q.x);
                last.1.push(q.y);
            }
        }
        for p in &mut pieces {
            p.2 = if v.smooth {
                spline::second_derivatives(&p.0, &p.1)
            } else {
                vec![0.0; p.0.len()]
            };
        }
        Ok(Reader {
            pieces,
            constant: None,
            smooth: v.smooth,
        })
    }

    /// The y the view runs over.
    fn span(&self) -> Option<(f64, f64)> {
        let first = self.pieces.first()?;
        let last = self.pieces.last()?;
        Some((first.0[0], last.0[last.0.len() - 1]))
    }

    /// Where it steps.
    fn steps(&self) -> Vec<f64> {
        self.pieces.iter().skip(1).map(|p| p.0[0]).collect()
    }

    /// The y of its points.
    fn ys(&self) -> impl Iterator<Item = f64> + '_ {
        self.pieces.iter().flat_map(|p| p.0.iter().copied())
    }

    /// The value at y: what it runs on with, or (`before`) what it
    /// reaches y with.
    fn at(&self, y: f64, before: bool) -> f64 {
        if let Some(c) = self.constant {
            return c;
        }
        let first = &self.pieces[0];
        if y <= first.0[0] {
            return first.1[0];
        }
        let last = &self.pieces[self.pieces.len() - 1];
        if y >= last.0[last.0.len() - 1] {
            return last.1[last.1.len() - 1];
        }
        let inside = |p: &&(Vec<f64>, Vec<f64>, Vec<f64>)| {
            let (a, b) = (p.0[0], p.0[p.0.len() - 1]);
            if before {
                a < y && y <= b
            } else {
                a <= y && y < b
            }
        };
        let Some((ys, vs, second)) = self.pieces.iter().find(inside) else {
            // Between pieces only at a step of one point: its value.
            return self
                .pieces
                .iter()
                .find(|p| p.0[0] == y)
                .map_or(first.1[0], |p| p.1[0]);
        };
        let k = ys.partition_point(|&q| q <= y).clamp(1, ys.len() - 1) - 1;
        let f = (y - ys[k]) / (ys[k + 1] - ys[k]);
        if self.smooth {
            spline::eval(ys, vs, second, k, f)
        } else {
            vs[k] + (vs[k + 1] - vs[k]) * f
        }
    }

    fn curved(&self) -> bool {
        self.smooth
    }
}

/// A solid along +y from its views; see the module.
pub fn body(d: &Body) -> Result<Solid, GenError> {
    let plan = Reader::new(&d.plan, "plan")?;
    let top = Reader::new(&d.top, "top")?;
    let bottom = Reader::new(&d.bottom, "bottom")?;
    let n_view = if d.n.points.is_empty() && d.n.value == 0.0 {
        View::constant(2.0)
    } else {
        d.n.clone()
    };
    let square = Reader::new(&n_view, "n")?;
    let Some((y0, y1)) = plan.span() else {
        return Err(GenError::Invalid(
            "body: plan is the width seen from above, (y, width) points from one end to the other"
                .into(),
        ));
    };
    if y1 <= y0 {
        return Err(GenError::NotPositive("body length"));
    }
    let reach = 1e-6 + 1e-9 * (y0.abs() + y1.abs());
    for (what, v) in [("top", &top), ("bottom", &bottom), ("n", &square)] {
        if let Some((a, b)) = v.span()
            && (a > y0 + reach || b < y1 - reach)
        {
            return Err(GenError::Invalid(format!(
                "body: {what} runs from y = {a} to {b}, the plan from {y0} to {y1}: \
                 each view covers the whole length"
            )));
        }
    }
    let views = [&plan, &top, &bottom, &square];
    // Sections: the ends, every point of a view, and on a curved body
    // 48 more, cosine-spaced (closer at the ends).
    let curved = views.iter().any(|v| v.curved()) || square.constant.is_none();
    let mut ys: Vec<f64> = if curved {
        (0..=SECTIONS)
            .map(|i| {
                let c = trig::cos(trig::PI * (i as f64 / SECTIONS as f64));
                y0 + (y1 - y0) * (1.0 - c) / 2.0
            })
            .collect()
    } else {
        vec![y0, y1]
    };
    for v in views {
        ys.extend(v.ys().filter(|&y| y0 <= y && y <= y1));
    }
    // A step: a section at its y (what the views reach it with) and one
    // a step later (what they run on with); at the far end, a step back.
    let mut before: Vec<f64> = Vec::new();
    let mut all_steps: Vec<f64> = views.iter().flat_map(|v| v.steps()).collect();
    all_steps.sort_by(f64::total_cmp);
    for y in all_steps {
        if y0 + reach < y && y < y1 - reach {
            let after = if y + STEP < y1 - reach { y + STEP } else { y };
            let at = if after > y { y } else { y - STEP };
            ys.push(at);
            ys.push(after);
            before.push(at);
        }
    }
    ys.sort_by(f64::total_cmp);
    let mut kept: Vec<f64> = Vec::with_capacity(ys.len());
    for y in ys {
        if kept.last().is_none_or(|&q| y - q > reach) {
            kept.push(y);
        }
    }
    let ys = kept;
    let last = ys.len() - 1;
    // Each section: width, height, centre, squareness.
    let mut sections = Vec::with_capacity(ys.len());
    for (i, &y) in ys.iter().enumerate() {
        let side = i == last || before.iter().any(|&b| (b - y).abs() <= reach);
        let (w, zt, zb, k) = (
            plan.at(y, side),
            top.at(y, side),
            bottom.at(y, side),
            square.at(y, side),
        );
        if w < -TINY || zt < zb - TINY {
            return Err(GenError::Invalid(if w < -TINY {
                format!("body: at y = {y} the plan's width is below 0")
            } else {
                format!(
                    "body: at y = {y} the top line (z = {zt}) is under the bottom line (z = {zb})"
                )
            }));
        }
        if k.is_nan() || k <= 0.0 {
            return Err(GenError::Invalid(format!(
                "body: n is a squareness above 0 (2 round, 4 boxy), not {k} at y = {y}"
            )));
        }
        let closes = (w <= TINY, zt - zb <= TINY);
        if closes.0 && closes.1 && (i == 0 || i == last) {
            sections.push((y, None, (zt + zb) / 2.0, k));
            continue;
        }
        if closes.0 || closes.1 {
            return Err(GenError::Invalid(format!(
                "body: at y = {y} only one view closes: a body closes to a point where both \
                 views close, at an end"
            )));
        }
        sections.push((y, Some((w / 2.0, (zt - zb) / 2.0)), (zt + zb) / 2.0, k));
    }
    // One point count for every section, by the widest.
    let widest = sections
        .iter()
        .filter_map(|s| s.1)
        .fold(0.0f64, |m, (a, b)| m.max(a).max(b));
    let count = segments(widest, trig::TAU).max(4);
    let mut s = Solid::default();
    let mut rings: Vec<Vec<u32>> = Vec::with_capacity(sections.len());
    let mut flats: Vec<Option<Vec<DVec2>>> = Vec::with_capacity(sections.len());
    for &(y, half, centre, k) in &sections {
        match half {
            None => {
                let apex = s.push_vertex(DVec3::new(0.0, y, centre));
                rings.push(vec![apex; count as usize]);
                flats.push(None);
            }
            Some((rx, ry)) => {
                let p = superellipse(&Superellipse {
                    rx,
                    ry,
                    exponent: k,
                    segments: count,
                })?;
                let pts = p.regions[0].outer.points.clone();
                rings.push(
                    pts.iter()
                        .map(|q| s.push_vertex(DVec3::new(q.x, y, centre + q.y)))
                        .collect(),
                );
                flats.push(Some(pts));
            }
        }
    }
    let n = count as usize;
    for w in rings.windows(2) {
        let (lo, hi) = (&w[0], &w[1]);
        for k in 0..n {
            let k1 = (k + 1) % n;
            s.push_triangle([lo[k], hi[k1], lo[k1]], 2, &["side"]);
            s.push_triangle([lo[k], hi[k], hi[k1]], 2, &["side"]);
        }
    }
    // Flat ends: the section filled, `start` facing −y, `end` facing +y.
    for (i, name, surface) in [(0, "start", 0), (last, "end", 1)] {
        let Some(pts) = &flats[i] else {
            continue;
        };
        let fill = Profile::from_contour(Contour::smooth(pts.clone())).triangulate()?;
        let (y, centre) = (sections[i].0, sections[i].2);
        let ids: Vec<u32> = fill
            .points
            .iter()
            .map(|q| s.push_vertex(DVec3::new(q.x, y, centre + q.y)))
            .collect();
        for t in &fill.triangles {
            // Counter-clockwise in (x, z) faces −y.
            let t = if i == 0 { *t } else { [t[0], t[2], t[1]] };
            s.push_triangle(t.map(|v| ids[v as usize]), surface, &[name]);
        }
        let ring = &rings[i];
        for k in 0..n {
            s.tag_edge(ring[k], ring[(k + 1) % n], &[name]);
        }
    }
    finish(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn closed(s: &Solid) -> f64 {
        let m = s.measure();
        assert!(m.closed, "open: {m:?}");
        assert_eq!(m.degenerate_triangles, 0);
        assert_eq!(m.components, 1);
        m.volume
    }

    /// The area of a section polygon of `count` points.
    fn polygon(rx: f64, ry: f64, count: u32) -> f64 {
        superellipse(&Superellipse {
            rx,
            ry,
            exponent: 2.0,
            segments: count,
        })
        .unwrap()
        .area()
    }

    #[test]
    fn a_body_of_constant_views_is_an_elliptic_prism() {
        let s = body(&Body {
            plan: View::points(&[[-50.0, 20.0], [50.0, 20.0]]),
            top: View::constant(15.0),
            bottom: View::constant(5.0),
            ..Default::default()
        })
        .unwrap();
        let count = segments(10.0, trig::TAU);
        let want = polygon(10.0, 5.0, count) * 100.0;
        assert!((closed(&s) - want).abs() < 1e-9 * want);
        let m = s.measure();
        assert_eq!((m.bounds_min.y, m.bounds_max.y), (-50.0, 50.0));
        assert_eq!((m.bounds_min.z, m.bounds_max.z), (5.0, 15.0));
        assert_eq!(s.edges(&["start"]).unwrap().len(), count as usize);
        assert!(!s.faces(&["end"]).unwrap().is_empty());
    }

    #[test]
    fn a_body_closing_to_a_point_is_a_cone() {
        // Width and height grow linearly from a point: an elliptic cone,
        // a third of its base times its length.
        let s = body(&Body {
            plan: View::points(&[[0.0, 0.0], [60.0, 20.0]]),
            top: View::points(&[[0.0, 0.0], [60.0, 6.0]]),
            bottom: View::points(&[[0.0, 0.0], [60.0, -6.0]]),
            ..Default::default()
        })
        .unwrap();
        let count = segments(10.0, trig::TAU);
        let want = polygon(10.0, 6.0, count) * 60.0 / 3.0;
        assert!(
            (closed(&s) - want).abs() < 1e-9 * want,
            "{}",
            s.measure().volume
        );
        assert!(s.faces(&["start"]).is_err());
        assert!(s.faces(&["end"]).is_ok());
    }

    #[test]
    fn a_step_is_a_shoulder_and_a_spline_nose_is_smooth() {
        // A boxy cabin stepping up from a round nose: a spline plan, a
        // step in the top line, n from 2 to 4.
        let s = body(&Body {
            plan: View::spline(&[[0.0, 0.0], [20.0, 30.0], [80.0, 40.0], [120.0, 36.0]]),
            top: View::points(&[[0.0, 0.0], [50.0, 15.0], [50.0, 30.0], [120.0, 28.0]]),
            bottom: View::constant(0.0),
            n: View::points(&[[0.0, 2.0], [120.0, 4.0]]),
        })
        .unwrap();
        let v = closed(&s);
        assert!(v > 0.0);
        // Both sides of the step are sections 0.05 apart.
        let near: Vec<f64> = s
            .positions
            .iter()
            .filter(|p| (p.y - 50.0).abs() < 0.1)
            .map(|p| p.y)
            .collect();
        assert!(near.contains(&50.0) && near.iter().any(|&y| (y - 50.05).abs() < 1e-12));
        // The nose is a point; the cabin's roof falls from 30 after the step.
        assert!(s.faces(&["start"]).is_err());
        let top = s.measure().bounds_max.z;
        assert!(29.99 < top && top < 30.0, "{top}");
    }

    #[test]
    fn bad_views_are_error_values() {
        let only_plan_closes = body(&Body {
            plan: View::points(&[[0.0, 0.0], [10.0, 5.0]]),
            top: View::constant(5.0),
            bottom: View::constant(0.0),
            ..Default::default()
        });
        assert!(matches!(only_plan_closes, Err(GenError::Invalid(_))));
        let short = body(&Body {
            plan: View::points(&[[0.0, 4.0], [10.0, 5.0]]),
            top: View::points(&[[2.0, 4.0], [10.0, 5.0]]),
            bottom: View::constant(0.0),
            ..Default::default()
        });
        assert!(matches!(short, Err(GenError::Invalid(_))));
        assert!(body(&Body::default()).is_err());
    }
}

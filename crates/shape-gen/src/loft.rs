//! [`loft`]: a solid through sections stacked along +z, each a profile of
//! one region whose holes match in count.
//!
//! | faces | edges |
//! |---|---|
//! | `bottom`, `top`, `side` (`inner` on hole walls too) | `bottom`, `top` |
//!
//! Every contour is resampled to one count by length, starting at its
//! point of largest x. The outline runs to the outline, and each hole to
//! the next section's hole whose centre is nearest. Between sections the
//! points run straight, on a Catmull-Rom curve, or on a natural cubic
//! spline through every section by height (sand3d's, `solids.py` `loft`).

use shape_2d::{Contour, Profile, ProfileError, Region};
use shape_core::{DVec2, DVec3};
use shape_mesh::Solid;

use crate::solids::{GenError, finish};
use crate::spline;

/// How a loft's surface runs between its sections.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoftCurve {
    /// Straight with one step, Catmull-Rom with more.
    #[default]
    Auto,
    /// Straight between consecutive sections, whatever the steps.
    Straight,
    /// A uniform Catmull-Rom curve through the sections (local, C¹).
    CatmullRom,
    /// A natural cubic spline through every section, by height (global,
    /// C², sand3d's). With `steps` 0: one step for two sections, else
    /// about six per span, more on longer spans (sand3d's rule).
    Spline,
}

/// A loft through sections stacked along +z.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Loft {
    /// The sections: a profile of one region and its height, bottom first,
    /// heights increasing. Every section has as many holes.
    pub sections: Vec<(Profile, f64)>,
    /// Points each contour is resampled to; 0 for the most any section
    /// has for that contour.
    pub samples: u32,
    /// Rings between consecutive sections (0 for 1, or the spline's rule).
    pub steps: u32,
    /// How the surface runs between sections.
    pub curve: LoftCurve,
}

/// Steps the spline takes per span with `steps` 0: sand3d's `LOFT_STEPS`.
const SPLINE_STEPS: f64 = 6.0;

/// Resamples a closed contour to `n` points evenly by length, starting at
/// the point with the largest x (then the smallest y).
fn resample(c: &Contour, n: usize) -> Vec<DVec2> {
    let m = c.points.len();
    let start = (0..m)
        .max_by(|&a, &b| {
            let (p, q) = (c.points[a], c.points[b]);
            p.x.total_cmp(&q.x).then(q.y.total_cmp(&p.y))
        })
        .unwrap_or(0);
    let pts: Vec<DVec2> = (0..=m).map(|i| c.points[(start + i) % m]).collect();
    let mut acc = vec![0.0];
    for w in pts.windows(2) {
        acc.push(acc.last().copied().unwrap_or(0.0) + (w[1] - w[0]).length());
    }
    let total = *acc.last().unwrap_or(&0.0);
    let mut out = Vec::with_capacity(n);
    let mut seg = 0;
    for k in 0..n {
        let target = total * (k as f64 / n as f64);
        while seg + 1 < acc.len() - 1 && acc[seg + 1] < target {
            seg += 1;
        }
        let len = acc[seg + 1] - acc[seg];
        let f = if len > 0.0 {
            (target - acc[seg]) / len
        } else {
            0.0
        };
        out.push(pts[seg] + (pts[seg + 1] - pts[seg]) * f);
    }
    out
}

fn centroid(c: &Contour) -> DVec2 {
    let n = c.points.len().max(1) as f64;
    c.points.iter().fold(DVec2::ZERO, |a, p| a + *p) / n
}

/// Per contour slot (the outline, then each hole), its contour in every
/// section: holes matched to the previous section's by nearest centre.
fn slots(d: &Loft) -> Result<Vec<Vec<&Contour>>, GenError> {
    let regions: Vec<&Region> = d.sections.iter().map(|(p, _)| &p.regions[0]).collect();
    let holes = regions[0].holes.len();
    if let Some(i) = regions.iter().position(|r| r.holes.len() != holes) {
        return Err(GenError::Invalid(format!(
            "loft: section {i} has {} holes, section 0 has {holes}: every section needs as many",
            regions[i].holes.len()
        )));
    }
    let mut slots: Vec<Vec<&Contour>> = vec![regions.iter().map(|r| &r.outer).collect()];
    slots.extend((0..holes).map(|h| vec![&regions[0].holes[h]]));
    for r in &regions[1..] {
        let mut used = vec![false; holes];
        for slot in slots.iter_mut().skip(1) {
            let prev = centroid(slot[slot.len() - 1]);
            let mut best: Option<(f64, usize)> = None;
            for (j, h) in r.holes.iter().enumerate() {
                let dist = (centroid(h) - prev).length_squared();
                if !used[j] && best.is_none_or(|(b, _)| dist < b) {
                    best = Some((dist, j));
                }
            }
            let (_, j) = best.ok_or(GenError::Profile(ProfileError::Empty))?;
            used[j] = true;
            slot.push(&r.holes[j]);
        }
    }
    Ok(slots)
}

/// A surface through the sections, capped `bottom` and `top`.
pub fn loft(d: &Loft) -> Result<Solid, GenError> {
    let k = d.sections.len();
    if k < 2 {
        return Err(GenError::Profile(ProfileError::TooFewPoints(k)));
    }
    for (p, z) in &d.sections {
        if p.regions.len() != 1 {
            return Err(GenError::Profile(ProfileError::Empty));
        }
        if !z.is_finite() {
            return Err(GenError::NotFinite("height"));
        }
    }
    if d.sections.windows(2).any(|w| w[1].1 <= w[0].1) {
        return Err(GenError::NotPositive("height step"));
    }
    let heights: Vec<f64> = d.sections.iter().map(|s| s.1).collect();
    let slots = slots(d)?;
    // Per slot, per section, the resampled ring in the section's plane.
    let rings2: Vec<Vec<Vec<DVec2>>> = slots
        .iter()
        .map(|cs| {
            let n = if d.samples >= 3 {
                d.samples as usize
            } else {
                cs.iter().map(|c| c.points.len()).max().unwrap_or(3)
            };
            cs.iter().map(|c| resample(c, n)).collect()
        })
        .collect();
    // Rings per span.
    let spans: Vec<usize> = match d.curve {
        LoftCurve::Spline if d.steps == 0 => {
            if k == 2 {
                vec![1]
            } else {
                let mean = (heights[k - 1] - heights[0]) / (k - 1) as f64;
                heights
                    .windows(2)
                    .map(|w| ((SPLINE_STEPS * (w[1] - w[0]) / mean).round() as usize).max(2))
                    .collect()
            }
        }
        _ => vec![d.steps.max(1) as usize; k - 1],
    };
    let mut s = Solid::default();
    let mut ring_ids: Vec<Vec<Vec<u32>>> = Vec::with_capacity(slots.len());
    for ring2 in &rings2 {
        let n = ring2[0].len();
        // Per point, its x and y through the sections and the spline's
        // second derivatives of each.
        let through: Vec<[Vec<f64>; 4]> = if d.curve == LoftCurve::Spline {
            (0..n)
                .map(|j| {
                    let xs: Vec<f64> = ring2.iter().map(|r| r[j].x).collect();
                    let ys: Vec<f64> = ring2.iter().map(|r| r[j].y).collect();
                    let sx = spline::second_derivatives(&heights, &xs);
                    let sy = spline::second_derivatives(&heights, &ys);
                    [xs, ys, sx, sy]
                })
                .collect()
        } else {
            Vec::new()
        };
        // Point j of the ring at fraction f of span sec.
        let at = |sec: usize, f: f64, j: usize| -> DVec3 {
            let get = |i: isize| {
                let i = i.clamp(0, k as isize - 1) as usize;
                (ring2[i][j], heights[i])
            };
            let i = sec as isize;
            let (p0, z0) = get(i - 1);
            let (p1, z1) = get(i);
            let (p2, z2) = get(i + 1);
            let (p3, z3) = get(i + 2);
            let straight = match d.curve {
                LoftCurve::Auto => spans[sec] == 1,
                LoftCurve::Straight => true,
                LoftCurve::CatmullRom => false,
                LoftCurve::Spline => {
                    let [xs, ys, sx, sy] = &through[j];
                    return DVec3::new(
                        spline::eval(&heights, xs, sx, sec, f),
                        spline::eval(&heights, ys, sy, sec, f),
                        z1 + (z2 - z1) * f,
                    );
                }
            };
            if straight {
                let p = p1 + (p2 - p1) * f;
                return DVec3::new(p.x, p.y, z1 + (z2 - z1) * f);
            }
            let cr = |a: f64, b: f64, c: f64, e: f64| {
                let (f2, f3) = (f * f, f * f * f);
                0.5 * (2.0 * b
                    + (c - a) * f
                    + (2.0 * a - 5.0 * b + 4.0 * c - e) * f2
                    + (3.0 * b - a - 3.0 * c + e) * f3)
            };
            DVec3::new(
                cr(p0.x, p1.x, p2.x, p3.x),
                cr(p0.y, p1.y, p2.y, p3.y),
                cr(z0, z1, z2, z3),
            )
        };
        let mut rings: Vec<Vec<u32>> = Vec::new();
        for (sec, &steps) in spans.iter().enumerate() {
            for st in 0..steps {
                let f = st as f64 / steps as f64;
                rings.push((0..n).map(|j| s.push_vertex(at(sec, f, j))).collect());
            }
        }
        rings.push(
            (0..n)
                .map(|j| {
                    s.push_vertex(DVec3::new(
                        ring2[k - 1][j].x,
                        ring2[k - 1][j].y,
                        heights[k - 1],
                    ))
                })
                .collect(),
        );
        ring_ids.push(rings);
    }
    // Walls: the outline on surface 2, each hole on its own.
    for (slot, rings) in ring_ids.iter().enumerate() {
        let tags: &[&str] = if slot == 0 {
            &["side"]
        } else {
            &["side", "inner"]
        };
        let surface = 2 + slot as u32;
        for w in rings.windows(2) {
            let n = w[0].len();
            for j in 0..n {
                let j1 = (j + 1) % n;
                s.push_triangle([w[0][j], w[0][j1], w[1][j1]], surface, tags);
                s.push_triangle([w[0][j], w[1][j1], w[1][j]], surface, tags);
            }
        }
    }
    // Caps from the end rings, holes and all.
    for (end, (name, surface, z)) in [
        (0, ("bottom", 0, heights[0])),
        (1, ("top", 1, heights[k - 1])),
    ] {
        let ends: Vec<&Vec<u32>> = ring_ids
            .iter()
            .map(|r| if end == 0 { &r[0] } else { &r[r.len() - 1] })
            .collect();
        let flat = |ring: &Vec<u32>| -> Contour {
            Contour::smooth(
                ring.iter()
                    .map(|&v| s.positions[v as usize].truncate())
                    .collect(),
            )
        };
        let outer = flat(ends[0]);
        let outer = if outer.signed_area2() < 0.0 {
            outer.reversed()
        } else {
            outer
        };
        let holes = ends[1..]
            .iter()
            .map(|r| {
                let h = flat(r);
                if h.signed_area2() > 0.0 {
                    h.reversed()
                } else {
                    h
                }
            })
            .collect();
        let cap = Profile {
            regions: vec![Region { outer, holes }],
        };
        let fill = cap.triangulate()?;
        let ids: Vec<u32> = fill
            .points
            .iter()
            .map(|p| s.push_vertex(p.extend(z)))
            .collect();
        for t in &fill.triangles {
            let t = if name == "bottom" {
                [t[0], t[2], t[1]]
            } else {
                *t
            };
            s.push_triangle(t.map(|i| ids[i as usize]), surface, &[name]);
        }
        for ring in ends {
            let n = ring.len();
            for j in 0..n {
                s.tag_edge(ring[j], ring[(j + 1) % n], &[name]);
            }
        }
    }
    finish(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_2d::{Circle, Rect, circle, rect};

    fn square(side: f64) -> Profile {
        rect(&Rect {
            width: side,
            height: side,
            radius: 0.0,
        })
        .unwrap()
    }

    fn closed(s: &Solid) -> f64 {
        let m = s.measure();
        assert!(m.closed, "open: {m:?}");
        assert_eq!(m.degenerate_triangles, 0);
        m.volume
    }

    #[test]
    fn a_loft_from_square_to_circle_is_closed() {
        let ci = circle(&Circle {
            radius: 4.0,
            segments: 0,
        })
        .unwrap();
        for steps in [1, 4] {
            let s = loft(&Loft {
                sections: vec![
                    (square(10.0), 0.0),
                    (ci.clone(), 10.0),
                    (square(10.0), 20.0),
                ],
                samples: 32,
                steps,
                ..Default::default()
            })
            .unwrap();
            closed(&s);
            assert!(!s.faces(&["top"]).unwrap().is_empty());
        }
    }

    #[test]
    fn a_square_frame_lofts_through_its_hole() {
        let frame = |outer: f64, inner: f64| square(outer).subtract(&square(inner));
        let s = loft(&Loft {
            sections: vec![(frame(20.0, 10.0), 0.0), (frame(20.0, 10.0), 10.0)],
            samples: 16,
            ..Default::default()
        })
        .unwrap();
        // Corners land on samples (16 a square), so the area is exact.
        assert!((closed(&s) - 3000.0).abs() < 1e-9);
        assert_eq!(s.faces(&["inner"]).unwrap().len(), 32);
        // Both rims of both caps are named.
        assert_eq!(s.edges(&["top"]).unwrap().len(), 32);
        // A frame tapering inward: the outline and the hole both shrink
        // linearly, so the volume is a prismatoid's.
        let s = loft(&Loft {
            sections: vec![(frame(20.0, 10.0), 0.0), (frame(10.0, 4.0), 6.0)],
            samples: 16,
            ..Default::default()
        })
        .unwrap();
        let area = |a0: f64, a1: f64| 6.0 * (a0 * a0 + a0 * a1 + a1 * a1) / 3.0;
        let want = area(20.0, 10.0) - area(10.0, 4.0);
        assert!((closed(&s) - want).abs() < 1e-9, "{}", s.measure().volume);
    }

    #[test]
    fn holes_must_match_in_count() {
        let r = loft(&Loft {
            sections: vec![
                (square(20.0).subtract(&square(5.0)), 0.0),
                (square(20.0), 5.0),
            ],
            ..Default::default()
        });
        assert!(matches!(r, Err(GenError::Invalid(_))));
    }

    #[test]
    fn a_spline_loft_through_linear_sections_is_a_frustum() {
        // Half-widths 5 + z / 10 at four heights, unevenly spaced: the
        // natural spline through linear data is that line.
        let zs = [0.0, 4.0, 10.0, 20.0];
        let s = loft(&Loft {
            sections: zs
                .iter()
                .map(|&z| (square(2.0 * (5.0 + z / 10.0)), z))
                .collect(),
            samples: 16,
            curve: LoftCurve::Spline,
            ..Default::default()
        })
        .unwrap();
        // ∫ (2a)² dz with a = 5 + z/10, z from 0 to 20.
        let want = 4.0 * (10.0 * (343.0 - 125.0) / 3.0);
        assert!(
            (closed(&s) - want).abs() / want < 1e-12,
            "{}",
            s.measure().volume
        );
    }

    #[test]
    fn the_spline_bulges_where_catmull_rom_differs() {
        // A bulge in the middle: square, wider, square. Both curves pass
        // through the sections; they part between them.
        let sections = vec![
            (square(10.0), 0.0),
            (square(20.0), 10.0),
            (square(10.0), 15.0),
        ];
        let spline = loft(&Loft {
            sections: sections.clone(),
            samples: 16,
            curve: LoftCurve::Spline,
            ..Default::default()
        })
        .unwrap();
        let cr = loft(&Loft {
            sections,
            samples: 16,
            steps: 6,
            ..Default::default()
        })
        .unwrap();
        let (a, b) = (closed(&spline), closed(&cr));
        assert!(a > 0.0 && b > 0.0 && (a - b).abs() > 1.0, "{a} {b}");
    }
}

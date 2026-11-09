//! [`loft`]: a solid through sections stacked along +z, each a profile of
//! one region whose holes match in count.
//!
//! | faces | edges |
//! |---|---|
//! | `bottom`, `top`, `side` (`inner` on hole walls too) | `bottom`, `top` |
//!
//! Every contour is resampled to one count by length. The outline runs to
//! the outline, and each hole to the next section's hole whose centre is
//! nearest. Between sections the points run straight, on a Catmull-Rom
//! curve, or on a natural cubic spline through every section by height
//! (sand3d's, `solids.py` `loft`).
//!
//! Resampling, per contour slot (the outline, or one hole through every
//! section):
//!
//! - **No corner in any section** (circles, ellipses, rounded shapes):
//!   evenly by length from the point of largest x, then smallest y.
//! - **A corner in some section** (a sharp point of the [`Contour`]):
//!   corners are kept as samples. Section 0 starts at its corner of largest
//!   x, then smallest y; each later section at its corner nearest the
//!   previous start by angle about the contour's centre (a contour without
//!   corners starts at its point nearest by angle). The runs between
//!   corners share the samples by length, at least one each; when every
//!   section has as many corners, one share is taken for all (by length
//!   relative to each section's perimeter), so corner `i` of one section
//!   runs to corner `i` of the next. The count is raised to the most
//!   corners a section has.
//!
//! A Catmull-Rom curve carries z on the same curve as x and y; where that
//! would not rise strictly through the rings (uneven heights overshoot
//! past an end section), z runs straight between sections instead. Rings
//! stay flat either way. The loft does not test its surface for folds
//! (points overshooting through each other on a curve); `shape-model`
//! does, with `shape-bool`'s exact check.

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

/// The corners of a contour: its sharp point indices.
fn corners(c: &Contour) -> Vec<usize> {
    (0..c.points.len())
        .filter(|&i| c.sharp.get(i).copied().unwrap_or(false))
        .collect()
}

/// Of `candidates` (point indices of `c`), the first whose direction from
/// the centre `o` is nearest `dir` by angle (largest cosine); ties to the
/// earlier candidate.
fn nearest_by_angle(c: &Contour, candidates: &[usize], o: DVec2, dir: DVec2) -> usize {
    let mut best: Option<(f64, f64, usize)> = None;
    for &i in candidates {
        let v = c.points[i] - o;
        let len = v.length();
        if len == 0.0 {
            continue;
        }
        let dot = v.dot(dir);
        // dot / len > b_dot / b_len, without dividing.
        if best.is_none_or(|(bd, bl, _)| dot * bl > bd * len) {
            best = Some((dot, len, i));
        }
    }
    best.map_or(candidates[0], |b| b.2)
}

/// Shares `n` samples among runs of the given weights: at least one each,
/// the rest by largest remainder (ties to the earlier run).
fn share(n: usize, weights: &[f64]) -> Vec<usize> {
    let c = weights.len();
    let total: f64 = weights.iter().sum();
    let w: Vec<f64> = if total > 0.0 && total.is_finite() {
        weights.iter().map(|x| x / total).collect()
    } else {
        vec![1.0 / c as f64; c]
    };
    let extra = n.saturating_sub(c) as f64;
    let mut counts: Vec<usize> = w.iter().map(|x| 1 + (extra * x).floor() as usize).collect();
    let rem: Vec<f64> = w
        .iter()
        .zip(&counts)
        .map(|(x, k)| extra * x - (*k - 1) as f64)
        .collect();
    let mut order: Vec<usize> = (0..c).collect();
    order.sort_by(|&a, &b| rem[b].total_cmp(&rem[a]).then(a.cmp(&b)));
    let left = n.saturating_sub(counts.iter().sum());
    for k in 0..left {
        counts[order[k % c]] += 1;
    }
    counts
}

/// One contour with its runs: the anchor indices (its corners, or its
/// start point alone) in order from the start, and each run's length.
struct Runs {
    anchors: Vec<usize>,
    lengths: Vec<f64>,
}

fn runs_from(c: &Contour, anchors: Vec<usize>) -> Runs {
    let m = c.points.len();
    let lengths = (0..anchors.len())
        .map(|e| {
            let a = anchors[e];
            let b = anchors[(e + 1) % anchors.len()];
            let steps = if b > a { b - a } else { b + m - a };
            (0..steps)
                .map(|i| (c.points[(a + i + 1) % m] - c.points[(a + i) % m]).length())
                .sum()
        })
        .collect();
    Runs { anchors, lengths }
}

/// The contour's points along the run from anchor `e`, `count` samples
/// evenly by length, the anchor first.
fn sample_run(c: &Contour, r: &Runs, e: usize, count: usize, out: &mut Vec<DVec2>) {
    let m = c.points.len();
    let a = r.anchors[e];
    let b = r.anchors[(e + 1) % r.anchors.len()];
    let steps = if b > a { b - a } else { b + m - a };
    let total = r.lengths[e];
    let (mut seg, mut acc) = (0, 0.0);
    for k in 0..count {
        let target = total * (k as f64 / count as f64);
        loop {
            let len = (c.points[(a + seg + 1) % m] - c.points[(a + seg) % m]).length();
            if seg + 1 < steps && acc + len < target {
                acc += len;
                seg += 1;
            } else {
                break;
            }
        }
        let (p, q) = (c.points[(a + seg) % m], c.points[(a + seg + 1) % m]);
        let len = (q - p).length();
        let f = if len > 0.0 { (target - acc) / len } else { 0.0 };
        out.push(p + (q - p) * f);
    }
}

/// A slot whose sections have corners, resampled keeping them (see the
/// module's notes).
fn resample_corners(cs: &[&Contour], n: usize) -> Vec<Vec<DVec2>> {
    let counts: Vec<Vec<usize>> = cs.iter().map(|c| corners(c)).collect();
    let n = n.max(counts.iter().map(Vec::len).max().unwrap_or(0));
    // Each section's runs, from its start.
    let mut runs: Vec<Runs> = Vec::with_capacity(cs.len());
    for (s, c) in cs.iter().enumerate() {
        let cand: Vec<usize> = if counts[s].is_empty() {
            (0..c.points.len()).collect()
        } else {
            counts[s].clone()
        };
        let start = if s == 0 {
            cand.iter()
                .copied()
                .max_by(|&a, &b| {
                    let (p, q) = (c.points[a], c.points[b]);
                    p.x.total_cmp(&q.x).then(q.y.total_cmp(&p.y))
                })
                .unwrap_or(0)
        } else {
            let prev = &runs[s - 1];
            let pc = cs[s - 1];
            let dir = pc.points[prev.anchors[0]] - centroid(pc);
            nearest_by_angle(c, &cand, centroid(c), dir)
        };
        let anchors = if counts[s].is_empty() {
            vec![start]
        } else {
            let k = cand.iter().position(|&i| i == start).unwrap_or(0);
            (0..cand.len())
                .map(|i| cand[(k + i) % cand.len()])
                .collect()
        };
        runs.push(runs_from(c, anchors));
    }
    // One share for all when the corner counts agree, else one each.
    let same = counts
        .iter()
        .all(|k| !k.is_empty() && k.len() == counts[0].len());
    let shared = same.then(|| {
        let mut w = vec![0.0; counts[0].len()];
        for r in &runs {
            let t: f64 = r.lengths.iter().sum();
            for (e, l) in r.lengths.iter().enumerate() {
                if t > 0.0 {
                    w[e] += l / t;
                }
            }
        }
        share(n, &w)
    });
    cs.iter()
        .zip(&runs)
        .map(|(c, r)| {
            let per = shared.clone().unwrap_or_else(|| share(n, &r.lengths));
            let mut out = Vec::with_capacity(n);
            for (e, &k) in per.iter().enumerate() {
                sample_run(c, r, e, k, &mut out);
            }
            out
        })
        .collect()
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
            if cs.iter().any(|c| !corners(c).is_empty()) {
                resample_corners(cs, n)
            } else {
                cs.iter().map(|c| resample(c, n)).collect()
            }
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
    // Catmull-Rom z, unless it would not rise strictly through the rings.
    let cr = |a: f64, b: f64, c: f64, e: f64, f: f64| {
        let (f2, f3) = (f * f, f * f * f);
        0.5 * (2.0 * b
            + (c - a) * f
            + (2.0 * a - 5.0 * b + 4.0 * c - e) * f2
            + (3.0 * b - a - 3.0 * c + e) * f3)
    };
    let curved = |sec: usize| match d.curve {
        LoftCurve::Auto => spans[sec] != 1,
        LoftCurve::CatmullRom => true,
        LoftCurve::Straight | LoftCurve::Spline => false,
    };
    let ring_z = |sec: usize, f: f64| {
        let h = |i: isize| heights[i.clamp(0, k as isize - 1) as usize];
        let i = sec as isize;
        cr(h(i - 1), h(i), h(i + 1), h(i + 2), f)
    };
    let mut zs = Vec::new();
    for (sec, &steps) in spans.iter().enumerate() {
        for st in 0..steps {
            let f = st as f64 / steps as f64;
            zs.push(if curved(sec) {
                ring_z(sec, f)
            } else {
                heights[sec] + (heights[sec + 1] - heights[sec]) * f
            });
        }
    }
    zs.push(heights[k - 1]);
    let straight_z = zs.windows(2).any(|w| w[1] <= w[0]);
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
            let z = if straight_z {
                z1 + (z2 - z1) * f
            } else {
                cr(z0, z1, z2, z3, f)
            };
            DVec3::new(
                cr(p0.x, p1.x, p2.x, p3.x, f),
                cr(p0.y, p1.y, p2.y, p3.y, f),
                z,
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
    use shape_2d::{Circle, Polygon, Rect, circle, polygon, rect};

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

    fn hexagon(radius: f64) -> Profile {
        polygon(&Polygon { sides: 6, radius }).unwrap()
    }

    #[test]
    fn corners_stay_samples_and_line_up() {
        // 32 samples do not divide by 6: evenly by length would cut every
        // corner but the first. Kept, the loft is an exact frustum.
        let s = loft(&Loft {
            sections: vec![(hexagon(10.0), 0.0), (hexagon(5.0), 10.0)],
            samples: 32,
            ..Default::default()
        })
        .unwrap();
        let area = |r: f64| 1.5 * 3f64.sqrt() * r * r;
        let (a0, a1) = (area(10.0), area(5.0));
        let want = 10.0 / 3.0 * (a0 + (a0 * a1).sqrt() + a1);
        assert!((closed(&s) - want).abs() < 1e-9, "{}", s.measure().volume);
        // A hexagon turned by 10°: corner i runs to corner i, so every wall
        // triangle's bottom corner sits under a top corner turned 10°, and
        // the volume is the untwisted one's (no corner cut).
        let turned = hexagon(5.0).rotate(shape_core::trig::to_radians(10.0));
        let s = loft(&Loft {
            sections: vec![(hexagon(10.0), 0.0), (turned, 10.0)],
            samples: 32,
            ..Default::default()
        })
        .unwrap();
        let tops: Vec<DVec3> = s
            .positions
            .iter()
            .copied()
            .filter(|p| p.z == 10.0)
            .collect();
        let c = shape_core::trig::cos(shape_core::trig::to_radians(10.0)) * 5.0;
        assert!(tops.iter().any(|p| (p.x - c).abs() < 1e-12));
        closed(&s);
    }

    #[test]
    fn a_square_to_a_circle_starts_at_the_matching_angle() {
        let ci = circle(&Circle {
            radius: 5.0,
            segments: 64,
        })
        .unwrap();
        let s = loft(&Loft {
            sections: vec![(square(10.0), 0.0), (ci, 10.0)],
            samples: 16,
            ..Default::default()
        })
        .unwrap();
        closed(&s);
        // The square's corners are samples; the wall from each corner runs
        // up to the circle's point at the same angle (−45°, …).
        let corner = s
            .positions
            .iter()
            .any(|p| p.z == 0.0 && p.x == 5.0 && p.y == -5.0);
        assert!(corner);
        let mut up = false;
        for t in &s.triangles {
            let p = t.map(|v| s.positions[v as usize]);
            for (a, b) in [(p[0], p[1]), (p[1], p[2]), (p[2], p[0])] {
                let from_corner = a.z == 0.0 && a.x == 5.0 && a.y == -5.0;
                up |= from_corner && b.z == 10.0 && (b.x + b.y).abs() < 1e-9 && b.x > 0.0;
            }
        }
        assert!(up);
    }

    #[test]
    fn uneven_heights_stay_between_the_end_sections() {
        // Catmull-Rom on z with heights 0, 1, 20, 21 ran to z = -0.69 and
        // 21.69: the solid hung past both caps.
        let ci = |r: f64| {
            circle(&Circle {
                radius: r,
                segments: 16,
            })
            .unwrap()
        };
        let s = loft(&Loft {
            sections: vec![
                (ci(10.0), 0.0),
                (ci(10.0), 1.0),
                (ci(8.0), 20.0),
                (ci(0.5), 21.0),
            ],
            steps: 4,
            ..Default::default()
        })
        .unwrap();
        closed(&s);
        let m = s.measure();
        assert_eq!((m.bounds_min.z, m.bounds_max.z), (0.0, 21.0));
    }
}

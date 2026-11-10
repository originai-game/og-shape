//! `hole`, `pocket`, `seat` and `dent` swept on a plate and a pad
//! (`docs/research.md` §25.1, §27): one line per case with the crossing
//! pairs of the result (exact, `shape_bool::crossing_pairs`), whether it
//! is closed, and its volume, or the error.
//!
//! `cargo run --release -p shape-edit --example edit_sweep > sweep.txt`

use shape_2d::{Circle, Polygon, Rect, circle, polygon, rect};
use shape_core::{DVec3, trig};
use shape_edit::{Dent, Hole, Pocket, above, dent, hole, pocket, seat};
use shape_gen::solids::{BoxDesc, CylinderDesc, cuboid, cylinder, sphere};
use shape_mesh::Solid;

fn line(key: &str, r: Result<Solid, shape_edit::EditError>) {
    match r {
        Ok(s) => {
            let m = s.measure();
            println!(
                "{key} pairs {} closed {} volume {:.9}",
                shape_bool::crossing_pairs(&s),
                m.closed,
                m.volume
            );
        }
        Err(e) => println!("{key} error {e}"),
    }
}

/// A hole's name, counterbore and countersink.
type Variant = (String, (f64, f64), (f64, f64));

fn boxed(w: f64, d: f64, h: f64) -> Solid {
    cuboid(&BoxDesc {
        width: w,
        depth: d,
        height: h,
    })
    .unwrap()
}

fn main() {
    let plate = boxed(80.0, 50.0, 10.0);
    let at = [
        (0.0, -5.0),
        (-30.0, -15.0),
        (0.0, 0.0),
        (12.3, 4.56),
        (7.77, -3.21),
        (-21.4, 11.9),
        (23.3, -11.7),
        (0.1, 0.1),
    ];
    let angles = [60.0f64, 82.0, 90.0].map(|a| a.to_radians());
    // Holes, as §25.1.
    for &(x, y) in &at {
        for d in [2.5, 3.0, 4.0, 5.0, 6.6, 8.4] {
            if d > 6.0 && f64::abs(x) > 25.0 {
                continue;
            }
            for depth in [0.0, 6.0, 8.5] {
                // (name, counterbore, countersink)
                let mut variants: Vec<Variant> = vec![("plain".into(), (0.0, 0.0), (0.0, 0.0))];
                for cb in [2.0, 4.0, 5.5] {
                    if depth == 0.0 || cb < depth {
                        variants.push((format!("cb{cb}"), (2.0 * d, cb), (0.0, 0.0)));
                    }
                }
                for a in angles {
                    variants.push((format!("cs{a:.3}"), (0.0, 0.0), (2.0 * d, a)));
                    for k in [2.5, 3.0] {
                        if depth == 0.0 || 4.0 < depth {
                            variants.push((format!("cb4cs{k}x{a:.3}"), (2.0 * d, 4.0), (k * d, a)));
                        }
                    }
                }
                for (v, counterbore, countersink) in variants {
                    let r = hole(
                        &plate,
                        &Hole {
                            diameter: d,
                            at: vec![above(x, y)],
                            depth,
                            counterbore,
                            countersink,
                        },
                    );
                    line(&format!("hole ({x}, {y}) d{d} depth{depth} {v}"), r);
                }
            }
        }
    }
    // Pockets on the plate's top.
    let mut profiles: Vec<(String, shape_2d::Profile)> = Vec::new();
    for r in [0.0, 2.0] {
        profiles.push((
            format!("rect10x6r{r}"),
            rect(&Rect {
                width: 10.0,
                height: 6.0,
                radius: r,
            })
            .unwrap(),
        ));
    }
    for seg in [0u32, 12] {
        profiles.push((
            format!("circle{seg}"),
            circle(&Circle {
                radius: 4.0,
                segments: seg,
            })
            .unwrap(),
        ));
    }
    for sides in 3..=8u32 {
        for a in [0.0, 0.3] {
            let p = polygon(&Polygon {
                sides,
                radius: 5.0,
                ..Default::default()
            })
            .unwrap();
            profiles.push((format!("poly{sides}@{a}"), p.rotate(a)));
        }
    }
    for (pn, p) in &profiles {
        for &(x, y) in &at {
            for depth in [3.0, 6.0, 0.0] {
                let r = pocket(
                    &plate,
                    &Pocket {
                        profile: p.clone(),
                        depth,
                        at: vec![above(x, y)],
                    },
                );
                line(&format!("pocket {pn} ({x}, {y}) depth{depth}"), r);
            }
        }
    }
    // Parts seated in the plate.
    let mut parts: Vec<(String, Solid)> = vec![
        ("box10".into(), boxed(10.0, 10.0, 10.0)),
        (
            "cylinder5".into(),
            cylinder(&CylinderDesc {
                radius: 5.0,
                height: 10.0,
                ..Default::default()
            })
            .unwrap(),
        ),
        ("sphere6".into(), sphere(6.0).unwrap()),
    ];
    let turned: Vec<(String, Solid)> = parts
        .iter()
        .map(|(n, s)| {
            let mut s = s.clone();
            let (c, si) = (trig::cos(0.3), trig::sin(0.3));
            s.map_positions(
                |p| DVec3::new(c * p.x - si * p.y, si * p.x + c * p.y, p.z),
                false,
            );
            (format!("{n}@0.3"), s)
        })
        .collect();
    parts.extend(turned);
    for (pn, part) in &parts {
        for &(x, y) in &at {
            for z in [-3.0, 0.0, 5.0, 7.3] {
                let mut p = part.clone();
                p.map_positions(|q| q + DVec3::new(x, y, z), false);
                line(
                    &format!("seat {pn} ({x}, {y}, {z})"),
                    seat(&plate, &p, "part"),
                );
            }
        }
    }
    // Dents in a pad's top and a ball.
    let pad = boxed(100.0, 100.0, 30.0);
    let ball = sphere(20.0).unwrap();
    for (sn, s, top) in [("pad", &pad, 30.0), ("ball", &ball, 20.0)] {
        for &(x, y) in &at {
            for radius in [5.0, 15.0] {
                for depth in [2.0, 5.0] {
                    let p = if sn == "pad" {
                        DVec3::new(x, y, top)
                    } else {
                        DVec3::new(x / 40.0, y / 40.0, 1.0).normalize() * top
                    };
                    let r = dent(
                        s,
                        &Dent {
                            at: vec![p],
                            radius,
                            depth,
                        },
                    );
                    line(&format!("dent {sn} ({x}, {y}) r{radius} depth{depth}"), r);
                }
            }
        }
    }
}

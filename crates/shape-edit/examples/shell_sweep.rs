//! `shell` swept over profiles, bottoms, thicknesses and openings
//! (`docs/research.md` §27): one line per case with the crossing pairs of
//! the result (exact, `shape_bool::crossing_pairs`), whether it is
//! closed, its volume and the pairs `shell_report` gives, or the error.
//!
//! `cargo run --release -p shape-edit --example shell_sweep > sweep.txt`

use shape_2d::{Circle, Polygon, Rect, circle, polygon, rect};
use shape_bevel::{Chamfer, Fillet, chamfer, fillet};
use shape_edit::{Shell, shell_report};
use shape_gen::solids::{Extrude, extrude};
use shape_mesh::Solid;

fn main() {
    let mut profiles: Vec<(String, shape_2d::Profile)> = Vec::new();
    for sides in 3..=12u32 {
        for (an, a) in [
            ("0", 0.0),
            ("pi/8", std::f64::consts::FRAC_PI_8),
            ("0.3", 0.3),
            ("pi/n", std::f64::consts::PI / f64::from(sides)),
        ] {
            let p = polygon(&Polygon {
                sides,
                radius: 50.0,
                ..Default::default()
            })
            .unwrap();
            profiles.push((format!("poly{sides}@{an}"), p.rotate(a)));
        }
    }
    for (w, h, r) in [(80.0, 50.0, 5.0), (80.0, 50.0, 15.0), (60.0, 60.0, 29.0)] {
        let p = rect(&Rect {
            width: w,
            height: h,
            radius: r,
        })
        .unwrap();
        profiles.push((format!("rrect{w}x{h}r{r}"), p));
    }
    for seg in [0u32, 24, 64] {
        let p = circle(&Circle {
            radius: 40.0,
            segments: seg,
        })
        .unwrap();
        profiles.push((format!("circle{seg}"), p));
    }
    let bottoms = [
        "plain",
        "taper0.2",
        "taper-0.2",
        "chamfer2",
        "chamfer10",
        "chamfer25",
        "fillet5",
        "fillet15",
    ];
    let opens: [&[&str]; 4] = [&["top"], &["bottom"], &["+x"], &[]];
    let only = std::env::args().nth(1);
    for (pn, p) in &profiles {
        if only.as_ref().is_some_and(|o| !pn.starts_with(o.as_str())) {
            continue;
        }
        for b in bottoms {
            let taper = match b {
                "taper0.2" => 0.2,
                "taper-0.2" => -0.2,
                _ => 0.0,
            };
            let s = match extrude(
                p,
                &Extrude {
                    height: 80.0,
                    taper,
                    ..Default::default()
                },
            ) {
                Ok(s) => s,
                Err(e) => {
                    println!("{pn} {b} gen-error {e}");
                    continue;
                }
            };
            let s: Result<Solid, String> = if let Some(d) = b.strip_prefix("chamfer") {
                chamfer(
                    &s,
                    &Chamfer {
                        distance: d.parse().unwrap(),
                        edges: vec!["bottom".into()],
                        ..Default::default()
                    },
                )
                .map_err(|e| e.to_string())
            } else if let Some(r) = b.strip_prefix("fillet") {
                fillet(
                    &s,
                    &Fillet {
                        radius: r.parse().unwrap(),
                        edges: vec!["bottom".into()],
                        ..Default::default()
                    },
                )
                .map_err(|e| e.to_string())
            } else {
                Ok(s)
            };
            let s = match s {
                Ok(s) => s,
                Err(e) => {
                    println!("{pn} {b} bevel-error {e}");
                    continue;
                }
            };
            for t in [1.0, 3.0, 10.0, 20.0, 30.0] {
                for o in opens {
                    let key = format!("{pn} {b} t{t} open[{}]", o.join(","));
                    match shell_report(
                        &s,
                        &Shell {
                            thickness: t,
                            open: o.iter().map(|x| x.to_string()).collect(),
                        },
                    ) {
                        Ok((r, report)) => {
                            let m = r.measure();
                            println!(
                                "{key} pairs {} closed {} volume {:.9} reported {}",
                                shape_bool::crossing_pairs(&r),
                                m.closed,
                                m.volume,
                                report.crossing_pairs
                            );
                        }
                        Err(e) => println!("{key} error {e}"),
                    }
                }
            }
        }
    }
}

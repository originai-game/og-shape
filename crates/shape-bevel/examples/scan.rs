//! The bevel scan of `docs/research.md` §7.5 and §7.6: fillet and chamfer
//! at 0.5 and 1.5 on about 19 000 selections (every edge, each name, each
//! single real edge, seeded random subsets) of generator solids and
//! booleans. Prints one line per bevel (`solid | selection | bevel |
//! outcome | hash`, a result's first 16 hex digits of its output hash) to
//! the file given, and a count per outcome. An outcome is
//! `ok` only when the result is closed, has no triangle of zero area and
//! no two triangles crossing (`shape_bool::selfcheck`).
//!
//! `cargo run --release -p shape-bevel --example scan -- <out.txt>`
//!
//! Two runs' files compare line by line: a selection that fitted before
//! and fails after is a regression.

use std::fmt::Write as _;

use shape_bevel::{BevelError, Chamfer, Fillet, chamfer, fillet};
use shape_bool::{BoolOp, Boolean, boolean};
use shape_core::{DVec2, DVec3};
use shape_gen::solids::{
    Axis, BoxDesc, CylinderDesc, DomeDesc, Extrude, Revolve, TubeDesc, cone, cuboid, cylinder,
    dome_desc, extrude, revolve, tube_desc, wedge,
};
use shape_mesh::Solid;

type Error = Box<dyn std::error::Error>;

fn profile(points: &[(f64, f64)]) -> shape_2d::Profile {
    shape_2d::Profile::from_contour(shape_2d::Contour::corners(
        points.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
    ))
}

fn prism(p: &shape_2d::Profile, height: f64, twist: f64, taper: f64) -> Result<Solid, Error> {
    Ok(extrude(
        p,
        &Extrude {
            height,
            twist,
            taper,
            ..Default::default()
        },
    )?)
}

fn moved(s: &Solid, by: DVec3) -> Solid {
    let mut out = s.clone();
    out.map_positions(|p| p + by, false);
    out
}

fn op(a: &Solid, b: &Solid, op: BoolOp) -> Result<Solid, Error> {
    Ok(boolean(
        a,
        b,
        &Boolean {
            op,
            ..Default::default()
        },
    )?)
}

fn solids() -> Result<Vec<(String, Solid)>, Error> {
    let mut out: Vec<(String, Solid)> = Vec::new();
    let block = |w, d, h| {
        cuboid(&BoxDesc {
            width: w,
            depth: d,
            height: h,
        })
    };
    out.push(("box".into(), block(20.0, 16.0, 10.0)?));
    out.push(("thin box".into(), block(20.0, 16.0, 2.5)?));
    for (r, n) in [(5.0, 0), (3.0, 12), (8.0, 0)] {
        out.push((
            format!("cylinder r{r} n{n}"),
            cylinder(&CylinderDesc {
                radius: r,
                height: 10.0,
                segments: n,
                ..Default::default()
            })?,
        ));
    }
    out.push((
        "frustum".into(),
        cylinder(&CylinderDesc {
            radius: 6.0,
            radius_top: 3.0,
            height: 8.0,
            ..Default::default()
        })?,
    ));
    out.push(("cone".into(), cone(5.0, 10.0)?));
    out.push((
        "tube".into(),
        tube_desc(&TubeDesc {
            outer: 8.0,
            inner: 5.0,
            height: 10.0,
            ..Default::default()
        })?,
    ));
    out.push(("wedge".into(), wedge(12.0, 8.0, 10.0)?));
    out.push((
        "dome".into(),
        dome_desc(&DomeDesc {
            radius: 8.0,
            axis: Axis::Z,
            ..Default::default()
        })?,
    ));
    for sides in 3..=8u32 {
        let p = shape_2d::polygon(&shape_2d::Polygon {
            sides,
            radius: 8.0,
            ..Default::default()
        })?;
        out.push((format!("prism {sides}"), prism(&p, 10.0, 0.0, 0.0)?));
        out.push((format!("pyramid {sides}"), prism(&p, 10.0, 0.0, 1.0)?));
    }
    let square = profile(&[(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)]);
    out.push(("twisted".into(), prism(&square, 20.0, 1.0, 0.0)?));
    out.push(("twisted tapered".into(), prism(&square, 20.0, 0.6, 0.4)?));
    out.push(("tapered".into(), prism(&square, 20.0, 0.0, 0.5)?));
    out.push((
        "d-slot".into(),
        prism(
            &shape_2d::Profile::from_contour(shape_2d::Contour::smooth({
                let mut p: Vec<DVec2> = (0..=12)
                    .map(|k| {
                        let a =
                            -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * k as f64 / 12.0;
                        DVec2::new(
                            6.0 * shape_core::trig::cos(a),
                            6.0 * shape_core::trig::sin(a),
                        )
                    })
                    .collect();
                p.push(DVec2::new(-2.0, 6.0));
                p.push(DVec2::new(-2.0, -6.0));
                p
            })),
            8.0,
            0.0,
            0.0,
        )?,
    ));
    out.push((
        "rounded rect".into(),
        prism(
            &shape_2d::rect(&shape_2d::Rect {
                width: 30.0,
                height: 20.0,
                radius: 4.0,
            })?,
            6.0,
            0.0,
            0.0,
        )?,
    ));
    out.push((
        "slot".into(),
        prism(
            &shape_2d::slot(&shape_2d::Slot {
                length: 24.0,
                width: 8.0,
            })?,
            5.0,
            0.0,
            0.0,
        )?,
    ));
    let section = profile(&[(5.0, 0.0), (10.0, 0.0), (10.0, 8.0), (5.0, 8.0)]);
    for angle in [0.0, 2.0, 4.0] {
        out.push((
            format!("ring {angle}"),
            revolve(
                &section,
                &Revolve {
                    angle,
                    axis: Axis::Z,
                    ..Default::default()
                },
            )?,
        ));
    }
    let apex = profile(&[(0.0, 0.0), (8.0, 0.0), (8.0, 4.0), (0.0, 10.0)]);
    for angle in [0.0, 3.0] {
        out.push((
            format!("apex revolve {angle}"),
            revolve(
                &apex,
                &Revolve {
                    angle,
                    axis: Axis::Z,
                    ..Default::default()
                },
            )?,
        ));
    }
    // Booleans.
    let big = block(20.0, 20.0, 10.0)?;
    let small = block(8.0, 8.0, 10.0)?;
    out.push((
        "join".into(),
        op(
            &big,
            &moved(&small, DVec3::new(3.0, 2.0, 8.0)),
            BoolOp::Join,
        )?,
    ));
    let a = block(20.0, 20.0, 10.0)?;
    out.push((
        "join corner".into(),
        op(&a, &moved(&a, DVec3::new(20.0, 20.0, 10.0)), BoolOp::Join)?,
    ));
    out.push((
        "join edge".into(),
        op(&a, &moved(&a, DVec3::new(20.0, 0.0, 10.0)), BoolOp::Join)?,
    ));
    let pin = |r: f64, h: f64| {
        cylinder(&CylinderDesc {
            radius: r,
            height: h,
            ..Default::default()
        })
    };
    out.push((
        "bore".into(),
        op(
            &a,
            &moved(&pin(4.0, 30.0)?, DVec3::new(0.0, 0.0, -10.0)),
            BoolOp::Cut,
        )?,
    ));
    out.push((
        "blind bore".into(),
        op(
            &a,
            &moved(&pin(3.0, 10.0)?, DVec3::new(2.0, 1.0, 5.0)),
            BoolOp::Cut,
        )?,
    ));
    out.push((
        "notch".into(),
        op(&a, &moved(&small, DVec3::new(10.0, 0.0, 4.0)), BoolOp::Cut)?,
    ));
    out.push((
        "post".into(),
        op(
            &a,
            &moved(&pin(3.0, 8.0)?, DVec3::new(0.0, 0.0, 10.0)),
            BoolOp::Join,
        )?,
    ));
    let bar = block(30.0, 4.0, 4.0)?;
    out.push((
        "cross bar".into(),
        op(&a, &moved(&bar, DVec3::new(0.0, 0.0, 8.0)), BoolOp::Join)?,
    ));
    let mut side_pin = pin(2.5, 30.0)?;
    side_pin.map_positions(|p| DVec3::new(p.z - 15.0, p.y, p.x + 5.0), true);
    out.push(("cross bore".into(), op(&a, &side_pin, BoolOp::Cut)?));
    let plate = block(30.0, 30.0, 10.0)?;
    for (label, d) in [
        (
            "hole",
            shape_edit::Hole {
                diameter: 6.0,
                at: vec![shape_edit::above(0.0, 0.0)],
                ..Default::default()
            },
        ),
        (
            "blind hole",
            shape_edit::Hole {
                diameter: 8.0,
                depth: 5.0,
                at: vec![shape_edit::above(4.0, 3.0)],
                ..Default::default()
            },
        ),
        (
            "counterbore",
            shape_edit::Hole {
                diameter: 5.0,
                counterbore: (9.0, 3.0),
                at: vec![shape_edit::above(-3.0, 2.0)],
                ..Default::default()
            },
        ),
    ] {
        out.push((label.into(), shape_edit::hole(&plate, &d)?));
    }
    Ok(out)
}

/// A small deterministic generator (an LCG) for the random subsets.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

fn tagged(s: &Solid, picks: &[(u32, u32)]) -> Solid {
    let mut out = s.clone();
    for &(a, b) in picks {
        out.tag_edge(a, b, &["pick"]);
    }
    out
}

/// The outcome, and for a result the first 16 hex digits of its output
/// hash (so two runs show which results changed their bytes).
fn outcome(r: Result<Solid, BevelError>) -> (String, String) {
    match r {
        Ok(s) => {
            let m = s.measure();
            let mut h = shape_core::OutputHash::new();
            let hash = match s.hash_into(&mut h) {
                Ok(()) => h.finish_hex()[..16].to_string(),
                Err(_) => "NaN".into(),
            };
            let o = if !(m.closed && m.degenerate_triangles == 0) {
                "open"
            } else if shape_bool::selfcheck::self_crossings(&s.positions, &s.triangles).is_empty() {
                "ok"
            } else {
                "crossing"
            };
            (o.into(), hash)
        }
        Err(BevelError::TooLarge(_)) => ("TooLarge".into(), String::new()),
        Err(BevelError::Unsupported(_, what)) => (format!("Unsupported({what})"), String::new()),
        Err(e) => (
            format!("{e:?}")
                .split('(')
                .next()
                .unwrap_or("error")
                .to_string(),
            String::new(),
        ),
    }
}

fn main() -> Result<(), Error> {
    let path = std::env::args().nth(1).ok_or("usage: scan <out.txt>")?;
    let mut lines = String::new();
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    let mut rng = Lcg(0x5eed);
    for (name, s) in solids()? {
        let real = s.real_edges();
        let mut sels: Vec<(String, Solid, Vec<String>)> = vec![("all".into(), s.clone(), vec![])];
        let mut names: Vec<String> = s.names.all().map(String::from).collect();
        names.sort();
        for n in names {
            if s.edges(&[n.as_str()]).is_ok_and(|e| !e.is_empty()) {
                sels.push((format!("name {n}"), s.clone(), vec![n]));
            }
        }
        let stride = real.len().div_ceil(200).max(1);
        for (i, &e) in real.iter().enumerate().step_by(stride) {
            sels.push((format!("edge {i}"), tagged(&s, &[e]), vec!["pick".into()]));
        }
        for k in 0..50 {
            let picks: Vec<(u32, u32)> = real
                .iter()
                .copied()
                .filter(|_| rng.next() % 4 == 0)
                .collect();
            if !picks.is_empty() {
                sels.push((
                    format!("subset {k}"),
                    tagged(&s, &picks),
                    vec!["pick".into()],
                ));
            }
        }
        for (sel, solid, edges) in &sels {
            for size in [0.5, 1.5] {
                let f = fillet(
                    solid,
                    &Fillet {
                        radius: size,
                        edges: edges.clone(),
                        segments: 4,
                        ..Default::default()
                    },
                );
                let c = chamfer(
                    solid,
                    &Chamfer {
                        distance: size,
                        edges: edges.clone(),
                        ..Default::default()
                    },
                );
                for (kind, r) in [("fillet", f), ("chamfer", c)] {
                    let (o, hash) = outcome(r);
                    *counts.entry(o.clone()).or_default() += 1;
                    writeln!(lines, "{name} | {sel} | {kind} {size} | {o} | {hash}")?;
                }
            }
        }
    }
    std::fs::write(&path, &lines)?;
    for (o, n) in &counts {
        println!("{o}: {n}");
    }
    println!("total: {}", counts.values().sum::<usize>());
    Ok(())
}

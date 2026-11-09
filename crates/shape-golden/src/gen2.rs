//! Golden cases for the generators the phase 1 rebuilds found missing
//! (`docs/research.md` §6.4, §10): loft through holes and by spline, the
//! +z axis, body, wire, knurl and the gear solid.

use shape_2d::{Circle, Rect, circle, rect};
use shape_core::{DVec3, HashError, OutputHash, trig};
use shape_gen::body::{Body, View, body};
use shape_gen::parts::{GearDesc, Knurl, gear_solid, knurl};
use shape_gen::paths::{Loft, LoftCurve, loft};
use shape_gen::solids::{
    Axis, DomeDesc, Revolve, SphereDesc, TorusDesc, dome_desc, revolve, sphere_desc, torus_desc,
};
use shape_gen::wire::{Wire, wire};
use shape_mesh::Solid;

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "gen2.loft.holes",
            run: loft_holes,
        },
        Case {
            name: "gen2.loft.spline",
            run: loft_spline,
        },
        Case {
            name: "gen2.axis.z",
            run: axis_z,
        },
        Case {
            name: "gen2.body",
            run: bodies,
        },
        Case {
            name: "gen2.wire",
            run: wires,
        },
        Case {
            name: "gen2.knurl",
            run: knurls,
        },
        Case {
            name: "gen2.gear",
            run: gears,
        },
    ]
}

fn solid<E: std::fmt::Display>(h: &mut OutputHash, s: Result<Solid, E>) -> Result<(), HashError> {
    match s {
        Ok(s) => {
            let m = s.measure();
            if !m.closed || m.degenerate_triangles > 0 {
                eprintln!("gen2: not closed or degenerate: {m:?}");
            }
            s.hash_into(h)
        }
        Err(e) => {
            eprintln!("gen2: error: {e}");
            h.str(&format!("error: {e}"));
            Ok(())
        }
    }
}

fn disc(radius: f64) -> shape_2d::Profile {
    circle(&Circle {
        radius,
        segments: 0,
    })
    .unwrap_or_default()
}

fn square(side: f64, radius: f64) -> shape_2d::Profile {
    rect(&Rect {
        width: side,
        height: side,
        radius,
    })
    .unwrap_or_default()
}

/// A bevel gear's blank with its bore (A12's), and a plate with two holes
/// narrowing into a round boss.
fn loft_holes(h: &mut OutputHash) -> Result<(), HashError> {
    let blank = |r: f64| disc(r).subtract(&disc(3.0));
    solid(
        h,
        loft(&Loft {
            sections: vec![(blank(20.0), 0.0), (blank(12.0), 8.0)],
            ..Default::default()
        }),
    )?;
    let two = |side: f64, dx: f64| {
        square(side, 4.0)
            .subtract(&disc(2.5).translate(shape_core::DVec2::new(-dx, 0.0)))
            .subtract(&disc(2.5).translate(shape_core::DVec2::new(dx, 0.0)))
    };
    solid(
        h,
        loft(&Loft {
            sections: vec![
                (two(40.0, 10.0), 0.0),
                (two(30.0, 8.0), 6.0),
                (two(24.0, 6.0), 20.0),
            ],
            samples: 48,
            steps: 4,
            curve: LoftCurve::CatmullRom,
        }),
    )
}

/// sand3d's binocular shell, four sections, on its natural spline; and
/// the same with fixed steps.
fn loft_spline(h: &mut OutputHash) -> Result<(), HashError> {
    let sections = vec![
        (square(30.0, 6.0), 0.0),
        (square(36.0, 8.0), 12.0),
        (disc(16.0), 40.0),
        (disc(14.0), 52.0),
    ];
    solid(
        h,
        loft(&Loft {
            sections: sections.clone(),
            samples: 64,
            curve: LoftCurve::Spline,
            ..Default::default()
        }),
    )?;
    solid(
        h,
        loft(&Loft {
            sections,
            samples: 32,
            steps: 3,
            curve: LoftCurve::Spline,
        }),
    )
}

fn axis_z(h: &mut OutputHash) -> Result<(), HashError> {
    solid(
        h,
        revolve(
            &square(4.0, 0.0).translate(shape_core::DVec2::new(8.0, 2.0)),
            &Revolve {
                angle: 2.0,
                axis: Axis::Z,
            },
        ),
    )?;
    solid(
        h,
        sphere_desc(&SphereDesc {
            radius: 12.5,
            axis: Axis::Z,
        }),
    )?;
    solid(
        h,
        torus_desc(&TorusDesc {
            radius: 20.0,
            tube_radius: 4.0,
            axis: Axis::Z,
        }),
    )?;
    solid(
        h,
        dome_desc(&DomeDesc {
            radius: 7.0,
            height: 3.0,
            axis: Axis::Z,
        }),
    )
}

/// A fish (spline views, round), and a car body (a step at the
/// windscreen's foot, boxy sections).
fn bodies(h: &mut OutputHash) -> Result<(), HashError> {
    solid(
        h,
        body(&Body {
            plan: View::spline(&[[0.0, 0.0], [30.0, 24.0], [90.0, 18.0], [120.0, 2.0]]),
            top: View::spline(&[[0.0, 0.0], [40.0, 22.0], [120.0, 1.0]]),
            bottom: View::spline(&[[0.0, 0.0], [40.0, -14.0], [120.0, -1.0]]),
            ..Default::default()
        }),
    )?;
    solid(
        h,
        body(&Body {
            plan: View::points(&[[0.0, 160.0], [400.0, 170.0]]),
            top: View::points(&[[0.0, 70.0], [120.0, 80.0], [120.0, 130.0], [400.0, 125.0]]),
            bottom: View::constant(20.0),
            n: View::points(&[[0.0, 3.0], [400.0, 5.0]]),
        }),
    )
}

/// sand3d's stool leg (bent), a mitred bracket, and a closed loop.
fn wires(h: &mut OutputHash) -> Result<(), HashError> {
    solid(
        h,
        wire(&Wire {
            path: vec![
                DVec3::ZERO,
                DVec3::new(0.0, 0.0, 650.0),
                DVec3::new(-250.0, 0.0, 650.0),
            ],
            diameter: 33.0,
            fillet: 60.0,
            ..Default::default()
        }),
    )?;
    solid(
        h,
        wire(&Wire {
            path: vec![
                DVec3::ZERO,
                DVec3::new(0.0, 0.0, 30.0),
                DVec3::new(20.0, 5.0, 40.0),
                DVec3::new(20.0, 25.0, 40.0),
            ],
            diameter: 4.0,
            fillets: vec![0.0, 0.0, 6.0, 0.0],
            ..Default::default()
        }),
    )?;
    solid(
        h,
        wire(&Wire {
            path: (0..5)
                .map(|k| {
                    let (s, c) = trig::sin_cos(trig::TAU * (k as f64 / 5.0));
                    DVec3::new(30.0 * c, 30.0 * s, if k % 2 == 0 { 0.0 } else { 8.0 })
                })
                .collect(),
            diameter: 3.0,
            closed: true,
            fillet: 5.0,
            ..Default::default()
        }),
    )
}

fn knurls(h: &mut OutputHash) -> Result<(), HashError> {
    solid(
        h,
        knurl(&Knurl {
            diameter: 16.0,
            height: 12.0,
            ..Default::default()
        }),
    )?;
    solid(
        h,
        knurl(&Knurl {
            diameter: 10.0,
            height: 6.0,
            pitch: 0.8,
            straight: true,
        }),
    )
}

fn gears(h: &mut OutputHash) -> Result<(), HashError> {
    solid(
        h,
        gear_solid(&GearDesc {
            teeth: 24,
            module: 1.5,
            width: 6.0,
            bore: 5.0,
            hub: 14.0,
            hub_height: 4.0,
            ..Default::default()
        }),
    )?;
    solid(
        h,
        gear_solid(&GearDesc {
            teeth: 12,
            module: 2.0,
            pressure_angle: trig::to_radians(14.5),
            width: 4.0,
            bore: 4.0,
            ..Default::default()
        }),
    )
}

//! Golden cases for profiles (shape-2d) and named solids (shape-gen).

use shape_2d::{
    Circle, Gear, Join, Outline, Path, Poly, PolyPoint, Profile, Rect, Slot, StrokeDesc,
    Superellipse, circle, gear, outline_profile, poly, rect, slot, stroke, superellipse,
};
use shape_bool::{BoolOp, Boolean, boolean};
use shape_core::{DVec2, HashError, OutputHash, trig};
use shape_gen::paths::{Helix, Loft, Sweep, Thread, helix, loft, sweep, thread};
use shape_gen::solids::{
    BoxDesc, CylinderDesc, Extrude, Revolve, cone, cuboid, cylinder, dome, extrude, revolve,
    sphere, torus, tube, wedge,
};
use shape_mesh::Solid;

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "2d.profiles",
            run: profiles,
        },
        Case {
            name: "2d.booleans",
            run: booleans,
        },
        Case {
            name: "2d.offset.stroke.hull",
            run: offsets,
        },
        Case {
            name: "2d.triangulate",
            run: fill,
        },
        Case {
            name: "solid.box",
            run: |h| {
                solid(
                    h,
                    cuboid(&BoxDesc {
                        width: 40.0,
                        depth: 20.0,
                        height: 6.0,
                    }),
                )
            },
        },
        Case {
            name: "solid.tube",
            run: |h| solid(h, tube(10.0, 6.5, 30.0)),
        },
        Case {
            name: "solid.extrude.twist",
            run: |h| {
                solid(
                    h,
                    extrude(
                        &plate(),
                        &Extrude {
                            height: 12.0,
                            twist: trig::PI / 3.0,
                            taper: 0.3,
                            both_sides: true,
                        },
                    ),
                )
            },
        },
        Case {
            name: "solid.cylinder.cone",
            run: |h| {
                solid(
                    h,
                    cylinder(&CylinderDesc {
                        radius: 5.0,
                        radius_top: 3.0,
                        height: 8.0,
                    }),
                )?;
                solid(h, cone(4.0, 9.0))
            },
        },
        Case {
            name: "solid.sphere.dome.torus",
            run: |h| {
                solid(h, sphere(12.5))?;
                solid(h, dome(7.0))?;
                solid(h, torus(20.0, 4.0))
            },
        },
        Case {
            name: "solid.revolve.partial",
            run: |h| {
                solid(
                    h,
                    revolve(
                        &plate().translate(DVec2::new(30.0, 0.0)),
                        &Revolve { angle: 2.0 },
                    ),
                )
            },
        },
        Case {
            name: "solid.wedge",
            run: |h| solid(h, wedge(10.0, 4.0, 3.0)),
        },
        Case {
            name: "2d.gear",
            run: |h| {
                profile(
                    h,
                    &gear(&Gear {
                        teeth: 17,
                        module: 1.5,
                        pressure_angle: 0.0,
                    })
                    .unwrap_or_default(),
                )
            },
        },
        Case {
            name: "solid.gear",
            run: |h| {
                solid(
                    h,
                    extrude(
                        &gear(&Gear {
                            teeth: 12,
                            module: 2.0,
                            pressure_angle: 0.0,
                        })
                        .unwrap_or_default(),
                        &Extrude {
                            height: 5.0,
                            ..Default::default()
                        },
                    ),
                )
            },
        },
        Case {
            name: "solid.sweep.helix",
            run: |h| {
                let path = helix(&Helix {
                    radius: 15.0,
                    pitch: 8.0,
                    turns: 1.5,
                })
                .unwrap_or_default();
                solid(
                    h,
                    sweep(
                        &circle(&Circle {
                            radius: 2.0,
                            segments: 0,
                        })
                        .unwrap(),
                        &Sweep {
                            path,
                            twist: 1.0,
                            scale_end: 0.5,
                            ..Default::default()
                        },
                    ),
                )
            },
        },
        Case {
            name: "solid.thread",
            run: |h| {
                solid(
                    h,
                    thread(&Thread {
                        radius: 4.0,
                        pitch: 0.7,
                        length: 5.0,
                        depth: 0.0,
                    }),
                )
            },
        },
        Case {
            name: "solid.loft",
            run: |h| {
                let sq = rect(&Rect {
                    width: 10.0,
                    height: 10.0,
                    radius: 1.0,
                })
                .unwrap();
                let ci = circle(&Circle {
                    radius: 4.0,
                    segments: 0,
                })
                .unwrap();
                solid(
                    h,
                    loft(&Loft {
                        sections: vec![(sq.clone(), 0.0), (ci, 8.0), (sq, 16.0)],
                        samples: 40,
                        steps: 3,
                    }),
                )
            },
        },
        Case {
            name: "2d.outline",
            run: |h| {
                let v = DVec2::new;
                let cmds = [
                    Outline::Move(v(0.0, 0.0)),
                    Outline::Line(v(6.0, 0.0)),
                    Outline::Cubic(v(9.0, 0.0), v(9.0, 8.0), v(6.0, 8.0)),
                    Outline::Line(v(0.0, 8.0)),
                    Outline::Close,
                    Outline::Move(v(2.0, 2.0)),
                    Outline::Line(v(2.0, 6.0)),
                    Outline::Quad(v(6.0, 6.0), v(5.0, 2.0)),
                    Outline::Close,
                ];
                profile(h, &outline_profile(&cmds).unwrap_or_default())
            },
        },
        Case {
            name: "bool.named.hole",
            run: |h| {
                let plate = cuboid(&BoxDesc {
                    width: 80.0,
                    depth: 40.0,
                    height: 6.0,
                })
                .unwrap();
                let mut pin = cylinder(&CylinderDesc {
                    radius: 3.3,
                    radius_top: 0.0,
                    height: 8.0,
                })
                .unwrap();
                pin.map_positions(|p| p + shape_core::DVec3::new(10.0, 5.0, -1.0), false);
                solid(
                    h,
                    boolean(
                        &plate,
                        &pin,
                        &Boolean {
                            op: BoolOp::Cut,
                            target: "plate",
                            tool: "hole",
                        },
                    ),
                )
            },
        },
        Case {
            name: "bool.named.join.intersect",
            run: |h| {
                let a = sphere(10.0).unwrap();
                let mut b = cuboid(&BoxDesc {
                    width: 12.0,
                    depth: 12.0,
                    height: 12.0,
                })
                .unwrap();
                b.map_positions(|p| p + shape_core::DVec3::new(3.0, 2.0, -4.0), false);
                solid(
                    h,
                    boolean(
                        &a,
                        &b,
                        &Boolean {
                            op: BoolOp::Join,
                            target: "ball",
                            tool: "block",
                        },
                    ),
                )?;
                solid(
                    h,
                    boolean(
                        &a,
                        &b,
                        &Boolean {
                            op: BoolOp::Intersect,
                            ..Default::default()
                        },
                    ),
                )
            },
        },
    ]
}

fn plate() -> Profile {
    rect(&Rect {
        width: 20.0,
        height: 10.0,
        radius: 2.0,
    })
    .unwrap()
    .subtract(
        &circle(&Circle {
            radius: 3.0,
            segments: 0,
        })
        .unwrap(),
    )
}

fn profile(h: &mut OutputHash, p: &Profile) -> Result<(), HashError> {
    h.u64(p.regions.len() as u64);
    for c in p.contours() {
        h.u64(c.points.len() as u64);
        for (q, s) in c.points.iter().zip(&c.sharp) {
            h.f64(q.x)?;
            h.f64(q.y)?;
            h.u64(u64::from(*s));
        }
    }
    Ok(())
}

fn profiles(h: &mut OutputHash) -> Result<(), HashError> {
    let pp = |x, y, radius| PolyPoint { x, y, radius };
    let all = [
        rect(&Rect {
            width: 30.0,
            height: 12.0,
            radius: 3.0,
        }),
        slot(&Slot {
            length: 25.0,
            width: 6.0,
        }),
        poly(&Poly {
            points: vec![
                pp(0.0, 0.0, 0.0),
                pp(20.0, 0.0, 4.0),
                pp(20.0, 10.0, 1.0),
                pp(5.0, 15.0, 2.0),
            ],
        }),
        superellipse(&Superellipse {
            rx: 10.0,
            ry: 6.0,
            exponent: 4.0,
            segments: 0,
        }),
        Path::new(0.0, -5.0)
            .arc_to(5.0, 0.0, 0.0, 5.0)
            .tangent_arc_to(-3.0, 0.0)
            .spline_through(&[DVec2::new(-2.0, -3.0)])
            .close(),
    ];
    for p in all {
        match p {
            Ok(p) => profile(h, &p)?,
            Err(e) => h.str(&format!("error: {e}")),
        }
    }
    Ok(())
}

fn booleans(h: &mut OutputHash) -> Result<(), HashError> {
    let a = plate();
    let b = slot(&Slot {
        length: 30.0,
        width: 4.0,
    })
    .unwrap()
    .rotate(0.4);
    for p in [a.union(&b), a.subtract(&b), a.intersect(&b), a.xor(&b)] {
        profile(h, &p)?;
    }
    Ok(())
}

fn offsets(h: &mut OutputHash) -> Result<(), HashError> {
    let a = plate();
    for p in [
        a.offset(1.5, Join::Round),
        a.offset(-0.75, Join::Miter),
        a.offset(0.5, Join::Bevel),
        a.hull(),
    ] {
        profile(h, &p.unwrap_or_default())?;
    }
    let pts = [
        DVec2::ZERO,
        DVec2::new(10.0, 0.0),
        DVec2::new(14.0, 6.0),
        DVec2::new(8.0, 9.0),
    ];
    let s = stroke(
        &pts,
        &StrokeDesc {
            width: 1.2,
            join: Join::Round,
            round_caps: true,
            closed: false,
        },
    );
    profile(h, &s.unwrap_or_default())?;
    profile(h, &a.mirror())
}

fn fill(h: &mut OutputHash) -> Result<(), HashError> {
    let f = plate().triangulate().unwrap_or_default();
    h.u64(f.points.len() as u64);
    for p in &f.points {
        h.f64(p.x)?;
        h.f64(p.y)?;
    }
    f.triangles
        .iter()
        .flatten()
        .for_each(|&i| h.u64(u64::from(i)));
    Ok(())
}

fn solid<E: std::fmt::Display>(h: &mut OutputHash, s: Result<Solid, E>) -> Result<(), HashError> {
    match s {
        Ok(s) => s.hash_into(h),
        Err(e) => {
            h.str(&format!("error: {e}"));
            Ok(())
        }
    }
}

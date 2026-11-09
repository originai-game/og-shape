//! Golden cases for profiles (shape-2d) and named solids (shape-gen).

use shape_2d::{
    Circle, Join, Path, Poly, PolyPoint, Profile, Rect, Slot, StrokeDesc, Superellipse, circle,
    poly, rect, slot, stroke, superellipse,
};
use shape_core::{DVec2, HashError, OutputHash, trig};
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

//! Golden cases for fillet, chamfer and soften (shape-bevel).

use shape_bevel::{Chamfer, Fillet, Miter, Soften, chamfer, fillet, soften};
use shape_core::{DVec2, DVec3};
use shape_core::{HashError, OutputHash, trig};
use shape_edit::{Hole, above, hole};
use shape_gen::solids::{
    Axis, BoxDesc, CylinderDesc, Extrude, Revolve, cuboid, cylinder, extrude, revolve,
};
use shape_mesh::Solid;

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "bevel.box",
            run: boxes,
        },
        Case {
            name: "bevel.curved",
            run: curved,
        },
        Case {
            name: "bevel.sand3d",
            run: sand3d,
        },
        Case {
            name: "bevel.adj",
            run: adj,
        },
        Case {
            name: "bevel.gaps",
            run: gaps,
        },
        Case {
            name: "bevel.miters",
            run: miters,
        },
        Case {
            name: "bevel.pipe",
            run: pipe,
        },
        Case {
            name: "bevel.slide",
            run: slide,
        },
        Case {
            name: "bevel.slivers",
            run: slivers,
        },
        Case {
            name: "bevel.cones",
            run: cones,
        },
    ]
}

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn block() -> Solid {
    cuboid(&BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    })
    .unwrap_or_default()
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

fn boxes(h: &mut OutputHash) -> Result<(), HashError> {
    let b = block();
    solid(
        h,
        fillet(
            &b,
            &Fillet {
                radius: 3.0,
                edges: names(&["top"]),
                ..Default::default()
            },
        ),
    )?;
    solid(
        h,
        chamfer(
            &b,
            &Chamfer {
                distance: 2.0,
                distance2: 4.0,
                edges: names(&["top"]),
                ..Default::default()
            },
        ),
    )?;
    solid(
        h,
        soften(
            &b,
            &Soften {
                radius: 2.0,
                ..Default::default()
            },
        ),
    )?;
    // Too large: the error is the output.
    solid(
        h,
        fillet(
            &b,
            &Fillet {
                radius: 16.0,
                edges: names(&["top"]),
                ..Default::default()
            },
        ),
    )
}

fn curved(h: &mut OutputHash) -> Result<(), HashError> {
    let c = cylinder(&CylinderDesc {
        radius: 15.0,
        radius_top: 0.0,
        height: 20.0,
        ..Default::default()
    })
    .unwrap_or_default();
    solid(
        h,
        fillet(
            &c,
            &Fillet {
                radius: 2.0,
                edges: names(&["top"]),
                profile: 0.7,
                ..Default::default()
            },
        ),
    )?;
    solid(
        h,
        chamfer(
            &c,
            &Chamfer {
                distance: 2.0,
                angle: trig::to_radians(30.0),
                edges: names(&["top"]),
                ..Default::default()
            },
        ),
    )
}

fn sand3d(h: &mut OutputHash) -> Result<(), HashError> {
    // The bracket.
    let plate = shape_2d::rect(&shape_2d::Rect {
        width: 80.0,
        height: 40.0,
        radius: 4.0,
    })
    .map_err(|_| HashError::NaN)?;
    let plate = extrude(
        &plate,
        &Extrude {
            height: 6.0,
            ..Default::default()
        },
    )
    .unwrap_or_default();
    let holes = |s: &Solid, d: f64, at| {
        hole(
            s,
            &Hole {
                diameter: d,
                at,
                ..Default::default()
            },
        )
        .unwrap_or_default()
    };
    let bracket = holes(&plate, 6.6, vec![above(-25.0, 0.0), above(25.0, 0.0)]);
    solid(
        h,
        fillet(
            &bracket,
            &Fillet {
                radius: 1.0,
                edges: names(&["top"]),
                ..Default::default()
            },
        ),
    )?;
    // The washer plate.
    let p = cuboid(&BoxDesc {
        width: 60.0,
        depth: 40.0,
        height: 8.0,
    })
    .unwrap_or_default();
    solid(
        h,
        chamfer(
            &holes(&p, 10.0, vec![above(0.0, 0.0)]),
            &Chamfer {
                distance: 1.0,
                edges: names(&["hole"]),
                ..Default::default()
            },
        ),
    )?;
    // Enclosure corners.
    solid(
        h,
        fillet(
            &block(),
            &Fillet {
                radius: 4.0,
                edges: names(&["bottom", "side"]),
                ..Default::default()
            },
        ),
    )
}

fn adj(h: &mut OutputHash) -> Result<(), HashError> {
    let square = shape_2d::rect(&shape_2d::Rect {
        width: 20.0,
        height: 20.0,
        radius: 0.0,
    })
    .map_err(|_| HashError::NaN)?;
    let pyramid = extrude(
        &square,
        &Extrude {
            height: 20.0,
            taper: 1.0,
            ..Default::default()
        },
    )
    .unwrap_or_default();
    for segments in [3, 4, 6] {
        solid(
            h,
            fillet(
                &pyramid,
                &Fillet {
                    radius: 2.0,
                    edges: names(&["side"]),
                    segments,
                    ..Default::default()
                },
            ),
        )?;
    }
    Ok(())
}

fn prism(corners: &[(f64, f64)], d: &Extrude) -> Solid {
    extrude(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(
            corners.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
        )),
        d,
    )
    .unwrap_or_default()
}

/// Names the real edges whose ends both pass `pick`.
fn tag(s: &Solid, name: &str, pick: impl Fn(DVec3) -> bool) -> Solid {
    let mut out = s.clone();
    for (a, b) in s.real_edges() {
        if pick(s.positions[a as usize]) && pick(s.positions[b as usize]) {
            out.tag_edge(a, b, &[name]);
        }
    }
    out
}

fn round(r: f64, edges: &[&str], segments: u32) -> Fillet {
    Fillet {
        radius: r,
        edges: names(edges),
        segments,
        ..Default::default()
    }
}

/// Unbevelled edges between bevelled ones, and tips (`docs/research.md`
/// §7.4).
fn gaps(h: &mut OutputHash) -> Result<(), HashError> {
    let square = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    let pyramid = prism(
        &square,
        &Extrude {
            height: 20.0,
            taper: 1.0,
            ..Default::default()
        },
    );
    // Two adjacent side edges, then one: the faces past them are cut on
    // profiles around the apex.
    let pair = tag(&pyramid, "pair", |p| {
        p.x >= 0.0 && (p.z == 20.0 || p.z == 0.0)
    });
    let one = tag(&pyramid, "one", |p| {
        p.z == 20.0 || (p.z == 0.0 && p.x > 0.0 && p.y > 0.0)
    });
    solid(h, fillet(&pair, &round(1.5, &["pair"], 4)))?;
    solid(h, fillet(&one, &round(1.5, &["one"], 4)))?;
    solid(
        h,
        chamfer(
            &pair,
            &Chamfer {
                distance: 1.5,
                edges: names(&["pair"]),
                ..Default::default()
            },
        ),
    )?;
    // Part of a cylinder's rim: tips where it meets the rest.
    let c = cylinder(&CylinderDesc {
        radius: 15.0,
        radius_top: 0.0,
        height: 20.0,
        ..Default::default()
    })
    .unwrap_or_default();
    let arc = tag(&c, "arc", |p| p.z == 20.0 && p.x > 0.0 && p.y > 0.0);
    solid(h, fillet(&arc, &round(1.5, &["arc"], 4)))
}

fn l_prism() -> Solid {
    prism(
        &[
            (0.0, 0.0),
            (30.0, 0.0),
            (30.0, 10.0),
            (10.0, 10.0),
            (10.0, 30.0),
            (0.0, 30.0),
        ],
        &Extrude {
            height: 10.0,
            ..Default::default()
        },
    )
}

fn miters(h: &mut OutputHash) -> Result<(), HashError> {
    let l = l_prism();
    for (inner, outer, spread) in [
        (Miter::Arc, Miter::Sharp, 0.0),
        (Miter::Sharp, Miter::Arc, 0.0),
        (Miter::Arc, Miter::Patch, 1.5),
        // A patch miter needs a reflex corner: the error is the output.
        (Miter::Patch, Miter::Sharp, 0.0),
    ] {
        solid(
            h,
            fillet(
                &l,
                &Fillet {
                    inner_miter: inner,
                    outer_miter: outer,
                    spread,
                    ..round(2.0, &["top"], 4)
                },
            ),
        )?;
    }
    solid(
        h,
        chamfer(
            &l,
            &Chamfer {
                distance: 2.0,
                edges: names(&["top"]),
                segments: 2,
                inner_miter: Miter::Arc,
                outer_miter: Miter::Patch,
                ..Default::default()
            },
        ),
    )?;
    solid(
        h,
        fillet(
            &block(),
            &Fillet {
                inner_miter: Miter::Arc,
                ..round(3.0, &[], 4)
            },
        ),
    )
}

fn pipe(h: &mut OutputHash) -> Result<(), HashError> {
    let b = prism(
        &[
            (-20.0, -15.0),
            (0.0, -15.0),
            (20.0, -15.0),
            (20.0, 15.0),
            (-20.0, 15.0),
        ],
        &Extrude {
            height: 20.0,
            ..Default::default()
        },
    );
    let b = tag(&b, "seam", |p| p.x == 0.0 && p.y == -15.0);
    solid(h, fillet(&b, &round(3.0, &["top", "seam"], 8)))
}

fn slide(h: &mut OutputHash) -> Result<(), HashError> {
    let quad = prism(
        &[(0.0, 0.0), (30.0, 0.0), (24.0, 20.0), (-6.0, 14.0)],
        &Extrude {
            height: 10.0,
            taper: 0.3,
            ..Default::default()
        },
    );
    solid(
        h,
        chamfer(
            &quad,
            &Chamfer {
                distance: 2.0,
                edges: names(&["top"]),
                loop_slide: true,
                ..Default::default()
            },
        ),
    )?;
    solid(
        h,
        fillet(
            &quad,
            &Fillet {
                loop_slide: true,
                ..round(2.0, &["top"], 4)
            },
        ),
    )
}

/// Faces in slivers (a join's, a part revolve's), and stream C's
/// untested twisted and tapered extrusions.
fn slivers(h: &mut OutputHash) -> Result<(), HashError> {
    let big = cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })
    .unwrap_or_default();
    let mut small = cuboid(&BoxDesc {
        width: 8.0,
        depth: 8.0,
        height: 10.0,
    })
    .unwrap_or_default();
    small.map_positions(|p| p + DVec3::new(3.0, 2.0, 8.0), false);
    let joined =
        shape_bool::boolean(&big, &small, &shape_bool::Boolean::default()).unwrap_or_default();
    solid(h, fillet(&joined, &round(1.0, &[], 4)))?;
    solid(
        h,
        chamfer(
            &joined,
            &Chamfer {
                distance: 1.0,
                ..Default::default()
            },
        ),
    )?;
    let section = shape_2d::Profile::from_contour(shape_2d::Contour::corners(vec![
        DVec2::new(5.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(10.0, 8.0),
        DVec2::new(5.0, 8.0),
    ]));
    let ring = |angle| {
        revolve(
            &section,
            &Revolve {
                angle,
                axis: Axis::Z,
                ..Default::default()
            },
        )
        .unwrap_or_default()
    };
    for angle in [0.0, 2.0] {
        solid(h, fillet(&ring(angle), &round(0.3, &[], 4)))?;
    }
    // Wider than a segment of the bore: refused.
    solid(h, fillet(&ring(2.0), &round(1.5, &[], 4)))?;
    let square = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    for (twist, taper) in [(1.0, 0.0), (0.0, 0.5)] {
        let s = prism(
            &square,
            &Extrude {
                height: 20.0,
                twist,
                taper,
                ..Default::default()
            },
        );
        solid(h, fillet(&s, &round(1.5, &[], 4)))?;
    }
    Ok(())
}

/// Two cones of faces at one vertex: two boxes joined at a corner
/// (`docs/research.md` §7.5).
fn cones(h: &mut OutputHash) -> Result<(), HashError> {
    let a = cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })
    .unwrap_or_default();
    let mut b = a.clone();
    b.map_positions(|p| p + DVec3::new(20.0, 20.0, 10.0), false);
    let j = shape_bool::boolean(&a, &b, &shape_bool::Boolean::default()).unwrap_or_default();
    solid(h, fillet(&j, &round(2.0, &[], 4)))?;
    let low = tag(&j, "low", |p| p.x <= 10.0 && p.y <= 10.0 && p.z <= 10.0);
    solid(
        h,
        chamfer(
            &low,
            &Chamfer {
                distance: 1.0,
                edges: names(&["low"]),
                ..Default::default()
            },
        ),
    )
}

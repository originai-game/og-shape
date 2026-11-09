//! Golden cases for fillet, chamfer and soften (shape-bevel).

use shape_bevel::{Chamfer, Fillet, Soften, chamfer, fillet, soften};
use shape_core::{HashError, OutputHash, trig};
use shape_edit::{Hole, above, hole};
use shape_gen::solids::{BoxDesc, CylinderDesc, Extrude, cuboid, cylinder, extrude};
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

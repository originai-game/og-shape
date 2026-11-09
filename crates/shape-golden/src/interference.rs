//! Golden cases for stream N (`docs/research.md` §15): loft corners kept
//! as samples, the loft's z kept between its end sections, a folded loft
//! refused, and the interference report of the washing machine at rest,
//! as recorded, and with the door and drawer fully open.

use shape_2d::{Circle, Polygon, Profile, Rect, circle, polygon, rect};
use shape_core::{HashError, OutputHash, trig};
use shape_gen::paths::{Loft, LoftCurve, loft};
use shape_model::program;

use crate::cases::Case;

const WASHING_MACHINE: &str = include_str!("../../../golden/programs/washing_machine.json");

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "interference.washing_machine",
            run: washing_machine,
        },
        Case {
            name: "loft.corners",
            run: corners,
        },
        Case {
            name: "loft.folds",
            run: folds,
        },
    ]
}

/// A generator's solid, which must be closed, into the hash.
fn solid<E: std::fmt::Display>(
    h: &mut OutputHash,
    s: Result<shape_mesh::Solid, E>,
) -> Result<(), HashError> {
    let s = s.map_err(|e| {
        eprintln!("{e}");
        HashError::NaN
    })?;
    let m = s.measure();
    if !m.closed || m.degenerate_triangles > 0 {
        eprintln!("not a closed solid: {m:?}");
        return Err(HashError::NaN);
    }
    s.hash_into(h)
}

fn hexagon(radius: f64) -> Profile {
    polygon(&Polygon { sides: 6, radius }).unwrap_or_default()
}

fn disc(radius: f64, segments: u32) -> Profile {
    circle(&Circle { radius, segments }).unwrap_or_default()
}

/// Hexagon to a smaller hexagon turned 10° (corner to corner), a square
/// to a circle, and a hexagonal frame with a square hole into a round
/// frame, on a Catmull-Rom curve.
fn corners(h: &mut OutputHash) -> Result<(), HashError> {
    solid(
        h,
        loft(&Loft {
            sections: vec![
                (hexagon(10.0), 0.0),
                (hexagon(5.0).rotate(trig::to_radians(10.0)), 10.0),
            ],
            samples: 32,
            ..Default::default()
        }),
    )?;
    let square = |side: f64| {
        rect(&Rect {
            width: side,
            height: side,
            radius: 0.0,
        })
        .unwrap_or_default()
    };
    solid(
        h,
        loft(&Loft {
            sections: vec![(square(10.0), 0.0), (disc(5.0, 0), 10.0)],
            samples: 24,
            ..Default::default()
        }),
    )?;
    solid(
        h,
        loft(&Loft {
            sections: vec![
                (hexagon(20.0).subtract(&square(8.0)), 0.0),
                (hexagon(16.0).subtract(&disc(5.0, 0)), 6.0),
                (disc(12.0, 0).subtract(&disc(4.0, 0)), 14.0),
            ],
            samples: 40,
            steps: 4,
            curve: LoftCurve::CatmullRom,
        }),
    )
}

/// Uneven heights on a Catmull-Rom curve (z runs straight, inside the end
/// sections), and the narrow neck `shape_model::loft` refuses: its
/// message is hashed.
fn folds(h: &mut OutputHash) -> Result<(), HashError> {
    solid(
        h,
        loft(&Loft {
            sections: vec![
                (disc(10.0, 16), 0.0),
                (disc(10.0, 16), 1.0),
                (disc(8.0, 16), 20.0),
                (disc(0.5, 16), 21.0),
            ],
            steps: 4,
            ..Default::default()
        }),
    )?;
    let neck = Loft {
        sections: vec![
            (disc(20.0, 24), 0.0),
            (disc(1.0, 24), 10.0),
            (disc(60.0, 24), 20.0),
        ],
        steps: 6,
        ..Default::default()
    };
    match shape_model::loft(&neck) {
        Ok(_) => Err(HashError::NaN),
        Err(e) => {
            h.bytes(e.to_string().as_bytes());
            Ok(())
        }
    }
}

/// The report at rest, at the recorded pose, and fully open.
fn washing_machine(h: &mut OutputHash) -> Result<(), HashError> {
    let m = program::run(WASHING_MACHINE).map_err(|e| {
        eprintln!("{e}");
        HashError::NaN
    })?;
    let p = |pairs: &[(&str, f64)]| -> Vec<(String, f64)> {
        pairs.iter().map(|(k, v)| ((*k).to_string(), *v)).collect()
    };
    let poses = [
        p(&[
            ("door_hinge", 1.605_702_911_834_783),
            ("drawer_slide", 120.0),
        ]),
        p(&[
            ("door_hinge", 2.495_820_830_351_891_4),
            ("drawer_slide", 200.0),
        ]),
    ];
    let json = program::report_json(&m, &poses).map_err(|e| {
        eprintln!("{e}");
        HashError::NaN
    })?;
    h.bytes(json.as_bytes());
    Ok(())
}

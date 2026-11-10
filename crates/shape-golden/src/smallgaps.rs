//! Golden cases for two kernel gaps from phase 6 (`docs/research.md`
//! §13.6, items 12 and 13; §15.4):
//!
//! - the lever knob's tapered bands, joined: two crossings less than an ulp
//!   apart left a zero-area triangle on one line; it is flipped away, in
//!   the join and when the part is moved;
//! - a hole on a turned box whose axis runs through a face's diagonal, and
//!   a pocket whose axis runs inside a through hole (refused, with the
//!   origin, direction and clearance).
//!
//! Each case hashes the solid, the model's GLB, or the error.

use shape_bool::{BoolOp, Boolean, boolean};
use shape_core::{DVec3, Frame, HashError, OutputHash, trig};
use shape_gen::solids::{BoxDesc, Extrude, cuboid, extrude};
use shape_mesh::Solid;
use shape_model::program;

use crate::cases::Case;

const KNOB: &str = include_str!("../../../golden/programs/smallgaps_knob.json");
const THROUGH: &str = include_str!("../../../golden/programs/smallgaps_through_hole.json");

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "smallgaps.knob.bands",
            run: |h| {
                let (a, b) = bands();
                solid(
                    h,
                    boolean(
                        &a,
                        &b,
                        &Boolean {
                            op: BoolOp::Join,
                            ..Default::default()
                        },
                    ),
                )
            },
        },
        Case {
            name: "smallgaps.knob.program",
            run: |h| replay(h, KNOB),
        },
        Case {
            name: "smallgaps.hole.diagonal",
            run: |h| {
                let (b, at) = diagonal();
                solid(
                    h,
                    shape_edit::hole(
                        &b,
                        &shape_edit::Hole {
                            diameter: 1.0,
                            depth: 3.0,
                            at: vec![at],
                            ..Default::default()
                        },
                    ),
                )
            },
        },
        Case {
            name: "smallgaps.pocket.through_hole",
            run: |h| replay(h, THROUGH),
        },
    ]
}

fn fail(e: impl std::fmt::Display) -> HashError {
    eprintln!("{e}");
    HashError::NaN
}

fn solid<E: std::fmt::Display>(h: &mut OutputHash, r: Result<Solid, E>) -> Result<(), HashError> {
    match r {
        Ok(s) => s.hash_into(h),
        Err(e) => {
            h.str(&format!("error: {e}"));
            Ok(())
        }
    }
}

fn replay(h: &mut OutputHash, json: &str) -> Result<(), HashError> {
    match program::run(json) {
        Ok(m) => {
            h.bytes(&m.glb().map_err(fail)?);
            Ok(())
        }
        Err(e) => {
            h.str(&format!("error: {e}"));
            Ok(())
        }
    }
}

/// A hexagon turned a quarter turn, extruded with a taper and stood at
/// height `z`: one of the knob's bands.
fn band(radius: f64, height: f64, taper: f64, z: f64) -> Solid {
    let p = shape_2d::polygon(&shape_2d::Polygon {
        sides: 6,
        radius,
        ..Default::default()
    })
    .expect("polygon")
    .rotate(trig::PI / 2.0);
    let mut s = extrude(
        &p,
        &Extrude {
            height,
            taper,
            ..Default::default()
        },
    )
    .expect("extrude");
    s.map_positions(|p| p + DVec3::new(0.0, 0.0, z), false);
    s
}

/// The knob's `band_6` and `band_7` (ops 15 and 17 of the phase 6
/// program): the second sunk 0.5 into the first, its corner edges a
/// rounding error off the first's top diagonals.
fn bands() -> (Solid, Solid) {
    (
        band(13.2848, 7.393999999999998, 0.26086956521739124, 54.652),
        band(9.8192, 7.393999999999998, 0.3529411764705883, 61.546),
    )
}

/// A 10 mm box, turned, and a frame on its top face's diagonal looking
/// down the face's normal.
fn diagonal() -> (Solid, Frame) {
    let (s, c) = trig::sin_cos(21.0 * 0.0137);
    let (s2, c2) = trig::sin_cos(21.0 * 0.0137 * 1.7);
    let z = DVec3::new(s * c2, s * s2, c).normalize();
    let turned = Frame::from_normal(DVec3::new(0.3, -0.2, 0.1), z, DVec3::X).expect("frame");
    let mut b = cuboid(&BoxDesc {
        width: 10.0,
        depth: 10.0,
        height: 10.0,
    })
    .expect("box");
    b.map_positions(|p| turned.point_to_world(p), false);
    let at = Frame::from_normal(
        DVec3::new(4.014963521695149, 2.3280884594923625, 9.206745564330646),
        DVec3::new(0.2504811681798565, 0.13331104117759812, 0.9588990304966416),
        DVec3::X,
    )
    .expect("frame");
    (b, at)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §15.4: the phase 6 knob measures closed, with no zero-area
    /// triangle, as built and as moved by its `at`.
    #[test]
    fn the_knob_has_no_degenerate_triangle() {
        let m = program::run(KNOB).expect("knob");
        for p in m.measure().expect("measures") {
            let m = &p.measures;
            assert!(m.closed && m.degenerate_triangles == 0, "{}: {m:?}", p.name);
        }
    }

    /// §13.6, item 13: the pocket's axis runs inside the through hole.
    #[test]
    fn a_pocket_on_a_through_hole_axis_names_the_clearance() {
        let Err(e) = program::run(THROUGH) else {
            panic!("the pocket was cut");
        };
        let e = e.to_string();
        assert!(e.contains("op 9 pocket"), "{e}");
        assert!(e.contains("along [0, 0, 1] meets no face"), "{e}");
        assert!(e.contains("its nearest edge is 2.97"), "{e}");
    }
}

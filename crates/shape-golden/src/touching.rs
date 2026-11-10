//! Golden cases for booleans on operands that only touch
//! (`docs/research.md` §5.4, item 5; §13.6, items 1, 2 and 5): two lofts
//! stacked on a shared face, a box through a torus's ring of vertices, a
//! strap plate along a tube's vertex column with flush ends, boxes sharing
//! only an edge, a box face on a tube's vertex column, a tool wholly inside
//! (a void), and the phase 6 programs: a cup's rim on its shelled wall, two
//! elbows sharing a ring face, and the same elbows turned (still refused,
//! with the faces named). Each case hashes the solid and its report, or the
//! error.

use shape_bool::{BoolOp, BoolReport, Boolean, boolean_report};
use shape_core::{DVec3, HashError, OutputHash};
use shape_gen::loft::{Loft, loft};
use shape_gen::solids::{BoxDesc, cuboid, torus, tube};
use shape_mesh::Solid;
use shape_model::program;

use crate::cases::Case;

const RIM: &str = include_str!("../../../golden/programs/touching_rim.json");
const ELBOWS: &str = include_str!("../../../golden/programs/touching_elbows.json");
const ELBOWS_TURNED: &str = include_str!("../../../golden/programs/touching_elbows_turned.json");

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "touching.lofts.join",
            run: |h| {
                let (a, b) = lofts();
                both(h, &a, &b, BoolOp::Join)
            },
        },
        Case {
            name: "touching.lofts.cut",
            run: |h| {
                let (a, b) = lofts();
                both(h, &a, &b, BoolOp::Cut)
            },
        },
        Case {
            name: "touching.lofts.hexagon",
            run: |h| {
                let (a, b) = polygon_lofts(6);
                both(h, &a, &b, BoolOp::Join)
            },
        },
        Case {
            name: "touching.torus.ring",
            run: |h| {
                let t = torus(351.0, 127.0).map_err(fail)?;
                let q = moved(block(600.0, 400.0, 600.0)?, DVec3::new(300.0, 0.0, 0.0));
                both(h, &t, &q, BoolOp::Intersect)
            },
        },
        Case {
            name: "touching.strap",
            run: |h| {
                let (a, b) = strap()?;
                both(h, &a, &b, BoolOp::Join)
            },
        },
        Case {
            name: "touching.boxes.edge",
            run: |h| {
                let a = block(10.0, 10.0, 10.0)?;
                let b = moved(block(10.0, 10.0, 10.0)?, DVec3::new(10.0, 10.0, 0.0));
                both(h, &a, &b, BoolOp::Join)
            },
        },
        Case {
            name: "touching.tube.tangent",
            run: |h| {
                let t = tube(10.0, 8.0, 30.0).map_err(fail)?;
                let b = moved(block(20.0, 20.0, 20.0)?, DVec3::new(20.0, 0.0, 5.0));
                both(h, &t, &b, BoolOp::Join)
            },
        },
        Case {
            name: "touching.void",
            run: |h| {
                let a = block(40.0, 40.0, 40.0)?;
                let b = moved(block(10.0, 10.0, 10.0)?, DVec3::new(0.0, 0.0, 15.0));
                both(h, &a, &b, BoolOp::Cut)
            },
        },
        Case {
            name: "touching.program.rim",
            run: |h| replay(h, RIM),
        },
        Case {
            name: "touching.program.elbows",
            run: |h| replay(h, ELBOWS),
        },
        Case {
            name: "touching.program.elbows_turned",
            run: |h| replay(h, ELBOWS_TURNED),
        },
    ]
}

fn fail(e: impl std::fmt::Display) -> HashError {
    eprintln!("{e}");
    HashError::NaN
}

fn moved(mut s: Solid, d: DVec3) -> Solid {
    s.map_positions(|p| p + d, false);
    s
}

fn block(w: f64, d: f64, h: f64) -> Result<Solid, HashError> {
    cuboid(&BoxDesc {
        width: w,
        depth: d,
        height: h,
    })
    .map_err(fail)
}

fn lofts() -> (Solid, Solid) {
    let circle = |r: f64| {
        shape_2d::circle(&shape_2d::Circle {
            radius: r,
            segments: 0,
        })
        .expect("circle")
    };
    let a = loft(&Loft {
        sections: vec![(circle(10.0), 0.0), (circle(8.0), 20.0)],
        ..Default::default()
    })
    .expect("loft");
    let b = loft(&Loft {
        sections: vec![(circle(8.0), 0.0), (circle(5.0), 10.0)],
        ..Default::default()
    })
    .expect("loft");
    (a, moved(b, DVec3::new(0.0, 0.0, 20.0)))
}

/// Hexagonal (or other) lofts stacked on a shared face: on main, a join
/// with edges used four times and no error.
fn polygon_lofts(sides: u32) -> (Solid, Solid) {
    let p = |r: f64| {
        shape_2d::polygon(&shape_2d::Polygon {
            sides,
            radius: r,
            ..Default::default()
        })
        .expect("polygon")
    };
    let a = loft(&Loft {
        sections: vec![(p(10.0), 0.0), (p(8.0), 20.0)],
        ..Default::default()
    })
    .expect("loft");
    let b = loft(&Loft {
        sections: vec![(p(8.0), 0.0), (p(5.0), 15.0)],
        ..Default::default()
    })
    .expect("loft");
    (a, moved(b, DVec3::new(0.0, 0.0, 20.0)))
}

/// Phase 6's hinged strap (run `A`'s first build): a plate whose top edges
/// lie on a tube's vertex columns, the tube's ends flush with the plate's.
fn strap() -> Result<(Solid, Solid), HashError> {
    let plate = moved(block(914.4, 1.2, 38.14)?, DVec3::new(0.0, -2.26, 0.0));
    let mut t = tube(2.86, 1.66, 914.4).map_err(fail)?;
    t.map_positions(|p| DVec3::new(-457.2 + p.z, p.x, 38.14 + p.y), false);
    Ok((plate, t))
}

fn both(h: &mut OutputHash, a: &Solid, b: &Solid, op: BoolOp) -> Result<(), HashError> {
    let d = Boolean {
        op,
        ..Default::default()
    };
    match boolean_report(a, b, &d) {
        Ok((s, r)) => {
            let BoolReport {
                touching_edges,
                voids,
                touching,
            } = r;
            h.str(&format!("{touching_edges} {voids} {touching}"));
            s.hash_into(h)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elbows_turned_are_refused_naming_the_faces_that_touch() {
        let Err(e) = program::run(ELBOWS_TURNED) else {
            panic!("the turned elbows joined");
        };
        let e = e.to_string();
        assert!(e.contains("where the operands touch"), "{e}");
        assert!(e.contains("qa.bottom") && e.contains("qb.top"), "{e}");
    }

    #[test]
    fn the_cup_rim_and_untilted_elbows_join_closed() {
        for json in [RIM, ELBOWS] {
            let m = program::run(json).unwrap();
            let v = program::measures_json(&m).unwrap();
            assert!(
                v.contains("\"closed\":true") && !v.contains("\"closed\":false"),
                "{v}"
            );
        }
    }
}

//! Golden cases for booleans on operands that only touch
//! (`docs/research.md` §5.4, item 5; §13.6, items 1, 2 and 5): two lofts
//! stacked on a shared face, a box through a torus's ring of vertices, a
//! strap plate along a tube's vertex column with flush ends, boxes sharing
//! only an edge, a box face on a tube's vertex column, a tool wholly inside
//! (a void), and the phase 6 programs: a cup's rim on its shelled wall, two
//! elbows sharing a ring face, and the same elbows turned about their axis
//! (joined by snapping the tool onto the target, `docs/research.md` §18).
//! Each case hashes the solid and its report, or the error.

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
            name: "touching.elbows.turned",
            run: |h| {
                let (a, b) = turned_elbows()?;
                both(h, &a, &b, BoolOp::Join)
            },
        },
        Case {
            name: "touching.countersink.ring",
            run: |h| {
                // A countersink whose narrow end is the bore's own polygon,
                // cut after the bore: a ring of coincident edges
                // (`docs/research.md` §18).
                let plate = block(40.0, 40.0, 10.0)?;
                let bore = frustum(2.0, 2.0, -30.0, 40.06)?;
                let sink = frustum(2.0, 4.06, 8.0, 2.06)?;
                let d = Boolean {
                    op: BoolOp::Cut,
                    ..Default::default()
                };
                let bored = shape_bool::boolean(&plate, &bore, &d).map_err(fail)?;
                both(h, &bored, &sink, BoolOp::Cut)
            },
        },
        Case {
            name: "touching.gap.slabs",
            run: |h| {
                // Slabs stacked two ulps apart: joined whole, one shell
                // (`docs/research.md` §23).
                let a = block(10.0, 10.0, 1.0)?;
                let top = a.positions.iter().map(|p| p.z).fold(f64::MIN, f64::max);
                let low = a.positions.iter().map(|p| p.z).fold(f64::MAX, f64::min);
                let gap = top.next_up().next_up();
                let mut b = moved(a.clone(), DVec3::new(0.0, 0.0, top - low));
                b.map_positions(
                    |p| {
                        if p.z == top {
                            DVec3::new(p.x, p.y, gap)
                        } else {
                            p
                        }
                    },
                    false,
                );
                both(h, &a, &b, BoolOp::Join)?;
                both(h, &a, &b, BoolOp::Cut)
            },
        },
        Case {
            name: "touching.uncross",
            run: |h| {
                // Outputs that crossed themselves by a rounding error,
                // repaired (`shape_bool::uncross_output`, §23): the post,
                // the countersink ring, the torus ring on a box face.
                let plate = block(20.0, 20.0, 10.0)?;
                let pin = moved(frustum(3.0, 3.0, 0.0, 8.0)?, DVec3::new(0.0, 0.0, 10.0));
                let big = block(40.0, 40.0, 10.0)?;
                let d = Boolean {
                    op: BoolOp::Cut,
                    ..Default::default()
                };
                let bored = shape_bool::boolean(&big, &frustum(2.0, 2.0, -30.0, 40.06)?, &d)
                    .map_err(fail)?;
                let sink = frustum(2.0, 4.06, 8.0, 2.06)?;
                let t = torus(351.0, 127.0).map_err(fail)?;
                let q = moved(block(600.0, 400.0, 600.0)?, DVec3::new(300.0, 0.0, 0.0));
                use shape_bool::CsgOp;
                for (a, b, op) in [
                    (&plate, &pin, CsgOp::Union),
                    (&bored, &sink, CsgOp::Subtract),
                    (&t, &q, CsgOp::Intersect),
                ] {
                    let o = shape_bool::csg_points_traced(
                        (&a.positions, &a.triangles),
                        (&b.positions, &b.triangles),
                        op,
                    )
                    .map_err(|e| fail(format!("{e:?}")))?;
                    let (u, r) = shape_bool::uncross_output(&a.positions, &b.positions, o);
                    h.str(&format!("{r:?}"));
                    h.dvec3s(&u.points)?;
                    for t in &u.tris {
                        for &v in t {
                            h.u64(u64::from(v));
                        }
                    }
                }
                Ok(())
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

/// Two quarter tori turned half a facet about their axis, cut by boxes
/// whose faces lie on the world plane z = 0 in the tori's frame, the
/// second brought back through the world (as a part's placement does): ring
/// faces a few ulps apart (`docs/research.md` §18).
fn turned_elbows() -> Result<(Solid, Solid), HashError> {
    let a = std::f64::consts::PI / 64.0;
    let (c, s) = (shape_core::trig::cos(a), shape_core::trig::sin(a));
    let turn = |mut m: Solid, s: f64| -> Result<Solid, HashError> {
        m.map_positions(
            |p| DVec3::new(c * p.x + s * p.z, p.y, -s * p.x + c * p.z),
            false,
        );
        m.canonicalize()?;
        Ok(m)
    };
    let elbow = |z: f64| -> Result<Solid, HashError> {
        let t = torus(351.0, 127.0).map_err(fail)?;
        let q = turn(
            moved(block(1000.0, 400.0, 600.0)?, DVec3::new(500.0, 0.0, z)),
            -s,
        )?;
        let d = Boolean {
            op: BoolOp::Intersect,
            ..Default::default()
        };
        shape_bool::boolean(&t, &q, &d).map_err(fail)
    };
    let b = turn(turn(elbow(-600.0)?, s)?, -s)?;
    Ok((elbow(0.0)?, b))
}

/// A frustum along z from `z0`, `h` high, radii `r0` to `r1`, on a
/// 24-gon: a hole's tools in `shape-edit`.
fn frustum(r0: f64, r1: f64, z0: f64, h: f64) -> Result<Solid, HashError> {
    let c = shape_2d::circle(&shape_2d::Circle {
        radius: r0,
        segments: 24,
    })
    .map_err(fail)?;
    let s = shape_gen::solids::extrude(
        &c,
        &shape_gen::solids::Extrude {
            height: h,
            taper: 1.0 - r1 / r0,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    Ok(moved(s, DVec3::new(0.0, 0.0, z0)))
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
                ..
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
    fn the_cup_rim_and_the_elbows_join_closed() {
        for json in [RIM, ELBOWS, ELBOWS_TURNED] {
            let m = program::run(json).unwrap();
            let v = program::measures_json(&m).unwrap();
            assert!(
                v.contains("\"closed\":true") && !v.contains("\"closed\":false"),
                "{v}"
            );
        }
    }
}

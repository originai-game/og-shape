//! Golden cases for the self-union and the exact self-crossing check
//! (`docs/research.md` §5.4, item 3): a sphere folded through itself, a
//! sphere folded many times by noise (triple points), two overlapping
//! copies, and repair's fuse step on the folded sphere.

use shape_bool::{Boolean, boolean, check_operands, crossing_pairs, self_union};
use shape_core::{DVec3, HashError, OutputHash};
use shape_gen::solids::sphere;
use shape_mesh::Solid;

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "selfunion.ball.folded",
            run: |h| solid(h, self_union(&folded_ball())),
        },
        Case {
            name: "selfunion.ball.noise",
            run: |h| solid(h, self_union(&noisy_ball(1.0, 7))),
        },
        Case {
            name: "selfunion.copies",
            run: |h| solid(h, self_union(&copies())),
        },
        Case {
            name: "selfunion.check",
            run: check,
        },
        Case {
            name: "selfunion.repair.fuse",
            run: |h| {
                let d = shape_tools::Repair {
                    fuse: true,
                    check_intersections: true,
                    ..Default::default()
                };
                match shape_tools::repair(&folded_ball(), &d) {
                    Ok((s, r)) => {
                        h.str(&format!("{:?} {:?}", r.fused, r.self_intersections));
                        s.hash_into(h)
                    }
                    Err(e) => {
                        h.str(&format!("error: {e}"));
                        Ok(())
                    }
                }
            },
        },
    ]
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

/// A sphere with one vertex pulled through the far side.
fn folded_ball() -> Solid {
    let mut ball = sphere(10.0).unwrap();
    let dir = DVec3::new(-0.51, -0.38, 0.77);
    let mut v = 0;
    for i in 1..ball.positions.len() {
        if ball.positions[i].dot(dir) > ball.positions[v].dot(dir) {
            v = i;
        }
    }
    ball.positions[v] = -ball.positions[v] * 1.5;
    ball
}

/// A ball of radius 10, every vertex moved by up to `amount` per axis
/// (splitmix64).
fn noisy_ball(amount: f64, seed: u64) -> Solid {
    let mut s = sphere(10.0).unwrap();
    let mut state = seed;
    let mut next = || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    };
    for p in &mut s.positions {
        *p += DVec3::new(next(), next(), next()) * amount;
    }
    s
}

/// Two noisy balls, the second turned and moved, in one solid.
fn copies() -> Solid {
    let a = noisy_ball(0.2, 3);
    let mut b = a.clone();
    b.map_positions(
        |p| {
            DVec3::new(p.x * 0.9 - p.y * 0.3, p.x * 0.3 + p.y * 0.9, p.z)
                + DVec3::new(4.0, 2.0, 0.5)
        },
        false,
    );
    let mut two = a;
    two.append(&b);
    two.canonicalize().expect("finite");
    two
}

/// The crossing counts and the refusals they give.
fn check(h: &mut OutputHash) -> Result<(), HashError> {
    let tool = sphere(4.0).unwrap();
    for s in [folded_ball(), noisy_ball(1.0, 7), copies()] {
        h.str(&format!(
            "{} {:?} {:?}",
            crossing_pairs(&s),
            check_operands(&s, &tool),
            boolean(&s, &tool, &Boolean::default()).map(|r| r.triangles.len())
        ));
    }
    Ok(())
}

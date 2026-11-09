//! Self-union on the benchmark's synthetic defects (`bench/booleans`,
//! `docs/research.md` §5.4): a surface whose vertices are moved by noise
//! until it folds through itself in many places, three sheets at a point
//! among them, and two overlapping copies of one shell.

use glam::DVec3;
use shape_bool::{BoolError, BoolOp, Boolean, boolean, check_operands, crossing_pairs, self_union};
use shape_gen::solids::sphere;
use shape_mesh::Solid;

/// A deterministic generator (splitmix64), in [-1, 1).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }
}

/// A ball of radius 10 with every vertex moved by up to `amount` on each
/// axis.
fn noisy_ball(amount: f64, seed: u64) -> Solid {
    let mut s = sphere(10.0).unwrap();
    let mut rng = Rng(seed);
    for p in &mut s.positions {
        *p += DVec3::new(rng.next(), rng.next(), rng.next()) * amount;
    }
    s
}

/// The volume `s` winds round at least once, by the parity-free count of
/// an `n`³ grid of cell centres whose winding number (solid angles) is
/// at least 1/2.
fn sampled_volume(s: &Solid, n: usize) -> f64 {
    let m = s.measure();
    let (lo, size) = (m.bounds_min, m.bounds_max - m.bounds_min);
    let cell = size / n as f64;
    let mut inside = 0usize;
    for i in 0..n {
        for j in 0..n {
            for k in 0..n {
                let p = lo + cell * DVec3::new(i as f64 + 0.5, j as f64 + 0.5, k as f64 + 0.5);
                let mut w = 0.0;
                for t in &s.triangles {
                    let [a, b, c] = t.map(|v| s.positions[v as usize] - p);
                    let (la, lb, lc) = (a.length(), b.length(), c.length());
                    let det = a.dot(b.cross(c));
                    let den = la * lb * lc + a.dot(b) * lc + a.dot(c) * lb + b.dot(c) * la;
                    w += 2.0 * shape_core::trig::atan2(det, den);
                }
                if w / (4.0 * shape_core::trig::PI) > 0.5 {
                    inside += 1;
                }
            }
        }
    }
    inside as f64 * cell.x * cell.y * cell.z
}

#[test]
fn noise_that_folds_the_surface_is_refused_then_untangled() {
    // Noise of about half an edge: hundreds of folds, with three sheets
    // through one point in places (the self-union's triple points).
    let ball = noisy_ball(1.0, 7);
    let pairs = crossing_pairs(&ball);
    assert!(pairs > 50, "{pairs}");
    let tool = sphere(4.0).unwrap();
    assert_eq!(
        check_operands(&ball, &tool),
        Err(BoolError::SelfIntersecting {
            operand: "target",
            pairs
        })
    );
    let u = self_union(&ball).unwrap();
    let m = u.measure();
    assert!(m.closed && m.non_manifold_edges == 0, "{m:?}");
    assert_eq!(crossing_pairs(&u), 0);
    let want = sampled_volume(&ball, 24);
    assert!((m.volume - want).abs() / want < 0.02, "{} {want}", m.volume);
    // The result takes a boolean.
    let mut moved = tool.clone();
    moved.map_positions(|p| p + DVec3::new(7.3, 1.1, 0.7), false);
    let cut = boolean(
        &u,
        &moved,
        &Boolean {
            op: BoolOp::Cut,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(cut.measure().closed);
}

#[test]
fn two_overlapping_noisy_copies_become_one_shell() {
    let a = noisy_ball(0.2, 3);
    let mut b = a.clone();
    b.map_positions(
        |p| {
            DVec3::new(p.x * 0.9 - p.y * 0.3, p.x * 0.3 + p.y * 0.9, p.z)
                + DVec3::new(4.0, 2.0, 0.5)
        },
        false,
    );
    let mut two = a.clone();
    two.append(&b);
    two.canonicalize().unwrap();
    let u = self_union(&two).unwrap();
    let m = u.measure();
    assert!(m.closed && m.components == 1, "{m:?}");
    assert_eq!(crossing_pairs(&u), 0);
    // fuse (which joins the shells in turn) gives the same volume.
    let f = shape_bool::fuse(&two).unwrap().measure();
    assert!(
        (m.volume - f.volume).abs() < 1e-6 * f.volume,
        "{} {}",
        m.volume,
        f.volume
    );
}

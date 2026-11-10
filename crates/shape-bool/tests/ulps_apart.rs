#![allow(missing_docs)]
//! Faces a few ulps apart (`docs/research.md` §23.4, §25): two 32-gon
//! prisms of radius 10 stacked on one ring at z = 10, tilted so that ring
//! is not one plane in f64, the tool's bottom cap triangulated another way
//! or with a vertex at each edge's midpoint.

use shape_bool::{BoolError, BoolOp, Boolean, boolean, boolean_report, crossing_pairs};
use shape_core::{DVec3, trig};
use shape_mesh::Solid;

/// A 32-gon prism of radius 10 from `z0` to `z1`, each cap fanned from
/// its corner `fan`; with `midpoints`, the bottom ring has a vertex at
/// each edge's midpoint too (the walls take them as well).
fn prism(z0: f64, z1: f64, fan: [usize; 2], midpoints: bool) -> Solid {
    const N: usize = 32;
    let corner = |k: usize, z: f64| {
        let a = trig::TAU * (k % N) as f64 / N as f64;
        DVec3::new(10.0 * trig::cos(a), 10.0 * trig::sin(a), z)
    };
    let mut s = Solid::default();
    // Bottom ring: corner k at 2k (and its edge's midpoint at 2k + 1).
    let step = if midpoints { 2 } else { 1 };
    for k in 0..N {
        s.positions.push(corner(k, z0));
        if midpoints {
            s.positions.push((corner(k, z0) + corner(k + 1, z0)) / 2.0);
        }
    }
    let bottom = s.positions.len();
    let top = bottom as u32;
    for k in 0..N {
        s.positions.push(corner(k, z1));
    }
    let b = |i: usize| (i % bottom) as u32;
    let t = |k: usize| top + (k % N) as u32;
    // Caps: fans, the bottom's facing down; with midpoints, from the
    // midpoint after corner `fan[0]`, so no fan triangle lies along an edge.
    let f0 = fan[0] * step + usize::from(midpoints);
    for i in 1..bottom - 1 {
        s.push_triangle([b(f0), b(f0 + i + 1), b(f0 + i)], 0, &["bottom"]);
    }
    for i in 1..N - 1 {
        s.push_triangle([t(fan[1]), t(fan[1] + i), t(fan[1] + i + 1)], 1, &["top"]);
    }
    for k in 0..N {
        let (v0, v1) = (b(k * step), b((k + 1) * step));
        if midpoints {
            let m = b(k * step + 1);
            s.push_triangle([v0, m, t(k)], 2, &["side"]);
            s.push_triangle([m, v1, t(k + 1)], 2, &["side"]);
            s.push_triangle([m, t(k + 1), t(k)], 2, &["side"]);
        } else {
            s.push_triangle([v0, v1, t(k + 1)], 2, &["side"]);
            s.push_triangle([v0, t(k + 1), t(k)], 2, &["side"]);
        }
    }
    s.canonicalize().unwrap();
    s
}

/// Turned 0.3 rad about x, then 0.2 about y: the ring at z = 10 is no
/// longer one plane in f64. Both prisms turn their shared ring's corners
/// to the same f64 points.
fn tilted(mut s: Solid) -> Solid {
    let (c1, s1) = (trig::cos(0.3), trig::sin(0.3));
    let (c2, s2) = (trig::cos(0.2), trig::sin(0.2));
    s.map_positions(
        |p| {
            let q = DVec3::new(p.x, c1 * p.y - s1 * p.z, s1 * p.y + c1 * p.z);
            DVec3::new(c2 * q.x + s2 * q.z, q.y, -s2 * q.x + c2 * q.z)
        },
        false,
    );
    s
}

fn op(o: BoolOp) -> Boolean<'static> {
    Boolean {
        op: o,
        ..Default::default()
    }
}

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// One ring, two triangulations, not one plane by ulps: the join and the
/// cut are refused (open, naming the faces that touch; not manifold),
/// never returned.
#[test]
fn a_tilted_ring_fanned_two_ways_is_refused() {
    let a = tilted(prism(0.0, 10.0, [0, 0], false));
    let b = tilted(prism(10.0, 20.0, [5, 0], false));
    assert_eq!(
        boolean(&a, &b, &op(BoolOp::Join)),
        Err(BoolError::OpenTouching {
            boundary: 8,
            target: names(&["side", "top"]),
            tool: names(&["bottom", "side"]),
        })
    );
    assert_eq!(
        boolean(&a, &b, &op(BoolOp::Cut)),
        Err(BoolError::NotManifold { edges: 1 })
    );
}

/// The tool's bottom ring with a vertex at each edge's midpoint: flat, the
/// join and the cut close, touching, crossing nothing; tilted, the join is
/// refused, naming the faces that touch.
#[test]
fn a_ring_with_midpoints_joins_flat_and_is_refused_tilted() {
    let a = prism(0.0, 10.0, [0, 0], false);
    let b = prism(10.0, 20.0, [0, 0], true);
    let (va, vb) = (a.measure().volume, b.measure().volume);
    for (o, v) in [(BoolOp::Join, va + vb), (BoolOp::Cut, va)] {
        let (s, r) = boolean_report(&a, &b, &op(o)).unwrap();
        let m = s.measure();
        assert!(m.closed && r.touching, "{o:?} {r:?}");
        assert_eq!(r.touching_edges, 0, "{o:?}");
        assert!((m.volume - v).abs() < 1e-9 * v, "{o:?} {}", m.volume);
        assert_eq!(crossing_pairs(&s), 0, "{o:?}");
    }
    match boolean(&tilted(a), &tilted(b), &op(BoolOp::Join)) {
        Err(BoolError::OpenTouching { target, tool, .. }) => {
            assert_eq!(target, names(&["side", "top"]));
            assert_eq!(tool, names(&["bottom", "side"]));
        }
        r => panic!("{r:?}"),
    }
    // The cut is closed and of the right volume, but its top cap's pieces
    // fold over one another by more than the repair can part: refused.
    match boolean(
        &tilted(prism(0.0, 10.0, [0, 0], false)),
        &tilted(prism(10.0, 20.0, [0, 0], true)),
        &op(BoolOp::Cut),
    ) {
        Err(BoolError::CrossingResult { pairs }) => assert!(pairs > 0),
        r => panic!("{r:?}"),
    }
}

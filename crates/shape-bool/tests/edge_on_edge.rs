#![allow(missing_docs)]
//! A plate minus a pin whose vertex columns lie on the plate caps' 45°
//! diagonal, within rounding: crossings there are a rounding error apart
//! and must still close (`docs/research.md` §5.4, follow-up 4).

use shape_bool::{BoolOp, Boolean, CsgOp, boolean, csg_points};
use shape_core::DVec3;
use shape_gen::solids::{BoxDesc, CylinderDesc, cuboid, cylinder};
use shape_mesh::Solid;
use std::collections::BTreeMap;

fn plate() -> Solid {
    cuboid(&BoxDesc {
        width: 40.0,
        depth: 40.0,
        height: 10.0,
    })
    .unwrap()
}

fn pin(radius: f64, z: f64, height: f64) -> Solid {
    let mut s = cylinder(&CylinderDesc {
        radius,
        height,
        ..Default::default()
    })
    .unwrap();
    s.map_positions(|p| p + DVec3::new(0.0, 0.0, z), false);
    s
}

/// Edges not used exactly once each way (open or non-manifold).
fn bad_edges(tris: &[[u32; 3]]) -> usize {
    let mut directed: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in tris {
        for k in 0..3 {
            *directed.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
        }
    }
    directed
        .iter()
        .filter(|&(&(a, b), &n)| n != 1 || directed.get(&(b, a)) != Some(&1))
        .count()
}

fn check(radius: f64, z: f64, height: f64) {
    let (a, b) = (plate(), pin(radius, z, height));
    let (_, tris, _) = csg_points(
        (&a.positions, &a.triangles),
        (&b.positions, &b.triangles),
        CsgOp::Subtract,
    )
    .unwrap();
    assert_eq!(bad_edges(&tris), 0, "csg_points r = {radius}, z = {z}");
    let s = boolean(
        &a,
        &b,
        &Boolean {
            op: BoolOp::Cut,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(bad_edges(&s.triangles), 0, "boolean r = {radius}, z = {z}");
    // The bore removes the pin's polygon times the plate's thickness it
    // spans.
    let span = 10.0 - z.max(0.0);
    let unit = pin(1.0, 0.0, 1.0).measure().volume;
    let v = s.measure().volume;
    assert!(
        (v - (16_000.0 - unit * radius * radius * span)).abs() < 1e-9,
        "volume {v}, r = {radius}, z = {z}"
    );
}

#[test]
fn through_bore_on_the_cap_diagonal_is_closed() {
    check(2.0, -1.0, 12.0);
    check(3.0, -1.0, 12.0);
}

#[test]
fn blind_bore_on_the_cap_diagonal_is_closed() {
    check(2.0, 4.0, 7.0);
}

/// A frustum along z from `z0`, `h` high, radii `r0` to `r1`, on an
/// `n`-gon (`shape-edit`'s hole tools, unturned).
fn frustum(r0: f64, r1: f64, z0: f64, h: f64, n: u32) -> Solid {
    let c = shape_2d::circle(&shape_2d::Circle {
        radius: r0,
        segments: n,
    })
    .unwrap();
    let mut s = shape_gen::solids::extrude(
        &c,
        &shape_gen::solids::Extrude {
            height: h,
            taper: 1.0 - r1 / r0,
            ..Default::default()
        },
    )
    .unwrap();
    s.map_positions(|p| p + DVec3::new(0.0, 0.0, z0), false);
    s
}

#[test]
fn a_countersink_ending_on_the_bore_closes_in_either_order() {
    // The cone's narrow end is the bore's own polygon, at the same angles:
    // its ring of vertices lies on the bore's vertex columns and its ring
    // edges in the bore's wall planes (`docs/research.md` §5.4, item 4;
    // §18). With the bore cut first, the wall's pieces are bent by the
    // rounding of the first cut, and a ring edge almost in a piece's plane
    // made its crossing 0/0: a NaN, refused as a position not finite.
    let cut = |a: &Solid, b: &Solid| {
        boolean(
            a,
            b,
            &Boolean {
                op: BoolOp::Cut,
                ..Default::default()
            },
        )
        .unwrap()
    };
    for (r, mouth, depth) in [(2.0, 4.0, 0.0), (2.0, 4.0, 6.0), (3.3, 6.6, 0.0)] {
        let n = 24;
        let d = if depth > 0.0 { depth } else { 40.0 };
        let h = mouth - r;
        let bore = frustum(r, r, 10.0 - d, d + 0.06, n);
        let sink = frustum(r, mouth + 0.06, 10.0 - h, h + 0.06, n);
        let one = cut(&cut(&plate(), &bore), &sink).measure();
        let two = cut(&cut(&plate(), &sink), &bore).measure();
        assert!(one.closed && two.closed, "{r} {depth}: {one:?} {two:?}");
        assert!(
            (one.volume - two.volume).abs() < 1e-9 * one.volume,
            "{r} {depth}"
        );
    }
}

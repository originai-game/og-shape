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

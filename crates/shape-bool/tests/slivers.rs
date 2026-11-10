#![allow(missing_docs)]
//! Zero-area triangles on one line (`docs/research.md` §15.4; §13.6,
//! item 12): the phase 6 lever knob's tapered bands, joined. `band_7`'s
//! corner edges pass a rounding error off `band_6`'s top diagonals, so the
//! corner edge through the top and the diagonal through the side are two
//! crossings less than an ulp apart, and with a third point of the cut they
//! rounded onto one line.
//!
//! The intersection keeps a needle along `band_6`'s top diagonal whose
//! short side is that ulp: its exact area is about 7·10⁻¹⁵, not zero, but
//! its f64 cross product rounds to zero, so `measure` counts it. Only a
//! tolerance could remove it, so it is not tested here.

use shape_bool::{BoolOp, Boolean, boolean};
use shape_core::{DVec3, trig};
use shape_gen::solids::{Extrude, extrude};
use shape_mesh::Solid;

fn band(radius: f64, height: f64, taper: f64, z: f64) -> Solid {
    let p = shape_2d::polygon(&shape_2d::Polygon {
        sides: 6,
        radius,
        ..Default::default()
    })
    .unwrap()
    .rotate(trig::PI / 2.0);
    let mut s = extrude(
        &p,
        &Extrude {
            height,
            taper,
            ..Default::default()
        },
    )
    .unwrap();
    s.map_positions(|p| p + DVec3::new(0.0, 0.0, z), false);
    s
}

#[test]
fn tapered_bands_join_without_a_degenerate_triangle() {
    let a = band(13.2848, 7.393999999999998, 0.26086956521739124, 54.652);
    let b = band(9.8192, 7.393999999999998, 0.3529411764705883, 61.546);
    for op in [BoolOp::Join, BoolOp::Cut] {
        let s = boolean(
            &a,
            &b,
            &Boolean {
                op,
                ..Default::default()
            },
        )
        .unwrap();
        let m = s.measure();
        assert!(m.closed, "{op:?}: {m:?}");
        assert_eq!(m.degenerate_triangles, 0, "{op:?}");
    }
    let v = |op| {
        boolean(
            &a,
            &b,
            &Boolean {
                op,
                ..Default::default()
            },
        )
        .unwrap()
        .measure()
        .volume
    };
    let (va, vb) = (a.measure().volume, b.measure().volume);
    let (j, i) = (v(BoolOp::Join), v(BoolOp::Intersect));
    assert!((j + i - va - vb).abs() < 1e-9 * va, "{j} {i} {va} {vb}");
}

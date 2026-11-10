//! `docs/research.md` §24: a `sweep` and a `wire` along a path of curve
//! pieces end square to the path's tangent, not to the first and last
//! chord (`eval/phase6/Z/repro/sweep_end_tangent.json`).

use shape_core::DVec3;
use shape_gen::path3::{Path3, Piece, flatten};
use shape_gen::paths::{Sweep, sweep};
use shape_gen::wire::{Wire, wire};

#[test]
fn a_quarter_arcs_tube_ends_on_the_arcs_end_planes() {
    // A quarter circle of radius 276.39 about (276.39, 0, 0): tangent to
    // +z at the origin and to +x at (276.39, 0, 276.39).
    let r = 276.39;
    let half = (0.5f64).sqrt();
    let path = flatten(&Path3 {
        start: DVec3::ZERO,
        pieces: vec![Piece::Arc {
            through: DVec3::new(r * (1.0 - half), 0.0, r * half),
            to: DVec3::new(r, 0.0, r),
        }],
    })
    .unwrap();
    let disc = shape_2d::circle(&shape_2d::Circle {
        radius: 127.0,
        segments: 0,
    })
    .unwrap();
    let tube = sweep(
        &disc,
        &Sweep {
            path: path.points.clone(),
            up: DVec3::Y,
            tangents: path.tangents,
            ..Default::default()
        },
    )
    .unwrap();
    let rod = wire(&Wire {
        path: path.points.clone(),
        diameter: 254.0,
        tangents: path.tangents,
        ..Default::default()
    })
    .unwrap();
    for s in [tube, rod] {
        let m = s.measure();
        assert!(m.closed);
        // With the chord's ends: z from −5.85, x to 282.24.
        assert!(m.bounds_min.z.abs() < 1e-9, "{:?}", m.bounds_min);
        assert!((m.bounds_max.x - r).abs() < 1e-9, "{:?}", m.bounds_max);
    }
}

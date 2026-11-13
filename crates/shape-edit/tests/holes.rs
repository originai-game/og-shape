//! Holes that meet.

use shape_core::{DVec3, Frame};
use shape_edit::{Hole, hole};
use shape_gen::solids::{BoxDesc, cuboid};

/// Three holes of one diameter along the three axes, through one point.
/// The third's wall runs through the edges the first two already meet
/// on; cut as it stands, three faces share an edge and the cut is
/// refused, so the tool is turned about its own axis until its corners
/// are off them. The block comes out closed, in one piece, with the
/// volume of a block less three bores that share their crossing.
#[test]
fn three_holes_cross_at_one_point() {
    let block = cuboid(&BoxDesc {
        width: 60.0,
        depth: 60.0,
        height: 40.0,
    })
    .unwrap();
    let along = |origin: DVec3, x: DVec3, z: DVec3| Frame {
        origin,
        x,
        y: z.cross(x),
        z,
    };
    let mid = DVec3::new(0.0, 0.0, 20.0);
    let mut s = block.clone();
    for f in [
        along(mid + DVec3::X * 30.0, DVec3::Y, DVec3::X),
        along(mid + DVec3::Y * 30.0, DVec3::X, DVec3::Y),
        along(mid + DVec3::Z * 20.0, DVec3::X, DVec3::Z),
    ] {
        s = hole(
            &s,
            &Hole {
                diameter: 8.0,
                at: vec![f],
                ..Default::default()
            },
        )
        .unwrap();
    }
    let m = s.measure();
    assert!(m.closed);
    assert_eq!(shape_bool::components(&s).len(), 1);
    // 144 000 less bores of 60, 60 and 40 (a little under π·16 a
    // millimetre: the bores are polygons), plus what they share twice.
    let bores = core::f64::consts::PI * 16.0 * 160.0;
    assert!(
        m.volume > 144_000.0 - bores && m.volume < 144_000.0 - bores + 1_200.0,
        "{}",
        m.volume
    );
}

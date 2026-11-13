//! Morph targets ride a bevel.

use shape_bevel::{Fillet, fillet};
use shape_core::DVec3;
use shape_gen::solids::{BoxDesc, cuboid};
use shape_mesh::{Morph, Morphs};

/// A filleted morphing box: every vertex has an offset, each what the
/// box's surface nearest it has (the field is linear, so a vertex still
/// on a face of the box has exactly the field's), and the corners'
/// offsets are near the field's, a fillet's reach away.
#[test]
fn a_fillet_keeps_the_targets() {
    let field = |p: DVec3| DVec3::new(0.1 * p.z, 0.0, 0.05 * p.x);
    let mut s = cuboid(&BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    })
    .unwrap();
    s.morphs = Some(Morphs {
        targets: vec![Morph {
            name: "lean".into(),
            weight: 0.0,
            deltas: s.positions.iter().map(|p| field(*p)).collect(),
        }],
    });
    let round = fillet(
        &s,
        &Fillet {
            radius: 3.0,
            ..Default::default()
        },
    )
    .unwrap();
    let m = round.morphs.as_ref().expect("the target rode along");
    m.check(round.positions.len()).unwrap();
    let mut on_faces = 0;
    for (p, d) in round.positions.iter().zip(&m.targets[0].deltas) {
        let off = (*d - field(*p)).length();
        // The field changes by at most 0.1 a millimetre: a vertex of
        // the round, at most 3 mm from the box, is within 0.3 of it.
        assert!(off < 0.3 + 1e-9, "{p}: {off}");
        if off < 1e-9 {
            on_faces += 1;
        }
    }
    assert!(on_faces >= 24, "{on_faces}");
    assert!(round.morphed(&[1.0]).measure().closed);
}

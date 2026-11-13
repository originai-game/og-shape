//! Morph targets ride a bevel.

use shape_bevel::{Fillet, fillet};
use shape_core::DVec3;
use shape_gen::solids::{BoxDesc, cuboid};
use shape_mesh::{Morph, Morphs};

/// A filleted morphing box: every vertex has an offset. For a target
/// that leans the box (its round would be another mesh), each is what
/// the box's surface nearest it has: exactly the field's on a face,
/// near it a fillet's reach away. For one that stretches it, each is
/// what filleting the stretched box gives, less the box filleted.
#[test]
fn a_fillet_keeps_the_targets() {
    let field = |p: DVec3| DVec3::new(0.1 * p.z, 0.0, 0.05 * p.x);
    let stretch = |p: DVec3| DVec3::new(0.25 * p.x, 0.0, 0.0);
    let mut s = cuboid(&BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    })
    .unwrap();
    s.morphs = Some(Morphs {
        targets: vec![
            Morph {
                name: "lean".into(),
                weight: 0.0,
                deltas: s.positions.iter().map(|p| field(*p)).collect(),
            },
            Morph {
                name: "stretch".into(),
                weight: 0.0,
                deltas: s.positions.iter().map(|p| stretch(*p)).collect(),
            },
        ],
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
    assert!(round.morphed(&[1.0, 0.0]).measure().closed);
    // Stretched, the box's corners stay square and its round is the
    // same mesh: the round at the target is the round of the box at
    // the target, vertex for vertex (the rounds keep their radius; a
    // stretched round would not).
    let at = round.morphed(&[0.0, 1.0]);
    let mut moved = shape_mesh::bare(&s);
    for p in &mut moved.positions {
        *p += stretch(*p);
    }
    let again = fillet(
        &moved,
        &Fillet {
            radius: 3.0,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(again.triangles == at.triangles, "the mesh differs");
    let off = again
        .positions
        .iter()
        .zip(&at.positions)
        .map(|(a, b)| (*a - *b).length())
        .fold(0.0, f64::max);
    assert!(off < 1e-9, "{off}");
}

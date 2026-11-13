//! Morph targets ride what the kernel makes of a solid.

use shape_core::{DVec3, Frame};
use shape_edit::{Hole, Keep, Subdivide, hole, split, subdivide};
use shape_gen::solids::{BoxDesc, cuboid};
use shape_mesh::{Morph, Morphs, Solid};

/// A 40 × 30 × 20 box whose target `lean` moves each point by a field
/// linear in its position, so that the offset any point of its surface
/// should have is known: 0.1 z along x, 0.05 x along z.
fn leaning() -> Solid {
    let mut s = cuboid(&BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    })
    .unwrap();
    s.morphs = Some(Morphs {
        targets: vec![Morph {
            name: "lean".into(),
            weight: 0.25,
            deltas: s.positions.iter().map(|p| field(*p)).collect(),
        }],
    });
    s
}

fn field(p: DVec3) -> DVec3 {
    DVec3::new(0.1 * p.z, 0.0, 0.05 * p.x)
}

fn lean(s: &Solid) -> &Morph {
    let m = s.morphs.as_ref().expect("the targets rode along");
    m.check(s.positions.len()).unwrap();
    assert_eq!((m.targets.len(), m.targets[0].weight), (1, 0.25));
    &m.targets[0]
}

/// A hole through a morphing box: the box's own corners keep their
/// offsets, the hole's rims on the box's faces move as the faces do
/// there (the field is linear, so exactly), and the tool, which has no
/// target, adds none of its own.
#[test]
fn a_hole_s_rim_moves_with_the_face_it_is_cut_in() {
    let s = leaning();
    let cut = hole(
        &s,
        &Hole {
            diameter: 8.0,
            at: vec![Frame::from_normal(DVec3::new(5.0, 3.0, 30.0), DVec3::Z, DVec3::X).unwrap()],
            ..Default::default()
        },
    )
    .unwrap();
    assert!(cut.positions.len() > s.positions.len());
    let m = lean(&cut);
    for (p, d) in cut.positions.iter().zip(&m.deltas) {
        assert!((*d - field(*p)).length() < 1e-9, "{p}: {d}");
    }
    // The shape at the target is closed, and the field (a shear of
    // determinant 1 − 0.1 × 0.05) scales its volume by just that.
    let at = cut.morphed(&[1.0]);
    assert!(at.measure().closed);
    let ratio = at.measure().volume / cut.measure().volume;
    assert!((ratio - 0.995).abs() < 1e-9, "{ratio}");
}

/// Cutting a morphing box in two keeps the target on the half kept,
/// the cut's new corners included.
#[test]
fn a_split_keeps_the_target_on_the_half_kept() {
    let s = leaning();
    let frame = Frame::from_normal(DVec3::new(3.0, 0.0, 0.0), DVec3::X, DVec3::Y).unwrap();
    let half = split(&s, &frame, Keep::Above).unwrap();
    let m = lean(&half);
    for (p, d) in half.positions.iter().zip(&m.deltas) {
        assert!((*d - field(*p)).length() < 1e-9, "{p}: {d}");
    }
}

/// A subdivided morphing box has a target on every new vertex, and at
/// the target it is the shape the moved box subdivides to, nearly: the
/// offsets are read from the cage's surface.
#[test]
fn a_subdivided_solid_keeps_its_targets() {
    let s = leaning();
    let fine = subdivide(&s, &Subdivide::default()).unwrap();
    let m = lean(&fine);
    assert!(m.deltas.iter().any(|d| d.length() > 0.5));
    let mut moved = s.clone();
    moved.morphs = None;
    moved.map_positions(|p| p + field(p), false);
    let want = subdivide(&moved, &Subdivide::default())
        .unwrap()
        .measure()
        .volume;
    let got = fine.morphed(&[1.0]).measure().volume;
    assert!((got - want).abs() < 0.02 * want, "{got} {want}");
}

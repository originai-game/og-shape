use shape_bool::{Boolean, boolean};
use shape_core::{DVec3, Frame, trig};
use shape_gen::solids::{CylinderDesc, cylinder};
use shape_mesh::Solid;

use crate::{Pose, Rig, RigBone, RigError, Turn, measure_pose, pose, skin};

/// An arm along +z: an upper arm (r 10, z 0..60) joined to a forearm
/// (r 9, z 55..120), each face named after its part.
fn arm() -> Solid {
    let part = |r: f64, z0: f64, z1: f64, name: &str| {
        let mut c = cylinder(&CylinderDesc {
            radius: r,
            height: z1 - z0,
            ..Default::default()
        })
        .unwrap();
        c.map_positions(|p| p + DVec3::Z * z0, false);
        c.tag_all_faces(name);
        c
    };
    boolean(
        &part(10.0, 0.0, 60.0, "upper_arm"),
        &part(9.0, 55.0, 120.0, "forearm"),
        &Boolean::default(),
    )
    .unwrap()
}

fn rig(blend: f64) -> Rig {
    Rig {
        bones: vec![
            RigBone {
                name: "upper_arm".into(),
                pivot: Frame::IDENTITY,
                regions: vec!["upper_arm".into()],
                ..Default::default()
            },
            RigBone {
                name: "forearm".into(),
                parent: "upper_arm".into(),
                pivot: Frame {
                    origin: DVec3::new(0.0, 0.0, 57.5),
                    ..Frame::IDENTITY
                },
                regions: vec!["forearm".into()],
            },
        ],
        blend,
        ..Default::default()
    }
}

/// The forearm's weight at each vertex.
fn forearm_weight(s: &Solid) -> Vec<(DVec3, f64)> {
    let skin = s.skin.as_ref().unwrap();
    s.positions
        .iter()
        .zip(&skin.weights)
        .map(|(&p, w)| (p, w.pairs().find(|x| x.0 == 1).map_or(0.0, |x| x.1)))
        .collect()
}

#[test]
fn weights_are_pure_away_from_the_seam_and_blend_across_it() {
    let s = skin(&arm(), &rig(20.0)).unwrap();
    let skin_ = s.skin.as_ref().unwrap();
    skin_.check(s.positions.len()).unwrap();
    assert!(s.measure().closed);
    // The seam: where the forearm leaves the upper arm's bottom... its
    // end cap at z = 60, r = 9.
    for (p, w) in forearm_weight(&s) {
        let r = DVec3::new(p.x, p.y, 0.0).length();
        if p.z < 40.0 || (p.z < 60.0 && r > 9.5 && p.z < 45.0) {
            assert_eq!(w, 0.0, "{p}");
        }
        if p.z > 75.0 {
            assert_eq!(w, 1.0, "{p}");
        }
        if (p.z - 60.0).abs() < 1e-9 && (r - 9.0).abs() < 1e-6 {
            assert!((w - 0.5).abs() < 1e-12, "{p}: {w}");
        }
    }
    // Along the forearm's side, the weight rises with distance from the
    // seam, and the band has vertices to bend (refined to blend / 4).
    let mut side: Vec<(f64, f64)> = forearm_weight(&s)
        .into_iter()
        .filter(|(p, _)| (DVec3::new(p.x, p.y, 0.0).length() - 9.0).abs() < 1e-6 && p.z >= 60.0)
        .map(|(p, w)| (p.z, w))
        .collect();
    side.sort_by(|a, b| a.0.total_cmp(&b.0));
    for pair in side.windows(2) {
        assert!(pair[1].1 >= pair[0].1 - 1e-12, "{pair:?}");
    }
    let in_band = side.iter().filter(|(z, _)| *z > 60.0 && *z < 70.0).count();
    assert!(in_band >= 2, "{side:?}");
}

#[test]
fn skinning_is_deterministic_and_independent_of_build_order() {
    let a = skin(&arm(), &rig(20.0)).unwrap();
    let b = skin(&arm(), &rig(20.0)).unwrap();
    assert_eq!(a, b);
}

#[test]
fn hard_seams_split_seam_vertices_in_half() {
    let s = skin(&arm(), &rig(0.0)).unwrap();
    let n = s.positions.len();
    assert_eq!(n, arm().positions.len());
    for (p, w) in forearm_weight(&s) {
        assert!(w == 0.0 || w == 1.0 || (w - 0.5).abs() < 1e-15, "{p}: {w}");
    }
}

#[test]
fn rest_pose_leaves_the_mesh_and_a_bend_turns_the_forearm() {
    // A blend of four radii: no triangle inverts at 90° (two radii fold
    // the inner crease: see docs/research.md §11).
    let s = skin(&arm(), &rig(40.0)).unwrap();
    let rest = pose(&s, &Pose::default()).unwrap();
    for (a, b) in s.positions.iter().zip(&rest.positions) {
        assert!((*a - *b).length() < 1e-12);
    }
    assert!(rest.skin.is_none());
    let bend = Pose {
        turns: vec![Turn {
            bone: "forearm".into(),
            axis: DVec3::X,
            angle: trig::to_radians(90.0),
        }],
    };
    let posed = pose(&s, &bend).unwrap();
    // The wrist's centre (0, 0, 120) turns about x through (0, 0, 57.5)
    // to (0, -62.5, 57.5).
    let wrist = s
        .positions
        .iter()
        .position(|p| *p == DVec3::new(0.0, 0.0, 120.0));
    if let Some(i) = wrist {
        assert!((posed.positions[i] - DVec3::new(0.0, -62.5, 57.5)).length() < 1e-9);
    }
    for (p, q) in s.positions.iter().zip(&posed.positions) {
        if p.z < 40.0 {
            assert_eq!(p, q);
        }
        if p.z > 80.0 {
            let expected = DVec3::new(p.x, -(p.z - 57.5), 57.5 + p.y);
            assert!((*q - expected).length() < 1e-9, "{p} -> {q}");
        }
    }
    let m = measure_pose(&s, &bend).unwrap();
    assert!(m.closed);
    assert_eq!(m.inverted, 0, "{m:?}");
    assert!(m.min_area_ratio > 0.0);
}

#[test]
fn a_parent_turn_carries_its_children() {
    let s = skin(&arm(), &rig(20.0)).unwrap();
    let swing = Pose {
        turns: vec![Turn {
            bone: "upper_arm".into(),
            axis: DVec3::Y,
            angle: trig::to_radians(30.0),
        }],
    };
    let posed = pose(&s, &swing).unwrap();
    // A rigid turn of the whole arm: every vertex turned about y.
    let (sn, cs) = trig::sin_cos(trig::to_radians(30.0));
    for (p, q) in s.positions.iter().zip(&posed.positions) {
        let e = DVec3::new(cs * p.x + sn * p.z, p.y, -sn * p.x + cs * p.z);
        assert!((*q - e).length() < 1e-9);
    }
}

#[test]
fn rig_errors_are_values() {
    let a = arm();
    let mut r = rig(20.0);
    r.bones[1].regions = vec!["hand".into()];
    assert!(matches!(skin(&a, &r), Err(RigError::NoSuchRegion(m, _)) if m == ["hand"]));
    let mut r = rig(20.0);
    r.bones[1].regions.clear();
    assert!(matches!(skin(&a, &r), Err(RigError::Unassigned(_))));
    let mut r = rig(20.0);
    r.bones[1].regions = vec!["upper_arm".into()];
    assert!(matches!(skin(&a, &r), Err(RigError::Ambiguous(_, b)) if b.len() == 2));
    let mut r = rig(20.0);
    r.bones[1].parent = "hand".into();
    assert!(matches!(skin(&a, &r), Err(RigError::UnknownParent { .. })));
    assert_eq!(skin(&a, &rig(-1.0)), Err(RigError::Negative("blend")));
    let s = skin(&a, &rig(20.0)).unwrap();
    assert_eq!(skin(&s, &rig(20.0)), Err(RigError::AlreadySkinned));
    let bad = Pose {
        turns: vec![Turn {
            bone: "hand".into(),
            ..Default::default()
        }],
    };
    assert!(matches!(pose(&s, &bad), Err(RigError::NoSuchBone(..))));
    assert_eq!(pose(&a, &Pose::default()), Err(RigError::NotSkinned));
}

#[test]
fn operations_that_do_not_carry_skins_refuse_them() {
    let s = skin(&arm(), &rig(20.0)).unwrap();
    let plain = arm();
    assert_eq!(
        boolean(&s, &plain, &Boolean::default()),
        Err(shape_bool::BoolError::Skinned("target"))
    );
    assert_eq!(
        boolean(&plain, &s, &Boolean::default()),
        Err(shape_bool::BoolError::Skinned("tool"))
    );
    assert_eq!(
        shape_bool::fuse(&s),
        Err(shape_bool::BoolError::Skinned("target"))
    );
    assert_eq!(
        shape_edit::split(&s, &Frame::IDENTITY, shape_edit::Keep::default()),
        Err(shape_edit::EditError::Skinned)
    );
    assert_eq!(
        shape_edit::shell(
            &s,
            &shape_edit::Shell {
                thickness: 1.0,
                ..Default::default()
            }
        ),
        Err(shape_edit::EditError::Skinned)
    );
    assert_eq!(
        shape_bevel::fillet(
            &s,
            &shape_bevel::Fillet {
                radius: 1.0,
                edges: vec!["top".into()],
                ..Default::default()
            }
        ),
        Err(shape_bevel::BevelError::Skinned)
    );
    // Pieces carry their part of the skin.
    let pieces = shape_bool::components(&s);
    assert_eq!(pieces.len(), 1);
    assert_eq!(pieces[0], s);
}

#[test]
fn refining_a_band_keeps_uvs_on_their_surface() {
    // The arm with UVs as an affine function of position on each face:
    // every corner the refinement adds must carry that function's value
    // (a midpoint of its triangle's edge).
    let mut a = arm();
    let f = |p: DVec3| shape_core::DVec2::new(p.x + 0.5 * p.z, p.y - 0.25 * p.z);
    a.uvs = Some(
        a.triangles
            .iter()
            .map(|t| t.map(|v| f(a.positions[v as usize])))
            .collect(),
    );
    let s = skin(&a, &rig(20.0)).unwrap();
    assert!(s.triangles.len() > a.triangles.len());
    let uvs = s.uvs.as_ref().unwrap();
    for (t, uv) in s.triangles.iter().zip(uvs) {
        for k in 0..3 {
            assert!((uv[k] - f(s.positions[t[k] as usize])).length() < 1e-9);
        }
    }
}

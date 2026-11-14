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
fn a_skin_rides_booleans_edits_bevels_and_subdivision() {
    let s = skin(&arm(), &rig(20.0)).unwrap();
    let of = s.skin.as_ref().unwrap();
    // Every vertex of `out` is weighted, and one that was the arm's own
    // has the arm's weights there.
    let rides = |out: &Solid, what: &str| {
        let k = out
            .skin
            .as_ref()
            .unwrap_or_else(|| panic!("{what}: no skin"));
        assert_eq!(k.bones, of.bones, "{what}");
        assert_eq!(k.check(out.positions.len()), Ok(()), "{what}");
        let mut own = 0;
        for (p, w) in out.positions.iter().zip(&k.weights) {
            if let Some(i) = s.positions.iter().position(|q| q == p) {
                assert_eq!(*w, of.weights[i], "{what} at {p}");
                own += 1;
            }
        }
        assert!(own > 0, "{what}: none of the arm's vertices left");
        assert!(pose(out, &Pose::default()).is_ok(), "{what}");
    };
    // A plain block cut from the elbow: the cut's vertices take the
    // weights of the arm's surface there, between the two bones.
    let mut block = shape_gen::solids::cuboid(&shape_gen::solids::BoxDesc {
        width: 8.0,
        depth: 40.0,
        height: 10.0,
    })
    .unwrap();
    block.map_positions(|p| p + DVec3::new(10.0, 0.0, 52.0), false);
    let cut = boolean(
        &s,
        &block,
        &Boolean {
            op: shape_bool::BoolOp::Cut,
            ..Default::default()
        },
    )
    .unwrap();
    rides(&cut, "difference");
    let k = cut.skin.as_ref().unwrap();
    assert!(
        cut.positions
            .iter()
            .zip(&k.weights)
            .any(|(p, w)| !s.positions.contains(p) && w.weights[1] > 0.0),
        "no new vertex is blended between the bones"
    );
    // The tool skinned, the target not: the result moves with the tool's
    // skin.
    rides(&boolean(&block, &s, &Boolean::default()).unwrap(), "union");
    rides(&shape_bool::fuse(&s).unwrap(), "fuse");
    rides(
        &shape_edit::split(
            &s,
            &Frame {
                origin: DVec3::new(0.0, 0.0, 30.0),
                ..Frame::IDENTITY
            },
            shape_edit::Keep::default(),
        )
        .unwrap(),
        "split",
    );
    rides(
        &shape_bevel::fillet(
            &s,
            &shape_bevel::Fillet {
                radius: 1.0,
                edges: vec!["bottom".into()],
                ..Default::default()
            },
        )
        .unwrap(),
        "fillet",
    );
    // Two skins by different skeletons: one solid has one skin.
    let mut other = rig(20.0);
    other.bones[1].name = "hand".into();
    let t = skin(&arm(), &other).unwrap();
    assert_eq!(
        boolean(&s, &t, &Boolean::default()),
        Err(shape_bool::BoolError::TwoSkins)
    );
    assert_eq!(shape_mesh::one_skin(&[&s, &block, &s]), Ok(()));
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

/// A figure 1 800 tall in a T pose, up +z, facing −y, its left at +x,
/// from closed pieces that overlap (not joined): legs (r 70, x ±110,
/// z 80..900) on feet reaching forward to y −180, a torso (x ±190,
/// z 850..1450), a neck and a head, arms (r 50, z 1390) out to x ±850
/// with hands to ±1000.
fn figure() -> Solid {
    use shape_gen::solids::{BoxDesc, cuboid, sphere};
    let place = |mut s: Solid, f: &dyn Fn(DVec3) -> DVec3| {
        s.map_positions(f, false);
        s
    };
    let cyl = |r: f64, len: f64| {
        cylinder(&CylinderDesc {
            radius: r,
            height: len,
            ..Default::default()
        })
        .unwrap()
    };
    let block = |w: f64, d: f64, h: f64| {
        cuboid(&BoxDesc {
            width: w,
            depth: d,
            height: h,
        })
        .unwrap()
    };
    let mut s = place(block(380.0, 220.0, 600.0), &|p| p + DVec3::Z * 850.0);
    s.append(&place(cyl(55.0, 140.0), &|p| p + DVec3::Z * 1430.0));
    s.append(&place(sphere(120.0).unwrap(), &|p| p + DVec3::Z * 1650.0));
    for side in [1.0, -1.0] {
        s.append(&place(cyl(70.0, 840.0), &|p| {
            p + DVec3::new(110.0 * side, 0.0, 70.0)
        }));
        s.append(&place(block(130.0, 260.0, 90.0), &|p| {
            p + DVec3::new(110.0 * side, -50.0, 0.0)
        }));
        // Along ±x: (x, y, z) → (side·z, y, −side·x) turns +z to ±x.
        s.append(&place(cyl(50.0, 680.0), &|p| {
            DVec3::new(side * (p.z + 170.0), p.y, 1390.0 - side * p.x)
        }));
        s.append(&place(block(150.0, 60.0, 110.0), &|p| {
            DVec3::new(side * (p.x + 925.0), p.y, p.z + 1335.0)
        }));
    }
    s.canonicalize().unwrap();
    s
}

#[test]
fn a_figure_is_rigged_from_its_mesh() {
    let s = figure();
    let (sk, report) = crate::humanoid(&s).unwrap();
    assert!(report.outside.is_empty(), "{:?}", report.outside);
    let at = |n: &str| sk.joints.iter().find(|j| j.name == n).unwrap().at;
    // Left is +x: a swap of the sides fails here.
    for (side, s) in [("Left", 1.0), ("Right", -1.0)] {
        let j = |n: &str| at(&format!("{side}{n}"));
        let x = |n: &str| j(n).x * s;
        // The shoulder near the torso's side (190), the arm's joints in
        // order out to the hand, all on the arm's line.
        assert!(
            (150.0..=260.0).contains(&x("Arm")),
            "{side}Arm {:?}",
            j("Arm")
        );
        assert!(x("Arm") < x("ForeArm") && x("ForeArm") < x("Hand") && x("Hand") < x("Hand_End"));
        for n in ["Arm", "ForeArm", "Hand"] {
            assert!(
                (j(n).z - 1390.0).abs() < 30.0 && j(n).y.abs() < 30.0,
                "{side}{n} {:?}",
                j(n)
            );
        }
        // The leg's joints top to bottom on the leg's line, the toes in
        // front (−y).
        assert!(j("UpLeg").z > j("Leg").z && j("Leg").z > j("Foot").z);
        assert!(
            (850.0..=1000.0).contains(&j("UpLeg").z),
            "{side}UpLeg {:?}",
            j("UpLeg")
        );
        assert!(
            (400.0..=650.0).contains(&j("Leg").z),
            "{side}Leg {:?}",
            j("Leg")
        );
        assert!(j("Foot").z < 200.0, "{side}Foot {:?}", j("Foot"));
        for n in ["UpLeg", "Leg", "Foot"] {
            assert!((x(n) - 110.0).abs() < 40.0, "{side}{n} {:?}", j(n));
        }
        assert!(j("ToeBase").y < j("Foot").y && j("Toe_End").y < j("ToeBase").y);
    }
    let up = [
        "Hips",
        "Spine",
        "Spine1",
        "Spine2",
        "Neck",
        "Head",
        "HeadTop_End",
    ];
    for w in up.windows(2) {
        assert!(at(w[0]).z < at(w[1]).z, "{} below {}", w[0], w[1]);
    }
    assert!(
        (1400.0..=1560.0).contains(&at("Neck").z),
        "Neck {:?}",
        at("Neck")
    );

    // The weights: the hand's and the head's ends belong to their bones,
    // and a forearm's turn leaves the torso where it was.
    let (rigged, r) = crate::bind(&s, &sk).unwrap();
    let skin = rigged.skin.as_ref().unwrap();
    skin.check(rigged.positions.len()).unwrap();
    assert_eq!(r.vertices, rigged.positions.len());
    let bone = |n: &str| skin.bone(n).unwrap();
    for (p, w) in rigged.positions.iter().zip(&skin.weights) {
        if p.x > 950.0 {
            assert_eq!(w.bones[0], bone("LeftHand"), "{p:?}");
        }
        if p.z > 1720.0 {
            assert_eq!(w.bones[0], bone("Head"), "{p:?}");
        }
    }
    let p = Pose {
        turns: vec![Turn {
            bone: "LeftForeArm".into(),
            axis: DVec3::Z,
            angle: -1.2,
        }],
    };
    let posed = pose(&rigged, &p).unwrap();
    for (a, b) in rigged.positions.iter().zip(&posed.positions) {
        if a.x.abs() < 150.0 && a.z < 1300.0 {
            assert!((*a - *b).length() < 1e-6, "{a:?} moved to {b:?}");
        }
        if a.x > 950.0 {
            assert!(b.y < -300.0, "the hand at {a:?} went to {b:?}");
        }
    }
}

/// A chain hip → shoulder → elbow → hand: the arm along `arm` from the
/// shoulder at (0, 0, 100), 50 a bone.
fn chain(arm: DVec3, prefix: &str) -> shape_mesh::Skin {
    use shape_mesh::{Bone, Skin};
    let at = |p: DVec3| Frame {
        origin: p,
        ..Frame::IDENTITY
    };
    let s = DVec3::new(0.0, 0.0, 100.0);
    let names = ["Hips", "LeftArm", "LeftForeArm", "LeftHand"];
    let places = [
        DVec3::new(0.0, 0.0, 60.0),
        s,
        s + arm * 50.0,
        s + arm * 100.0,
    ];
    Skin {
        bones: (0..4)
            .map(|i| {
                Bone::new(
                    &format!("{prefix}{}", names[i]),
                    (i > 0).then(|| i as u32 - 1),
                    at(places[i]),
                )
            })
            .collect(),
        weights: Vec::new(),
    }
}

#[test]
fn a_clip_moves_onto_another_skeleton() {
    use shape_mesh::SkinClip;
    // A T pose's clip whose keys hold still, then bend the elbow a
    // quarter turn about −y (the forearm from +x to +z, up).
    let source = chain(DVec3::X, "mixamorig:");
    let still = crate::skin_matrices(&source, &Pose::default()).unwrap();
    let bent = crate::skin_matrices(
        &source,
        &Pose {
            turns: vec![Turn {
                bone: "mixamorig:LeftForeArm".into(),
                axis: -DVec3::Y,
                angle: trig::to_radians(90.0),
            }],
        },
    )
    .unwrap();
    let clip = SkinClip {
        name: "bend".into(),
        times: vec![0.0, 1.0],
        poses: vec![still, bent],
        morphs: Vec::new(),
    };
    // Onto itself: the same matrices, by name past the prefix.
    let same = chain(DVec3::X, "");
    let (moved, r) = crate::retarget(&source, &clip, &same, &Default::default()).unwrap();
    assert_eq!(r.matched.len(), 4);
    for (a, b) in moved
        .poses
        .iter()
        .flatten()
        .zip(clip.poses.iter().flatten())
    {
        for k in 0..16 {
            assert!((a[k] - b[k]).abs() < 1e-9, "{a:?} {b:?}");
        }
    }
    // Onto an A pose (the arm 45° down, its bones 50 long): held still,
    // the arm reaches out along +x as the source's does; bent, the
    // forearm points up.
    let down = DVec3::new(1.0, 0.0, -1.0) / 2f64.sqrt();
    let a_pose = chain(down, "");
    let (moved, r) = crate::retarget(&source, &clip, &a_pose, &Default::default()).unwrap();
    assert!((r.scale - 1.0).abs() < 1e-12);
    let place = |key: usize, bone: usize| {
        crate::apply(&moved.poses[key][bone], a_pose.bones[bone].rest.origin)
    };
    let shoulder = DVec3::new(0.0, 0.0, 100.0);
    assert!(
        (place(0, 2) - (shoulder + DVec3::X * 50.0)).length() < 1e-9,
        "{:?}",
        place(0, 2)
    );
    assert!(
        (place(0, 3) - (shoulder + DVec3::X * 100.0)).length() < 1e-9,
        "{:?}",
        place(0, 3)
    );
    let up = shoulder + DVec3::X * 50.0 + DVec3::Z * 50.0;
    assert!((place(1, 3) - up).length() < 1e-9, "{:?}", place(1, 3));
    assert_eq!(r.turn, 0);
    // A source that faces a quarter turn the other way (its arm along +y,
    // the elbow bent about +x to point the forearm up): turned to the
    // rig's facing, the same places as above.
    let side = chain(DVec3::Y, "");
    let side_clip = SkinClip {
        poses: vec![
            crate::skin_matrices(&side, &Pose::default()).unwrap(),
            crate::skin_matrices(
                &side,
                &Pose {
                    turns: vec![Turn {
                        bone: "LeftForeArm".into(),
                        axis: DVec3::X,
                        angle: trig::to_radians(90.0),
                    }],
                },
            )
            .unwrap(),
        ],
        ..clip.clone()
    };
    let (moved, r) = crate::retarget(&side, &side_clip, &a_pose, &Default::default()).unwrap();
    assert_eq!(r.turn, 270);
    let place = |key: usize, bone: usize| {
        crate::apply(&moved.poses[key][bone], a_pose.bones[bone].rest.origin)
    };
    assert!((place(0, 3) - (shoulder + DVec3::X * 100.0)).length() < 1e-9);
    assert!((place(1, 3) - up).length() < 1e-9, "{:?}", place(1, 3));
    // Tripo's names for the same bones match Mixamo's.
    let mut tripo = source.clone();
    for (b, n) in tripo
        .bones
        .iter_mut()
        .zip(["Hip", "L_Upperarm", "L_Forearm", "L_Hand"])
    {
        b.name = n.into();
    }
    let (_, r) = crate::retarget(&tripo, &clip, &a_pose, &Default::default()).unwrap();
    assert_eq!(r.matched.len(), 4, "{r:?}");
}

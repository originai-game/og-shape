//! Joints through copies and mirrors, reads off rest, the model's pose in
//! OBJ, STL and measures, and how joints hold (`docs/research.md` §20).

use shape_core::{DVec3, Frame, trig};
use shape_gen::solids::{BoxDesc, CylinderDesc, cuboid, cylinder};
use shape_model::{Id, JointDesc, JointKind, Mimic, Model, ModelError, Placement};

fn block(w: f64, d: f64, h: f64) -> shape_mesh::Solid {
    cuboid(&BoxDesc {
        width: w,
        depth: d,
        height: h,
    })
    .unwrap()
}

fn rod(r: f64, h: f64) -> shape_mesh::Solid {
    cylinder(&CylinderDesc {
        radius: r,
        height: h,
        ..Default::default()
    })
    .unwrap()
}

/// A cabinet off the origin with a door group (panel, handle and a flap)
/// on a hinge about −Z at its left front edge, the flap sliding on the
/// panel as the hinge's follower, and a drawer sliding out of its front.
fn cabinet() -> (Model, Id, Id, Id) {
    let mut m = Model::new("cabinet");
    let cab = m.add("cab", block(400.0, 300.0, 500.0)).unwrap();
    m.at(cab, &Frame::IDENTITY.translate(260.0, 0.0, 0.0))
        .unwrap();
    let front = m.face(cab, "-Y").unwrap();
    let panel = m.add("panel", block(380.0, 480.0, 10.0)).unwrap();
    m.at(panel, &front).unwrap();
    let handle = m.add("handle", rod(8.0, 30.0)).unwrap();
    m.at(handle, &front.translate(150.0, 0.0, 10.0)).unwrap();
    let flap = m.add("flap", block(40.0, 40.0, 4.0)).unwrap();
    let on = front.translate(0.0, 180.0, 10.0);
    m.at(flap, &on).unwrap();
    let door = m.group("door", &[panel, handle, flap]).unwrap();
    m.joint(
        door,
        cab,
        &JointDesc {
            name: "hinge".into(),
            on: front.translate(-190.0, 0.0, 0.0),
            axis: DVec3::NEG_Z,
            range: Some((0.0, trig::to_radians(110.0))),
            ..Default::default()
        },
    )
    .unwrap();
    m.joint(
        flap,
        panel,
        &JointDesc {
            name: "flap".into(),
            kind: JointKind::Prismatic,
            on,
            axis: DVec3::X,
            mimic: Some(Mimic {
                joint: "hinge".into(),
                multiplier: 20.0,
            }),
            ..Default::default()
        },
    )
    .unwrap();
    let drawer = m.add("drawer", block(200.0, 100.0, 80.0)).unwrap();
    let bay = front.translate(0.0, -150.0, -50.0);
    m.at(drawer, &bay).unwrap();
    m.joint(
        drawer,
        cab,
        &JointDesc {
            name: "slide".into(),
            kind: JointKind::Prismatic,
            on: bay,
            axis: DVec3::NEG_Y,
            range: Some((0.0, 250.0)),
            ..Default::default()
        },
    )
    .unwrap();
    (m, cab, door, drawer)
}

/// Every part's positions at the model's pose, by name.
fn posed(m: &Model) -> Vec<(String, Vec<DVec3>)> {
    let built = m.build().unwrap();
    let scene = built.scene();
    let solids = shape_tools::posed(&scene).unwrap();
    scene
        .parts
        .iter()
        .zip(solids)
        .map(|(p, s)| (p.name.clone(), s.positions))
        .collect()
}

fn of<'a>(parts: &'a [(String, Vec<DVec3>)], name: &str) -> &'a [DVec3] {
    &parts
        .iter()
        .find(|p| p.0 == name)
        .unwrap_or_else(|| panic!("no part {name}"))
        .1
}

/// Whether every point of `a` has one of `b` within 1e-9 (as sets: a
/// mirrored solid is canonicalized, so its order differs).
fn same_points(a: &[DVec3], b: &[DVec3]) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|p| b.iter().any(|q| (*p - *q).length() < 1e-9))
}

#[test]
fn a_copy_brings_its_joints_and_moves_like_the_original() {
    let (mut m, cab, door, _) = cabinet();
    let twin = m.copy(door, "twin").unwrap();
    assert_eq!(
        m.joint_names(),
        ["hinge", "flap", "slide", "hinge.001", "flap.001"]
    );
    // Moved aside, the twin's pivot moves with it; its follower follows
    // the twin's hinge, not the original's.
    m.transform(twin, &Frame::IDENTITY.translate(0.0, 0.0, 600.0))
        .unwrap();
    m.pose("hinge.001", 0.7).unwrap();
    let p = posed(&m);
    let lifted = |v: &[DVec3]| -> Vec<DVec3> {
        v.iter().map(|q| *q - DVec3::new(0.0, 0.0, 600.0)).collect()
    };
    let mut alone = m.clone();
    alone.pose("hinge.001", 0.0).unwrap();
    alone.pose("hinge", 0.7).unwrap();
    let q = posed(&alone);
    for (copy, original) in [("panel.001", "panel"), ("flap.001", "flap")] {
        assert!(
            same_points(&lifted(of(&p, copy)), of(&q, original)),
            "{copy}"
        );
    }
    // The original stayed at rest while only the twin was posed.
    assert!(m.face(cab, "top").is_ok());
}

#[test]
fn a_mirrored_part_moves_as_the_image_of_the_motion() {
    let (mut m, _, door, drawer) = cabinet();
    // The plane x = 500 (the cabinet's right side lies at x = 460).
    let plane = Frame::facing(DVec3::new(500.0, 0.0, 0.0), DVec3::X).unwrap();
    m.mirror(door, &plane, "door_r").unwrap();
    m.mirror(drawer, &plane, "drawer_r").unwrap();
    let image = |v: &[DVec3]| -> Vec<DVec3> {
        v.iter()
            .map(|q| DVec3::new(1000.0 - q.x, q.y, q.z))
            .collect()
    };
    for (hinge, slide) in [(0.0, 0.0), (1.1, 180.0), (trig::to_radians(110.0), 250.0)] {
        for (j, v) in [("hinge", hinge), ("slide", slide)] {
            m.pose(j, v).unwrap();
            // The same value poses the image: range, value and mimic kept.
            m.pose(&format!("{j}.001"), v).unwrap();
        }
        let p = posed(&m);
        for (copy, original) in [
            ("panel.001", "panel"),
            ("handle.001", "handle"),
            ("flap.001", "flap"),
            ("drawer_r", "drawer"),
        ] {
            assert!(
                same_points(of(&p, copy), &image(of(&p, original))),
                "{copy} at {hinge}, {slide}"
            );
        }
    }
}

#[test]
fn frames_are_not_read_off_a_posed_part() {
    let (mut m, cab, door, drawer) = cabinet();
    let panel = m.find("panel").unwrap();
    let flap = m.find("flap").unwrap();
    let at_rest = m.face(panel, "top").unwrap();
    m.pose("hinge", 0.5).unwrap();
    let posed = |r: Result<Frame, ModelError>| matches!(r, Err(ModelError::Posed { ref joint, .. }) if joint == "hinge");
    assert!(posed(m.face(panel, "top")));
    // The flap rides on the door, and slides as the hinge's follower.
    assert!(matches!(
        m.face(flap, "top"),
        Err(ModelError::Posed { ref joint, .. }) if joint == "flap"
    ));
    assert!(posed(m.bbox(&[door], "+X")));
    assert!(posed(m.placement(panel)));
    assert!(posed(m.local(panel, &Frame::IDENTITY)));
    assert!(matches!(
        m.edge(panel, "top"),
        Err(ModelError::Posed { .. })
    ));
    assert!(matches!(
        m.snap(&[panel], &[DVec3::ZERO], None),
        Err(ModelError::Posed { .. })
    ));
    let e = m.face(panel, "top").unwrap_err().to_string();
    assert!(e.contains("at rest") && e.contains("\"hinge\""), "{e}");
    // What the pose does not move reads as before.
    assert!(m.face(cab, "top").is_ok());
    assert!(m.face(drawer, "top").is_ok());
    // At rest again, the frame is the rest's.
    m.pose("hinge", 0.0).unwrap();
    assert_eq!(m.face(panel, "top").unwrap(), at_rest);
}

#[test]
fn obj_stl_and_measures_follow_the_pose() {
    let (mut m, ..) = cabinet();
    assert_eq!(m.mesh_placement(), Placement::Rest);
    let rest_obj = m.obj().unwrap();
    let rest = m.measure().unwrap();
    assert_eq!(m.measure_posed().unwrap(), rest);
    m.pose("slide", 100.0).unwrap();
    assert!(m.posed());
    let obj = m.obj().unwrap();
    assert!(obj.lines().next().unwrap().ends_with(", posed"));
    assert_ne!(obj, rest_obj);
    assert!(
        m.stl()
            .unwrap()
            .starts_with(b"cabinet - written by og-shape, metres, Z up, posed")
    );
    let posed = m.measure_posed().unwrap();
    for (a, b) in posed.iter().zip(&rest) {
        if a.name == "drawer" {
            // Out by 100 along −Y; the counts and the closure are the rest's.
            let d = b.measures.bounds_min - a.measures.bounds_min;
            assert!((d - DVec3::new(0.0, 100.0, 0.0)).length() < 1e-9, "{d}");
            assert_eq!(a.measures.triangles, b.measures.triangles);
            assert!((a.measures.volume - b.measures.volume).abs() < 1e-6);
        } else {
            assert_eq!(a, b, "{} moved", a.name);
        }
    }
}

#[test]
fn seating_measures_a_parent_moved_off_its_pivot() {
    let (mut m, cab, ..) = cabinet();
    let before = m.seating().unwrap();
    let hinge = &before[0];
    assert_eq!(
        (
            hinge.joint.as_str(),
            hinge.child.as_str(),
            hinge.parent.as_str()
        ),
        ("hinge", "door", "cab")
    );
    // The hinge's pivot lies on the cabinet's front edge and the panel.
    assert!(hinge.pivot_to_parent.unwrap() < 1e-9);
    assert!(hinge.pivot_to_child.unwrap() < 1e-9);
    assert!(
        before
            .iter()
            .all(|s| s.parent_moved == 0.0 && s.parent_turned == 0.0)
    );
    // The cabinet alone moved and turned after the joints were declared:
    // the door and the drawer stay where their frames put them.
    m.transform(
        cab,
        &Frame::IDENTITY
            .turn(0.0, 0.0, 0.25)
            .translate(30.0, 0.0, 40.0),
    )
    .unwrap();
    let after = m.seating().unwrap();
    for s in after.iter().filter(|s| s.parent == "cab") {
        assert!(s.parent_turned > 0.25 - 1e-9 && s.parent_turned < 0.25 + 1e-9);
        assert!(s.parent_moved > 1.0);
        assert!(s.pivot_to_parent.unwrap() > 1.0, "{s:?}");
    }
    // The flap hangs on the panel, which did not move.
    let flap = after.iter().find(|s| s.joint == "flap").unwrap();
    assert_eq!((flap.parent_moved, flap.parent_turned), (0.0, 0.0));
}

#[test]
fn shell_opens_along_the_parts_own_axes() {
    let mut m = Model::new("cup");
    let b = m.add("b", block(20.0, 20.0, 20.0)).unwrap();
    // Its own +Z turned onto the world's +X.
    m.at(b, &Frame::IDENTITY.turn(0.0, trig::PI / 2.0, 0.0))
        .unwrap();
    m.shell(
        b,
        &shape_edit::Shell {
            thickness: 2.0,
            open: vec!["+z".into()],
        },
    )
    .unwrap();
    // The box stands on its own z = 0, so it spans world x 0 to 20 (and
    // z −10 to 10). A ray
    // from far along +X enters through the opening and meets the floor's
    // inside at x = 2; opened along the world's +Z it would meet the
    // wall at x = 20.
    let hit = m
        .snap(&[b], &[DVec3::new(100.0, 0.0, 0.0)], Some(DVec3::NEG_X))
        .unwrap();
    assert!((hit[0].origin.x - 2.0).abs() < 1e-9, "{:?}", hit[0].origin);
}

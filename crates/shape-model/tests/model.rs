//! The model API end to end: placement, edits, joints and pose, read back
//! from the GLB with an independent reader (the `gltf` crate), and the
//! JSON program replayed to the same bytes.

use shape_core::{DVec3, Frame, trig};
use shape_gen::solids::{BoxDesc, CylinderDesc, cuboid, cylinder};
use shape_model::program::{Session, run};
use shape_model::{Id, JointDesc, JointKind, Material, Mimic, Model, ModelError, Unit};

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

/// A cabinet with a door (a group of two parts) on a hinge and a knob
/// that turns and slides (two joints on one child).
fn cabinet() -> (Model, Id, Id, Id) {
    let mut m = Model::new("cabinet");
    let cab = m.add("cab", block(400.0, 300.0, 500.0)).unwrap();
    let front = m.face(cab, "+Y").unwrap();
    let panel = m.add("panel", block(380.0, 480.0, 10.0)).unwrap();
    // The panel's height (its +Z) runs out of the front face.
    m.at(panel, &front.translate(0.0, 0.0, 0.0)).unwrap();
    let handle = m.add("handle", rod(8.0, 30.0)).unwrap();
    m.at(handle, &front.translate(150.0, 0.0, 10.0)).unwrap();
    let door = m.group("door", &[panel, handle]).unwrap();
    m.material(
        &[door],
        &Material {
            color: Some([0.2, 0.3, 0.4]),
            ..Default::default()
        },
    )
    .unwrap();
    let hinge = front.translate(-190.0, 0.0, 0.0);
    m.joint(
        door,
        cab,
        &JointDesc {
            name: "hinge".into(),
            kind: JointKind::Revolute,
            on: hinge,
            axis: DVec3::NEG_Z,
            range: Some((0.0, trig::to_radians(110.0))),
            ..Default::default()
        },
    )
    .unwrap();
    let top = m.face(cab, "top").unwrap();
    let knob = m.add("knob", rod(20.0, 15.0)).unwrap();
    m.at(knob, &top).unwrap();
    for (name, kind, range) in [
        ("lift", JointKind::Prismatic, Some((0.0, 40.0))),
        ("spin", JointKind::Continuous, None),
    ] {
        m.joint(
            knob,
            cab,
            &JointDesc {
                name: name.into(),
                kind,
                on: top,
                range,
                ..Default::default()
            },
        )
        .unwrap();
    }
    (m, cab, door, knob)
}

fn node<'a>(doc: &'a gltf::Gltf, name: &str) -> gltf::Node<'a> {
    doc.nodes()
        .find(|n| n.name() == Some(name))
        .unwrap_or_else(|| panic!("no node {name}"))
}

fn extras(n: &gltf::Node) -> serde_json::Value {
    let raw = n.extras().as_ref().expect("extras");
    serde_json::from_str(raw.get()).unwrap()
}

fn children(n: &gltf::Node) -> Vec<String> {
    n.children()
        .map(|c| c.name().unwrap_or("").to_string())
        .collect()
}

#[test]
fn glb_nests_parts_under_their_joints() {
    let (mut m, ..) = cabinet();
    m.pose("hinge", trig::to_radians(90.0)).unwrap();
    m.pose("lift", 25.0).unwrap();
    let bytes = m.glb().unwrap();
    let doc = gltf::Gltf::from_slice(&bytes).expect("a valid GLB");
    // The door group is a body with no mesh; its parts hang on it.
    let door = node(&doc, "door");
    assert!(door.mesh().is_none());
    assert_eq!(children(&door), ["panel", "handle"]);
    let hinge = node(&doc, "hinge");
    assert_eq!(children(&hinge), ["door"]);
    let e = extras(&hinge)["s3d_joint"].clone();
    assert_eq!(e["type"], "revolute");
    assert!((e["value"].as_f64().unwrap() - trig::to_radians(90.0)).abs() < 1e-12);
    // The pivot faces +Y (the front face): its X is world −X, its Y world
    // +Z. World −Z is (0, −1, 0) in the pivot, (0, 0, 1) written Y up.
    assert_eq!(e["axis"], serde_json::json!([0, 0, 1]));
    // Two joints on the knob: a link for the first, the knob under the
    // second, in declaration order (the first nearest the cabinet).
    let lift = node(&doc, "lift");
    assert_eq!(children(&lift), ["knob:lift"]);
    let link = node(&doc, "knob:lift");
    assert!(link.mesh().is_none());
    assert_eq!(children(&link), ["spin"]);
    assert_eq!(children(&node(&doc, "spin")), ["knob"]);
    assert_eq!(extras(&lift)["s3d_joint"]["value"], 0.025);
    assert_eq!(
        extras(&node(&doc, "spin"))["s3d_joint"]["type"],
        "continuous"
    );
    // The cabinet carries both joints' nodes.
    assert_eq!(children(&node(&doc, "cab")), ["hinge", "lift"]);
}

#[test]
fn joints_are_checked_and_poses_kept_in_range() {
    let (mut m, cab, door, knob) = cabinet();
    let bad = |m: &mut Model, d: JointDesc, child: Id, parent: Id| m.joint(child, parent, &d);
    let on = Frame::IDENTITY;
    // A revolute joint needs a range; a continuous one takes none.
    for (kind, range) in [
        (JointKind::Revolute, None),
        (JointKind::Continuous, Some((0.0, 1.0))),
        (JointKind::Prismatic, Some((1.0, 2.0))),
    ] {
        let d = JointDesc {
            name: "x".into(),
            kind,
            on,
            range,
            ..Default::default()
        };
        assert!(matches!(
            bad(&mut m, d, knob, cab),
            Err(ModelError::Joint(..))
        ));
    }
    // A cycle: the cabinet on the door.
    let d = JointDesc {
        name: "back".into(),
        kind: JointKind::Fixed,
        on,
        ..Default::default()
    };
    assert!(matches!(
        bad(&mut m, d, cab, door),
        Err(ModelError::Joint(..))
    ));
    // A second parent for the knob.
    let d = JointDesc {
        name: "other".into(),
        kind: JointKind::Fixed,
        on,
        ..Default::default()
    };
    assert!(matches!(
        bad(&mut m, d, knob, door),
        Err(ModelError::Joint(..))
    ));
    assert!(matches!(
        m.pose("hinge", 3.0),
        Err(ModelError::OutOfRange { .. })
    ));
    assert!(matches!(
        m.pose("nope", 0.0),
        Err(ModelError::NoSuchJoint(..))
    ));
    // A follower is posed through its leader only.
    let d = JointDesc {
        name: "twin".into(),
        kind: JointKind::Revolute,
        on,
        mimic: Some(Mimic {
            joint: "hinge".into(),
            multiplier: -1.0,
        }),
        ..Default::default()
    };
    let flap = m.add("flap", block(10.0, 10.0, 10.0)).unwrap();
    m.joint(flap, door, &d).unwrap();
    assert!(matches!(m.pose("twin", 0.1), Err(ModelError::Joint(..))));
    m.pose("hinge", 1.0).unwrap();
    let doc = gltf::Gltf::from_slice(&m.glb().unwrap()).unwrap();
    let e = extras(&node(&doc, "twin"))["s3d_joint"].clone();
    assert_eq!(e["value"], -1.0);
    assert_eq!(e["mimic"]["joint"], "hinge");
}

#[test]
fn moving_a_jointed_part_moves_its_pivot() {
    let (mut m, _, door, _) = cabinet();
    let before = gltf::Gltf::from_slice(&m.glb().unwrap())
        .unwrap()
        .nodes()
        .find(|n| n.name() == Some("hinge"))
        .map(|n| n.transform().decomposed().0)
        .unwrap();
    m.transform(door, &Frame::IDENTITY.translate(0.0, 0.0, 100.0))
        .unwrap();
    let doc = gltf::Gltf::from_slice(&m.glb().unwrap()).unwrap();
    let after = node(&doc, "hinge").transform().decomposed().0;
    // Up 100 mm in the kernel is +0.1 m on glTF's Y.
    assert!((after[1] - before[1] - 0.1).abs() < 1e-6);
}

#[test]
fn booleans_use_their_tool_up_and_frames_are_world() {
    let mut m = Model::new("plate");
    let plate = m.add("plate", block(80.0, 40.0, 6.0)).unwrap();
    m.at(plate, &Frame::IDENTITY.translate(100.0, 0.0, 0.0))
        .unwrap();
    let pin = m.add("pin", rod(3.0, 20.0)).unwrap();
    m.at(pin, &Frame::IDENTITY.translate(120.0, 0.0, -5.0))
        .unwrap();
    m.cut(plate, pin).unwrap();
    assert!(matches!(m.name_of(pin), Err(ModelError::UsedUp(_))));
    // The bore went where the pin was in the world.
    let s = m.world_solid(plate).unwrap();
    let centre = |m: &Model, id: Id| {
        let rim = m.edge(id, "cut").unwrap();
        assert!(rim.closed);
        rim.points.iter().sum::<DVec3>() / rim.points.len() as f64
    };
    assert!((centre(&m, plate).x - 120.0).abs() < 1e-9);
    assert!(s.measure().closed);
    // Mirror across x = 0: the image's bore is at x = −120.
    let image = m
        .mirror(
            plate,
            &Frame::facing(DVec3::ZERO, DVec3::X).unwrap(),
            "image",
        )
        .unwrap();
    assert!((centre(&m, image).x + 120.0).abs() < 1e-9);
    assert!(m.world_solid(image).unwrap().measure().volume > 0.0);
}

#[test]
fn units_scale_to_millimetres_for_export() {
    let mut m = Model::new("cm");
    m.units(Unit::Centimetre).unwrap();
    let b = m.add("b", block(10.0, 10.0, 10.0)).unwrap();
    assert!(matches!(m.units(Unit::Metre), Err(ModelError::Units(_))));
    let doc = gltf::Gltf::from_slice(&m.glb().unwrap()).unwrap();
    let acc = doc.accessors().next().unwrap();
    let max = acc.max().unwrap();
    // 10 cm wide: ±5 cm, so 0.05 m.
    assert!((max[0].as_f64().unwrap() - 0.05).abs() < 1e-7);
    let _ = b;
}

#[test]
fn a_recorded_program_replays_to_the_same_bytes() {
    let mut s = Session::new("replay");
    let ap = |s: &mut Session, op: &str| -> serde_json::Value {
        serde_json::from_str(&s.apply(op).unwrap()).unwrap()
    };
    let cab = ap(
        &mut s,
        r#"{"op":"box","name":"cab","width":100,"depth":80,"height":120}"#,
    );
    s.apply(r#"{"op":"fillet","id":0,"radius":4,"edges":"side"}"#)
        .unwrap();
    let front = ap(&mut s, r#"{"op":"face","id":0,"name":"+X"}"#);
    let porthole = ap(
        &mut s,
        &format!(r#"{{"op":"translate","frame":{front},"y":10}}"#),
    );
    s.apply(&format!(
        r#"{{"op":"pocket","id":0,"profile":{{"circle":{{"radius":20}}}},"depth":30,"at":[{porthole}]}}"#
    ))
    .unwrap();
    let lid = ap(
        &mut s,
        &format!(r#"{{"op":"cylinder","name":"lid","radius":24,"height":4,"on":{porthole}}}"#),
    );
    s.apply(&format!(
        r##"{{"op":"material","ids":[{lid}],"color":"#3a3d42","metalness":0.5}}"##
    ))
    .unwrap();
    s.apply(&format!(
        r#"{{"op":"joint","child":{lid},"parent":{cab},"name":"door","on":{porthole},"axis":"-Z","range":[0,1.5]}}"#
    ))
    .unwrap();
    s.apply(r#"{"op":"pose","joint":"door","value":1}"#)
        .unwrap();
    let program = s.program();
    // Readers are not recorded.
    assert!(!program.contains(r#""op":"face""#));
    let replayed = run(&program).unwrap();
    assert_eq!(replayed.glb().unwrap(), s.model().glb().unwrap());
    // Errors are values, with the operation and field named.
    let e = s.apply(r#"{"op":"box","name":"x","width":1}"#).unwrap_err();
    assert!(e.to_string().contains("depth"), "{e}");
    let e = s
        .apply(r#"{"op":"face","id":0,"name":"nope"}"#)
        .unwrap_err();
    assert!(e.to_string().contains("top"), "{e}");
}

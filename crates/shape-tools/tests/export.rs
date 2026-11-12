//! The exports read back with independent readers: the `gltf` crate for
//! GLB, and byte layouts for OBJ and STL.

use shape_core::{DVec3, Frame};
use shape_gen::solids::{BoxDesc, CylinderDesc, cuboid, cylinder};
use shape_mesh::Solid;
use shape_tools::{
    Clip, ClipKey, ExportError, Joint, JointKind, Material, Mimic, Part, Placement, Scene, glb,
    glb_clips, obj, obj_at, stl, stl_at,
};

fn solids() -> (Solid, Solid) {
    let base = cuboid(&BoxDesc {
        width: 100.0,
        depth: 60.0,
        height: 20.0,
    })
    .unwrap();
    let mut wheel = cylinder(&CylinderDesc {
        radius: 15.0,
        height: 10.0,
        ..Default::default()
    })
    .unwrap();
    wheel.map_positions(|p| p + DVec3::new(30.0, 0.0, 20.0), false);
    (base, wheel)
}

fn scene<'a>(base: &'a Solid, wheel: &'a Solid) -> Scene<'a> {
    let pivot = Frame::from_normal(DVec3::new(30.0, 0.0, 20.0), DVec3::Z, DVec3::X).unwrap();
    Scene {
        name: "cart".into(),
        parts: vec![
            Part {
                material: Material {
                    color: Some([0.2, 0.4, 0.8]),
                    roughness: Some(0.4),
                    ..Default::default()
                },
                ..Part::new("base", base)
            },
            Part {
                parent: "base".into(),
                material: Material {
                    name: "brass".into(),
                    metalness: 1.0,
                    opacity: Some(0.5),
                    ..Default::default()
                },
                joint: Some(Joint {
                    name: "spin".into(),
                    kind: JointKind::Revolute,
                    pivot,
                    range: Some((-1.0, 2.0)),
                    ..Default::default()
                }),
                ..Part::new("wheel", wheel)
            },
            Part {
                parent: "wheel".into(),
                joint: Some(Joint {
                    name: "slide".into(),
                    kind: JointKind::Prismatic,
                    pivot,
                    axis: DVec3::X,
                    mimic: Some(Mimic {
                        joint: "spin".into(),
                        multiplier: 10.0,
                    }),
                    ..Default::default()
                }),
                ..Part::new("nub", wheel)
            },
        ],
    }
}

#[test]
fn glb_is_valid_gltf_with_joint_extras() {
    let (base, wheel) = solids();
    let bytes = glb(&scene(&base, &wheel)).unwrap();
    let doc = gltf::Gltf::from_slice(&bytes).expect("a valid GLB");
    let blob = doc.blob.as_ref().expect("a BIN chunk");
    assert_eq!(doc.meshes().count(), 3);
    assert_eq!(doc.materials().count(), 3);
    let names: Vec<_> = doc.nodes().map(|n| n.name().unwrap().to_string()).collect();
    assert_eq!(names, ["base", "spin", "wheel", "slide", "nub"]);
    // The joint node carries s3d_joint in SI units and Y up.
    let spin = doc.nodes().nth(1).unwrap();
    let extras = spin.extras().as_ref().unwrap().get();
    assert_eq!(
        extras,
        r#"{"s3d_joint":{"name":"spin","type":"revolute","value":0,"axis":[0,1,0],"lower":-1,"upper":2}}"#
    );
    let (t, _, _) = spin.transform().decomposed();
    assert_eq!(t, [0.03, 0.02, 0.0]);
    let slide = doc.nodes().nth(3).unwrap();
    assert_eq!(
        slide.extras().as_ref().unwrap().get(),
        r#"{"s3d_joint":{"name":"slide","type":"prismatic","value":0,"axis":[1,0,0],"lower":-0.01,"upper":0.02,"mimic":{"joint":"spin","multiplier":0.01}}}"#
    );
    // Hierarchy: base → spin → wheel → slide → nub.
    let kids = |i: usize| -> Vec<usize> {
        doc.nodes()
            .nth(i)
            .unwrap()
            .children()
            .map(|c| c.index())
            .collect()
    };
    assert_eq!(
        (kids(0), kids(1), kids(2), kids(3)),
        (vec![1], vec![2], vec![3], vec![4])
    );
    // The base's vertices read back in metres, Y up: 0.1 × 0.02 × 0.06.
    let prim = doc.meshes().next().unwrap().primitives().next().unwrap();
    let reader = prim.reader(|_| Some(blob.as_slice()));
    let pos: Vec<[f32; 3]> = reader.read_positions().unwrap().collect();
    let normals: Vec<[f32; 3]> = reader.read_normals().unwrap().collect();
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for p in &pos {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    assert_eq!(lo, [-0.05, 0.0, -0.03]);
    assert_eq!(hi, [0.05, 0.02, 0.03]);
    // A box's six faces each have their own four corners, normals flat.
    assert_eq!(pos.len(), 24);
    for n in &normals {
        assert_eq!(n.iter().map(|c| c.abs()).sum::<f32>(), 1.0);
    }
    let idx: Vec<u32> = reader.read_indices().unwrap().into_u32().collect();
    assert_eq!(idx.len(), base.triangles.len() * 3);
    // Materials by three.js names.
    let m = doc.materials().nth(1).unwrap();
    assert_eq!(m.name(), Some("brass"));
    assert_eq!(m.pbr_metallic_roughness().metallic_factor(), 1.0);
    assert_eq!(m.alpha_mode(), gltf::material::AlphaMode::Blend);
    // The same scene gives the same bytes.
    assert_eq!(bytes, glb(&scene(&base, &wheel)).unwrap());
}

#[test]
fn smooth_normals_stay_within_a_surface() {
    let (_, wheel) = solids();
    let bytes = glb(&Scene {
        parts: vec![Part::new("wheel", &wheel)],
        ..Default::default()
    })
    .unwrap();
    let doc = gltf::Gltf::from_slice(&bytes).unwrap();
    let blob = doc.blob.as_ref().unwrap();
    let prim = doc.meshes().next().unwrap().primitives().next().unwrap();
    let reader = prim.reader(|_| Some(blob.as_slice()));
    let pos: Vec<[f32; 3]> = reader.read_positions().unwrap().collect();
    let normals: Vec<[f32; 3]> = reader.read_normals().unwrap().collect();
    // Side vertices point out radially (y = 0 in glTF space); cap
    // vertices point straight up or down; a rim position has one of each.
    let rims = wheel.positions.len();
    assert_eq!(pos.len(), rims * 2);
    for n in &normals {
        let flat = n[0] == 0.0 && n[2] == 0.0 && n[1].abs() == 1.0;
        let radial = n[1].abs() < 1e-6;
        assert!(flat || radial, "{n:?}");
    }
}

#[test]
fn errors_are_values() {
    let (base, wheel) = solids();
    let mut s = scene(&base, &wheel);
    s.parts[1].parent = "nobody".into();
    assert!(matches!(
        glb(&s),
        Err(shape_tools::ExportError::UnknownParent { .. })
    ));
    let mut s = scene(&base, &wheel);
    s.parts[1].joint.as_mut().unwrap().range = Some((0.5, 1.0));
    assert!(matches!(
        glb(&s),
        Err(shape_tools::ExportError::BadRange(_))
    ));
    let mut s = scene(&base, &wheel);
    s.parts[2].name = "base".into();
    assert!(matches!(
        glb(&s),
        Err(shape_tools::ExportError::DuplicatePart(_))
    ));
}

#[test]
fn obj_and_stl_layouts() {
    let (base, wheel) = solids();
    let s = scene(&base, &wheel);
    let text = obj(&s).unwrap();
    let verts = text.lines().filter(|l| l.starts_with("v ")).count();
    let faces = text.lines().filter(|l| l.starts_with("f ")).count();
    assert_eq!(verts, base.positions.len() + 2 * wheel.positions.len());
    assert_eq!(faces, base.triangles.len() + 2 * wheel.triangles.len());
    assert!(text.contains("\nv -0.05 0.02 0.03\n"));
    let bin = stl(&s).unwrap();
    let count = u32::from_le_bytes(bin[80..84].try_into().unwrap()) as usize;
    assert_eq!(count, faces);
    assert_eq!(bin.len(), 84 + 50 * count);
}

#[test]
fn obj_and_stl_at_the_pose_move_only_what_the_joints_move() {
    let (base, wheel) = solids();
    let mut s = scene(&base, &wheel);
    // At rest, Pose and Rest write the same vertices; only the header says.
    let rest = obj(&s).unwrap();
    let still = obj_at(&s, Placement::Pose).unwrap();
    assert!(still.starts_with("# cart - written by og-shape, metres, Y up, posed\n"));
    assert_eq!(
        rest.split_once('\n').unwrap().1,
        still.split_once('\n').unwrap().1
    );
    // A quarter turn about the wheel's pivot (30, 0, 20), +Z: the wheel's
    // point at (45, 0, 25) goes to (30, 15, 25); the nub follows by a
    // slide of π/2 × 10 mm along X. The base does not move.
    s.parts[1].joint.as_mut().unwrap().value = core::f64::consts::FRAC_PI_2;
    let posed = obj_at(&s, Placement::Pose).unwrap();
    let blocks: Vec<&str> = posed.split("\no ").collect();
    let rest_blocks: Vec<&str> = rest.split("\no ").collect();
    assert_eq!(blocks[1], rest_blocks[1], "the base moved");
    assert_ne!(blocks[2], rest_blocks[2], "the wheel stayed at rest");
    let vs = |b: &str| -> Vec<[f32; 3]> {
        b.lines()
            .filter_map(|l| l.strip_prefix("v "))
            .map(|l| {
                let v: Vec<f32> = l.split(' ').map(|x| x.parse().unwrap()).collect();
                [v[0], v[1], v[2]]
            })
            .collect()
    };
    // OBJ is Y up: kernel (x, y, z) is (x, z, −y). The quarter turn
    // about kernel +Z through (30, 0, 20) mm takes OBJ (X, Y, Z) to
    // (0.03 + Z, Y, 0.03 − X); the nub first slides by s along kernel X
    // (OBJ X), then turns with the wheel.
    let slide = core::f64::consts::FRAC_PI_2 * 10.0 * 0.001;
    for (b, s) in [(2, 0.0), (3, slide)] {
        let (before, after) = (vs(rest_blocks[b]), vs(blocks[b]));
        assert_eq!(before.len(), after.len());
        for (p, q) in before.iter().zip(&after) {
            let (x, y, z) = (f64::from(p[0]) + s, f64::from(p[1]), f64::from(p[2]));
            let want = [0.03 + z, y, 0.03 - x];
            for k in 0..3 {
                assert!((f64::from(q[k]) - want[k]).abs() < 1e-6, "{p:?} → {q:?}");
            }
        }
    }
    let bin = stl_at(&s, Placement::Pose).unwrap();
    assert!(bin.starts_with(b"cart - written by og-shape, metres, Z up, posed"));
    assert_ne!(bin[84..], stl(&s).unwrap()[84..]);
}

#[test]
fn a_posed_joint_turns_its_node_and_its_followers() {
    let (base, wheel) = solids();
    let mut s = scene(&base, &wheel);
    s.parts[1].joint.as_mut().unwrap().value = core::f64::consts::FRAC_PI_2;
    let bytes = glb(&s).unwrap();
    let doc = gltf::Gltf::from_slice(&bytes).unwrap();
    // The pivot looks +Z (glTF +Y): a quarter turn about glTF's Y.
    let (t, r, _) = doc.nodes().nth(1).unwrap().transform().decomposed();
    assert_eq!(t, [0.03, 0.02, 0.0]);
    let h = core::f32::consts::FRAC_1_SQRT_2;
    assert!((r[1] - h).abs() < 1e-7 && (r[3] - h).abs() < 1e-7 && r[0] == 0.0 && r[2] == 0.0);
    // The part under it is unchanged: the pivot's inverse.
    let (t, r, _) = doc.nodes().nth(2).unwrap().transform().decomposed();
    assert_eq!((t, r), ([-0.03, -0.02, 0.0], [0.0, 0.0, 0.0, 1.0]));
    // The follower slides 10 mm per radian: π/2 × 10 mm, in metres.
    let extras = doc
        .nodes()
        .nth(3)
        .unwrap()
        .extras()
        .as_ref()
        .unwrap()
        .get()
        .to_string();
    assert!(extras.contains(&format!(
        "\"value\":{}",
        core::f64::consts::FRAC_PI_2 * 10.0 * 0.001
    )));
}

/// A clip's keys as `(time, [(joint, value)])`.
fn clip(name: &str, keys: &[(f64, &[(&str, f64)])]) -> Clip {
    Clip {
        name: name.into(),
        keys: keys
            .iter()
            .map(|(time, pose)| ClipKey {
                time: *time,
                pose: pose.iter().map(|(j, v)| (String::from(*j), *v)).collect(),
            })
            .collect(),
    }
}

#[test]
fn a_clip_is_an_animation_that_ends_where_the_pose_would() {
    let (base, wheel) = solids();
    let s = scene(&base, &wheel);
    // The wheel turns from −1 to 2 radians: more than a quarter, so keys
    // are written between; the nub follows, sliding 10 mm a radian.
    let clips = [clip(
        "turn",
        &[(0.0, &[("spin", -1.0)]), (1.5, &[("spin", 2.0)])],
    )];
    let bytes = glb_clips(&s, &clips).unwrap();
    let doc = gltf::Gltf::from_slice(&bytes).unwrap();
    let blob = doc.blob.clone().unwrap();
    let anim = doc.animations().next().unwrap();
    assert_eq!(anim.name(), Some("turn"));
    let paths: Vec<(usize, gltf::animation::Property)> = anim
        .channels()
        .map(|c| (c.target().node().index(), c.target().property()))
        .collect();
    // In joint-name order: the follower's node (3) slides, the wheel's (1) turns.
    assert_eq!(
        paths,
        [
            (3, gltf::animation::Property::Translation),
            (1, gltf::animation::Property::Rotation)
        ]
    );
    let turn = anim.channels().nth(1).unwrap();
    let reader = turn.reader(|_| Some(blob.as_slice()));
    let times: Vec<f32> = reader.read_inputs().unwrap().collect();
    assert_eq!(times, [0.0, 0.75, 1.5]);
    let gltf::animation::util::ReadOutputs::Rotations(r) = reader.read_outputs().unwrap() else {
        panic!("rotations");
    };
    let quats: Vec<[f32; 4]> = r.into_f32().collect();
    // Each key is within a quarter turn of the one before, the short way.
    for w in quats.windows(2) {
        let dot: f32 = (0..4).map(|k| w[0][k] * w[1][k]).sum();
        assert!(dot > core::f32::consts::FRAC_1_SQRT_2, "{quats:?}");
    }
    // The last key is the node of the scene posed at 2 radians, and the
    // follower's last translation its node there.
    let mut posed = scene(&base, &wheel);
    posed.parts[1].joint.as_mut().unwrap().value = 2.0;
    let at = gltf::Gltf::from_slice(&glb(&posed).unwrap()).unwrap();
    let (_, r, _) = at.nodes().nth(1).unwrap().transform().decomposed();
    let last = quats[quats.len() - 1];
    let same: f32 = (0..4).map(|k| last[k] * r[k]).sum();
    assert!(same.abs() > 1.0 - 1e-6, "{last:?} {r:?}");
    let slide = anim.channels().next().unwrap();
    let reader = slide.reader(|_| Some(blob.as_slice()));
    let gltf::animation::util::ReadOutputs::Translations(t) = reader.read_outputs().unwrap() else {
        panic!("translations");
    };
    let (want, _, _) = at.nodes().nth(3).unwrap().transform().decomposed();
    assert_eq!(t.last().unwrap(), want);
    // Without clips the bytes are glb's.
    assert_eq!(glb_clips(&s, &[]).unwrap(), glb(&s).unwrap());
}

#[test]
fn a_clip_that_cannot_be_written_says_why() {
    let (base, wheel) = solids();
    let s = scene(&base, &wheel);
    let why = |c: Clip| match glb_clips(&s, &[c]) {
        Err(ExportError::BadClip { why, .. }) => why,
        r => panic!("{:?}", r.map(|b| b.len())),
    };
    assert!(why(clip("a", &[(0.0, &[("door", 1.0)])])).contains("no joint \"door\""));
    assert!(why(clip("a", &[(0.0, &[("slide", 1.0)])])).contains("follows"));
    assert!(why(clip("a", &[(0.0, &[("spin", 3.0)])])).contains("outside its range"));
    assert!(
        why(clip(
            "a",
            &[(1.0, &[("spin", 0.0)]), (1.0, &[("spin", 1.0)])]
        ))
        .contains("rise")
    );
    assert!(why(clip("a", &[])).contains("no key"));
    assert!(why(clip("", &[(0.0, &[("spin", 0.0)])])).contains("no name"));
}

/// A URDF's links, put back where its joints' origins say, are the
/// parts where they were built; its joints carry the kind, axis, limits
/// (SI) and mimic of the scene's.
#[test]
fn a_urdf_s_links_stand_where_the_parts_were_built() {
    use shape_core::trig;
    let (base, wheel) = solids();
    let mut s = scene(&base, &wheel);
    // A pivot tilted every way, and a part welded to the wheel.
    let tilted = Frame::from_normal(
        DVec3::new(30.0, 0.0, 20.0),
        DVec3::new(1.0, 2.0, 3.0).normalize(),
        DVec3::new(3.0, -1.0, 0.5),
    )
    .unwrap();
    s.parts[1].joint.as_mut().unwrap().pivot = tilted;
    s.parts.push(Part {
        parent: "wheel".into(),
        ..Part::new("cap", &base)
    });
    let u = shape_tools::urdf(&s, "cart").unwrap();
    let names: Vec<&str> = u.meshes.iter().map(|m| m.0.as_str()).collect();
    assert_eq!(
        names,
        [
            "cart_base.stl",
            "cart_wheel.stl",
            "cart_nub.stl",
            "cart_cap.stl"
        ]
    );
    let attr = |text: &str, key: &str| -> String {
        let at = text.find(&format!("{key}=\"")).unwrap() + key.len() + 2;
        text[at..at + text[at..].find('"').unwrap()].to_string()
    };
    let nums = |t: &str| -> Vec<f64> { t.split(' ').map(|v| v.parse().unwrap()).collect() };
    // Each link's frame in the world, from the joints' origins.
    let mut frames: Vec<(String, [DVec3; 4])> =
        vec![("base".into(), [DVec3::X, DVec3::Y, DVec3::Z, DVec3::ZERO])];
    for joint in u.xml.split("<joint ").skip(1) {
        let parent = attr(&joint[joint.find("<parent").unwrap()..], "link");
        let child = attr(&joint[joint.find("<child").unwrap()..], "link");
        let origin = &joint[joint.find("<origin").unwrap()..];
        let (t, r) = (nums(&attr(origin, "xyz")), nums(&attr(origin, "rpy")));
        let ((sr, cr), (sp, cp), (sy, cy)) = (
            trig::sin_cos(r[0]),
            trig::sin_cos(r[1]),
            trig::sin_cos(r[2]),
        );
        // Rz(yaw) Ry(pitch) Rx(roll), by columns.
        let local = [
            DVec3::new(cy * cp, sy * cp, -sp),
            DVec3::new(cy * sp * sr - sy * cr, sy * sp * sr + cy * cr, cp * sr),
            DVec3::new(cy * sp * cr + sy * sr, sy * sp * cr - cy * sr, cp * cr),
            DVec3::new(t[0], t[1], t[2]),
        ];
        let [px, py, pz, pt] = frames.iter().find(|f| f.0 == parent).unwrap().1;
        let turn = |v: DVec3| px * v.x + py * v.y + pz * v.z;
        frames.push((
            child,
            [
                turn(local[0]),
                turn(local[1]),
                turn(local[2]),
                turn(local[3]) + pt,
            ],
        ));
    }
    assert_eq!(frames.len(), 4);
    for (part, (_, bytes)) in s.parts.iter().zip(&u.meshes) {
        let [x, y, z, t] = frames.iter().find(|f| f.0 == part.name).unwrap().1;
        assert_eq!(bytes.len(), 84 + 50 * part.solid.triangles.len());
        for (k, tri) in part.solid.triangles.iter().enumerate() {
            for (j, &v) in tri.iter().enumerate() {
                let at = 84 + 50 * k + 12 + 12 * j;
                let f = |i: usize| {
                    f64::from(f32::from_le_bytes(
                        bytes[at + 4 * i..][..4].try_into().unwrap(),
                    ))
                };
                let world = x * f(0) + y * f(1) + z * f(2) + t;
                let want = part.solid.positions[v as usize] * 0.001;
                assert!(
                    (world - want).length() < 1e-6,
                    "{} {world} {want}",
                    part.name
                );
            }
        }
    }
    let joint = |name: &str| {
        let at = u.xml.find(&format!("<joint name=\"{name}\"")).unwrap();
        &u.xml[at..at + u.xml[at..].find("</joint>").unwrap()]
    };
    let spin = joint("spin");
    assert!(spin.contains("type=\"revolute\"") && spin.contains("<axis xyz=\"0 0 1\"/>"));
    assert!(spin.contains("lower=\"-1\" upper=\"2\""));
    // 10 mm a radian is 0.01 m a radian.
    let slide = joint("slide");
    assert!(slide.contains("type=\"prismatic\"") && slide.contains("<axis xyz=\"1 0 0\"/>"));
    assert!(slide.contains("<mimic joint=\"spin\" multiplier=\"0.01\"/>"));
    assert!(slide.contains("lower=\"-0.01\" upper=\"0.02\""));
    assert!(joint("cap_weld").contains("type=\"fixed\""));
}

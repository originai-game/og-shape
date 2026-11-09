//! The exports read back with independent readers: the `gltf` crate for
//! GLB, and byte layouts for OBJ and STL.

use shape_core::{DVec3, Frame};
use shape_gen::solids::{BoxDesc, CylinderDesc, cuboid, cylinder};
use shape_mesh::Solid;
use shape_tools::{Joint, JointKind, Material, Mimic, Part, Scene, glb, obj, stl};

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

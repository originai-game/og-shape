//! Mesh tools end to end: files written by the exporters read back by the
//! importers, external buffers, and the platform chain (import → repair →
//! decimate → unwrap → measure → preview → export) on a broken mesh.

use shape_core::{DVec2, DVec3, Frame};
use shape_gen::solids::{BoxDesc, cuboid, sphere};
use shape_mesh::Solid;
use shape_tools::{
    Decimate, Floaters, Import, Joint, JointKind, Measure, Part, PlainGlb, Preview, Repair, Scene,
    Unwrap, decimate, glb, glb_plain, import, import_gltf, import_obj, join, measure, no_files,
    obj, preview, repair, unwrap,
};

fn scene_parts() -> (Solid, Solid) {
    let body = cuboid(&BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    })
    .unwrap();
    let mut ball = sphere(6.0).unwrap();
    ball.map_positions(|p| p + DVec3::new(0.0, 0.0, 30.0), false);
    (body, ball)
}

/// Both solids have the same positions, to within f32 rounding in metres
/// (a micrometre here).
fn same_points(a: &Solid, b: &Solid) {
    let near = |p: DVec3, s: &Solid| s.positions.iter().any(|q| (*q - p).length() < 1e-3);
    assert_eq!(a.positions.len(), b.positions.len());
    assert!(a.positions.iter().all(|&p| near(p, b)));
    assert!(b.positions.iter().all(|&p| near(p, a)));
}

#[test]
fn glb_round_trip_keeps_parts_transforms_and_names() {
    let (body, ball) = scene_parts();
    let mut lid = Part::new("ball", &ball);
    lid.parent = "body".into();
    lid.joint = Some(Joint {
        name: "hinge".into(),
        kind: JointKind::Revolute,
        pivot: Frame {
            origin: DVec3::new(0.0, 0.0, 20.0),
            ..Frame::IDENTITY
        },
        axis: DVec3::X,
        range: Some((-1.0, 1.0)),
        ..Default::default()
    });
    let scene = Scene {
        name: "model".into(),
        parts: vec![Part::new("body", &body), lid],
    };
    let bytes = glb(&scene).unwrap();
    let imported = import(&bytes, &Import::default(), &no_files).unwrap();
    let names: Vec<&str> = imported.parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["body", "ball"]);
    // The ball's node hangs under the joint's: its transform undoes the
    // pivot, so placed it lands where it was built.
    let joined = join(&imported.parts).unwrap();
    let mut both = body.clone();
    both.append(&ball);
    both.canonicalize().unwrap();
    same_points(&joined, &both);
    assert_eq!(joined.triangles.len(), both.triangles.len());
    assert_eq!(joined.faces(&["ball"]).unwrap().len(), ball.triangles.len());
    let m = joined.measure();
    assert_eq!(m.components, 2);
    assert!(m.closed);
}

#[test]
fn obj_round_trip_and_polygons() {
    let (body, _) = scene_parts();
    let scene = Scene {
        name: "box".into(),
        parts: vec![Part::new("body", &body)],
    };
    let text = obj(&scene).unwrap();
    let imported = import_obj(text.as_bytes(), &Import::default(), &no_files).unwrap();
    let s = join(&imported.parts).unwrap();
    same_points(&s, &body);
    assert!(s.measure().closed);
    // Quads and an n-gon, Z up, in centimetres, with vt and negative
    // indices, and a material library that cannot be read.
    let quad = "mtllib missing.mtl\no plate\nv 0 0 0\nv 2 0 0\nv 2 1 0\nv 1 1 0\nv 1 2 0\nv 0 2 0\n\
                vt 0 0\nvt 1 0\nvt 1 0.5\nvt 0.5 0.5\nvt 0.5 1\nvt 0 1\n\
                f -6/-6 -5/-5 -4/-4 -3/-3 -2/-2 -1/-1\nl 1 2\n";
    let d = Import {
        unit_mm: 10.0,
        z_up: true,
    };
    let imported = import_obj(quad.as_bytes(), &d, &no_files).unwrap();
    assert_eq!(imported.polygons_triangulated, 1);
    assert_eq!(imported.skipped.len(), 2);
    let p = &imported.parts[0];
    assert_eq!(p.name, "plate");
    assert_eq!(p.solid.triangles.len(), 4);
    let m = p.solid.measure();
    assert!((m.area - 300.0).abs() < 1e-9);
    // UVs follow their corners: u = x / 20 mm.
    let uvs = p.uvs.as_ref().unwrap();
    for (t, uv) in p.solid.triangles.iter().zip(uvs) {
        for k in 0..3 {
            let pos = p.solid.positions[t[k] as usize];
            assert_eq!(uv[k], DVec2::new(pos.x / 20.0, pos.y / 20.0));
        }
    }
}

#[test]
fn gltf_with_external_and_embedded_buffers() {
    // One triangle; positions in an external file, indices in a data URI.
    let mut pos = Vec::new();
    for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        pos.extend_from_slice(&v.to_le_bytes());
    }
    let json = r#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],
      "nodes":[{"name":"tri","mesh":0,"translation":[0,0,1],"scale":[2,2,2]}],
      "meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],
      "buffers":[{"uri":"tri%20pos.bin","byteLength":36},{"uri":"data:application/octet-stream;base64,AAABAAIAAAA=","byteLength":8}],
      "bufferViews":[{"buffer":0,"byteLength":36},{"buffer":1,"byteLength":6}],
      "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},
                   {"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],
      "images":[{"uri":"albedo.png"}]}"#;
    let files = |uri: &str| match uri {
        "tri pos.bin" => Ok(pos.clone()),
        _ => Err(format!("no {uri}")),
    };
    let imported = import_gltf(json.as_bytes(), &files).unwrap();
    let s = join(&imported.parts).unwrap();
    // glTF (x, y, z) m is kernel (x, −z, y) mm; scaled 2, moved 1 m up y.
    let mut pts = s.positions.clone();
    pts.sort_by(|a, b| shape_core::order::cmp_dvec3(*a, *b));
    assert_eq!(
        pts,
        [
            DVec3::new(0.0, -1000.0, 0.0),
            DVec3::new(0.0, -1000.0, 2000.0),
            DVec3::new(2000.0, -1000.0, 0.0),
        ]
    );
    assert_eq!(imported.images.len(), 1);
    assert!(imported.images[0].error.is_some());
    // A missing external buffer is an error value naming it.
    let err = import_gltf(json.as_bytes(), &no_files).unwrap_err();
    assert!(err.to_string().contains("tri pos.bin"), "{err}");
    // So is a required extension the importer does not read.
    let draco = json.replace(
        r#""asset":{"version":"2.0"}"#,
        r#""asset":{"version":"2.0"},"extensionsRequired":["KHR_draco_mesh_compression"]"#,
    );
    assert!(import_gltf(draco.as_bytes(), &files).is_err());
}

/// A sphere with the defects providers' meshes have: split at seams, a
/// hole, flipped triangles, a floater, a sliver.
fn broken() -> Solid {
    let s = sphere(25.0).unwrap();
    let mut b = Solid::default();
    for (i, t) in s.triangles.iter().enumerate() {
        // Triangle soup: every triangle its own corners (as a GLB split
        // at UV seams), each nudged by up to 1 µm.
        let wobble = |v: u32| {
            let k = f64::from(v % 7) * 1.0e-4;
            s.positions[v as usize] + DVec3::splat(k)
        };
        let ids = t.map(|v| b.push_vertex(wobble(v)));
        if i == 40 {
            continue; // a hole
        }
        let tri = if i % 17 == 0 {
            [ids[0], ids[2], ids[1]] // flipped
        } else {
            ids
        };
        b.push_triangle(tri, 0, &["scan"]);
    }
    let mut speck = sphere(0.5).unwrap();
    speck.map_positions(|p| p + DVec3::new(40.0, 0.0, 0.0), false);
    b.append(&speck);
    b
}

#[test]
fn the_platform_chain_meets_the_texturing_contract() {
    let raw = broken();
    let before = measure(&raw, &Measure::default());
    assert!(!before.closed && before.pieces.len() > 1);
    let (fixed, report) = repair(
        &raw,
        &Repair {
            weld: 0.01,
            floaters: Floaters {
                fraction: 0.1,
                ..Default::default()
            },
            max_hole_edges: 16,
            check_intersections: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!((report.floaters, report.holes_filled), (1, 1), "{report:?}");
    assert!(report.flipped > 0);
    let (low, d) = decimate(
        &fixed,
        &Decimate {
            target: 300,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(low.triangles.len() <= 300, "{d:?}");
    let uv = unwrap(&low, &Unwrap::default()).unwrap();
    let m = measure(
        &low,
        &Measure {
            uvs: Some(&uv.corners),
            intersections: true,
            ..Default::default()
        },
    );
    assert!(m.closed && m.pieces.len() == 1 && m.degenerate_triangles == 0);
    let u = m.uv.unwrap();
    assert_eq!((u.overlapping_pairs, u.flipped, u.outside), (0, 0, 0));
    let views = preview(
        &Scene {
            name: "chain".into(),
            parts: vec![Part::new("chain", &low)],
        },
        &Preview {
            width: 64,
            views: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(views.len(), 1);
    let bytes = glb_plain(
        &low,
        &PlainGlb {
            name: "chain",
            uvs: Some(&uv),
        },
    )
    .unwrap();
    // The `gltf` crate reads it: one node, one mesh, one primitive with
    // POSITION, NORMAL and TEXCOORD_0.
    let g = gltf::Gltf::from_slice(&bytes).unwrap();
    assert_eq!(g.nodes().count(), 1);
    let prim = g.meshes().next().unwrap().primitives().next().unwrap();
    assert_eq!(prim.attributes().count(), 3);
    // And our importer reads it back as the same closed mesh.
    let back = import(&bytes, &Import::default(), &no_files).unwrap();
    assert_eq!(back.parts.len(), 1);
    let again = measure(
        &back.parts[0].solid,
        &Measure {
            uvs: back.parts[0].uvs.as_deref(),
            ..Default::default()
        },
    );
    assert!(again.closed);
    assert_eq!(again.triangles, low.triangles.len());
    assert_eq!(again.uv.unwrap().overlapping_pairs, 0);
}

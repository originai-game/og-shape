//! Mesh tools end to end: files written by the exporters read back by the
//! importers, external buffers, and the whole chain (import → repair →
//! decimate → unwrap → measure → preview → export) on a broken mesh.

#[path = "support/hausdorff.rs"]
mod hausdorff;
#[path = "support/meshopt.rs"]
mod meshopt;
#[path = "support/skinned.rs"]
mod skinned;
#[path = "support/fbx.rs"]
mod support;

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
    let quad = "mtllib missing.mtl\no plate\nv 0 0 0 1 0.5 0\nv 2 0 0\nv 2 1 0\nv 1 1 0\nv 1 2 0\nv 0 2 0\n\
                vt 0 0\nvt 1 0\nvt 1 0.5\nvt 0.5 0.5\nvt 0.5 1\nvt 0 1\n\
                f -6/-6 -5/-5 -4/-4 -3/-3 -2/-2 -1/-1\nl 1 2\n";
    let d = Import {
        unit_mm: 10.0,
        z_up: true,
        ..Default::default()
    };
    let imported = import_obj(quad.as_bytes(), &d, &no_files).unwrap();
    assert_eq!(imported.polygons_triangulated, 1);
    // The library, the line, and the vertices without a colour.
    assert_eq!(
        imported.skipped[1..],
        [
            "5 of 6 vertices have no colour and are white",
            "1 line elements"
        ]
    );
    let p = &imported.parts[0];
    // The one coloured vertex: sRGB (1, 0.5, 0) made linear; the rest white.
    let colors = p.solid.colors.as_ref().unwrap();
    let first = p.solid.triangles.iter().zip(colors).find_map(|(t, c)| {
        let k = t
            .iter()
            .position(|&v| p.solid.positions[v as usize] == DVec3::ZERO)?;
        Some(c[k])
    });
    let half = shape_core::trig::pow((0.5 + 0.055) / 1.055, 2.4);
    assert_eq!(first, Some([1.0, half, 0.0, 1.0]));
    assert!(colors.iter().flatten().filter(|c| **c == [1.0; 4]).count() > 0);
    assert_eq!(p.name, "plate");
    assert_eq!(p.solid.triangles.len(), 4);
    let m = p.solid.measure();
    assert!((m.area - 300.0).abs() < 1e-9);
    // UVs follow their corners: u = x / 20 mm.
    let uvs = p.solid.uvs.as_ref().unwrap();
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
    let instanced = json.replace(
        r#""asset":{"version":"2.0"}"#,
        r#""asset":{"version":"2.0"},"extensionsRequired":["EXT_mesh_gpu_instancing"]"#,
    );
    assert!(import_gltf(instanced.as_bytes(), &files).is_err());
    // Vertex colours are read (image-to-3D outputs carry their colour
    // there): here the positions as RGB, clamped to 1.
    assert!(imported.skipped.is_empty());
    let coloured = json.replace(r#""POSITION":0}"#, r#""POSITION":0,"COLOR_0":0}"#);
    let imported = import_gltf(coloured.as_bytes(), &files).unwrap();
    assert!(imported.skipped.is_empty());
    let mut colors: Vec<[f64; 4]> = imported.parts[0].solid.colors.as_ref().unwrap()[0].to_vec();
    colors.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(
        colors,
        [
            [0.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0]
        ]
    );
}

/// A sphere with the defects meshes from outside the kernel have: split at seams, a
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
fn the_chain_meets_a_texturing_budget() {
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
    let mut low = low;
    unwrap(&mut low, &Unwrap::default()).unwrap();
    let m = measure(
        &low,
        &Measure {
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
            ..Default::default()
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
    let again = measure(&back.parts[0].solid, &Measure::default());
    assert!(again.closed);
    assert_eq!(again.triangles, low.triangles.len());
    assert_eq!(again.uv.unwrap().overlapping_pairs, 0);
}

#[test]
fn binary_fbx_with_pivots_compressed_arrays_and_a_texture() {
    let bytes = support::binary_fbx();
    let imported = import(&bytes, &Import::default(), &no_files).unwrap();
    assert_eq!(imported.parts.len(), 1);
    assert_eq!(imported.polygons_triangulated, 6);
    let p = &imported.parts[0];
    assert_eq!(
        (p.name.as_str(), p.material.name.as_str()),
        ("Cube", "Steel")
    );
    assert_eq!(p.solid.uvs.as_ref().map(Vec::len), Some(12));
    // Steel's DiffuseColor texture is the embedded PNG.
    let map = p.material.map.as_ref().unwrap();
    assert_eq!(
        (map.name.as_str(), map.mime.as_str()),
        ("textures/albedo.png", "image/png")
    );
    assert_eq!(imported.images.len(), 1);
    assert_eq!(imported.images[0].name, "textures/albedo.png");
    assert_eq!(imported.images[0].width, Some(16));
    let s = join(&imported.parts).unwrap();
    let m = s.measure();
    // 2 cm cube: 20 mm a side, closed, two surfaces (materials).
    assert!(m.closed);
    assert!((m.volume - 8000.0).abs() < 1e-6, "{}", m.volume);
    let mut surfaces = s.surfaces.clone();
    surfaces.sort_unstable();
    surfaces.dedup();
    assert_eq!(surfaces.len(), 2);
    // Scaled 2, turned 90° about y around the pivot x = 1 cm, then moved
    // 10 cm in x by its parent: x ∈ [11, 13] cm, y ∈ [0, 2], z ∈ [−1, 1]
    // (Y up); in kernel space (x, −z, y) mm: x ∈ [110, 130],
    // y ∈ [−10, 10], z ∈ [0, 20].
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(
        near(m.bounds_min.x, 110.0) && near(m.bounds_max.x, 130.0),
        "{m:?}"
    );
    assert!(
        near(m.bounds_min.y, -10.0) && near(m.bounds_max.y, 10.0),
        "{m:?}"
    );
    assert!(
        near(m.bounds_min.z, 0.0) && near(m.bounds_max.z, 20.0),
        "{m:?}"
    );
}

#[test]
fn a_skinned_glb_is_read_whole_and_never_broken_silently() {
    use shape_tools::{PlainGlbError, apply_transform, collapse_short_edges};
    let imported = import(&skinned::skinned_glb(true), &Import::default(), &no_files).unwrap();
    assert_eq!(imported.parts.len(), 1);
    let part = &imported.parts[0];
    let s = &part.solid;
    let skin = s.skin.as_ref().expect("the skin is read");
    skin.check(s.positions.len()).unwrap();
    // Bones, parents first, at their world rest frames (kernel: mm, Z up):
    // the knee 0.25 m up glTF's +Y is 250 mm up +Z.
    let names: Vec<&str> = skin.bones.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["hip", "knee"]);
    assert_eq!(skin.bones[1].parent, Some(0));
    assert_eq!(skin.bones[1].rest.origin, DVec3::new(0.0, 0.0, 250.0));
    // The inverse bind matrix, as given, in kernel units: −250 mm in z.
    assert_eq!(skin.bones[1].inverse_bind[14], -250.0);
    // Weights follow JOINTS_0: the two low vertices on the hip, the two
    // high ones on the knee.
    for (p, w) in s.positions.iter().zip(&skin.weights) {
        let bone = if p.z < 250.0 { 0 } else { 1 };
        assert_eq!(*w, shape_mesh::Weights::single(bone), "{p:?}");
    }
    // Measured, in its bind pose, with the skin reported.
    let json = measure(s, &Measure::default()).to_json();
    assert!(
        json.contains(
            r#""skinned":true,"skin":{"bones":2,"bone_names":["hip","knee"],"weighted_vertices":4}"#
        ),
        "{json}"
    );
    // The mesh tools carry the skin (phase 5b part 2); a plain GLB still
    // refuses it.
    let joined = join(&imported.parts).unwrap();
    joined
        .skin
        .as_ref()
        .unwrap()
        .check(joined.positions.len())
        .unwrap();
    let placed = apply_transform(part).unwrap();
    assert_eq!(placed.skin, s.skin);
    let (fixed, _) = repair(s, &Repair::default()).unwrap();
    fixed
        .skin
        .as_ref()
        .unwrap()
        .check(fixed.positions.len())
        .unwrap();
    let half = Decimate {
        ratio: 0.5,
        ..Default::default()
    };
    let (low, _) = decimate(s, &half).unwrap();
    low.skin
        .as_ref()
        .unwrap()
        .check(low.positions.len())
        .unwrap();
    let (short, _) = collapse_short_edges(s, 1.0).unwrap();
    short
        .skin
        .as_ref()
        .unwrap()
        .check(short.positions.len())
        .unwrap();
    assert_eq!(
        glb_plain(s, &PlainGlb::default()).unwrap_err(),
        PlainGlbError::Skinned
    );
    // Unwrap does not re-index, and a preview shows the bind pose.
    let uv = unwrap(&mut s.clone(), &Unwrap::default()).unwrap();
    assert_eq!(uv.corners.len(), s.triangles.len());
    let scene = Scene {
        name: "rig".into(),
        parts: vec![Part::new("legs", s)],
    };
    let d = Preview {
        width: 32,
        views: 1,
        ..Default::default()
    };
    assert!(preview(&scene, &d).is_ok());
    // Round trip: glb writes the skin; reading it back gives the same
    // bones and weights (to f32).
    let bytes = glb(&scene).unwrap();
    let back = import(&bytes, &Import::default(), &no_files).unwrap();
    let b = &back.parts[0].solid;
    let skin2 = b.skin.as_ref().expect("the skin comes back");
    assert_eq!(skin2.bones.len(), skin.bones.len());
    for (x, y) in skin.bones.iter().zip(&skin2.bones) {
        assert_eq!((&x.name, x.parent), (&y.name, y.parent));
        assert!((x.rest.origin - y.rest.origin).length() < 1e-3);
        for k in 0..16 {
            assert!((x.inverse_bind[k] - y.inverse_bind[k]).abs() < 1e-3, "{k}");
        }
    }
    for (p, w) in b.positions.iter().zip(&skin2.weights) {
        let i = s
            .positions
            .iter()
            .position(|q| (*q - *p).length() < 1e-3)
            .expect("same vertex");
        let w0 = skin.weights[i];
        assert_eq!(w.bones, w0.bones);
        for k in 0..4 {
            assert!((w.weights[k] - w0.weights[k]).abs() < 1e-6);
        }
    }
    // Weights with no skin on the node: no skin, and the report says so.
    let bare = import(&skinned::skinned_glb(false), &Import::default(), &no_files).unwrap();
    assert!(bare.parts[0].solid.skin.is_none());
    assert!(
        bare.skipped
            .iter()
            .any(|x| x.contains("bone weights with no skin"))
    );
}

#[test]
fn a_uniform_armature_scale_comes_out_of_the_bones() {
    use shape_skin::{Pose, Turn, pose};
    use shape_tools::SkinScale;
    let plain = import(&skinned::skinned_glb(true), &Import::default(), &no_files).unwrap();
    let cm = skinned::skinned_glb_under(true, Some([0.01; 3]));
    let scaled = import(&cm, &Import::default(), &no_files).unwrap();
    // The scale is reported, with how well the file's matrices agree.
    let [
        SkinScale {
            part,
            scale,
            bind_error,
            uneven,
            node_offset,
        },
    ] = scaled.skin_scales.as_slice()
    else {
        panic!("{:?}", scaled.skin_scales)
    };
    assert_eq!(part, "legs");
    assert!((scale - 0.01).abs() < 1e-9, "{scale}");
    assert!(*bind_error < 1e-6, "{bind_error}");
    assert!(*uneven < 1e-12, "{uneven}");
    // Saved at its bind pose: the nodes stand where the matrices say.
    assert!(*node_offset < 1e-6, "{node_offset}");
    // The same mesh and skeleton as the file without the armature, to
    // f32: the bind pose lands where it did.
    let (a, b) = (&plain.parts[0].solid, &scaled.parts[0].solid);
    let (ka, kb) = (a.skin.as_ref().unwrap(), b.skin.as_ref().unwrap());
    for (p, q) in a.positions.iter().zip(&b.positions) {
        assert!((*p - *q).length() < 1e-4, "{p:?} {q:?}");
    }
    assert_eq!(ka.weights, kb.weights);
    for (x, y) in ka.bones.iter().zip(&kb.bones) {
        assert_eq!((&x.name, x.parent), (&y.name, y.parent));
        assert!((x.rest.origin - y.rest.origin).length() < 1e-4);
        assert!((x.rest.x - y.rest.x).length() < 1e-9);
        for k in 0..16 {
            assert!((x.inverse_bind[k] - y.inverse_bind[k]).abs() < 1e-4, "{k}");
        }
    }
    // Posing the knee 0.7 rad about its own glTF z gives what the file's
    // own matrices give: S · T(knee) · R · T(knee)⁻¹ on the knee's
    // vertices (cm), the hip's still, then metres to kernel space.
    let (sn, cs) = shape_core::trig::sin_cos(0.7);
    let file_posed: Vec<DVec3> = [(0.0, 0.0), (10.0, 0.0), (10.0, 50.0), (0.0, 50.0)]
        .iter()
        .map(|&(x, y): &(f64, f64)| {
            let (x, y) = if y > 25.0 {
                let (dx, dy) = (x, y - 25.0);
                (dx * cs - dy * sn, dx * sn + dy * cs + 25.0)
            } else {
                (x, y)
            };
            // glTF (x, y, z) cm under the 0.01 armature → kernel mm.
            DVec3::new(x * 10.0, 0.0, y * 10.0)
        })
        .collect();
    let posed = pose(
        b,
        &Pose {
            turns: vec![Turn {
                bone: "knee".into(),
                // glTF's +Z is kernel −Y.
                axis: DVec3::new(0.0, -1.0, 0.0),
                angle: 0.7,
            }],
        },
    )
    .unwrap();
    for (p, q) in posed.positions.iter().zip(&file_posed) {
        assert!((*p - *q).length() < 1e-3, "{p:?} {q:?}");
    }
    // Round trip: glb writes rigid bones; reading them back gives the same
    // skeleton and weights, and no scale to take out.
    let scene = Scene {
        name: "rig".into(),
        parts: vec![Part::new("legs", b)],
    };
    let back = import(&glb(&scene).unwrap(), &Import::default(), &no_files).unwrap();
    assert!(back.skin_scales.is_empty());
    let kc = back.parts[0].solid.skin.as_ref().unwrap();
    for (x, y) in kb.bones.iter().zip(&kc.bones) {
        assert_eq!((&x.name, x.parent), (&y.name, y.parent));
        assert!((x.rest.origin - y.rest.origin).length() < 1e-3);
    }
    for (p, w) in back.parts[0].solid.positions.iter().zip(&kc.weights) {
        let i = b
            .positions
            .iter()
            .position(|q| (*q - *p).length() < 1e-3)
            .expect("same vertex");
        assert_eq!(w.bones, kb.weights[i].bones);
    }
    // Saved posed (the knee node turned 90° about z): the bind pose comes
    // from the inverse bind matrices, so the mesh and the inverse binds
    // are as before; the knee rests where its node stands, and how far
    // the nodes stood from the bind pose is reported.
    let q = std::f32::consts::FRAC_1_SQRT_2;
    let posed_file = skinned::skinned_glb_posed(true, Some([0.01; 3]), Some([0.0, 0.0, q, q]));
    let posed_import = import(&posed_file, &Import::default(), &no_files).unwrap();
    let pb = &posed_import.parts[0].solid;
    for (p, q) in b.positions.iter().zip(&pb.positions) {
        assert!((*p - *q).length() < 1e-6, "{p:?} {q:?}");
    }
    let kp = pb.skin.as_ref().unwrap();
    for (x, y) in kp.bones.iter().zip(&kb.bones) {
        for (m, n) in x.inverse_bind.iter().zip(&y.inverse_bind) {
            assert!((m - n).abs() < 1e-9, "{}", x.name);
        }
    }
    assert!((kp.bones[0].rest.x - kb.bones[0].rest.x).length() < 1e-9);
    assert!((kp.bones[1].rest.x - kb.bones[1].rest.x).length() > 1.0);
    // Posed by nothing, the mesh stands as the file shows it: the knee's
    // far vertex as far from its bind place as the report says.
    let rest = shape_skin::pose(pb, &Default::default()).unwrap();
    let moved = rest
        .positions
        .iter()
        .zip(&pb.positions)
        .map(|(a, b)| (*a - *b).length())
        .fold(0.0, f64::max);
    assert!((moved - posed_import.skin_scales[0].node_offset).abs() < 1e-6, "{moved}");
    // The knee's far vertex, (10, 25) cm from the knee, turned 90° to
    // (−25, 10): √(35² + 15²) cm off.
    let off = posed_import.skin_scales[0].node_offset;
    assert!((off - 380.788_655).abs() < 1e-3, "{off}");
    // An uneven scale is still refused, with an error value.
    let lopsided = skinned::skinned_glb_under(true, Some([0.01, 0.02, 0.01]));
    let err = import(&lopsided, &Import::default(), &no_files).unwrap_err();
    assert!(err.to_string().contains("unevenly"), "{err}");
}

/// The phase 5b acceptance on a skinned GLB written here: decimated to
/// half, it keeps its bones and weights and bends as the full mesh does.
#[test]
fn a_skinned_glb_decimated_to_half_keeps_its_skin_and_bends_the_same() {
    use shape_skin::{Pose, Turn, measure_pose, pose};
    let tube = skinned::skinned_tube();
    let bytes = glb(&Scene {
        name: "tube".into(),
        parts: vec![Part::new("tube", &tube)],
    })
    .unwrap();
    let full = import(&bytes, &Import::default(), &no_files).unwrap().parts[0]
        .solid
        .clone();
    let (mut full, _) = repair(&full, &Repair::default()).unwrap();
    full.canonicalize().unwrap();
    let k = full.skin.clone().unwrap();
    let bend = Pose {
        turns: vec![Turn {
            bone: "knee".into(),
            axis: DVec3::X,
            angle: 0.8,
        }],
    };
    let posed_full = pose(&full, &bend).unwrap();
    let mf = measure_pose(&full, &bend).unwrap();
    assert_eq!(mf.inverted, 0);
    for cross in [false, true] {
        let (half, r) = decimate(
            &full,
            &Decimate {
                ratio: 0.5,
                cross_bone_borders: cross,
                ..Default::default()
            },
        )
        .unwrap();
        let kh = half.skin.as_ref().unwrap();
        // The same bones; every vertex's weights sum to 1.
        assert_eq!(kh.bones, k.bones);
        kh.check(half.positions.len()).unwrap();
        // No vertex weighted to a bone that did not weight a vertex of
        // the full mesh within one edge of it.
        let reach = half
            .triangles
            .iter()
            .flat_map(|t| {
                (0..3).map(|i| {
                    (half.positions[t[i] as usize] - half.positions[t[(i + 1) % 3] as usize])
                        .length()
                })
            })
            .fold(0.0f64, f64::max);
        for (p, w) in half.positions.iter().zip(&kh.weights) {
            for (b, _) in w.pairs() {
                let near =
                    full.positions.iter().zip(&k.weights).any(|(q, wq)| {
                        (*q - *p).length() <= reach && wq.pairs().any(|(x, _)| x == b)
                    });
                assert!(near, "vertex {p:?} weighted to bone {b} from nowhere near");
            }
        }
        // Bent the same way, the two surfaces stay close, and nothing
        // turns inside out.
        let posed_half = pose(&half, &bend).unwrap();
        let d = hausdorff::hausdorff(&posed_full, &posed_half);
        let m = measure_pose(&half, &bend).unwrap();
        eprintln!(
            "tube: {} → {} triangles (bone borders {}), posed Hausdorff {d:.4} mm, inverted {}",
            full.triangles.len(),
            half.triangles.len(),
            if cross { "crossed" } else { "kept" },
            m.inverted
        );
        // Crossing borders: 5.8 mm and 41 inverted triangles here.
        if !cross {
            assert_eq!(m.inverted, 0);
            assert!(
                half.triangles.len() <= full.triangles.len() / 2 + 2,
                "{r:?}"
            );
            assert!(d < 0.5, "{d}");
        }
    }
}

/// Joining skinned parts merges their skeletons by bone name; bones of one
/// name that disagree are an error naming them.
#[test]
fn skinned_parts_join_by_bone_name() {
    use shape_tools::{Affine, ImportedPart, RepairError, apply_transform};
    let tube = skinned::skinned_tube();
    let part = |name: &str, t: DVec3| ImportedPart {
        name: name.into(),
        solid: tube.clone(),
        transform: Affine {
            t,
            ..Affine::IDENTITY
        },
        ..Default::default()
    };
    let same = [part("a", DVec3::ZERO), part("b", DVec3::ZERO)];
    let j = join(&same).unwrap();
    let k = j.skin.as_ref().unwrap();
    assert_eq!(k.bones.len(), 2);
    k.check(j.positions.len()).unwrap();
    // Moved, a part's bones move with it, so they no longer match.
    let moved = apply_transform(&part("b", DVec3::new(30.0, 0.0, 0.0))).unwrap();
    let mk = moved.skin.as_ref().unwrap();
    assert_eq!(mk.bones[1].rest.origin, DVec3::new(30.0, 0.0, 50.0));
    // The inverse bind matrix moved with it: rest · IBM is the identity.
    assert_eq!(mk.bones[1].inverse_bind[12], -30.0);
    let err = join(&[
        part("a", DVec3::ZERO),
        part("b", DVec3::new(30.0, 0.0, 0.0)),
    ])
    .unwrap_err();
    assert!(
        matches!(&err, RepairError::Skinned(m) if m.contains("\"knee\"") && m.contains("\"root\"")),
        "{err}"
    );
    // Skinned and unskinned parts are not joined.
    let mut bare = part("c", DVec3::ZERO);
    bare.solid.skin = None;
    assert!(join(&[part("a", DVec3::ZERO), bare]).is_err());
}

/// A file's animation is read as a clip of the part's skin: at each time,
/// the matrices that move the bind pose's vertices to where the file's
/// own nodes, turned and moved by its curves, would. The same under an
/// armature scaled for centimetres.
#[test]
fn a_file_s_animation_poses_its_skin() {
    for armature in [None, Some([0.01f32; 3])] {
        let imported = import(
            &skinned::skinned_glb_animated(armature),
            &Import::default(),
            &no_files,
        )
        .unwrap();
        let part = &imported.parts[0];
        assert_eq!(part.clips.len(), 1);
        let clip = &part.clips[0];
        assert_eq!(clip.name, "bend");
        assert_eq!(clip.duration(), 1.0);
        // Sixty samples a second between the two keys a second apart.
        assert_eq!(clip.times.len(), 61);
        let posed = |time: f64| {
            let s = shape_skin::pose_by(&part.solid, &clip.at(time).unwrap()).unwrap();
            // The vertex that stands at (0, 0, 500) in the bind pose (the
            // file's (0, 0.5, 0)), on the knee; and the one at the origin,
            // on the hip.
            let find = |rest: DVec3| {
                let i = part
                    .solid
                    .positions
                    .iter()
                    .position(|p| (*p - rest).length() < 1e-6)
                    .unwrap();
                s.positions[i]
            };
            (find(DVec3::new(0.0, 0.0, 500.0)), find(DVec3::ZERO))
        };
        let near = |a: DVec3, b: DVec3| assert!((a - b).length() < 0.05, "{a:?} {b:?}");
        // At the start nothing has moved.
        let (top, foot) = posed(0.0);
        near(top, DVec3::new(0.0, 0.0, 500.0));
        near(foot, DVec3::ZERO);
        // Halfway: the knee (at z = 250) has turned 45° about the file's
        // z (the kernel's −y axis... the turn takes +z to −x), and the hip
        // has stepped 100 along x, taking the knee with it.
        let (top, foot) = posed(0.5);
        let d = 250.0 * shape_core::trig::sin(shape_core::trig::PI / 4.0);
        near(top, DVec3::new(100.0 - d, 0.0, 250.0 + d));
        near(foot, DVec3::new(100.0, 0.0, 0.0));
        // At the end, and held after it: a quarter turn.
        for time in [1.0, 7.0] {
            let (top, _) = posed(time);
            near(top, DVec3::new(100.0 - 250.0, 0.0, 250.0));
        }
    }
}

/// A file's clip goes out with the skin and comes back the same motion:
/// read, joined, placed (turned, scaled and moved), written as a GLB
/// with the clip on its bones' nodes, and read again, the mesh stands at
/// each time where the first reading, placed the same way, has it (to
/// a tenth of a millimetre: the written clip is thinned).
#[test]
fn a_clip_is_written_back_and_read_as_the_same_motion() {
    use shape_tools::{Part, Place, Scene, glb_skin_clips, join, join_clips, place};
    for armature in [None, Some([0.01f32; 3])] {
        let first = import(
            &skinned::skinned_glb_animated(armature),
            &Import::default(),
            &no_files,
        )
        .unwrap();
        let solid = join(&first.parts).unwrap();
        let clips = join_clips(&first.parts).unwrap();
        assert_eq!(clips.len(), 1);
        // Placed: a quarter turn about up, twice the size, moved to its base.
        let (mut placed, report) = place(
            std::slice::from_ref(&solid),
            &Place {
                turn: 90.0,
                size: shape_tools::Size::Scale(2.0),
                pivot: shape_tools::Pivot::Base,
                ..Default::default()
            },
        )
        .unwrap();
        let placed = placed.pop().unwrap();
        let clip = clips[0].moved(&report.matrix).unwrap();
        let bytes = glb_skin_clips(
            &Scene {
                name: "legs".into(),
                parts: vec![Part::new("legs", &placed)],
            },
            &[("legs", std::slice::from_ref(&clip))],
        )
        .unwrap();
        let back = import(&bytes, &Import::default(), &no_files).unwrap();
        let again = &back.parts[0];
        assert_eq!(again.clips.len(), 1);
        assert_eq!(again.clips[0].name, "bend");
        assert!((again.clips[0].duration() - 1.0).abs() < 1e-6);
        // The same corners, by where they stand at rest, at each time.
        for time in [0.0, 0.3, 0.5, 0.9, 1.0] {
            let a = shape_skin::pose_by(&placed, &clip.at(time).unwrap()).unwrap();
            let b = shape_skin::pose_by(&again.solid, &again.clips[0].at(time).unwrap()).unwrap();
            for (i, rest) in placed.positions.iter().enumerate() {
                let j = again
                    .solid
                    .positions
                    .iter()
                    .position(|p| (*p - *rest).length() < 1e-3)
                    .expect("the same vertex");
                let d = (a.positions[i] - b.positions[j]).length();
                // The clip is written thinned: to 0.01 mm in a bone's
                // place and 0.04 mm a metre in its turn.
                assert!(d < 0.1, "at {time}: {d} mm");
            }
        }
    }
}

/// A file whose buffer views are meshopt-compressed reads as the same
/// file uncompressed: both extensions, both attribute versions, every
/// control and channel mode, both index streams, and the exponential
/// and colour filters. The encoder is the test's own
/// (`support/meshopt.rs`); `docs/research.md` §42 has the files of
/// others' encoders that were read.
#[test]
fn meshopt_compressed_views_read_as_the_plain_file() {
    // A 20 × 20 sheet, heights in 1/1024 m so 24 bits of mantissa hold
    // them, and a colour a vertex.
    let n = 20u32;
    let mut pos: Vec<f32> = Vec::new();
    let mut yco: Vec<u8> = Vec::new();
    let mut rgba: Vec<u8> = Vec::new();
    for i in 0..n * n {
        let (x, z) = (i % n, i / n);
        let h = (x * 37 + z * 91) % 64;
        pos.extend([x as f32 / 16.0, h as f32 / 1024.0, z as f32 / 16.0]);
        let (y, co, cg, a) = (
            100 + (i % 50) as i32,
            (i % 7) as i32 - 3,
            (i % 5) as i32 - 2,
            i % 128,
        );
        yco.extend([y as u8, co as i8 as u8, cg as i8 as u8, 0x80 | a as u8]);
        rgba.extend([
            (y + co - cg) as u8,
            (y + cg) as u8,
            (y - co - cg) as u8,
            ((a << 1) | (a & 1)) as u8,
        ]);
    }
    let mut tris: Vec<[u32; 3]> = Vec::new();
    for z in 0..n - 1 {
        for x in 0..n - 1 {
            let a = z * n + x;
            tris.push([a, a + 1, a + n]);
            tris.push([a + 1, a + n + 1, a + n]);
        }
    }
    let flat: Vec<u32> = tris.iter().flatten().copied().collect();
    let pos_bytes: Vec<u8> = pos.iter().flat_map(|v| v.to_le_bytes()).collect();
    let exp_bytes: Vec<u8> = pos
        .iter()
        .flat_map(|v| ((-10i32 << 24) | ((v * 1024.0) as i32 & 0x00ff_ffff)).to_le_bytes())
        .collect();
    let (count, corners) = (n * n, flat.len());
    let file = |views: &str, buffers: &str, required: &str, index_type: u32| {
        format!(
            r#"{{"asset":{{"version":"2.0"}},{required}"scene":0,"scenes":[{{"nodes":[0]}}],
            "nodes":[{{"name":"sheet","mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"COLOR_0":2}},"indices":1}}]}}],
            "buffers":[{buffers}],"bufferViews":[{views}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":{count},"type":"VEC3"}},
              {{"bufferView":1,"componentType":{index_type},"count":{corners},"type":"SCALAR"}},
              {{"bufferView":2,"componentType":5121,"normalized":true,"count":{count},"type":"VEC4"}}]}}"#
        )
    };
    let read = |json: &str, bin: Vec<u8>| {
        let files = move |uri: &str| match uri {
            "sheet.bin" => Ok(bin.clone()),
            _ => Err(format!("no {uri}")),
        };
        import_gltf(json.as_bytes(), &files).map(|i| i.parts[0].solid.clone())
    };
    let turned = |s: &Solid| -> Vec<[u32; 3]> {
        s.triangles
            .iter()
            .map(|t| {
                let k = (0..3).min_by_key(|k| t[*k]).unwrap();
                [t[k], t[(k + 1) % 3], t[(k + 2) % 3]]
            })
            .collect()
    };

    // Uncompressed: positions, u32 indices, colours.
    let mut bin = pos_bytes.clone();
    bin.extend(flat.iter().flat_map(|i| i.to_le_bytes()));
    bin.extend(&rgba);
    let (p, i, c) = (pos_bytes.len(), corners * 4, rgba.len());
    let plain = read(
        &file(
            &format!(
                r#"{{"buffer":0,"byteLength":{p}}},{{"buffer":0,"byteOffset":{p},"byteLength":{i}}},
                {{"buffer":0,"byteOffset":{},"byteLength":{c}}}"#,
                p + i
            ),
            &format!(r#"{{"uri":"sheet.bin","byteLength":{}}}"#, bin.len()),
            "",
            5125,
        ),
        bin,
    )
    .unwrap();
    assert_eq!(plain.triangles.len(), tris.len());

    // EXT: version 0 attributes under the exponential filter, a
    // triangle stream of u16, and a fallback buffer that is not there.
    let streams = [
        meshopt::attributes(&exp_bytes, 12, None),
        meshopt::triangles(&tris),
        meshopt::attributes(&rgba, 4, None),
    ];
    let packed = |name: &str, streams: &[Vec<u8>], extra: [&str; 3], strides: [usize; 3]| {
        let mut at = 0;
        let counts = [count as usize, corners, count as usize];
        let views: Vec<String> = (0..3)
            .map(|k| {
                let v = format!(
                    r#"{{"buffer":1,"byteLength":{},"extensions":{{"{name}":{{"buffer":0,"byteOffset":{at},
                    "byteLength":{},"byteStride":{},"count":{}{}}}}}}}"#,
                    counts[k] * strides[k],
                    streams[k].len(),
                    strides[k],
                    counts[k],
                    extra[k]
                );
                at += streams[k].len();
                v
            })
            .collect();
        (views.join(","), streams.concat())
    };
    let (views, bin) = packed(
        "EXT_meshopt_compression",
        &streams,
        [
            r#","mode":"ATTRIBUTES","filter":"EXPONENTIAL""#,
            r#","mode":"TRIANGLES""#,
            r#","mode":"ATTRIBUTES""#,
        ],
        [12, 2, 4],
    );
    let ext = file(
        &views,
        &format!(
            r#"{{"uri":"sheet.bin","byteLength":{}}},
            {{"byteLength":9999,"extensions":{{"EXT_meshopt_compression":{{"fallback":true}}}}}}"#,
            bin.len()
        ),
        r#""extensionsRequired":["EXT_meshopt_compression"],"#,
        5123,
    );
    let got = read(&ext, bin.clone()).unwrap();
    assert_eq!(got.positions, plain.positions);
    assert_eq!(turned(&got), turned(&plain));
    // A stream that is not one is an error that names the view.
    let mut broken = bin;
    broken[streams[0].len()] = 0xe0;
    let err = read(&ext, broken).unwrap_err().to_string();
    assert!(err.contains("bufferView 1's meshopt data"), "{err}");

    // KHR: version 1 attributes (XOR deltas turned 5 bits, plain XOR,
    // 2-byte deltas), an index stream of u32, colours under the colour
    // filter, and a placeholder buffer with no URI.
    let streams = [
        meshopt::attributes(&pos_bytes, 12, Some(&[0x52, 0x02, 0x01])),
        meshopt::indices(&flat),
        meshopt::attributes(&yco, 4, Some(&[0x00])),
    ];
    let (views, bin) = packed(
        "KHR_meshopt_compression",
        &streams,
        [
            r#","mode":"ATTRIBUTES""#,
            r#","mode":"INDICES""#,
            r#","mode":"ATTRIBUTES","filter":"COLOR""#,
        ],
        [12, 4, 4],
    );
    let khr = file(
        &views,
        &format!(
            r#"{{"uri":"sheet.bin","byteLength":{}}},{{"byteLength":9999}}"#,
            bin.len()
        ),
        r#""extensionsRequired":["KHR_meshopt_compression"],"#,
        5125,
    );
    let got = read(&khr, bin).unwrap();
    assert_eq!(got.positions, plain.positions);
    assert_eq!(got.triangles, plain.triangles);
    assert_eq!(got.colors, plain.colors);
    assert!(plain.colors.is_some());
}

/// A file's morph target and the clip that drives it are read, joined,
/// written and read again as the same shape and motion; a decimated
/// solid's targets still fit its vertices and move them as before.
#[test]
fn morph_targets_are_read_carried_and_written() {
    use shape_tools::{glb_skin_clips, join_clips};
    // A square of four vertices; the target `lift` raises one 0.5 m
    // (glTF's +y, the kernel's +z); the mesh rests at 0.25 of it, and
    // the clip takes it from 0 to 1 over a second.
    let mut bin: Vec<u8> = Vec::new();
    let f32s = |bin: &mut Vec<u8>, v: &[f32]| v.iter().for_each(|x| bin.extend(x.to_le_bytes()));
    f32s(
        &mut bin,
        &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0, -1.0],
    );
    f32s(&mut bin, &[0.0; 6]);
    f32s(&mut bin, &[0.0, 0.5, 0.0, 0.0, 0.0, 0.0]);
    for i in [0u16, 1, 2, 0, 2, 3] {
        bin.extend(i.to_le_bytes());
    }
    f32s(&mut bin, &[0.0, 1.0, 0.0, 1.0]);
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
        "nodes":[{{"name":"flag","mesh":0}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"indices":2,"targets":[{{"POSITION":1}}]}}],
          "weights":[0.25],"extras":{{"targetNames":["lift"]}}}}],
        "animations":[{{"name":"raise","channels":[{{"sampler":0,"target":{{"node":0,"path":"weights"}}}}],
          "samplers":[{{"input":3,"output":4}}]}}],
        "buffers":[{{"uri":"flag.bin","byteLength":{}}}],
        "bufferViews":[{{"buffer":0,"byteLength":48}},{{"buffer":0,"byteOffset":48,"byteLength":48}},
          {{"buffer":0,"byteOffset":96,"byteLength":12}},{{"buffer":0,"byteOffset":108,"byteLength":8}},
          {{"buffer":0,"byteOffset":116,"byteLength":8}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3"}},
          {{"bufferView":1,"componentType":5126,"count":4,"type":"VEC3"}},
          {{"bufferView":2,"componentType":5123,"count":6,"type":"SCALAR"}},
          {{"bufferView":3,"componentType":5126,"count":2,"type":"SCALAR"}},
          {{"bufferView":4,"componentType":5126,"count":2,"type":"SCALAR"}}]}}"#,
        bin.len()
    );
    let files = move |uri: &str| match uri {
        "flag.bin" => Ok(bin.clone()),
        _ => Err(format!("no {uri}")),
    };
    let first = import_gltf(json.as_bytes(), &files).unwrap();
    assert!(first.skipped.is_empty(), "{:?}", first.skipped);
    let check = |s: &Solid, clips: &[shape_mesh::SkinClip]| {
        let m = s.morphs.as_ref().expect("targets");
        m.check(s.positions.len()).unwrap();
        assert_eq!(m.targets.len(), 1);
        assert_eq!(
            (m.targets[0].name.as_str(), m.targets[0].weight),
            ("lift", 0.25)
        );
        // Only the corner at glTF's (1, 0, −1) m moves: 500 mm up.
        for (p, d) in s.positions.iter().zip(&m.targets[0].deltas) {
            let want = if *p == DVec3::new(1000.0, 1000.0, 0.0) {
                DVec3::new(0.0, 0.0, 500.0)
            } else {
                DVec3::ZERO
            };
            assert_eq!(*d, want, "at {p:?}");
        }
        assert_eq!(clips.len(), 1);
        assert_eq!(clips[0].name, "raise");
        assert!((clips[0].duration() - 1.0).abs() < 1e-6);
        let w = clips[0].weights_at(0.5).unwrap();
        assert!((w[0] - 0.5).abs() < 1e-6, "{w:?}");
        // Half way through, the corner stands 250 mm up.
        let top = s
            .morphed(&w)
            .positions
            .iter()
            .map(|p| p.z)
            .fold(0.0, f64::max);
        assert!((top - 250.0).abs() < 1e-3, "{top}");
    };
    let solid = join(&first.parts).unwrap();
    let clips = join_clips(&first.parts).unwrap();
    check(&solid, &clips);
    let bytes = glb_skin_clips(
        &Scene {
            name: "flag".into(),
            parts: vec![Part::new("flag", &solid)],
        },
        &[("flag", &clips)],
    )
    .unwrap();
    let back = import(&bytes, &Import::default(), &no_files).unwrap();
    check(
        &join(&back.parts).unwrap(),
        &join_clips(&back.parts).unwrap(),
    );

    // A ball that swells by a tenth, at half its triangles: every
    // vertex still has its offset, a tenth of where it stands, to within
    // what moving a vertex along a collapsed edge leaves.
    let mut ball = sphere(25.0).unwrap();
    ball.morphs = Some(shape_mesh::Morphs {
        targets: vec![shape_mesh::Morph {
            name: "swell".into(),
            weight: 0.0,
            deltas: ball.positions.iter().map(|p| *p * 0.1).collect(),
        }],
    });
    let (half, _) = decimate(
        &ball,
        &Decimate {
            ratio: 0.5,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(half.triangles.len() < ball.triangles.len() * 6 / 10);
    let m = half.morphs.as_ref().expect("targets kept");
    m.check(half.positions.len()).unwrap();
    let off = half
        .positions
        .iter()
        .zip(&m.targets[0].deltas)
        .map(|(p, d)| (*p * 0.1 - *d).length())
        .fold(0.0, f64::max);
    assert!(off < 0.5, "{off}");
    // A plain GLB would drop the targets: refused.
    assert!(glb_plain(&half, &PlainGlb::default()).is_err());
}

/// A primitive under `KHR_draco_mesh_compression` is read from its
/// Draco data: Draco's own cube with texture coordinates
/// (`third_party/draco/testdata`), 12 triangles on a unit cube, each
/// with UVs inside the image.
#[test]
fn a_draco_primitive_is_read() {
    let packed =
        include_bytes!("../../../third_party/draco/testdata/cube_att.obj.edgebreaker.cl4.2.2.drc");
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"extensionsRequired":["KHR_draco_mesh_compression"],
        "scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"name":"cube","mesh":0}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"indices":2,
          "extensions":{{"KHR_draco_mesh_compression":{{"bufferView":0,
            "attributes":{{"POSITION":0,"TEXCOORD_0":1}}}}}}}}]}}],
        "buffers":[{{"uri":"cube.drc","byteLength":{n}}}],
        "bufferViews":[{{"buffer":0,"byteLength":{n}}}],
        "accessors":[{{"componentType":5126,"count":24,"type":"VEC3"}},
          {{"componentType":5126,"count":24,"type":"VEC2"}},
          {{"componentType":5123,"count":36,"type":"SCALAR"}}]}}"#,
        n = packed.len()
    );
    let files = |uri: &str| match uri {
        "cube.drc" => Ok(packed.to_vec()),
        _ => Err(format!("no {uri}")),
    };
    let imported = import_gltf(json.as_bytes(), &files).unwrap();
    let s = &imported.parts[0].solid;
    assert_eq!(s.triangles.len(), 12);
    let (lo, hi) = s.positions.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
    );
    assert!(
        (hi - lo - DVec3::splat(1000.0)).abs().max_element() < 1.0,
        "{lo:?} {hi:?}"
    );
    let uvs = s.uvs.as_ref().expect("UVs");
    assert!(
        uvs.iter()
            .flatten()
            .all(|q| (-1e-3..=1.001).contains(&q.x) && (-1e-3..=1.001).contains(&q.y))
    );
    // Every face of the cube is covered: the area is six square metres.
    let area: f64 = s
        .triangles
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|v| s.positions[v as usize]);
            (b - a).cross(c - a).length() / 2.0
        })
        .sum();
    assert!((area - 6.0e6).abs() < 1.0e4, "{area}");
}

/// A part asked for tangents is written with `TANGENT` beside its skin
/// and clips (what `process --bake` writes for a skinned file), and
/// reads back with both.
#[test]
fn a_skinned_part_is_written_with_tangents() {
    use shape_tools::{glb_skin_clips, join_clips};
    let first = import(
        &skinned::skinned_glb_animated(None),
        &Import::default(),
        &no_files,
    )
    .unwrap();
    let mut solid = join(&first.parts).unwrap();
    let clips = join_clips(&first.parts).unwrap();
    unwrap(&mut solid, &Unwrap::default()).unwrap();
    let write = |tangents: bool| {
        let mut part = Part::new("legs", &solid);
        part.tangents = tangents;
        glb_skin_clips(
            &Scene {
                name: "legs".into(),
                parts: vec![part],
            },
            &[("legs", &clips)],
        )
        .unwrap()
    };
    let has = |bytes: &[u8]| bytes.windows(9).any(|w| w == b"\"TANGENT\"");
    assert!(!has(&write(false)));
    let bytes = write(true);
    assert!(has(&bytes));
    let back = import(&bytes, &Import::default(), &no_files).unwrap();
    let s = &back.parts[0].solid;
    assert!(s.uvs.is_some());
    s.skin
        .as_ref()
        .expect("a skin")
        .check(s.positions.len())
        .unwrap();
    assert_eq!(back.parts[0].clips.len(), clips.len());
}

/// A mesh with no skin that a clip moves by a node above it is read as
/// a skin of one bone, so its motion goes where a skin's goes: its
/// vertices stand where the node puts them at rest, and the clip moves
/// them as the node does.
#[test]
fn a_mesh_its_node_s_animation_moves_is_a_skin_of_one_bone() {
    // A triangle on `tip`, 1 m along x under `arm`, which a clip lifts
    // 1 m (glTF's +y) over a second.
    let mut bin: Vec<u8> = Vec::new();
    let f32s = |bin: &mut Vec<u8>, v: &[f32]| v.iter().for_each(|x| bin.extend(x.to_le_bytes()));
    f32s(&mut bin, &[0.0, 0.0, 0.0, 0.1, 0.0, 0.0, 0.0, 0.1, 0.0]);
    f32s(&mut bin, &[0.0, 1.0]);
    f32s(&mut bin, &[0.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
        "nodes":[{{"name":"arm","children":[1]}},{{"name":"tip","mesh":0,"translation":[1,0,0]}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],
        "animations":[{{"name":"lift","channels":[{{"sampler":0,"target":{{"node":0,"path":"translation"}}}}],
          "samplers":[{{"input":1,"output":2}}]}}],
        "buffers":[{{"uri":"arm.bin","byteLength":{}}}],
        "bufferViews":[{{"buffer":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":8}},
          {{"buffer":0,"byteOffset":44,"byteLength":24}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
          {{"bufferView":1,"componentType":5126,"count":2,"type":"SCALAR"}},
          {{"bufferView":2,"componentType":5126,"count":2,"type":"VEC3"}}]}}"#,
        bin.len()
    );
    let files = move |uri: &str| match uri {
        "arm.bin" => Ok(bin.clone()),
        _ => Err(format!("no {uri}")),
    };
    let imported = import_gltf(json.as_bytes(), &files).unwrap();
    let part = &imported.parts[0];
    let skin = part.solid.skin.as_ref().expect("a skin of one bone");
    assert_eq!(skin.bones.len(), 1);
    assert_eq!(skin.bones[0].name, "tip.node");
    skin.check(part.solid.positions.len()).unwrap();
    // At rest: 1 m along x, where the node stands.
    let lowest = |s: &Solid| {
        s.positions
            .iter()
            .fold(DVec3::splat(f64::INFINITY), |lo, p| lo.min(*p))
    };
    assert_eq!(lowest(&part.solid), DVec3::new(1000.0, 0.0, 0.0));
    assert_eq!(part.clips.len(), 1);
    assert_eq!(part.clips[0].name, "lift");
    // Half way through the clip: half a metre up.
    let posed = shape_skin::pose_by(&part.solid, &part.clips[0].at(0.5).unwrap()).unwrap();
    assert!((lowest(&posed) - DVec3::new(1000.0, 0.0, 500.0)).length() < 1e-6);
    // Without the clip the mesh is as it was: no skin.
    let still = json.replace(r#""animations""#, r#""extras""#);
    let imported = import_gltf(still.as_bytes(), &files).unwrap();
    assert!(imported.parts[0].solid.skin.is_none());
}

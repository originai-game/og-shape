//! Mesh tools end to end: files written by the exporters read back by the
//! importers, external buffers, and the whole chain (import → repair →
//! decimate → unwrap → measure → preview → export) on a broken mesh.

#[path = "support/hausdorff.rs"]
mod hausdorff;
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
    let draco = json.replace(
        r#""asset":{"version":"2.0"}"#,
        r#""asset":{"version":"2.0"},"extensionsRequired":["KHR_draco_mesh_compression"]"#,
    );
    assert!(import_gltf(draco.as_bytes(), &files).is_err());
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
    // from the inverse bind matrices, so the mesh and bones are as before,
    // and how far the nodes stood from it is reported.
    let q = std::f32::consts::FRAC_1_SQRT_2;
    let posed_file = skinned::skinned_glb_posed(true, Some([0.01; 3]), Some([0.0, 0.0, q, q]));
    let posed_import = import(&posed_file, &Import::default(), &no_files).unwrap();
    let pb = &posed_import.parts[0].solid;
    for (p, q) in b.positions.iter().zip(&pb.positions) {
        assert!((*p - *q).length() < 1e-6, "{p:?} {q:?}");
    }
    let kp = pb.skin.as_ref().unwrap();
    assert!((kp.bones[1].rest.x - kb.bones[1].rest.x).length() < 1e-9);
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

//! UVs carried on the solid (`docs/research.md` §17): a generator's UVs
//! and texture through GLB and back, and through repair, decimation and
//! booleans, measured against the UVs that went in.

#[path = "support/uverror.rs"]
mod uverror;

use core::cmp::Ordering;

use shape_core::predicates::orient2d;
use shape_core::{DVec2, DVec3};
use shape_gen::solids::{Axis, BoxDesc, CylinderDesc, Generator, SphereDesc, cylinder, textured};
use shape_mesh::Solid;
use shape_tools::{
    Decimate, Import, Part, Repair, Scene, Texture, decimate, glb, import, no_files, repair,
};
use uverror::uv_error;

fn flipped(s: &Solid) -> usize {
    let u = s.uvs.as_ref().unwrap();
    s.triangles
        .iter()
        .enumerate()
        .filter(|&(i, t)| {
            let [a, b, c] = t.map(|v| s.positions[v as usize]);
            (b - a).cross(c - a).length() > 0.0
                && orient2d(u[i][0], u[i][1], u[i][2]) != Ordering::Greater
        })
        .count()
}

fn png() -> Vec<u8> {
    shape_tools::png(&shape_tools::Image {
        width: 4,
        height: 2,
        rgba: (0..32).map(|i| (i * 8) as u8).collect(),
    })
}

#[test]
fn a_textured_generator_round_trips_through_glb() {
    let s = textured(&Generator::Cylinder(CylinderDesc {
        radius: 10.0,
        height: 20.0,
        segments: 24,
        ..Default::default()
    }))
    .unwrap();
    let map = Texture::from_bytes("checker.png", png());
    let mut part = Part::new("can", &s);
    part.material.map = Some(map.clone());
    let bytes = glb(&Scene {
        name: "can".into(),
        parts: vec![part],
    })
    .unwrap();
    // An independent reader sees TEXCOORD_0 and the embedded PNG.
    let g = gltf::Gltf::from_slice(&bytes).unwrap();
    let prim = g.meshes().next().unwrap().primitives().next().unwrap();
    assert!(prim.get(&gltf::Semantic::TexCoords(0)).is_some());
    let tex = prim
        .material()
        .pbr_metallic_roughness()
        .base_color_texture()
        .unwrap();
    match tex.texture().source().source() {
        gltf::image::Source::View { mime_type, .. } => assert_eq!(mime_type, "image/png"),
        other => panic!("{other:?}"),
    }
    // Ours reads back the same UVs at the same corners (to f32), and the
    // same image bytes.
    let back = import(&bytes, &Import::default(), &no_files).unwrap();
    let p = &back.parts[0];
    assert_eq!(p.material.map.as_ref().map(|t| &t.bytes), Some(&map.bytes));
    let e = uv_error(&s, &p.solid);
    assert!(e.max < 1e-5 && e.max_distance < 1e-3, "{e:?}");
    // A map with no UVs to sample it is an error value.
    let plain = cylinder(&CylinderDesc {
        radius: 10.0,
        height: 20.0,
        ..Default::default()
    })
    .unwrap();
    let mut bare = Part::new("can", &plain);
    bare.material.map = Some(map);
    assert!(
        glb(&Scene {
            name: "can".into(),
            parts: vec![bare],
        })
        .is_err()
    );
}

#[test]
fn decimation_keeps_uvs_and_their_seams() {
    let s = textured(&Generator::Sphere(SphereDesc {
        radius: 20.0,
        axis: Axis::Z,
        segments: 48,
    }))
    .unwrap();
    let (half, r) = decimate(
        &s,
        &Decimate {
            ratio: 0.5,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(half.triangles.len() <= s.triangles.len() / 2 + 1, "{r:?}");
    assert!(half.measure().closed);
    assert_eq!(flipped(&half), 0);
    // UVs here are millimetres on the surface: the error is about the
    // geometric error, nowhere near a seam's jump (2π·20 mm).
    let e = uv_error(&s, &half);
    assert!(e.max < 2.0 && e.mean < 0.2, "{e:?}, geometric {}", r.error);
}

#[test]
fn repair_keeps_uvs_through_welds_flips_and_hole_fills() {
    let s = textured(&Generator::Box(BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    }))
    .unwrap();
    // As an exporter splits it: every triangle its own corners; two
    // triangles turned over, one dropped from the top (a hole).
    let u = s.uvs.as_ref().unwrap();
    let top = s.faces(&["top"]).unwrap()[0] as usize;
    let mut soup = Solid::default();
    for (i, t) in s.triangles.iter().enumerate() {
        if i == top {
            continue;
        }
        let ids = t.map(|v| soup.push_vertex(s.positions[v as usize]));
        let tags: Vec<&str> = s.face_tags[i].iter().map(|&x| s.names.name(x)).collect();
        soup.push_triangle_uv(ids, s.surfaces[i], &tags, u[i]);
        if i % 5 == 1 {
            soup.reverse_triangle(soup.triangles.len() - 1);
        }
    }
    let (fixed, r) = repair(
        &soup,
        &Repair {
            weld: 1e-6,
            max_hole_edges: 8,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(fixed.measure().closed);
    assert!(r.flipped > 0);
    // The hole's rim runs along two walls and the top: three UV charts,
    // so the fill stretches across seams, and the report says so.
    assert_eq!((r.holes_filled, r.uv_seam_fills), (1, 1), "{r:?}");
    // Every other triangle keeps its UVs exactly, turned ones included.
    let fill = fixed.faces(&["fill"]).unwrap();
    let mut kept = fixed.clone();
    let others: Vec<usize> = (0..fixed.triangles.len())
        .filter(|i| !fill.contains(&(*i as u32)))
        .collect();
    let u = fixed.uvs.as_ref().unwrap();
    kept.triangles = others.iter().map(|&i| fixed.triangles[i]).collect();
    kept.uvs = Some(others.iter().map(|&i| u[i]).collect());
    assert_eq!(flipped(&kept), 0);
    let e = uv_error(&s, &kept);
    assert!(e.max < 1e-12, "{e:?}");
}

#[test]
fn a_cut_interpolates_uvs_on_the_faces_it_cuts() {
    let block = textured(&Generator::Box(BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    }))
    .unwrap();
    let mut pin = cylinder(&CylinderDesc {
        radius: 5.0,
        height: 40.0,
        ..Default::default()
    })
    .unwrap();
    pin.map_positions(|p| p + DVec3::new(3.0, 2.0, -10.0), false);
    let (cut, report) = shape_bool::boolean_report(
        &block,
        &pin,
        &shape_bool::Boolean {
            op: shape_bool::BoolOp::Cut,
            ..Default::default()
        },
    )
    .unwrap();
    // The pin has no UVs: its faces in the result (the bore) get (0, 0)
    // and are counted.
    let bore = cut.faces(&["cut"]).unwrap();
    assert_eq!(report.faces_without_uvs, bore.len());
    let u = cut.uvs.as_ref().unwrap();
    assert!(bore.iter().all(|&i| u[i as usize] == [DVec2::ZERO; 3]));
    // Every other face is a piece of a block face, with the block's UVs
    // where it lies.
    let mut kept = cut.clone();
    let others: Vec<usize> = (0..cut.triangles.len())
        .filter(|i| !bore.contains(&(*i as u32)))
        .collect();
    kept.triangles = others.iter().map(|&i| cut.triangles[i]).collect();
    kept.uvs = Some(others.iter().map(|&i| u[i]).collect());
    let e = uv_error(&block, &kept);
    assert!(e.max < 1e-9, "{e:?}");
}

#[test]
fn obj_keeps_its_vt_and_map_kd() {
    let obj = "mtllib box.mtl\no plate\nv 0 0 0\nv 1 0 0\nv 1 1 0\nvt 0 0\nvt 1 0\nvt 1 1\n\
               usemtl painted\nf 1/1 2/2 3/3\n";
    let image = png();
    let files = move |uri: &str| match uri {
        "box.mtl" => Ok(b"newmtl painted\nKd 1 1 1\nmap_Kd albedo.png\n".to_vec()),
        "albedo.png" => Ok(image.clone()),
        _ => Err(format!("no {uri}")),
    };
    let im = shape_tools::import_obj(obj.as_bytes(), &Import::default(), &files).unwrap();
    let p = &im.parts[0];
    assert_eq!(
        p.material.map.as_ref().map(|t| t.name.as_str()),
        Some("albedo.png")
    );
    assert_eq!(
        p.solid.uvs.as_ref().unwrap()[0],
        [DVec2::ZERO, DVec2::X, DVec2::ONE]
    );
}

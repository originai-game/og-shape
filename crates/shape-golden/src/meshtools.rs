//! Golden cases for the mesh tools (shape-tools, phase 5): import, repair,
//! decimate and LODs, UV unwrap, measure and the plain GLB.

use shape_core::{DVec2, DVec3, Frame, HashError, OutputHash};
use shape_gen::solids::{BoxDesc, cuboid, sphere, torus};
use shape_mesh::Solid;
use shape_tools::{
    Decimate, Floaters, Import, ImportedPart, Joint, JointKind, Measure, Part, PlainGlb, Repair,
    Scene, Unwrap, Uvs, glb, glb_plain, import, import_obj, join, lods, measure, no_files, repair,
    unwrap,
};

use crate::cases::Case;

#[allow(dead_code)]
#[path = "../../shape-tools/tests/support/fbx.rs"]
mod fbx_support;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "meshtools.import.glb",
            run: import_glb,
        },
        Case {
            name: "meshtools.import.obj",
            run: import_obj_case,
        },
        Case {
            name: "meshtools.import.fbx",
            run: import_fbx_case,
        },
        Case {
            name: "meshtools.repair",
            run: repair_case,
        },
        Case {
            name: "meshtools.decimate.lods",
            run: decimate_case,
        },
        Case {
            name: "meshtools.unwrap",
            run: unwrap_case,
        },
        Case {
            name: "meshtools.measure.json",
            run: measure_case,
        },
        Case {
            name: "meshtools.glb.plain",
            run: plain_case,
        },
    ]
}

fn fail(e: impl std::fmt::Display) -> HashError {
    eprintln!("meshtools: {e}");
    HashError::NaN
}

fn hash_uvs(h: &mut OutputHash, uvs: &[[DVec2; 3]]) -> Result<(), HashError> {
    h.u64(uvs.len() as u64);
    for t in uvs {
        for q in t {
            h.f64(q.x)?;
            h.f64(q.y)?;
        }
    }
    Ok(())
}

fn hash_part(h: &mut OutputHash, p: &ImportedPart) -> Result<(), HashError> {
    h.str(&p.name);
    h.str(&p.material);
    let t = &p.transform;
    h.dvec3s(&[t.x, t.y, t.z, t.t])?;
    p.solid.hash_into(h)?;
    match &p.uvs {
        Some(u) => hash_uvs(h, u),
        None => {
            h.str("no uvs");
            Ok(())
        }
    }
}

/// A box and a torus on a hinge, written by the exporter and read back.
fn import_glb(h: &mut OutputHash) -> Result<(), HashError> {
    let body = cuboid(&BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    })
    .map_err(fail)?;
    let mut ring = torus(12.0, 3.0).map_err(fail)?;
    ring.map_positions(|p| p + DVec3::new(0.0, 0.0, 28.0), false);
    let mut lid = Part::new("ring", &ring);
    lid.parent = "body".into();
    lid.joint = Some(Joint {
        name: "hinge".into(),
        kind: JointKind::Revolute,
        pivot: Frame {
            origin: DVec3::new(0.0, 15.0, 20.0),
            ..Frame::IDENTITY
        },
        axis: DVec3::X,
        value: 0.3,
        range: Some((-1.0, 1.0)),
        ..Default::default()
    });
    let bytes = glb(&Scene {
        name: "golden".into(),
        parts: vec![Part::new("body", &body), lid],
    })
    .map_err(fail)?;
    let imported = import(&bytes, &Import::default(), &no_files).map_err(fail)?;
    for p in &imported.parts {
        hash_part(h, p)?;
    }
    join(&imported.parts).map_err(fail)?.hash_into(h)
}

/// An OBJ with two objects, quads, an n-gon, negative indices, texture
/// coordinates and materials.
fn import_obj_case(h: &mut OutputHash) -> Result<(), HashError> {
    let text = "# golden\n\
        o plate\nv 0 0 0\nv 0.2 0 0\nv 0.2 0.1 0\nv 0.1 0.1 0\nv 0.1 0.2 0\nv 0 0.2 0\n\
        vt 0 0\nvt 1 0\nvt 1 0.5\nvt 0.5 0.5\nvt 0.5 1\nvt 0 1\n\
        usemtl steel\nf -6/-6 -5/-5 -4/-4 -3/-3 -2/-2 -1/-1\n\
        o post\nv 0 0 0.3\nv 0.05 0 0.3\nv 0.05 0.05 0.3\nv 0 0.05 0.3\n\
        usemtl wood\nf 7 8 9 10\nusemtl steel\nf 7 10 9\n";
    let imported = import_obj(text.as_bytes(), &Import::default(), &no_files).map_err(fail)?;
    h.u64(imported.polygons_triangulated as u64);
    for p in &imported.parts {
        hash_part(h, p)?;
    }
    Ok(())
}

/// A binary FBX (a cube under a parent, pivots, an Euler order, two
/// materials, zlib arrays, an embedded PNG) and an ASCII one (Z up,
/// millimetre units, a pre-rotation and a geometric offset).
fn import_fbx_case(h: &mut OutputHash) -> Result<(), HashError> {
    let ascii = r#"; FBX 7.5.0 project file
FBXHeaderExtension:  {
	FBXVersion: 7500
}
GlobalSettings:  {
	Properties70:  {
		P: "UpAxis", "int", "Integer", "",2
		P: "FrontAxis", "int", "Integer", "",1
		P: "FrontAxisSign", "int", "Integer", "",-1
		P: "CoordAxis", "int", "Integer", "",0
		P: "UnitScaleFactor", "double", "Number", "",0.1
	}
}
Objects:  {
	Geometry: 1, "Geometry::Tri", "Mesh" {
		Vertices: *15 {
			a: 0,0,0,4,0,0,4,3,0,2,5,0,0,3,0
		}
		PolygonVertexIndex: *5 {
			a: 0,1,2,3,-5
		}
	}
	Model: 2, "Model::Roof", "Mesh" {
		Properties70:  {
			P: "PreRotation", "Vector3D", "Vector", "",30,0,0
			P: "Lcl Rotation", "Lcl Rotation", "", "A",10,20,30
			P: "RotationOrder", "enum", "", "",5
			P: "GeometricTranslation", "Vector3D", "Vector", "",0,0,7
		}
	}
}
Connections:  {
	C: "OO",2,0
	C: "OO",1,2
}
"#;
    for bytes in [fbx_support::binary_fbx(), ascii.as_bytes().to_vec()] {
        let imported = import(&bytes, &Import::default(), &no_files).map_err(fail)?;
        h.u64(imported.polygons_triangulated as u64);
        for p in &imported.parts {
            hash_part(h, p)?;
        }
        for i in &imported.images {
            h.str(&i.name);
            h.u64(u64::from(i.width.unwrap_or(0)));
            h.u64(u64::from(i.height.unwrap_or(0)));
        }
    }
    Ok(())
}

/// A torus with the defects of a provider's mesh: triangle soup with
/// sub-micrometre noise, a hole, flipped triangles, a floater and a
/// sliver.
fn broken() -> Result<Solid, HashError> {
    let s = torus(20.0, 7.0).map_err(fail)?;
    let mut b = Solid::default();
    for (i, t) in s.triangles.iter().enumerate() {
        let ids = t.map(|v| {
            let nudge = f64::from(v % 5) * 2.0e-4;
            b.push_vertex(s.positions[v as usize] + DVec3::new(nudge, -nudge, nudge))
        });
        if i == 33 || i == 34 {
            continue;
        }
        let t = if i % 13 == 0 {
            [ids[0], ids[2], ids[1]]
        } else {
            ids
        };
        b.push_triangle(t, 0, &["scan"]);
    }
    let mut speck = sphere(0.8).map_err(fail)?;
    speck.map_positions(|p| p + DVec3::new(0.0, 0.0, 15.0), false);
    b.append(&speck);
    let a = b.push_vertex(DVec3::new(-40.0, 0.0, 0.0));
    let c = b.push_vertex(DVec3::new(-38.0, 0.0, 0.0));
    let m = b.push_vertex(DVec3::new(-39.0, 0.0, 0.0));
    b.push_triangle([a, m, c], 0, &["sliver"]);
    Ok(b)
}

fn repaired() -> Result<Solid, HashError> {
    let d = Repair {
        weld: 0.01,
        floaters: Floaters {
            size: 5.0,
            ..Default::default()
        },
        max_hole_edges: 12,
        ..Default::default()
    };
    Ok(repair(&broken()?, &d).map_err(fail)?.0)
}

fn repair_case(h: &mut OutputHash) -> Result<(), HashError> {
    let d = Repair {
        weld: 0.01,
        floaters: Floaters {
            size: 5.0,
            ..Default::default()
        },
        max_hole_edges: 12,
        check_intersections: true,
        ..Default::default()
    };
    let (s, r) = repair(&broken()?, &d).map_err(fail)?;
    for v in [
        r.welded,
        r.degenerate,
        r.duplicates,
        r.floaters,
        r.floater_triangles,
        r.flipped,
        r.holes_filled,
        r.fill_triangles,
        r.holes_left,
        r.self_intersections.unwrap_or(usize::MAX),
    ] {
        h.u64(v as u64);
    }
    s.hash_into(h)
}

fn decimate_case(h: &mut OutputHash) -> Result<(), HashError> {
    let s = repaired()?;
    let levels = [0.5, 0.25, 0.1].map(|ratio| Decimate {
        ratio,
        ..Default::default()
    });
    for (lod, r) in lods(&s, &levels).map_err(fail)? {
        h.u64(r.triangles as u64);
        h.f64(r.error)?;
        lod.hash_into(h)?;
    }
    // An error limit and locked borders on an open box.
    let mut open = cuboid(&BoxDesc {
        width: 30.0,
        depth: 30.0,
        height: 10.0,
    })
    .map_err(fail)?;
    let top = open.faces(&["top"]).map_err(fail)?;
    let keep: Vec<usize> = (0..open.triangles.len())
        .filter(|i| !top.contains(&(*i as u32)))
        .collect();
    open.triangles = keep.iter().map(|&i| open.triangles[i]).collect();
    open.surfaces = keep.iter().map(|&i| open.surfaces[i]).collect();
    open.face_tags = keep.iter().map(|&i| open.face_tags[i].clone()).collect();
    let (low, r) = shape_tools::decimate(
        &open,
        &Decimate {
            target: 4,
            max_error: 0.5,
            lock_border: true,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    h.u64(r.triangles as u64);
    low.hash_into(h)
}

fn hash_unwrap(h: &mut OutputHash, uv: &Uvs) -> Result<(), HashError> {
    h.u64(uv.chart_count as u64);
    h.f64(uv.scale)?;
    uv.charts.iter().for_each(|&c| h.u64(u64::from(c)));
    hash_uvs(h, &uv.corners)
}

fn unwrap_case(h: &mut OutputHash) -> Result<(), HashError> {
    let s = repaired()?;
    hash_unwrap(h, &unwrap(&s, &Unwrap::default()).map_err(fail)?)?;
    let b = cuboid(&BoxDesc {
        width: 40.0,
        depth: 10.0,
        height: 25.0,
    })
    .map_err(fail)?;
    let d = Unwrap {
        angle: 30.0,
        resolution: 256,
        padding: 4.0,
    };
    hash_unwrap(h, &unwrap(&b, &d).map_err(fail)?)
}

fn measure_case(h: &mut OutputHash) -> Result<(), HashError> {
    let raw = broken()?;
    h.str(
        &measure(
            &raw,
            &Measure {
                intersections: true,
                ..Default::default()
            },
        )
        .to_json(),
    );
    let s = repaired()?;
    let uv = unwrap(&s, &Unwrap::default()).map_err(fail)?;
    let png = shape_tools::png(&shape_tools::Image {
        width: 8,
        height: 4,
        rgba: vec![255; 8 * 4 * 4],
    });
    let images = [shape_tools::ImageInfo::from_bytes("albedo.png", &png)];
    let d = Measure {
        uvs: Some(&uv.corners),
        images: &images,
        uv_resolution: 128,
        intersections: true,
    };
    h.str(&measure(&s, &d).to_json());
    Ok(())
}

fn plain_case(h: &mut OutputHash) -> Result<(), HashError> {
    let s = repaired()?;
    let (low, _) = shape_tools::decimate(
        &s,
        &Decimate {
            target: 200,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    let uv = unwrap(&low, &Unwrap::default()).map_err(fail)?;
    let bytes = glb_plain(
        &low,
        &PlainGlb {
            name: "plain",
            uvs: Some(&uv),
        },
    )
    .map_err(fail)?;
    h.bytes(&bytes);
    h.bytes(&glb_plain(&low, &PlainGlb::default()).map_err(fail)?);
    Ok(())
}

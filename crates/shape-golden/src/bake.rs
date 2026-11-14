//! Golden cases for atlas packing and baking (`docs/research.md` §34):
//! the packer's placements and measures, and the PNG bytes of a bake.

use shape_core::{DVec2, DVec3, HashError, OutputHash};
use shape_gen::solids::{BoxDesc, cuboid, sphere, torus};
use shape_mesh::Solid;
use shape_tools::{
    Atlas, Bake, BakeSource, ChartPlace, Image, Material, Packing, PlainGlb, Texture, Unwrap, bake,
    glb_plain, pack, png, repack, unwrap, unwrap_atlas,
};

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "atlas.pack",
            run: pack_case,
        },
        Case {
            name: "bake.fixture",
            run: bake_case,
        },
        Case {
            name: "atlas.model",
            run: model_case,
        },
    ]
}

fn fail(e: impl std::fmt::Display) -> HashError {
    eprintln!("bake: {e}");
    HashError::NaN
}

fn hash_uvs(h: &mut OutputHash, uvs: &[[DVec2; 3]]) -> Result<(), HashError> {
    h.u64(uvs.len() as u64);
    for q in uvs.iter().flatten() {
        h.f64(q.x)?;
        h.f64(q.y)?;
    }
    Ok(())
}

fn hash_places(h: &mut OutputHash, places: &[ChartPlace]) {
    for p in places {
        for v in [p.x, p.y, p.width, p.height, u32::from(p.turned)] {
            h.u64(u64::from(v));
        }
        h.u64(p.triangles as u64);
    }
}

/// Charts given as triangles: squares, a long strip, a triangle, with
/// weights, packed with and without turns; a torus unwrapped onto a
/// 256-texel grid; a sphere and a box unwrapped apart and repacked into
/// one atlas.
fn pack_case(h: &mut OutputHash) -> Result<(), HashError> {
    let w = DVec2::new;
    let quad = |o: DVec2, x: f64, y: f64| {
        [
            [o, o + w(x, 0.0), o + w(x, y)],
            [o, o + w(x, y), o + w(0.0, y)],
        ]
    };
    let mut corners: Vec<[DVec2; 3]> = Vec::new();
    let mut charts: Vec<u32> = Vec::new();
    for (c, (x, y)) in [
        (10.0, 10.0),
        (40.0, 3.0),
        (5.0, 20.0),
        (7.0, 7.0),
        (12.0, 4.0),
    ]
    .into_iter()
    .enumerate()
    {
        for t in quad(w(100.0 * c as f64, 0.0), x, y) {
            corners.push(t);
            charts.push(c as u32 * 2);
        }
    }
    corners.push([w(0.0, 0.0), w(9.0, 1.0), w(3.0, 6.0)]);
    charts.push(11);
    let weights = [1.0, 1.0, 4.0, 1.0, 0.25, 2.0];
    for no_turns in [false, true] {
        let d = Atlas {
            size: 128,
            padding: 3,
            no_turns,
        };
        let Packing { uvs, places, scale } = pack(&corners, &charts, &weights, &d).map_err(fail)?;
        hash_uvs(h, &uvs)?;
        hash_places(h, &places);
        h.f64(scale)?;
    }
    let mut t = torus(20.0, 6.0).map_err(fail)?;
    let r = unwrap_atlas(
        &mut t,
        &Unwrap::default(),
        &Atlas {
            size: 256,
            padding: 2,
            no_turns: false,
        },
    )
    .map_err(fail)?;
    hash_uvs(h, t.uvs.as_ref().ok_or(HashError::NaN)?)?;
    hash_places(h, &r.places);
    h.str(&r.to_json());
    let mut a = sphere(10.0).map_err(fail)?;
    unwrap(&mut a, &Unwrap::default()).map_err(fail)?;
    let mut b = cuboid(&BoxDesc {
        width: 40.0,
        depth: 10.0,
        height: 25.0,
    })
    .map_err(fail)?;
    unwrap(&mut b, &Unwrap::default()).map_err(fail)?;
    let mut parts = vec![a, b, Solid::default()];
    let r = repack(&mut parts, &Atlas::default()).map_err(fail)?;
    for p in &parts[..2] {
        hash_uvs(h, p.uvs.as_ref().ok_or(HashError::NaN)?)?;
    }
    h.str(&r.to_json());
    Ok(())
}

/// A bump h (1 − r²/R²)² on a 40 mm square.
fn bump(x: f64, y: f64) -> f64 {
    let (dx, dy) = (x - 20.0, y - 20.0);
    let q = 1.0 - (dx * dx + dy * dy) / 144.0;
    if q <= 0.0 { 0.0 } else { 4.0 * q * q }
}

fn image(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    png(&Image {
        width: w,
        height: h,
        rgba: (0..w * h).flat_map(|k| f(k % w, k / w)).collect(),
    })
}

/// A dense bumpy square (48 × 48 cells) with UVs over the unit square, a
/// colour per corner, a checker base-colour map and a gradient
/// metallic-roughness map, baked onto a flat two-triangle square packed
/// on a 64-texel grid: the base colour (times the vertex colours), the
/// data map and the normal map from the bump, as PNG; the report. Then
/// the low mesh's plain GLB with `TANGENT`.
fn bake_case(h: &mut OutputHash) -> Result<(), HashError> {
    let n = 48;
    let mut src = Solid::default();
    for j in 0..=n {
        for i in 0..=n {
            let (x, y) = (40.0 * i as f64 / n as f64, 40.0 * j as f64 / n as f64);
            src.positions.push(DVec3::new(x, y, bump(x, y)));
        }
    }
    let at = |i: usize, j: usize| (j * (n + 1) + i) as u32;
    let (mut uvs, mut cols) = (Vec::new(), Vec::new());
    for j in 0..n {
        for i in 0..n {
            for t in [
                [at(i, j), at(i + 1, j), at(i + 1, j + 1)],
                [at(i, j), at(i + 1, j + 1), at(i, j + 1)],
            ] {
                src.push_triangle(t, 0, &[]);
                let p = t.map(|v| src.positions[v as usize]);
                uvs.push(p.map(|q| DVec2::new(q.x / 40.0, q.y / 40.0)));
                cols.push(
                    p.map(|q| shape_mesh::stored([0.25 + q.x / 80.0, 1.0, 0.5 + q.y / 80.0, 1.0])),
                );
            }
        }
    }
    src.uvs = Some(uvs);
    src.colors = Some(cols);
    let m = Material {
        roughness: Some(0.5),
        map: Some(Texture::from_bytes(
            "checker.png",
            image(32, 32, |x, y| {
                let v = if (x / 8 + y / 8) % 2 == 0 { 40 } else { 230 };
                [v, v / 2, 255 - v, 255]
            }),
        )),
        metalness_map: Some(Texture::from_bytes(
            "mr.png",
            image(16, 16, |x, y| [0, (x * 16) as u8, (y * 16) as u8, 255]),
        )),
        ..Default::default()
    };
    let mut low = Solid::default();
    for (x, y) in [(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)] {
        low.positions.push(DVec3::new(x, y, 0.0));
    }
    low.push_triangle([0, 1, 2], 0, &[]);
    low.push_triangle([0, 2, 3], 0, &[]);
    let r = unwrap_atlas(
        &mut low,
        &Unwrap::default(),
        &Atlas {
            size: 64,
            padding: 2,
            no_turns: false,
        },
    )
    .map_err(fail)?;
    h.str(&r.to_json());
    let b = bake(
        &BakeSource {
            solid: &src,
            materials: core::slice::from_ref(&m),
            material_of: &[],
        },
        &low,
        &Bake {
            size: 64,
            distance: 6.0,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    h.str(&b.report.to_json());
    for (kind, t) in b.material.maps() {
        h.str(kind);
        h.bytes(&t.bytes);
    }
    let glb = glb_plain(
        &low,
        &PlainGlb {
            name: "baked",
            material: Some(&b.material),
            tangents: true,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    h.bytes(&glb);
    Ok(())
}

/// The washing machine asked for UVs, exported with every part's UVs in
/// one 512-texel atlas: the GLB, the packing's report, the materials the
/// parts share.
fn model_case(h: &mut OutputHash) -> Result<(), HashError> {
    let text = include_str!("../../../golden/programs/washing_machine.json");
    let text = text.replacen("\"ops\":[", "\"ops\":[{\"op\":\"uvs\"},", 1);
    let m = shape_model::program::run(&text).map_err(fail)?;
    let e = m
        .glb_atlas(&Atlas {
            size: 512,
            ..Default::default()
        })
        .map_err(fail)?;
    h.bytes(&e.glb);
    h.str(&e.atlas.to_json());
    h.u64(e.materials as u64);
    h.u64(e.own.len() as u64);
    Ok(())
}

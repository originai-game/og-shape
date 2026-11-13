//! Golden cases for materials, maps and vertex colours through the mesh
//! chain (`docs/research.md` §29): import (glTF `COLOR_0` of two
//! encodings, a material with every factor and map; OBJ colours and an
//! MTL), the chain (repair, decimation, a cut) on a coloured solid, the
//! plain GLB with a material, and the texture decoder and box filter.

use shape_core::{DVec3, HashError, OutputHash};
use shape_gen::solids::{BoxDesc, CylinderDesc, Generator, SphereDesc, cuboid, cylinder, textured};
use shape_mesh::Solid;
use shape_tools::{
    ColorSpace, Decimate, Downsize, Floaters, Import, Material, Part, PlainGlb, Repair, Scene,
    Texture, decimate, decode, downsize, glb, glb_plain, import, repair,
};

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "meshtools.look.import",
            run: import_case,
        },
        Case {
            name: "meshtools.look.chain",
            run: chain_case,
        },
        Case {
            name: "meshtools.look.texture",
            run: texture_case,
        },
        Case {
            name: "meshtools.look.ktx2",
            run: ktx2_case,
        },
        Case {
            name: "meshtools.look.etc1s",
            run: etc1s_case,
        },
    ]
}

fn fail(e: impl std::fmt::Display) -> HashError {
    eprintln!("look: {e}");
    HashError::NaN
}

/// Every field of a material, its maps as their bytes.
pub fn hash_material(h: &mut OutputHash, m: &Material) -> Result<(), HashError> {
    h.str(&m.name);
    h.str(match m.color_space {
        ColorSpace::Srgb => "srgb",
        ColorSpace::Linear => "linear",
    });
    let opt = |x: Option<f64>| x.unwrap_or(-1.0);
    h.f64s(&m.color.unwrap_or([-1.0; 3]))?;
    h.f64s(&[
        m.metalness,
        opt(m.roughness),
        opt(m.opacity),
        opt(m.emissive_intensity),
        opt(m.alpha_test),
        opt(m.normal_scale),
        opt(m.ao_map_intensity),
    ])?;
    h.f64s(&m.emissive)?;
    h.u64(m.transparent.map_or(2, u64::from));
    h.u64(u64::from(m.double_sided));
    for (kind, t) in m.maps() {
        h.str(kind);
        h.str(&t.name);
        h.str(&t.mime);
        h.bytes(&t.bytes);
    }
    Ok(())
}

fn png(seed: u8, w: u32, h: u32) -> Vec<u8> {
    shape_tools::png(&shape_tools::Image {
        width: w,
        height: h,
        rgba: (0..w * h * 4)
            .map(|i| ((i * 37 + u32::from(seed) * 11) % 256) as u8)
            .collect(),
    })
}

fn affine(p: DVec3) -> [f64; 4] {
    [
        0.5 + p.x / 200.0,
        0.5 + p.y / 300.0,
        0.5 - p.z / 250.0,
        0.75,
    ]
}

pub fn painted_sphere() -> Result<Solid, HashError> {
    let mut s = textured(&Generator::Sphere(SphereDesc {
        radius: 20.0,
        ..Default::default()
    }))
    .map_err(fail)?;
    s.colors = Some(
        s.triangles
            .iter()
            .map(|t| t.map(|v| affine(s.positions[v as usize])))
            .collect(),
    );
    Ok(s)
}

pub fn material() -> Material {
    let tex = |seed: u8| {
        Some(Texture::from_bytes(
            &format!("t{seed}.png"),
            png(seed, 4, 2),
        ))
    };
    Material {
        name: "painted".into(),
        color: Some([0.8, 0.5, 0.25]),
        metalness: 0.25,
        roughness: Some(0.375),
        emissive: [0.25, 0.0, 0.5],
        opacity: Some(0.5),
        map: tex(0),
        color_space: ColorSpace::Linear,
        emissive_intensity: Some(4.0),
        alpha_test: Some(0.25),
        double_sided: true,
        metalness_map: tex(1),
        normal_map: tex(2),
        normal_scale: Some(0.5),
        emissive_map: tex(3),
        ao_map: tex(4),
        ao_map_intensity: Some(0.75),
        ..Default::default()
    }
}

/// A coloured sphere with a material through the scene GLB and back; a
/// glTF with unorm8 RGBA and float RGB colours and unset factors; an OBJ
/// with colours on one of its parts and an MTL with maps.
fn import_case(h: &mut OutputHash) -> Result<(), HashError> {
    let s = painted_sphere()?;
    let mut part = Part::new("ball", &s);
    part.material = material();
    let bytes = glb(&Scene {
        name: "look".into(),
        parts: vec![part],
    })
    .map_err(fail)?;
    h.bytes(&bytes);
    let none = |uri: &str| -> Result<Vec<u8>, String> { Err(format!("no {uri}")) };
    let hash_import = |h: &mut OutputHash, bytes: &[u8], files: shape_tools::Resolve| {
        let im = import(bytes, &Import::default(), files).map_err(fail)?;
        for p in &im.parts {
            h.str(&p.name);
            hash_material(h, &p.material)?;
            p.defaults.iter().for_each(|d| h.str(d));
            p.solid.hash_into(h)?;
        }
        im.skipped.iter().for_each(|x| h.str(x));
        Ok::<(), HashError>(())
    };
    hash_import(h, &bytes, &none)?;
    // Two primitives: unorm8 RGBA colours, and float RGB; the second
    // with no material.
    let mut bin = Vec::new();
    for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        bin.extend(v.to_le_bytes());
    }
    bin.extend([255u8, 0, 0, 255, 0, 128, 0, 64, 0, 0, 255, 0]);
    for v in [0.25f32, 0.5, 1.0, 1.0, 0.0, 0.5, 0.0, 0.0, 0.0] {
        bin.extend(v.to_le_bytes());
    }
    let b64 = base64(&bin);
    let gltf = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0,"name":"tri"}}],
        "materials":[{{"pbrMetallicRoughness":{{"roughnessFactor":0.5}},"alphaMode":"BLEND","emissiveFactor":[0.5,0.25,0],
          "extensions":{{"KHR_materials_emissive_strength":{{"emissiveStrength":2}}}}}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"COLOR_0":1}},"material":0}},{{"attributes":{{"POSITION":0,"COLOR_0":2}}}}]}}],
        "buffers":[{{"uri":"data:application/octet-stream;base64,{b64}","byteLength":{}}}],
        "bufferViews":[{{"buffer":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":12}},{{"buffer":0,"byteOffset":48,"byteLength":36}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
          {{"bufferView":1,"componentType":5121,"normalized":true,"count":3,"type":"VEC4"}},
          {{"bufferView":2,"componentType":5126,"count":3,"type":"VEC3"}}]}}"#,
        bin.len()
    );
    hash_import(h, gltf.as_bytes(), &none)?;
    let mtl = "newmtl clay\nKd 0.8 0.5 0.2\nKe 0.1 0 0\nd 0.75\nPr 0.25\nPm 0.5\nKs 0.5 0.5 0.5\nmap_Kd albedo.png\nnorm n.png\nmap_Bump h.png\n";
    let obj = "mtllib m.mtl\no a\nv 0 0 0 1 0.5 0\nv 1 0 0 0 0 1\nv 0 1 0 0.2 0.4 0.6\nvt 0 0\nvt 1 0\nvt 0 1\nusemtl clay\nf 1/1 2/2 3/3\n\
               o b\nv 0 0 1\nv 1 0 1\nv 0 1 1\nf 4 5 6\n";
    let files = |uri: &str| -> Result<Vec<u8>, String> {
        match uri {
            "m.mtl" => Ok(mtl.as_bytes().to_vec()),
            "albedo.png" => Ok(png(7, 2, 2)),
            "n.png" => Ok(png(8, 2, 2)),
            _ => Err(format!("no {uri}")),
        }
    };
    hash_import(h, obj.as_bytes(), &files)
}

fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in b.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= c.len() {
                out.push(T[(n >> (18 - 6 * k)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// A coloured, textured sphere as a soup with a hole, repaired, decimated
/// and written as a plain GLB with its material; a coloured block cut by
/// a pin without colours.
fn chain_case(h: &mut OutputHash) -> Result<(), HashError> {
    let s = painted_sphere()?;
    let (uvs, colors) = (
        s.uvs.clone().unwrap_or_default(),
        s.colors.clone().unwrap_or_default(),
    );
    let mut soup = Solid::default();
    for (i, t) in s.triangles.iter().enumerate().skip(2) {
        let v = t.map(|v| soup.push_vertex(s.positions[v as usize]));
        soup.push_triangle_uv(v, 0, &["ball"], uvs[i]);
        soup.set_colors(soup.triangles.len() - 1, colors[i]);
    }
    let (fixed, r) = repair(
        &soup,
        &Repair {
            weld: 0.001,
            floaters: Floaters {
                fraction: 0.05,
                ..Default::default()
            },
            max_hole_edges: 64,
            fuse: true,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    h.str(&format!("{} {}", r.holes_filled, r.fill_triangles));
    fixed.hash_into(h)?;
    let (low, _) = decimate(
        &fixed,
        &Decimate {
            ratio: 0.4,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    low.hash_into(h)?;
    let m = material();
    h.bytes(
        &glb_plain(
            &low,
            &PlainGlb {
                name: "ball",
                material: Some(&m),
                ..Default::default()
            },
        )
        .map_err(fail)?,
    );
    let measured = shape_tools::measure(
        &low,
        &shape_tools::Measure {
            materials: core::slice::from_ref(&m),
            ..Default::default()
        },
    );
    h.str(&measured.to_json());
    let mut block = cuboid(&BoxDesc {
        width: 60.0,
        depth: 40.0,
        height: 30.0,
    })
    .map_err(fail)?;
    block.colors = Some(
        block
            .triangles
            .iter()
            .map(|t| t.map(|v| affine(block.positions[v as usize])))
            .collect(),
    );
    let mut pin = cylinder(&CylinderDesc {
        radius: 6.0,
        height: 80.0,
        ..Default::default()
    })
    .map_err(fail)?;
    pin.map_positions(|p| p + DVec3::new(5.0, 3.0, -20.0), false);
    let (out, report) = shape_bool::boolean_report(
        &block,
        &pin,
        &shape_bool::Boolean {
            op: shape_bool::BoolOp::Cut,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    h.str(&format!("{report:?}"));
    out.hash_into(h)
}

/// The baseline JPEG fixture decoded; it and a PNG shrunk as colour and
/// as data.
/// Maps written as KTX2: the files' bytes (UASTC blocks fitted in f64,
/// mip levels, Zstandard) and what they decode to.
fn ktx2_case(h: &mut OutputHash) -> Result<(), HashError> {
    let jpg = include_bytes!("../../../golden/meshes/stripes_420.jpg");
    let tex = [
        (Texture::from_bytes("stripes.jpg", jpg.to_vec()), true),
        (Texture::from_bytes("noise.png", png(9, 37, 23)), false),
    ];
    for (t, color) in &tex {
        let k = shape_tools::to_ktx2(t, *color)
            .map_err(fail)?
            .ok_or(HashError::NaN)?;
        h.str(&k.name);
        h.bytes(&k.bytes);
        h.bytes(&decode(&k.bytes).map_err(fail)?.rgba);
    }
    Ok(())
}

/// Maps written as KTX2 of ETC1S blocks: the files' bytes (blocks
/// fitted and codebooks split in f64, Huffman tables) and what they
/// decode to.
fn etc1s_case(h: &mut OutputHash) -> Result<(), HashError> {
    let jpg = include_bytes!("../../../golden/meshes/stripes_420.jpg");
    let tex = [
        (Texture::from_bytes("stripes.jpg", jpg.to_vec()), true),
        (Texture::from_bytes("noise.png", png(9, 37, 23)), false),
    ];
    for (t, color) in &tex {
        let k = shape_tools::to_ktx2_etc1s(t, *color)
            .map_err(fail)?
            .ok_or(HashError::NaN)?;
        h.bytes(&k.bytes);
        h.bytes(&decode(&k.bytes).map_err(fail)?.rgba);
    }
    Ok(())
}

fn texture_case(h: &mut OutputHash) -> Result<(), HashError> {
    let jpg = include_bytes!("../../../golden/meshes/stripes_420.jpg");
    let img = decode(jpg).map_err(fail)?;
    h.bytes(&img.rgba);
    let tex = [
        Texture::from_bytes("stripes.jpg", jpg.to_vec()),
        Texture::from_bytes("noise.png", png(9, 37, 23)),
    ];
    for t in &tex {
        for (max, color) in [(8, true), (8, false), (5, true)] {
            match downsize(t, &Downsize { max, color }).map_err(fail)? {
                Some(small) => {
                    h.str(&small.name);
                    h.bytes(&small.bytes);
                }
                None => h.str("kept"),
            }
        }
    }
    Ok(())
}

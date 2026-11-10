//! Materials, maps and vertex colours through the mesh chain
//! (`docs/research.md` §29): colours through decimation, repair and a
//! boolean, measured against the colour function that made them; a
//! material with every factor and map through both GLB writers and back;
//! the JPEG decoder against a reference decode.

use std::collections::BTreeMap;

use shape_core::DVec3;
use shape_gen::solids::{BoxDesc, CylinderDesc, cuboid, cylinder, sphere};
use shape_mesh::{Solid, edge_key};
use shape_tools::{
    ColorSpace, Decimate, Import, Material, Part, PlainGlb, Repair, Scene, Texture, decimate, glb,
    glb_plain, import, no_files, repair,
};

/// An affine colour of position, inside 0..=1 on the solids below.
fn affine(p: DVec3) -> [f64; 4] {
    [0.5 + p.x / 200.0, 0.5 + p.y / 300.0, 0.5 - p.z / 250.0, 1.0]
}

fn paint(s: &mut Solid, f: impl Fn(usize, DVec3) -> [f64; 4]) {
    s.colors = Some(
        s.triangles
            .iter()
            .enumerate()
            .map(|(i, t)| t.map(|v| f(i, s.positions[v as usize])))
            .collect(),
    );
}

fn largest_error(s: &Solid) -> f64 {
    let c = s.colors.as_ref().unwrap();
    let mut worst: f64 = 0.0;
    for (i, t) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            let want = affine(s.positions[t[k] as usize]);
            for ch in 0..4 {
                worst = worst.max((c[i][k][ch] - want[ch]).abs());
            }
        }
    }
    worst
}

/// The vertices on a hard colour border: an edge whose two triangles
/// give one of its ends different colours.
fn border_points(s: &Solid) -> Vec<DVec3> {
    let c = s.colors.as_ref().unwrap();
    let mut by_edge: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (i, t) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            by_edge
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(i);
        }
    }
    let at = |t: usize, v: u32| c[t][s.triangles[t].iter().position(|&w| w == v).unwrap()];
    let mut out = Vec::new();
    for (&(a, b), ts) in &by_edge {
        if ts.len() == 2 && (at(ts[0], a) != at(ts[1], a) || at(ts[0], b) != at(ts[1], b)) {
            out.extend([s.positions[a as usize], s.positions[b as usize]]);
        }
    }
    out
}

#[test]
fn decimation_moves_colours_with_their_corners_and_keeps_hard_borders() {
    // A smooth colour: every corner keeps the colour of where it is.
    let mut s = sphere(40.0).unwrap();
    paint(&mut s, |_, p| affine(p));
    s.canonicalize().unwrap();
    let half = Decimate {
        ratio: 0.5,
        ..Default::default()
    };
    let (low, _) = decimate(&s, &half).unwrap();
    assert!(low.triangles.len() < s.triangles.len());
    assert!(largest_error(&low) < 1e-12, "{}", largest_error(&low));
    // A hard border (two hemispheres, by each triangle's centre): every
    // triangle stays one colour and the border stays on the input's
    // border vertices.
    let mut two = sphere(40.0).unwrap();
    let centre_z: Vec<f64> = two
        .triangles
        .iter()
        .map(|t| t.iter().map(|&v| two.positions[v as usize].z).sum())
        .collect();
    paint(&mut two, |i, _| {
        if centre_z[i] >= 0.0 {
            [1.0, 0.0, 0.0, 1.0]
        } else {
            [0.0, 0.0, 1.0, 1.0]
        }
    });
    two.canonicalize().unwrap();
    let before = border_points(&two);
    let (low, _) = decimate(&two, &half).unwrap();
    let c = low.colors.as_ref().unwrap();
    assert!(c.iter().all(|t| t[0] == t[1] && t[1] == t[2]));
    let after = border_points(&low);
    assert!(!after.is_empty());
    assert!(after.iter().all(|p| before.contains(p)));
}

#[test]
fn crossing_colour_borders_reaches_the_target_and_changes_nothing_smooth() {
    // Every triangle its own colour: every edge is a border, so keeping
    // borders keeps the mesh; crossing them reaches the target.
    let mut s = sphere(40.0).unwrap();
    paint(&mut s, |i, _| {
        let k = (i % 7) as f64 / 6.0;
        [k, 1.0 - k, 0.5, 1.0]
    });
    s.canonicalize().unwrap();
    let target = s.triangles.len() / 8;
    let keep = Decimate {
        target,
        ..Default::default()
    };
    let cross = Decimate {
        cross_color_borders: true,
        ..keep
    };
    let (kept, _) = decimate(&s, &keep).unwrap();
    assert!(
        kept.triangles.len() > 2 * target,
        "{}",
        kept.triangles.len()
    );
    let (low, r) = decimate(&s, &cross).unwrap();
    assert!(low.triangles.len() <= target, "{r:?}");
    // Colours blur between the input's, never outside them.
    let c = low.colors.as_ref().unwrap();
    assert!(
        c.iter()
            .flatten()
            .all(|v| v.iter().all(|x| (0.0..=1.0).contains(x)))
    );
    // A smooth colour has no border to cross: the same output.
    let mut smooth = sphere(40.0).unwrap();
    paint(&mut smooth, |_, p| affine(p));
    smooth.canonicalize().unwrap();
    let half = |cross_color_borders| Decimate {
        ratio: 0.5,
        cross_color_borders,
        ..Default::default()
    };
    assert_eq!(
        decimate(&smooth, &half(true)).unwrap().0,
        decimate(&smooth, &half(false)).unwrap().0
    );
}

#[test]
fn repair_fills_holes_with_their_rims_colours() {
    // A coloured sphere as a soup (every triangle its own corners) with
    // three triangles gone: welding joins it, the fill takes the rim's
    // colours, so every corner on an input vertex keeps its colour.
    let mut s = sphere(40.0).unwrap();
    paint(&mut s, |_, p| affine(p));
    let mut soup = Solid {
        colors: Some(Vec::new()),
        ..Default::default()
    };
    let c = s.colors.clone().unwrap();
    for (i, t) in s.triangles.iter().enumerate().skip(3) {
        let v = t.map(|v| soup.push_vertex(s.positions[v as usize]));
        soup.push_triangle(v, 0, &["ball"]);
        soup.set_colors(soup.triangles.len() - 1, c[i]);
    }
    let (fixed, r) = repair(
        &soup,
        &Repair {
            weld: 0.001,
            max_hole_edges: 16,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(r.holes_filled >= 1 && fixed.measure().closed, "{r:?}");
    let fills = fixed.faces(&["fill"]).unwrap();
    let col = fixed.colors.as_ref().unwrap();
    for &i in &fills {
        for (k, &v) in fixed.triangles[i as usize].iter().enumerate() {
            let p = fixed.positions[v as usize];
            if s.positions.contains(&p) {
                assert_eq!(col[i as usize][k], affine(p));
            }
        }
    }
}

#[test]
fn a_cut_interpolates_colours_and_counts_the_tools_faces() {
    let mut block = cuboid(&BoxDesc {
        width: 60.0,
        depth: 40.0,
        height: 30.0,
    })
    .unwrap();
    paint(&mut block, |_, p| affine(p));
    let pin = cylinder(&CylinderDesc {
        radius: 6.0,
        height: 80.0,
        ..Default::default()
    })
    .unwrap();
    let mut pin = pin;
    pin.map_positions(|p| p + DVec3::new(5.0, 3.0, -20.0), false);
    let (out, report) = shape_bool::boolean_report(
        &block,
        &pin,
        &shape_bool::Boolean {
            op: shape_bool::BoolOp::Cut,
            ..Default::default()
        },
    )
    .unwrap();
    let cut = out.faces(&["cut"]).unwrap();
    assert_eq!(report.faces_without_colors, cut.len());
    // The block's own faces, cut or not, keep the colour of where they
    // are; the hole's wall is white.
    let col = out.colors.as_ref().unwrap();
    for (i, t) in out.triangles.iter().enumerate() {
        for k in 0..3 {
            let want = if cut.contains(&(i as u32)) {
                [1.0; 4]
            } else {
                affine(out.positions[t[k] as usize])
            };
            for ch in 0..4 {
                assert!((col[i][k][ch] - want[ch]).abs() < 1e-12);
            }
        }
    }
}

fn png(seed: u8) -> Texture {
    Texture::from_bytes(
        &format!("map{seed}.png"),
        shape_tools::png(&shape_tools::Image {
            width: 4,
            height: 2,
            rgba: (0..32).map(|i| (i * 8) as u8 ^ seed).collect(),
        }),
    )
}

#[test]
fn a_material_and_colours_survive_both_glb_writers() {
    let mut s = shape_gen::solids::textured(&shape_gen::solids::Generator::Sphere(
        shape_gen::solids::SphereDesc {
            radius: 20.0,
            ..Default::default()
        },
    ))
    .unwrap();
    paint(&mut s, |_, p| affine(p));
    let m = Material {
        name: "painted".into(),
        color: Some([0.8, 0.5, 0.25]),
        metalness: 1.0,
        roughness: Some(0.375),
        emissive: [0.25, 0.0, 0.5],
        opacity: Some(0.5),
        map: Some(png(0)),
        color_space: ColorSpace::Linear,
        emissive_intensity: Some(4.0),
        transparent: Some(false),
        alpha_test: None,
        double_sided: true,
        metalness_map: Some(png(1)),
        normal_map: Some(png(2)),
        normal_scale: Some(0.5),
        emissive_map: Some(png(3)),
        ao_map: Some(png(4)),
        ao_map_intensity: Some(0.75),
    };
    let mut part = Part::new("ball", &s);
    part.material = m.clone();
    let scene = glb(&Scene {
        name: "s".into(),
        parts: vec![part],
    })
    .unwrap();
    let plain = glb_plain(
        &s,
        &PlainGlb {
            name: "painted",
            material: Some(&m),
            ..Default::default()
        },
    )
    .unwrap();
    for bytes in [scene, plain] {
        let back = import(&bytes, &Import::default(), &no_files).unwrap();
        assert!(back.skipped.is_empty(), "{:?}", back.skipped);
        let p = &back.parts[0];
        // The factors as written, the maps as their bytes.
        let got = Material {
            name: m.name.clone(),
            ..p.material.clone()
        };
        assert_eq!(got, m);
        assert!(p.defaults.is_empty());
        let c = p.solid.colors.as_ref().unwrap();
        let worst = p
            .solid
            .triangles
            .iter()
            .zip(c)
            .flat_map(|(t, c)| (0..3).map(move |k| (t[k], c[k])))
            .map(|(v, c)| {
                let want = affine(p.solid.positions[v as usize] / 1.0);
                (0..3)
                    .map(|ch| (c[ch] - want[ch]).abs())
                    .fold(0.0, f64::max)
            })
            .fold(0.0, f64::max);
        // Positions round to f32 metres and colours to f32: the colour
        // of the rounded point to within that.
        assert!(worst < 1e-6, "{worst}");
    }
    // glTF's defaults are kept as the file means them, and listed.
    let gltf = r#"{"asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],
      "nodes":[{"mesh":0}],"materials":[{"pbrMetallicRoughness":{"roughnessFactor":0.5}}],
      "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
      "buffers":[{"uri":"data:application/octet-stream;base64,AAAAAAAAAAAAAAAAAACAPwAAAAAAAAAAAAAAAAAAgD8AAAAA","byteLength":36}],
      "bufferViews":[{"buffer":0,"byteLength":36}],
      "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}]}"#;
    let back = import(gltf.as_bytes(), &Import::default(), &no_files).unwrap();
    let p = &back.parts[0];
    assert_eq!(p.material.metalness, 1.0);
    assert_eq!(p.material.roughness, Some(0.5));
    assert_eq!(p.defaults, ["metallicFactor", "baseColorFactor"]);
}

#[test]
fn baseline_jpeg_decodes_as_a_reference_decoder_does() {
    // Four 8-texel stripes, 4:2:0, written by libjpeg (PIL, quality 95);
    // libjpeg reads these texels at row 8.
    let bytes = include_bytes!("../../../golden/meshes/stripes_420.jpg");
    let img = shape_tools::decode(bytes).unwrap();
    assert_eq!((img.width, img.height), (32, 16));
    let reference = [[200, 31, 38], [20, 179, 60], [30, 60, 220], [240, 240, 22]];
    for (k, want) in reference.iter().enumerate() {
        let x = 4 + 8 * k;
        let at = (8 * 32 + x) * 4;
        for ch in 0..3 {
            let got = i32::from(img.rgba[at + ch]);
            assert!(
                (got - want[ch]).abs() <= 3,
                "stripe {k}: {got} for {}",
                want[ch]
            );
        }
    }
    // A progressive JPEG is refused, saying so.
    let mut prog = bytes.to_vec();
    let sof = prog.windows(2).position(|w| w == [0xFF, 0xC0]).unwrap();
    prog[sof + 1] = 0xC2;
    assert!(matches!(
        shape_tools::decode(&prog),
        Err(shape_tools::TextureError::Unsupported(_))
    ));
}

#[test]
fn an_fbx_materials_colours_are_read_as_srgb() {
    // A triangle on a model with a material: diffuse × factor, emissive,
    // opacity; and a texture on its specular, which is not read.
    let fbx = r#"; FBX 7.5.0 project file
FBXHeaderExtension:  {
	FBXVersion: 7500
}
Objects:  {
	Geometry: 1, "Geometry::Tri", "Mesh" {
		Vertices: *9 {
			a: 0,0,0,1,0,0,0,1,0
		}
		PolygonVertexIndex: *3 {
			a: 0,1,-3
		}
	}
	Model: 2, "Model::Tri", "Mesh" {
	}
	Material: 3, "Material::Clay", "" {
		Properties70:  {
			P: "DiffuseColor", "Color", "", "A",0.8,0.4,0.2
			P: "DiffuseFactor", "Number", "", "A",0.5
			P: "EmissiveColor", "Color", "", "A",0.1,0.2,0.3
			P: "Opacity", "double", "Number", "",0.25
		}
	}
	Texture: 4, "Texture::Spec", "" {
	}
}
Connections:  {
	C: "OO",2,0
	C: "OO",1,2
	C: "OO",3,2
	C: "OP",4,3, "SpecularColor"
}
"#;
    let im = import(fbx.as_bytes(), &Import::default(), &no_files).unwrap();
    let m = &im.parts[0].material;
    assert_eq!(m.name, "Clay");
    assert_eq!(m.color_space, ColorSpace::Srgb);
    assert_eq!(m.color, Some([0.4, 0.2, 0.1]));
    assert_eq!(m.emissive, [0.1, 0.2, 0.3]);
    assert_eq!(m.opacity, Some(0.25));
    assert_eq!(
        im.skipped,
        ["Tri: its material's textures on SpecularColor are not read"]
    );
}

#[test]
fn fusing_makes_no_colour_border_where_the_colour_is_continuous() {
    // Two overlapping boxes with one affine colour: the union's cut
    // vertices are interpolated in each triangle around them and round
    // differently; no border may come of it.
    let block = |at: DVec3| {
        let mut b = cuboid(&BoxDesc {
            width: 30.0,
            depth: 20.0,
            height: 10.0,
        })
        .unwrap();
        b.map_positions(|p| p + at, false);
        b
    };
    let mut two = block(DVec3::ZERO);
    two.append(&block(DVec3::new(11.0, 7.3, 3.1)));
    paint(&mut two, |_, p| affine(p));
    two.canonicalize().unwrap();
    let (fused, r) = repair(
        &two,
        &Repair {
            fuse: true,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(r.fused, Some(Ok(2)), "{r:?}");
    let m = shape_tools::measure(&fused, &Default::default());
    assert_eq!(m.color_borders, Some(0));
    assert!(largest_error(&fused) < 1e-9);
}

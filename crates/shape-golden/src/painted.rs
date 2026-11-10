//! Golden cases for the preview's maps and vertex colours and the UV view
//! (`docs/research.md` §30): the PNG bytes, which the gate compares on
//! every target, so a JPEG decoded, a mip chain filtered and a texel
//! sampled must come out the same on each.

use shape_core::{DVec3, HashError, OutputHash};
use shape_gen::solids::{BoxDesc, CylinderDesc, Generator, sphere, textured};
use shape_mesh::Solid;
use shape_tools::{Material, Part, Preview, Scene, Texture, UvView, png, sheet, uv_view, views};

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "tools.preview.textured",
            run: textured_case,
        },
        Case {
            name: "tools.preview.colors",
            run: colors_case,
        },
        Case {
            name: "tools.preview.uv",
            run: uv_case,
        },
    ]
}

const LABEL: &[u8] = include_bytes!("../../../golden/programs/label.jpg");
use crate::uvs::CHECKER;

fn fail(e: impl std::fmt::Display) -> HashError {
    eprintln!("painted: {e}");
    HashError::NaN
}

/// A solid's UVs divided by `size` (a model's `mapSize`).
fn per(mut s: Solid, size: f64) -> Solid {
    if let Some(uvs) = &mut s.uvs {
        for q in uvs.iter_mut().flatten() {
            *q /= size;
        }
    }
    s
}

/// A can with a JPEG label (baseline, 4:2:0, the layout of every JPEG
/// stream AG's generators wrote) tinted by its colour, and a box behind it
/// with the PNG checker at opacity 0.6.
fn solids() -> Result<(Solid, Solid), HashError> {
    let can = per(
        textured(&Generator::Cylinder(CylinderDesc {
            radius: 10.0,
            height: 24.0,
            segments: 32,
            ..Default::default()
        }))
        .map_err(fail)?,
        30.0,
    );
    let mut slab = per(
        textured(&Generator::Box(BoxDesc {
            width: 40.0,
            depth: 6.0,
            height: 30.0,
        }))
        .map_err(fail)?,
        10.0,
    );
    slab.map_positions(|p| p + DVec3::new(6.0, 18.0, 0.0), false);
    Ok((can, slab))
}

fn scene<'a>(can: &'a Solid, slab: &'a Solid) -> Scene<'a> {
    Scene {
        name: "painted".into(),
        parts: vec![
            Part {
                material: Material {
                    name: "label".into(),
                    color: Some([1.0, 0.9, 0.8]),
                    map: Some(Texture::from_bytes("label.jpg", LABEL.to_vec())),
                    ..Default::default()
                },
                ..Part::new("can", can)
            },
            Part {
                material: Material {
                    name: "glass".into(),
                    opacity: Some(0.6),
                    map: Some(Texture::from_bytes("checker.png", CHECKER.to_vec())),
                    ..Default::default()
                },
                ..Part::new("slab", slab)
            },
        ],
    }
}

/// Three perspective views (the maps sampled perspective-correctly, the
/// cameras' numbers), and one orthographic unlit view.
fn textured_case(h: &mut OutputHash) -> Result<(), HashError> {
    let (can, slab) = solids()?;
    let s = scene(&can, &slab);
    let v = views(
        &s,
        &Preview {
            width: 96,
            views: 3,
            fov: Some(0.8),
            ..Default::default()
        },
    )
    .map_err(fail)?;
    h.bytes(&png(&sheet(&v.images, 3)));
    for c in &v.cameras {
        h.dvec3s(&[c.position, c.target, c.up])?;
    }
    h.u64(v.undrawn.len() as u64);
    let flat = views(
        &s,
        &Preview {
            width: 96,
            views: 1,
            unlit: true,
            no_edges: true,
            no_ground: true,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    h.bytes(&png(&flat.images[0]));
    h.f64(flat.cameras[0].ortho_height)
}

/// A ball whose vertex colours run with its position (linear RGB),
/// half transparent at the top: four orthographic views.
fn colors_case(h: &mut OutputHash) -> Result<(), HashError> {
    let mut ball = sphere(12.0).map_err(fail)?;
    let colour = |p: DVec3| {
        let n = p / 12.0;
        [
            0.5 + 0.5 * n.x,
            0.5 + 0.5 * n.y,
            0.5 - 0.5 * n.z,
            1.0 - 0.5 * (0.5 + 0.5 * n.z),
        ]
    };
    ball.colors = Some(
        ball.triangles
            .iter()
            .map(|t| t.map(|v| colour(ball.positions[v as usize])))
            .collect(),
    );
    let s = Scene {
        name: "colors".into(),
        parts: vec![Part {
            material: Material {
                opacity: Some(0.9),
                ..Default::default()
            },
            ..Part::new("ball", &ball)
        }],
    };
    let v = views(
        &s,
        &Preview {
            width: 64,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    h.bytes(&png(&sheet(&v.images, 4)));
    Ok(())
}

/// The UV layout of the textured scene: one image per material, the map
/// under the can's charts, the checker map under the slab's (its UVs span
/// several tiles).
fn uv_case(h: &mut OutputHash) -> Result<(), HashError> {
    let (can, slab) = solids()?;
    let mut bare = slab.clone();
    bare.map_positions(|p| p + DVec3::new(0.0, 30.0, 0.0), false);
    let mut s = scene(&can, &slab);
    s.parts.push(Part::new("bare", &bare));
    let layouts = uv_view(&s, &UvView { size: 96 }).map_err(fail)?;
    for l in &layouts {
        h.str(&l.material);
        h.u64(u64::from(l.tiles));
        h.f64s(&l.uv_min)?;
        h.u64(u64::from(l.textured));
        h.bytes(&png(&l.image));
    }
    Ok(())
}

//! Golden cases for UVs carried on the solid (`docs/research.md` §17):
//! the generators' UVs, the mesh tools carrying them, a boolean
//! interpolating them, and GLB with a texture written and read back.

use shape_core::{DVec3, HashError, OutputHash};
use shape_gen::solids::{
    Axis, BoxDesc, CylinderDesc, DomeDesc, Extrude, Generator, SphereDesc, TorusDesc, cylinder,
    textured,
};
use shape_mesh::Solid;
use shape_tools::{
    Decimate, Import, Part, PlainGlb, Repair, Scene, Texture, decimate, glb, glb_plain, import,
    no_files, repair,
};

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "uvs.generators",
            run: generators,
        },
        Case {
            name: "uvs.chain",
            run: chain,
        },
        Case {
            name: "uvs.boolean",
            run: boolean,
        },
        Case {
            name: "uvs.glb",
            run: glb_case,
        },
        Case {
            name: "uvs.generators2",
            run: generators2,
        },
        Case {
            name: "uvs.edits",
            run: edits,
        },
        Case {
            name: "uvs.program",
            run: program,
        },
    ]
}

fn fail(e: impl std::fmt::Display) -> HashError {
    eprintln!("uvs: {e}");
    HashError::NaN
}

fn rounded_bar() -> Result<shape_2d::Profile, HashError> {
    shape_2d::rect(&shape_2d::Rect {
        width: 30.0,
        height: 12.0,
        radius: 3.0,
    })
    .map_err(fail)
}

/// Every generator `textured` builds, hashed with its UVs.
fn generators(h: &mut OutputHash) -> Result<(), HashError> {
    let bar = rounded_bar()?;
    let gens = [
        Generator::Box(BoxDesc {
            width: 40.0,
            depth: 30.0,
            height: 20.0,
        }),
        Generator::Extrude(
            &bar,
            Extrude {
                height: 10.0,
                twist: 0.4,
                taper: 0.2,
                ..Default::default()
            },
        ),
        Generator::Cylinder(CylinderDesc {
            radius: 8.0,
            radius_top: 5.0,
            height: 20.0,
            segments: 24,
        }),
        Generator::Sphere(SphereDesc {
            radius: 10.0,
            axis: Axis::Z,
            segments: 24,
        }),
        Generator::Torus(TorusDesc {
            radius: 20.0,
            tube_radius: 4.0,
            angle: 3.0,
            segments: 32,
            tube_segments: 12,
            ..Default::default()
        }),
        Generator::Dome(DomeDesc {
            radius: 10.0,
            height: 4.0,
            axis: Axis::Z,
            segments: 24,
        }),
    ];
    for g in &gens {
        textured(g).map_err(fail)?.hash_into(h)?;
    }
    Ok(())
}

/// A textured sphere written as a seam-split soup with a hole, repaired
/// (welded, filled) and decimated to half, its UVs carried.
fn chain(h: &mut OutputHash) -> Result<(), HashError> {
    let s = textured(&Generator::Sphere(SphereDesc {
        radius: 20.0,
        segments: 32,
        ..Default::default()
    }))
    .map_err(fail)?;
    let u = s.uvs.clone().unwrap_or_default();
    let mut soup = Solid::default();
    for (i, t) in s.triangles.iter().enumerate() {
        if i == 100 || i == 101 {
            continue;
        }
        let ids = t.map(|v| soup.push_vertex(s.positions[v as usize]));
        soup.push_triangle_uv(ids, s.surfaces[i], &["ball"], u[i]);
    }
    let (fixed, r) = repair(
        &soup,
        &Repair {
            weld: 1e-6,
            max_hole_edges: 8,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    for v in [r.welded, r.holes_filled, r.fill_triangles, r.uv_seam_fills] {
        h.u64(v as u64);
    }
    fixed.hash_into(h)?;
    let (half, d) = decimate(
        &fixed,
        &Decimate {
            ratio: 0.5,
            ..Default::default()
        },
    )
    .map_err(fail)?;
    h.u64(d.triangles as u64);
    half.hash_into(h)
}

/// A textured block cut by a pin with no UVs.
fn boolean(h: &mut OutputHash) -> Result<(), HashError> {
    let block = textured(&Generator::Box(BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    }))
    .map_err(fail)?;
    let mut pin = cylinder(&CylinderDesc {
        radius: 5.0,
        height: 40.0,
        segments: 16,
        ..Default::default()
    })
    .map_err(fail)?;
    pin.map_positions(|p| p + DVec3::new(3.0, 2.0, -10.0), false);
    let (cut, r) = shape_bool::boolean_report(
        &block,
        &pin,
        &shape_bool::Boolean {
            op: shape_bool::BoolOp::Cut,
            target: "block",
            tool: "pin",
        },
    )
    .map_err(fail)?;
    h.u64(r.faces_without_uvs as u64);
    cut.hash_into(h)
}

/// A textured can with a PNG map, as a scene GLB and a plain GLB, and
/// the scene GLB read back.
fn glb_case(h: &mut OutputHash) -> Result<(), HashError> {
    let can = textured(&Generator::Cylinder(CylinderDesc {
        radius: 10.0,
        height: 20.0,
        segments: 16,
        ..Default::default()
    }))
    .map_err(fail)?;
    let png = shape_tools::png(&shape_tools::Image {
        width: 4,
        height: 2,
        rgba: (0..32u8).map(|i| i * 8).collect(),
    });
    let map = Texture::from_bytes("label.png", png);
    let mut part = Part::new("can", &can);
    part.material.map = Some(map.clone());
    let scene = glb(&Scene {
        name: "can".into(),
        parts: vec![part],
    })
    .map_err(fail)?;
    h.bytes(&scene);
    h.bytes(
        &glb_plain(
            &can,
            &PlainGlb {
                name: "can",
                map: Some(&map),
            },
        )
        .map_err(fail)?,
    );
    let back = import(&scene, &Import::default(), &no_files).map_err(fail)?;
    for p in &back.parts {
        h.str(&p.name);
        h.bytes(p.map.as_ref().map_or(&[][..], |t| &t.bytes));
        p.solid.hash_into(h)?;
    }
    Ok(())
}

/// The generators stream Y gave UVs (`docs/research.md` §21.1).
fn generators2(h: &mut OutputHash) -> Result<(), HashError> {
    use shape_gen::body::{Body, View};
    use shape_gen::parts::{GearDesc, Knurl};
    use shape_gen::paths::{Loft, Sweep, Thread};
    use shape_gen::wire::Wire;
    let bar = rounded_bar()?;
    let disc = shape_2d::circle(&shape_2d::Circle {
        radius: 6.0,
        segments: 20,
    })
    .map_err(fail)?;
    let loft = Loft {
        sections: vec![
            (bar.clone(), 0.0),
            (disc.clone(), 15.0),
            (bar.clone(), 30.0),
        ],
        steps: 3,
        ..Default::default()
    };
    let sweep = Sweep {
        path: vec![
            DVec3::ZERO,
            DVec3::new(0.0, 0.0, 20.0),
            DVec3::new(15.0, 0.0, 30.0),
        ],
        twist: 0.3,
        scale_end: 0.6,
        ..Default::default()
    };
    let wire = Wire {
        path: vec![
            DVec3::ZERO,
            DVec3::new(30.0, 0.0, 0.0),
            DVec3::new(30.0, 20.0, 5.0),
        ],
        diameter: 3.0,
        fillet: 5.0,
        segments: 12,
        ..Default::default()
    };
    let ring = Wire {
        closed: true,
        fillet: 0.0,
        ..wire.clone()
    };
    let body = Body {
        plan: View::spline(&[[0.0, 4.0], [20.0, 10.0], [50.0, 6.0]]),
        top: View::constant(6.0),
        bottom: View::constant(-4.0),
        ..Default::default()
    };
    let gens = [
        Generator::Loft(&loft),
        Generator::Sweep(&disc, &sweep),
        Generator::Thread(Thread {
            radius: 4.0,
            pitch: 1.0,
            length: 4.0,
            depth: 0.0,
        }),
        Generator::Wire(&wire),
        Generator::Wire(&ring),
        Generator::Body(&body),
        Generator::Knurl(Knurl {
            diameter: 10.0,
            height: 6.0,
            pitch: 1.0,
            straight: false,
        }),
        Generator::Gear(GearDesc {
            teeth: 10,
            module: 2.0,
            width: 4.0,
            bore: 4.0,
            hub: 8.0,
            hub_height: 2.0,
            ..Default::default()
        }),
        Generator::Wedge(10.0, 6.0, 4.0),
    ];
    for g in &gens {
        textured(g).map_err(fail)?.hash_into(h)?;
    }
    Ok(())
}

/// A textured plate through every edit and bevel that carries UVs.
fn edits(h: &mut OutputHash) -> Result<(), HashError> {
    use shape_core::Frame;
    let plate = textured(&Generator::Extrude(
        &rounded_bar()?,
        Extrude {
            height: 8.0,
            ..Default::default()
        },
    ))
    .map_err(fail)?;
    let above = |x: f64| Frame {
        origin: DVec3::new(x, 0.0, 20.0),
        ..Frame::IDENTITY
    };
    let holed = shape_edit::hole(
        &plate,
        &shape_edit::Hole {
            diameter: 4.0,
            at: vec![above(-8.0)],
            counterbore: (6.0, 2.0),
            ..Default::default()
        },
    )
    .map_err(fail)?;
    holed.hash_into(h)?;
    let pocketed = shape_edit::pocket(
        &holed,
        &shape_edit::Pocket {
            profile: shape_2d::rect(&shape_2d::Rect {
                width: 6.0,
                height: 4.0,
                radius: 0.0,
            })
            .map_err(fail)?,
            depth: 3.0,
            at: vec![above(7.0)],
        },
    )
    .map_err(fail)?;
    pocketed.hash_into(h)?;
    let filleted = shape_bevel::fillet(
        &plate,
        &shape_bevel::Fillet {
            radius: 0.8,
            edges: vec!["bottom".into()],
            ..Default::default()
        },
    )
    .map_err(fail)?;
    filleted.hash_into(h)?;
    let chamfered = shape_bevel::chamfer(
        &plate,
        &shape_bevel::Chamfer {
            distance: 1.0,
            edges: vec!["top".into()],
            ..Default::default()
        },
    )
    .map_err(fail)?;
    chamfered.hash_into(h)?;
    let shelled = shape_edit::shell(
        &plate,
        &shape_edit::Shell {
            thickness: 1.0,
            open: vec!["top".into()],
        },
    )
    .map_err(fail)?;
    shelled.hash_into(h)?;
    let dented = shape_edit::dent(
        &plate,
        &shape_edit::Dent {
            at: vec![DVec3::new(0.0, 0.0, 8.0)],
            radius: 4.0,
            depth: 1.0,
        },
    )
    .map_err(fail)?;
    dented.hash_into(h)?;
    let split = shape_edit::split(
        &plate,
        &Frame {
            origin: DVec3::new(0.0, 0.0, 3.0),
            ..Frame::IDENTITY
        },
        shape_edit::Keep::Below,
    )
    .map_err(fail)?;
    split.hash_into(h)
}

const UV_PLATE: &str = include_str!("../../../golden/programs/uv_plate.json");
const CHECKER: &[u8] = include_bytes!("../../../golden/programs/checker.png");

/// The acceptance program (`golden/programs/uv_plate.json`, recorded by
/// `crates/shape-wasm/ts/examples/uv_plate.ts`): its GLB and measures.
fn program(h: &mut OutputHash) -> Result<(), HashError> {
    use shape_model::program::{Fonts, run_files};
    let mut textures = Fonts::new();
    textures.insert("checker.png".into(), CHECKER.to_vec());
    let m = run_files(UV_PLATE, &Fonts::new(), &textures).map_err(fail)?;
    h.bytes(&m.glb().map_err(fail)?);
    h.str(&shape_model::program::measures_json(&m).map_err(fail)?);
    Ok(())
}

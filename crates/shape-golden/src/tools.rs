//! Golden cases for export and the preview (shape-tools): the bytes of
//! each file and of the preview's framebuffer.

use shape_2d::{Circle, Rect, circle, rect};
use shape_core::{DVec3, Frame, HashError, OutputHash};
use shape_gen::solids::{CylinderDesc, Extrude, cylinder, extrude, sphere};
use shape_mesh::Solid;
use shape_tools::{
    Joint, JointKind, Material, Mimic, Part, Preview, Scene, glb, obj, png, preview, sheet, stl,
};

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "tools.glb",
            run: |h| with_scene(h, |h, s| glb(s).map(|b| h.bytes(&b))),
        },
        Case {
            name: "tools.obj",
            run: |h| with_scene(h, |h, s| obj(s).map(|t| h.str(&t))),
        },
        Case {
            name: "tools.stl",
            run: |h| with_scene(h, |h, s| stl(s).map(|b| h.bytes(&b))),
        },
        Case {
            name: "tools.preview",
            run: |h| {
                with_scene(h, |h, s| {
                    preview(s, &Preview::default()).map(|views| {
                        for v in &views {
                            h.u64(u64::from(v.width));
                            h.u64(u64::from(v.height));
                            h.bytes(&v.rgba);
                        }
                    })
                })
            },
        },
        Case {
            name: "tools.preview.png",
            run: |h| {
                with_scene(h, |h, s| {
                    let d = Preview {
                        width: 96,
                        views: 3,
                        elevation: Some(1.0),
                        azimuth: 0.3,
                        ..Default::default()
                    };
                    preview(s, &d).map(|views| h.bytes(&png(&sheet(&views, 2))))
                })
            },
        },
        Case {
            name: "tools.preview.facets",
            run: facets,
        },
    ]
}

/// A hexagonal loft (facets past the crease angle: flat), a 48-sided
/// cylinder (smooth), and a pane of glass at opacity 0.4 in front of
/// both, three views of 96²: the preview's crease and blending rules
/// (`docs/research.md` §20).
fn facets(h: &mut OutputHash) -> Result<(), HashError> {
    use shape_2d::{Polygon, polygon};
    use shape_gen::paths::{Loft, loft};
    let built = (|| -> Result<(Solid, Solid, Solid), String> {
        let hex = |r: f64| {
            polygon(&Polygon {
                sides: 6,
                radius: r,
                ..Default::default()
            })
            .map_err(|e| e.to_string())
        };
        let tower = loft(&Loft {
            sections: vec![(hex(12.0)?, 0.0), (hex(7.0)?, 30.0)],
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        let mut drum = cylinder(&CylinderDesc {
            radius: 10.0,
            height: 20.0,
            segments: 48,
            ..Default::default()
        })
        .map_err(|e| e.to_string())?;
        drum.map_positions(|p| p + DVec3::new(28.0, 6.0, 0.0), false);
        let mut pane = shape_gen::solids::cuboid(&shape_gen::solids::BoxDesc {
            width: 60.0,
            depth: 2.0,
            height: 26.0,
        })
        .map_err(|e| e.to_string())?;
        pane.map_positions(|p| p + DVec3::new(14.0, -20.0, 2.0), false);
        Ok((tower, drum, pane))
    })();
    let (tower, drum, pane) = match built {
        Ok(b) => b,
        Err(e) => {
            h.str(&format!("error: {e}"));
            return Ok(());
        }
    };
    let painted = |rgb: [f64; 3], opacity: Option<f64>| Material {
        color: Some(rgb),
        opacity,
        ..Default::default()
    };
    let scene = Scene {
        name: "facets".into(),
        parts: vec![
            Part {
                material: painted([0.8, 0.5, 0.3], None),
                ..Part::new("tower", &tower)
            },
            Part {
                material: painted([0.4, 0.6, 0.8], None),
                ..Part::new("drum", &drum)
            },
            Part {
                material: painted([0.6, 0.9, 0.7], Some(0.4)),
                ..Part::new("pane", &pane)
            },
        ],
    };
    let d = Preview {
        width: 96,
        views: 3,
        elevation: Some(0.4),
        ..Default::default()
    };
    match preview(&scene, &d) {
        Ok(views) => h.bytes(&png(&sheet(&views, 3))),
        Err(e) => h.str(&format!("error: {e}")),
    }
    Ok(())
}

/// A plate with a twisted, holed riser, a turning wheel and a ball that
/// follows it: every export feature at once.
fn with_scene(
    h: &mut OutputHash,
    run: impl FnOnce(&mut OutputHash, &Scene) -> Result<(), shape_tools::ExportError>,
) -> Result<(), HashError> {
    let built = (|| -> Result<(Solid, Solid, Solid), shape_gen::solids::GenError> {
        let profile = rect(&Rect {
            width: 60.0,
            height: 40.0,
            radius: 6.0,
        })?
        .subtract(&circle(&Circle {
            radius: 8.0,
            segments: 0,
        })?);
        let plate = extrude(
            &profile,
            &Extrude {
                height: 30.0,
                twist: 0.4,
                taper: 0.2,
                ..Default::default()
            },
        )?;
        let mut wheel = cylinder(&CylinderDesc {
            radius: 12.0,
            height: 6.0,
            ..Default::default()
        })?;
        wheel.map_positions(|p| DVec3::new(p.x + 40.0, p.z - 3.0, p.y + 15.0), true);
        let mut ball = sphere(5.0)?;
        ball.map_positions(|p| p + DVec3::new(40.0, -8.0, 30.0), false);
        Ok((plate, wheel, ball))
    })();
    let (plate, wheel, ball) = match built {
        Ok(b) => b,
        Err(e) => {
            h.str(&format!("error: {e}"));
            return Ok(());
        }
    };
    let pivot = Frame::from_normal(DVec3::new(40.0, 0.0, 15.0), DVec3::Y, DVec3::X)
        .ok_or(HashError::NaN)?;
    let scene = Scene {
        name: "golden".into(),
        parts: vec![
            Part {
                material: Material {
                    name: "painted".into(),
                    color: Some([0.2, 0.25, 0.3]),
                    roughness: Some(0.4),
                    ..Default::default()
                },
                ..Part::new("plate", &plate)
            },
            Part {
                parent: "plate".into(),
                material: Material {
                    color: Some([0.79, 0.64, 0.29]),
                    metalness: 1.0,
                    roughness: Some(0.25),
                    emissive: [0.1, 0.0, 0.0],
                    opacity: Some(0.8),
                    ..Default::default()
                },
                joint: Some(Joint {
                    name: "wheel_turn".into(),
                    kind: JointKind::Revolute,
                    pivot,
                    value: 0.25,
                    range: Some((-1.5, 2.5)),
                    ..Default::default()
                }),
                ..Part::new("wheel", &wheel)
            },
            Part {
                parent: "plate".into(),
                joint: Some(Joint {
                    name: "ball_lift".into(),
                    kind: JointKind::Prismatic,
                    pivot: Frame::IDENTITY,
                    axis: DVec3::new(0.0, 1.0, 1.0),
                    mimic: Some(Mimic {
                        joint: "wheel_turn".into(),
                        multiplier: -4.0,
                    }),
                    ..Default::default()
                }),
                ..Part::new("ball", &ball)
            },
        ],
    };
    if let Err(e) = run(h, &scene) {
        h.str(&format!("error: {e}"));
    }
    Ok(())
}

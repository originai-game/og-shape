//! Previews of the generators added after the phase 1 rebuilds, for a
//! person to look at: `cargo run --release -p shape-tools --example
//! gen2_preview -- <dir>`.

use shape_2d::{Circle, Rect, circle, rect};
use shape_core::{DVec2, DVec3};
use shape_gen::body::{Body, View, body};
use shape_gen::parts::{GearDesc, Knurl, gear_solid, knurl};
use shape_gen::paths::{Loft, LoftCurve, loft};
use shape_gen::wire::{Wire, wire};
use shape_mesh::Solid;
use shape_tools::{Material, Part, Preview, Scene, png, preview, sheet};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let disc = |r: f64| {
        circle(&Circle {
            radius: r,
            segments: 0,
        })
    };
    let plate = |s: f64| {
        rect(&Rect {
            width: s,
            height: s,
            radius: 4.0,
        })
    };
    let solids: Vec<(&str, Solid)> = vec![
        (
            "loft_holes",
            loft(&Loft {
                sections: vec![
                    (plate(40.0)?.subtract(&disc(6.0)?), 0.0),
                    (
                        disc(14.0)?.subtract(&disc(4.0)?.translate(DVec2::new(2.0, 0.0))),
                        20.0,
                    ),
                    (plate(24.0)?.subtract(&disc(5.0)?), 34.0),
                ],
                samples: 64,
                curve: LoftCurve::Spline,
                ..Default::default()
            })?,
        ),
        (
            "body",
            body(&Body {
                plan: View::spline(&[[0.0, 0.0], [30.0, 24.0], [90.0, 18.0], [120.0, 2.0]]),
                top: View::spline(&[[0.0, 0.0], [40.0, 22.0], [120.0, 1.0]]),
                bottom: View::spline(&[[0.0, 0.0], [40.0, -14.0], [120.0, -1.0]]),
                ..Default::default()
            })?,
        ),
        (
            "car",
            body(&Body {
                plan: View::points(&[[0.0, 160.0], [400.0, 170.0]]),
                top: View::points(&[[0.0, 70.0], [120.0, 80.0], [120.0, 130.0], [400.0, 125.0]]),
                bottom: View::constant(20.0),
                n: View::points(&[[0.0, 3.0], [400.0, 5.0]]),
            })?,
        ),
        (
            "wire",
            wire(&Wire {
                path: vec![
                    DVec3::ZERO,
                    DVec3::new(0.0, 0.0, 30.0),
                    DVec3::new(20.0, 5.0, 40.0),
                    DVec3::new(20.0, 25.0, 40.0),
                ],
                diameter: 4.0,
                fillets: vec![0.0, 0.0, 6.0, 0.0],
                ..Default::default()
            })?,
        ),
        (
            "knurl",
            knurl(&Knurl {
                diameter: 16.0,
                height: 12.0,
                ..Default::default()
            })?,
        ),
        (
            "gear",
            gear_solid(&GearDesc {
                teeth: 24,
                module: 1.5,
                width: 6.0,
                bore: 5.0,
                hub: 14.0,
                hub_height: 4.0,
                ..Default::default()
            })?,
        ),
    ];
    let mut images = Vec::new();
    for (name, s) in &solids {
        let scene = Scene {
            name: (*name).into(),
            parts: vec![Part {
                name: (*name).into(),
                solid: s,
                material: Material::default(),
                parent: String::new(),
                joint: None,
                tangents: false,
            }],
        };
        let views = preview(
            &scene,
            &Preview {
                width: 320,
                height: 320,
                views: 1,
                azimuth: 0.6,
                ..Default::default()
            },
        )?;
        images.extend(views);
    }
    std::fs::write(format!("{dir}/gen2.png"), png(&sheet(&images, 3)))?;
    let k = &solids[4].1;
    let close = preview(
        &Scene {
            name: "k".into(),
            parts: vec![Part {
                name: "k".into(),
                solid: k,
                material: Material::default(),
                parent: String::new(),
                joint: None,
                tangents: false,
            }],
        },
        &Preview {
            width: 900,
            height: 900,
            views: 1,
            elevation: Some(0.1),
            no_edges: true,
            ..Default::default()
        },
    )?;
    std::fs::write(format!("{dir}/knurl.png"), png(&close[0]))?;
    Ok(())
}

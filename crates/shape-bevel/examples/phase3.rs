//! Writes the previews of the bevel cases `docs/research.md` §7.4 adds:
//! one PNG per case, four views, of a close-up (the result intersected
//! with a cube around the vertex that matters) or of the whole solid.
//!
//! `cargo run --release -p shape-bevel --example phase3 -- docs/previews/phase3`

use shape_bevel::{Chamfer, Fillet, Miter, chamfer, fillet, quad_pairs};
use shape_bool::{BoolOp, Boolean, boolean};
use shape_core::{DVec2, DVec3};
use shape_gen::solids::{
    Axis, BoxDesc, CylinderDesc, Extrude, Revolve, cuboid, cylinder, extrude, revolve,
};
use shape_mesh::Solid;
use shape_tools::{Part, Preview, Scene, png, preview, sheet};

type Error = Box<dyn std::error::Error>;

fn prism(corners: &[(f64, f64)], d: &Extrude) -> Result<Solid, Error> {
    Ok(extrude(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(
            corners.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
        )),
        d,
    )?)
}

fn tag(s: &Solid, name: &str, pick: impl Fn(DVec3) -> bool) -> Solid {
    let mut out = s.clone();
    for (a, b) in s.real_edges() {
        if pick(s.positions[a as usize]) && pick(s.positions[b as usize]) {
            out.tag_edge(a, b, &[name]);
        }
    }
    out
}

fn round(r: f64, edges: &[&str], segments: u32) -> Fillet {
    Fillet {
        radius: r,
        edges: edges.iter().map(|s| s.to_string()).collect(),
        segments,
        ..Default::default()
    }
}

/// The part of `s` inside the cube of half-size `r` about `c`.
fn window(s: &Solid, c: DVec3, r: f64) -> Result<Solid, Error> {
    let mut b = cuboid(&BoxDesc {
        width: 2.0 * r,
        depth: 2.0 * r,
        height: 2.0 * r,
    })?;
    b.map_positions(|p| p + c - DVec3::new(0.0, 0.0, r), false);
    Ok(boolean(
        s,
        &b,
        &Boolean {
            op: BoolOp::Intersect,
            ..Default::default()
        },
    )?)
}

struct Case {
    name: &'static str,
    solid: Solid,
    /// Close-up centre and half-size.
    focus: Option<(DVec3, f64)>,
    azimuth: f64,
}

fn cases() -> Result<Vec<Case>, Error> {
    let mut out = Vec::new();
    let square = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    let pyramid = prism(
        &square,
        &Extrude {
            height: 20.0,
            taper: 1.0,
            ..Default::default()
        },
    )?;
    let apex = Some((DVec3::new(0.0, 0.0, 17.0), 4.0));
    let pair = tag(&pyramid, "pair", |p| {
        p.x >= 0.0 && (p.z == 20.0 || p.z == 0.0)
    });
    out.push(Case {
        name: "apex_two_of_four",
        solid: fillet(&pair, &round(1.5, &["pair"], 6))?,
        focus: apex,
        azimuth: 0.4,
    });
    let one = tag(&pyramid, "one", |p| {
        p.z == 20.0 || (p.z == 0.0 && p.x > 0.0 && p.y > 0.0)
    });
    out.push(Case {
        name: "apex_one_of_four",
        solid: fillet(&one, &round(1.5, &["one"], 6))?,
        focus: apex,
        azimuth: 0.8,
    });
    let c = cylinder(&CylinderDesc {
        radius: 15.0,
        radius_top: 0.0,
        height: 20.0,
    })?;
    let arc = tag(&c, "arc", |p| p.z == 20.0 && p.x > 0.0 && p.y > 0.0);
    // The arc's end nearest +y, where it tapers to a tip.
    let end = arc
        .edges(&["arc"])?
        .iter()
        .flat_map(|&(a, b)| [a, b])
        .map(|v| arc.positions[v as usize])
        .fold(DVec3::ZERO, |m, p| if p.y > m.y { p } else { m });
    out.push(Case {
        name: "rim_tip",
        solid: fillet(&arc, &round(1.5, &["arc"], 6))?,
        focus: Some((end - DVec3::new(0.0, 0.0, 2.0), 3.5)),
        azimuth: 1.2,
    });
    let l = prism(
        &[
            (0.0, 0.0),
            (30.0, 0.0),
            (30.0, 10.0),
            (10.0, 10.0),
            (10.0, 30.0),
            (0.0, 30.0),
        ],
        &Extrude {
            height: 10.0,
            ..Default::default()
        },
    )?;
    let reflex = Some((DVec3::new(10.0, 10.0, 7.0), 5.0));
    for (name, inner, outer, focus) in [
        ("miter_sharp", Miter::Sharp, Miter::Sharp, reflex),
        ("miter_outer_arc", Miter::Sharp, Miter::Arc, reflex),
        ("miter_outer_patch", Miter::Sharp, Miter::Patch, reflex),
        (
            "miter_inner_arc",
            Miter::Arc,
            Miter::Sharp,
            Some((DVec3::new(30.0, 0.0, 7.0), 5.0)),
        ),
    ] {
        out.push(Case {
            name,
            solid: fillet(
                &l,
                &Fillet {
                    inner_miter: inner,
                    outer_miter: outer,
                    ..round(2.0, &["top"], 6)
                },
            )?,
            focus,
            azimuth: 2.4,
        });
    }
    let block = cuboid(&BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    })?;
    out.push(Case {
        name: "miter_inner_arc_box_corner",
        solid: fillet(
            &block,
            &Fillet {
                inner_miter: Miter::Arc,
                ..round(3.0, &[], 6)
            },
        )?,
        focus: Some((DVec3::new(20.0, -15.0, 16.0), 6.0)),
        azimuth: 0.2,
    });
    let seamed = prism(
        &[
            (-20.0, -15.0),
            (0.0, -15.0),
            (20.0, -15.0),
            (20.0, 15.0),
            (-20.0, 15.0),
        ],
        &Extrude {
            height: 20.0,
            ..Default::default()
        },
    )?;
    let seamed = tag(&seamed, "seam", |p| p.x == 0.0 && p.y == -15.0);
    out.push(Case {
        name: "pipe_joint",
        solid: fillet(&seamed, &round(3.0, &["top", "seam"], 8))?,
        focus: Some((DVec3::new(0.0, -15.0, 16.0), 6.0)),
        azimuth: 0.3,
    });
    let quad = prism(
        &[(0.0, 0.0), (30.0, 0.0), (24.0, 20.0), (-6.0, 14.0)],
        &Extrude {
            height: 10.0,
            taper: 0.3,
            ..Default::default()
        },
    )?;
    for (name, slide) in [("averaged", false), ("loop_slide", true)] {
        out.push(Case {
            name,
            solid: chamfer(
                &quad,
                &Chamfer {
                    distance: 2.0,
                    edges: vec!["top".into()],
                    loop_slide: slide,
                    ..Default::default()
                },
            )?,
            focus: None,
            azimuth: -0.6,
        });
    }
    let big = cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })?;
    let mut small = cuboid(&BoxDesc {
        width: 8.0,
        depth: 8.0,
        height: 10.0,
    })?;
    small.map_positions(|p| p + DVec3::new(3.0, 2.0, 8.0), false);
    let joined = boolean(&big, &small, &Boolean::default())?;
    out.push(Case {
        name: "join_seams",
        solid: fillet(&joined, &round(1.0, &[], 6))?,
        focus: None,
        azimuth: -0.6,
    });
    let section = shape_2d::Profile::from_contour(shape_2d::Contour::corners(vec![
        DVec2::new(5.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(10.0, 8.0),
        DVec2::new(5.0, 8.0),
    ]));
    let ring = revolve(
        &section,
        &Revolve {
            angle: 2.0,
            axis: Axis::Z,
        },
    )?;
    out.push(Case {
        name: "part_revolve",
        solid: fillet(&ring, &round(0.3, &[], 6))?,
        focus: None,
        azimuth: -0.6,
    });
    for (name, twist, taper) in [("twisted", 1.0, 0.0), ("tapered", 0.0, 0.5)] {
        let s = prism(
            &square,
            &Extrude {
                height: 20.0,
                twist,
                taper,
                ..Default::default()
            },
        )?;
        out.push(Case {
            name,
            solid: fillet(&s, &round(1.5, &[], 6))?,
            focus: None,
            azimuth: -0.6,
        });
    }
    Ok(out)
}

fn main() -> Result<(), Error> {
    let dir = std::env::args().nth(1).ok_or("usage: phase3 <directory>")?;
    std::fs::create_dir_all(&dir)?;
    for case in cases()? {
        let shown = match case.focus {
            Some((c, r)) => window(&case.solid, c, r)?,
            None => case.solid.clone(),
        };
        let views = preview(
            &Scene {
                name: case.name.into(),
                parts: vec![Part::new(case.name, &shown)],
            },
            &Preview {
                width: 320,
                views: 4,
                azimuth: case.azimuth,
                elevation: Some(0.6),
                ..Default::default()
            },
        )?;
        let bytes = png(&sheet(&views, 2));
        std::fs::write(format!("{dir}/{}.png", case.name), &bytes)?;
        let m = case.solid.measure();
        let new = case
            .solid
            .faces(&["fillet"])
            .or_else(|_| case.solid.faces(&["chamfer"]))?;
        let q = quad_pairs(&case.solid, &new);
        println!(
            "{}: closed {}, volume {:.3}, quads {}/{}, worst fold {:.1}°, PNG {} bytes",
            case.name,
            m.closed,
            m.volume,
            q.paired,
            q.triangles,
            q.max_fold.to_degrees(),
            bytes.len()
        );
    }
    Ok(())
}

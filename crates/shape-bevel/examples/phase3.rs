//! Writes the previews of the bevel cases `docs/research.md` §7.4, §7.5,
//! §7.6, §7.7 and §7.8 add:
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
        ..Default::default()
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
            ..Default::default()
        },
    )?;
    out.push(Case {
        name: "part_revolve",
        solid: fillet(&ring, &round(0.3, &[], 6))?,
        focus: None,
        azimuth: -0.6,
    });
    // The twisted prism's `fillet(1.5)` on every edge is refused since
    // stream W (`docs/research.md` §7.7): its rows tilted.
    let tapered = prism(
        &square,
        &Extrude {
            height: 20.0,
            taper: 0.5,
            ..Default::default()
        },
    )?;
    out.push(Case {
        name: "tapered",
        solid: fillet(&tapered, &round(1.5, &[], 6))?,
        focus: None,
        azimuth: -0.6,
    });
    stream_p(&mut out)?;
    stream_q(&mut out)?;
    stream_w(&mut out)?;
    stream_x(&mut out)?;
    Ok(out)
}

/// The cases of `docs/research.md` §7.5.
fn stream_p(out: &mut Vec<Case>) -> Result<(), Error> {
    let a = cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })?;
    let mut b = a.clone();
    b.map_positions(|p| p + DVec3::new(20.0, 20.0, 10.0), false);
    let touch = boolean(&a, &b, &Boolean::default())?;
    out.push(Case {
        name: "two_cones",
        solid: fillet(&touch, &round(2.0, &[], 6))?,
        focus: Some((DVec3::new(10.0, 10.0, 8.0), 6.0)),
        azimuth: 0.6,
    });
    let mut bore = cylinder(&CylinderDesc {
        radius: 4.0,
        height: 30.0,
        ..Default::default()
    })?;
    bore.map_positions(|p| p + DVec3::new(0.0, 0.0, -10.0), false);
    let bored = boolean(
        &a,
        &bore,
        &Boolean {
            op: BoolOp::Cut,
            ..Default::default()
        },
    )?;
    out.push(Case {
        name: "bored_chamfer",
        solid: chamfer(
            &bored,
            &Chamfer {
                distance: 0.5,
                edges: vec!["side".into()],
                ..Default::default()
            },
        )?,
        focus: None,
        azimuth: -0.6,
    });
    let seven: Vec<(f64, f64)> = (0..7)
        .map(|i| {
            let t = shape_core::trig::TAU * i as f64 / 7.0;
            (
                10.0 * shape_core::trig::cos(t),
                10.0 * shape_core::trig::sin(t),
            )
        })
        .collect();
    let pyr = prism(
        &seven,
        &Extrude {
            height: 20.0,
            taper: 1.0,
            ..Default::default()
        },
    )?;
    out.push(Case {
        name: "apex_seven",
        solid: fillet(&pyr, &round(1.5, &["side"], 6))?,
        focus: Some((DVec3::new(0.0, 0.0, 15.0), 6.0)),
        azimuth: 0.3,
    });
    let part = |c: &[(f64, f64)]| -> Result<Solid, Error> {
        Ok(revolve(
            &shape_2d::Profile::from_contour(shape_2d::Contour::corners(
                c.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
            )),
            &Revolve {
                angle: 2.0,
                axis: Axis::Z,
                ..Default::default()
            },
        )?)
    };
    let radius = |p: DVec3| (p.x * p.x + p.y * p.y).sqrt();
    let ring = part(&[
        (5.0, 0.0),
        (10.0, 0.0),
        (10.0, 4.0),
        (10.0, 8.0),
        (5.0, 8.0),
    ])?;
    let ring = tag(&ring, "seam", |p| {
        (radius(p) - 10.0).abs() < 1e-9 && p.z == 4.0
    });
    let ring = tag(&ring, "post", |p| {
        p.y.abs() < 1e-12 && (radius(p) - 10.0).abs() < 1e-9
    });
    out.push(Case {
        name: "pipe_curved",
        solid: fillet(&ring, &round(1.0, &["seam", "post"], 8))?,
        focus: Some((DVec3::new(10.0, 0.0, 2.5), 3.0)),
        azimuth: -1.2,
    });
    let cone = part(&[(5.0, 0.0), (10.0, 0.0), (7.0, 8.0), (5.0, 8.0)])?;
    let cone = tag(&cone, "lid", |p| p.z == 8.0);
    out.push(Case {
        name: "cone_loop_slide",
        solid: fillet(
            &cone,
            &Fillet {
                loop_slide: true,
                ..round(0.3, &["lid"], 6)
            },
        )?,
        focus: Some((DVec3::new(8.5, 0.0, 6.5), 2.5)),
        azimuth: -1.2,
    });
    let curvy = extrude(
        &shape_2d::Path::new(0.0, 0.0)
            .line_to(30.0, 0.0)
            .tangent_arc_to(30.0, 10.0)
            .line_to(10.0, 10.0)
            .line_to(10.0, 30.0)
            .arc_to(5.0, 35.0, 0.0, 30.0)
            .close()?,
        &Extrude {
            height: 10.0,
            ..Default::default()
        },
    )?;
    out.push(Case {
        name: "curvy_patch_chamfer",
        solid: chamfer(
            &curvy,
            &Chamfer {
                distance: 2.0,
                edges: vec!["top".into()],
                segments: 3,
                outer_miter: Miter::Patch,
                ..Default::default()
            },
        )?,
        focus: Some((DVec3::new(10.0, 10.0, 7.0), 5.0)),
        azimuth: 2.4,
    });
    Ok(())
}

/// The cases of `docs/research.md` §7.6: bevels wider than the segments
/// of their faces.
fn stream_q(out: &mut Vec<Case>) -> Result<(), Error> {
    let ring = revolve(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(vec![
            DVec2::new(5.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 8.0),
            DVec2::new(5.0, 8.0),
        ])),
        &Revolve {
            angle: 2.0,
            axis: Axis::Z,
            ..Default::default()
        },
    )?;
    out.push(Case {
        name: "wide_bore",
        solid: fillet(&ring, &round(1.5, &[], 4))?,
        focus: Some((DVec3::new(5.5, 0.0, 6.0), 3.0)),
        azimuth: 2.6,
    });
    let plate = cuboid(&BoxDesc {
        width: 30.0,
        depth: 30.0,
        height: 10.0,
    })?;
    let holed = shape_edit::hole(
        &plate,
        &shape_edit::Hole {
            diameter: 6.0,
            at: vec![shape_edit::above(0.0, 0.0)],
            ..Default::default()
        },
    )?;
    out.push(Case {
        name: "wide_hole_rim",
        solid: fillet(&holed, &round(2.5, &["hole"], 4))?,
        focus: Some((DVec3::new(0.0, 0.0, 6.0), 6.0)),
        azimuth: 0.4,
    });
    let pie = revolve(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(5.0, 0.0),
            DVec2::new(5.0, 8.0),
            DVec2::new(0.0, 8.0),
        ])),
        &Revolve {
            angle: 2.0,
            axis: Axis::Z,
            segments: 24,
        },
    )?;
    out.push(Case {
        name: "wide_cylinder_slice",
        solid: chamfer(
            &pie,
            &Chamfer {
                distance: 1.5,
                segments: 3,
                ..Default::default()
            },
        )?,
        focus: None,
        azimuth: 2.2,
    });
    let square = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    let twisted = prism(
        &square,
        &Extrude {
            height: 20.0,
            twist: 1.0,
            ..Default::default()
        },
    )?;
    out.push(Case {
        name: "wide_twisted_top",
        solid: fillet(&twisted, &round(5.0, &["top"], 4))?,
        focus: None,
        azimuth: 0.6,
    });
    Ok(())
}

/// The cases of `docs/research.md` §7.7: results that crossed themselves
/// or tilted faces, now fitted.
fn stream_w(out: &mut Vec<Case>) -> Result<(), Error> {
    let square = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    let twisted = prism(
        &square,
        &Extrude {
            height: 20.0,
            twist: 1.0,
            ..Default::default()
        },
    )?;
    out.push(Case {
        name: "twisted_top",
        solid: fillet(&twisted, &round(1.5, &["top"], 4))?,
        focus: None,
        azimuth: 0.6,
    });
    out.push(Case {
        name: "twisted_top_corner",
        solid: chamfer(
            &twisted,
            &Chamfer {
                distance: 1.0,
                edges: vec!["top".into()],
                ..Default::default()
            },
        )?,
        focus: Some((DVec3::new(-13.8, -3.0, 18.0), 3.0)),
        azimuth: 2.6,
    });
    let plate = cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })?;
    let mut pin = cylinder(&CylinderDesc {
        radius: 4.0,
        height: 30.0,
        ..Default::default()
    })?;
    pin.map_positions(|p| p + DVec3::new(0.0, 0.0, -10.0), false);
    let bore = boolean(
        &plate,
        &pin,
        &Boolean {
            op: BoolOp::Cut,
            ..Default::default()
        },
    )?;
    let mut bottom = chamfer(
        &bore,
        &Chamfer {
            distance: 1.5,
            edges: vec!["bottom".into()],
            ..Default::default()
        },
    )?;
    // Turned over (a half turn about x), so the bottom faces the camera.
    bottom.map_positions(|p| DVec3::new(p.x, -p.y, -p.z), false);
    out.push(Case {
        name: "bore_bottom_chamfer",
        solid: bottom,
        focus: None,
        azimuth: 0.5,
    });
    Ok(())
}

/// Lone pieces tapering to tips at both ends, and faces cut on their
/// folds (`docs/research.md` §7.8).
fn stream_x(out: &mut Vec<Case>) -> Result<(), Error> {
    let pin = cylinder(&CylinderDesc {
        radius: 5.0,
        height: 10.0,
        ..Default::default()
    })?;
    let piece = tag(&pin, "pick", |p| p.z == 10.0 && p.x > 4.8 && p.y >= 0.0);
    out.push(Case {
        name: "lone_rim_piece",
        solid: fillet(&piece, &round(1.5, &["pick"], 4))?,
        focus: Some((DVec3::new(5.0, 0.6, 9.0), 1.5)),
        azimuth: 0.3,
    });
    let slot = extrude(
        &shape_2d::rect(&shape_2d::Rect {
            width: 30.0,
            height: 20.0,
            radius: 4.0,
        })?,
        &Extrude {
            height: 6.0,
            ..Default::default()
        },
    )?;
    let straight = tag(&slot, "pick", |p| p.z == 6.0 && p.y == -10.0);
    out.push(Case {
        name: "lone_straight_edge",
        solid: chamfer(
            &straight,
            &Chamfer {
                distance: 1.5,
                edges: vec!["pick".into()],
                ..Default::default()
            },
        )?,
        focus: None,
        azimuth: 0.3,
    });
    let rr = |w: f64, h: f64, r: f64| {
        shape_2d::rect(&shape_2d::Rect {
            width: w,
            height: h,
            radius: r,
        })
    };
    let housing = shape_gen::loft::loft(&shape_gen::loft::Loft {
        sections: vec![
            (rr(155.0, 285.0, 30.0)?, 0.0),
            (rr(151.0, 281.0, 32.0)?, 61.25),
            (rr(125.0, 275.0, 24.0)?, 175.0),
        ],
        ..Default::default()
    })?;
    out.push(Case {
        name: "loft_top_folds",
        solid: fillet(&housing, &round(14.0, &["top"], 0))?,
        focus: None,
        azimuth: 0.6,
    });
    let square = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    let twisted = prism(
        &square,
        &Extrude {
            height: 20.0,
            twist: 1.0,
            ..Default::default()
        },
    )?;
    out.push(Case {
        name: "twisted_all",
        solid: fillet(&twisted, &round(1.5, &[], 4))?,
        focus: None,
        azimuth: 0.6,
    });
    Ok(())
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

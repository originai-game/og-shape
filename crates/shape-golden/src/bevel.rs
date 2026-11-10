//! Golden cases for fillet, chamfer and soften (shape-bevel).

use shape_bevel::{Chamfer, Fillet, Miter, Soften, chamfer, fillet, soften};
use shape_core::{DVec2, DVec3};
use shape_core::{HashError, OutputHash, trig};
use shape_edit::{Hole, above, hole};
use shape_gen::solids::{
    Axis, BoxDesc, CylinderDesc, Extrude, Revolve, cuboid, cylinder, extrude, revolve,
};
use shape_mesh::Solid;

use crate::cases::Case;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "bevel.box",
            run: boxes,
        },
        Case {
            name: "bevel.curved",
            run: curved,
        },
        Case {
            name: "bevel.sand3d",
            run: sand3d,
        },
        Case {
            name: "bevel.adj",
            run: adj,
        },
        Case {
            name: "bevel.gaps",
            run: gaps,
        },
        Case {
            name: "bevel.miters",
            run: miters,
        },
        Case {
            name: "bevel.pipe",
            run: pipe,
        },
        Case {
            name: "bevel.slide",
            run: slide,
        },
        Case {
            name: "bevel.slivers",
            run: slivers,
        },
        Case {
            name: "bevel.cones",
            run: cones,
        },
        Case {
            name: "bevel.inline",
            run: inline,
        },
        Case {
            name: "bevel.chamfers",
            run: chamfers,
        },
        Case {
            name: "bevel.untested",
            run: untested,
        },
        Case {
            name: "bevel.wide",
            run: wide,
        },
        Case {
            name: "bevel.sound",
            run: sound,
        },
        Case {
            name: "bevel.tips",
            run: tips,
        },
        Case {
            name: "bevel.folds",
            run: folds,
        },
    ]
}

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn block() -> Solid {
    cuboid(&BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    })
    .unwrap_or_default()
}

fn solid<E: std::fmt::Display>(h: &mut OutputHash, s: Result<Solid, E>) -> Result<(), HashError> {
    match s {
        Ok(s) => s.hash_into(h),
        Err(e) => {
            h.str(&format!("error: {e}"));
            Ok(())
        }
    }
}

fn boxes(h: &mut OutputHash) -> Result<(), HashError> {
    let b = block();
    solid(
        h,
        fillet(
            &b,
            &Fillet {
                radius: 3.0,
                edges: names(&["top"]),
                ..Default::default()
            },
        ),
    )?;
    solid(
        h,
        chamfer(
            &b,
            &Chamfer {
                distance: 2.0,
                distance2: 4.0,
                edges: names(&["top"]),
                ..Default::default()
            },
        ),
    )?;
    solid(
        h,
        soften(
            &b,
            &Soften {
                radius: 2.0,
                ..Default::default()
            },
        ),
    )?;
    // Too large: the error is the output.
    solid(
        h,
        fillet(
            &b,
            &Fillet {
                radius: 16.0,
                edges: names(&["top"]),
                ..Default::default()
            },
        ),
    )
}

fn curved(h: &mut OutputHash) -> Result<(), HashError> {
    let c = cylinder(&CylinderDesc {
        radius: 15.0,
        radius_top: 0.0,
        height: 20.0,
        ..Default::default()
    })
    .unwrap_or_default();
    solid(
        h,
        fillet(
            &c,
            &Fillet {
                radius: 2.0,
                edges: names(&["top"]),
                profile: 0.7,
                ..Default::default()
            },
        ),
    )?;
    solid(
        h,
        chamfer(
            &c,
            &Chamfer {
                distance: 2.0,
                angle: trig::to_radians(30.0),
                edges: names(&["top"]),
                ..Default::default()
            },
        ),
    )
}

fn sand3d(h: &mut OutputHash) -> Result<(), HashError> {
    // The bracket.
    let plate = shape_2d::rect(&shape_2d::Rect {
        width: 80.0,
        height: 40.0,
        radius: 4.0,
    })
    .map_err(|_| HashError::NaN)?;
    let plate = extrude(
        &plate,
        &Extrude {
            height: 6.0,
            ..Default::default()
        },
    )
    .unwrap_or_default();
    let holes = |s: &Solid, d: f64, at| {
        hole(
            s,
            &Hole {
                diameter: d,
                at,
                ..Default::default()
            },
        )
        .unwrap_or_default()
    };
    let bracket = holes(&plate, 6.6, vec![above(-25.0, 0.0), above(25.0, 0.0)]);
    solid(
        h,
        fillet(
            &bracket,
            &Fillet {
                radius: 1.0,
                edges: names(&["top"]),
                ..Default::default()
            },
        ),
    )?;
    // The washer plate.
    let p = cuboid(&BoxDesc {
        width: 60.0,
        depth: 40.0,
        height: 8.0,
    })
    .unwrap_or_default();
    solid(
        h,
        chamfer(
            &holes(&p, 10.0, vec![above(0.0, 0.0)]),
            &Chamfer {
                distance: 1.0,
                edges: names(&["hole"]),
                ..Default::default()
            },
        ),
    )?;
    // Enclosure corners.
    solid(
        h,
        fillet(
            &block(),
            &Fillet {
                radius: 4.0,
                edges: names(&["bottom", "side"]),
                ..Default::default()
            },
        ),
    )
}

fn adj(h: &mut OutputHash) -> Result<(), HashError> {
    let square = shape_2d::rect(&shape_2d::Rect {
        width: 20.0,
        height: 20.0,
        radius: 0.0,
    })
    .map_err(|_| HashError::NaN)?;
    let pyramid = extrude(
        &square,
        &Extrude {
            height: 20.0,
            taper: 1.0,
            ..Default::default()
        },
    )
    .unwrap_or_default();
    for segments in [3, 4, 6] {
        solid(
            h,
            fillet(
                &pyramid,
                &Fillet {
                    radius: 2.0,
                    edges: names(&["side"]),
                    segments,
                    ..Default::default()
                },
            ),
        )?;
    }
    Ok(())
}

fn prism(corners: &[(f64, f64)], d: &Extrude) -> Solid {
    extrude(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(
            corners.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
        )),
        d,
    )
    .unwrap_or_default()
}

/// Names the real edges whose ends both pass `pick`.
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
        edges: names(edges),
        segments,
        ..Default::default()
    }
}

/// Unbevelled edges between bevelled ones, and tips (`docs/research.md`
/// §7.4).
fn gaps(h: &mut OutputHash) -> Result<(), HashError> {
    let square = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    let pyramid = prism(
        &square,
        &Extrude {
            height: 20.0,
            taper: 1.0,
            ..Default::default()
        },
    );
    // Two adjacent side edges, then one: the faces past them are cut on
    // profiles around the apex.
    let pair = tag(&pyramid, "pair", |p| {
        p.x >= 0.0 && (p.z == 20.0 || p.z == 0.0)
    });
    let one = tag(&pyramid, "one", |p| {
        p.z == 20.0 || (p.z == 0.0 && p.x > 0.0 && p.y > 0.0)
    });
    solid(h, fillet(&pair, &round(1.5, &["pair"], 4)))?;
    solid(h, fillet(&one, &round(1.5, &["one"], 4)))?;
    solid(
        h,
        chamfer(
            &pair,
            &Chamfer {
                distance: 1.5,
                edges: names(&["pair"]),
                ..Default::default()
            },
        ),
    )?;
    // Part of a cylinder's rim: tips where it meets the rest.
    let c = cylinder(&CylinderDesc {
        radius: 15.0,
        radius_top: 0.0,
        height: 20.0,
        ..Default::default()
    })
    .unwrap_or_default();
    let arc = tag(&c, "arc", |p| p.z == 20.0 && p.x > 0.0 && p.y > 0.0);
    solid(h, fillet(&arc, &round(1.5, &["arc"], 4)))
}

fn l_prism() -> Solid {
    prism(
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
    )
}

fn miters(h: &mut OutputHash) -> Result<(), HashError> {
    let l = l_prism();
    for (inner, outer, spread) in [
        (Miter::Arc, Miter::Sharp, 0.0),
        (Miter::Sharp, Miter::Arc, 0.0),
        (Miter::Arc, Miter::Patch, 1.5),
        // A patch miter needs a reflex corner: the error is the output.
        (Miter::Patch, Miter::Sharp, 0.0),
    ] {
        solid(
            h,
            fillet(
                &l,
                &Fillet {
                    inner_miter: inner,
                    outer_miter: outer,
                    spread,
                    ..round(2.0, &["top"], 4)
                },
            ),
        )?;
    }
    solid(
        h,
        chamfer(
            &l,
            &Chamfer {
                distance: 2.0,
                edges: names(&["top"]),
                segments: 2,
                inner_miter: Miter::Arc,
                outer_miter: Miter::Patch,
                ..Default::default()
            },
        ),
    )?;
    solid(
        h,
        fillet(
            &block(),
            &Fillet {
                inner_miter: Miter::Arc,
                ..round(3.0, &[], 4)
            },
        ),
    )
}

fn pipe(h: &mut OutputHash) -> Result<(), HashError> {
    let b = prism(
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
    );
    let b = tag(&b, "seam", |p| p.x == 0.0 && p.y == -15.0);
    solid(h, fillet(&b, &round(3.0, &["top", "seam"], 8)))
}

fn slide(h: &mut OutputHash) -> Result<(), HashError> {
    let quad = prism(
        &[(0.0, 0.0), (30.0, 0.0), (24.0, 20.0), (-6.0, 14.0)],
        &Extrude {
            height: 10.0,
            taper: 0.3,
            ..Default::default()
        },
    );
    solid(
        h,
        chamfer(
            &quad,
            &Chamfer {
                distance: 2.0,
                edges: names(&["top"]),
                loop_slide: true,
                ..Default::default()
            },
        ),
    )?;
    solid(
        h,
        fillet(
            &quad,
            &Fillet {
                loop_slide: true,
                ..round(2.0, &["top"], 4)
            },
        ),
    )
}

/// Faces in slivers (a join's, a part revolve's), and stream C's
/// untested twisted and tapered extrusions.
fn slivers(h: &mut OutputHash) -> Result<(), HashError> {
    let big = cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })
    .unwrap_or_default();
    let mut small = cuboid(&BoxDesc {
        width: 8.0,
        depth: 8.0,
        height: 10.0,
    })
    .unwrap_or_default();
    small.map_positions(|p| p + DVec3::new(3.0, 2.0, 8.0), false);
    let joined =
        shape_bool::boolean(&big, &small, &shape_bool::Boolean::default()).unwrap_or_default();
    solid(h, fillet(&joined, &round(1.0, &[], 4)))?;
    solid(
        h,
        chamfer(
            &joined,
            &Chamfer {
                distance: 1.0,
                ..Default::default()
            },
        ),
    )?;
    let section = shape_2d::Profile::from_contour(shape_2d::Contour::corners(vec![
        DVec2::new(5.0, 0.0),
        DVec2::new(10.0, 0.0),
        DVec2::new(10.0, 8.0),
        DVec2::new(5.0, 8.0),
    ]));
    let ring = |angle| {
        revolve(
            &section,
            &Revolve {
                angle,
                axis: Axis::Z,
                ..Default::default()
            },
        )
        .unwrap_or_default()
    };
    for angle in [0.0, 2.0] {
        solid(h, fillet(&ring(angle), &round(0.3, &[], 4)))?;
    }
    // Wider than a segment of the bore: re-meshed since stream Q
    // (`docs/research.md` §7.6); before, this hashed the `TooLarge` error.
    solid(h, fillet(&ring(2.0), &round(1.5, &[], 4)))?;
    let square = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    for (twist, taper) in [(1.0, 0.0), (0.0, 0.5)] {
        let s = prism(
            &square,
            &Extrude {
                height: 20.0,
                twist,
                taper,
                ..Default::default()
            },
        );
        // The twisted prism (twist 1) hashes the refusal since stream W
        // (`docs/research.md` §7.7): before, its result removed 14.7 mm³
        // where the tapered one removes 101, its twisted rows tilted.
        solid(h, fillet(&s, &round(1.5, &[], 4)))?;
    }
    Ok(())
}

/// Two cones of faces at one vertex: two boxes joined at a corner
/// (`docs/research.md` §7.5).
fn cones(h: &mut OutputHash) -> Result<(), HashError> {
    let a = cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })
    .unwrap_or_default();
    let mut b = a.clone();
    b.map_positions(|p| p + DVec3::new(20.0, 20.0, 10.0), false);
    let j = shape_bool::boolean(&a, &b, &shape_bool::Boolean::default()).unwrap_or_default();
    solid(h, fillet(&j, &round(2.0, &[], 4)))?;
    let low = tag(&j, "low", |p| p.x <= 10.0 && p.y <= 10.0 && p.z <= 10.0);
    solid(
        h,
        chamfer(
            &low,
            &Chamfer {
                distance: 1.0,
                edges: names(&["low"]),
                ..Default::default()
            },
        ),
    )
}

/// A corner of zero angle (refused: no offset fits it), and an edge in
/// line with its unbevelled neighbour across a curved face
/// (`docs/research.md` §7.5).
fn inline(h: &mut OutputHash) -> Result<(), HashError> {
    let needle = prism(
        &[(0.0, 0.0), (10.0, 0.0), (10.0, 1e-9)],
        &Extrude {
            height: 5.0,
            ..Default::default()
        },
    );
    solid(h, fillet(&needle, &round(0.5, &["top"], 2)))?;
    // A block 4 × 2 whose top has a straight edge along y = 1, split at
    // (2, 1, 1): behind it one curved surface, in front two flat faces.
    let pts = [
        (0.0, 1.0, 1.0),
        (2.0, 1.0, 1.0),
        (4.0, 1.0, 1.0),
        (0.0, 2.0, 1.2),
        (2.0, 2.0, 1.5),
        (4.0, 2.0, 1.2),
        (0.0, 0.0, 1.0),
        (2.0, 0.0, 1.0),
        (4.0, 0.0, 1.0),
        (0.0, 0.0, 0.0),
        (4.0, 0.0, 0.0),
        (4.0, 2.0, 0.0),
        (0.0, 2.0, 0.0),
    ];
    let mut r = Solid::default();
    for (x, y, z) in pts {
        r.push_vertex(DVec3::new(x, y, z));
    }
    let faces: [(u32, &str, &[[u32; 3]]); 8] = [
        (0, "back", &[[1, 2, 5], [1, 5, 4], [1, 4, 3], [1, 3, 0]]),
        (1, "top", &[[1, 0, 6], [1, 6, 7]]),
        (2, "top", &[[1, 7, 8], [1, 8, 2]]),
        (3, "side", &[[9, 10, 8], [9, 8, 7], [9, 7, 6]]),
        (4, "side", &[[10, 11, 5], [10, 5, 2], [10, 2, 8]]),
        (5, "side", &[[11, 12, 3], [11, 3, 4], [11, 4, 5]]),
        (6, "side", &[[12, 9, 6], [12, 6, 0], [12, 0, 3]]),
        (7, "bottom", &[[9, 12, 11], [9, 11, 10]]),
    ];
    for (surface, name, tris) in faces {
        for &t in tris {
            r.push_triangle(t, surface, &[name]);
        }
    }
    r.tag_edge(0, 1, &["along"]);
    r.tag_edge(1, 7, &["split"]);
    // Refused since stream W (`docs/research.md` §7.7): the result lifted
    // the flat face off its plane by 0.11 of the offset at (2, 1, 1).
    solid(h, fillet(&r, &round(0.3, &["along", "split"], 4)))?;
    solid(
        h,
        chamfer(
            &r,
            &Chamfer {
                distance: 0.3,
                edges: names(&["along", "split"]),
                ..Default::default()
            },
        ),
    )
}

/// Chamfers of several segments whose straight profiles once left a
/// flat triangle on an end face (`docs/research.md` §7.5).
fn chamfers(h: &mut OutputHash) -> Result<(), HashError> {
    let ngon = |k: usize| -> Vec<(f64, f64)> {
        (0..k)
            .map(|i| {
                let a = trig::TAU * i as f64 / k as f64;
                (10.0 * trig::cos(a), 10.0 * trig::sin(a))
            })
            .collect()
    };
    let side = |seg: u32| Chamfer {
        distance: 1.5,
        edges: names(&["side"]),
        segments: seg,
        ..Default::default()
    };
    for (k, taper, seg) in [(11, 0.0, 2), (9, 0.0, 3), (7, 1.0, 2)] {
        let p = prism(
            &ngon(k),
            &Extrude {
                height: 20.0,
                taper,
                ..Default::default()
            },
        );
        solid(h, chamfer(&p, &side(seg)))?;
    }
    let b = cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })
    .unwrap_or_default();
    let mut c = cylinder(&CylinderDesc {
        radius: 4.0,
        height: 30.0,
        ..Default::default()
    })
    .unwrap_or_default();
    c.map_positions(|p| p + DVec3::new(0.0, 0.0, -10.0), false);
    let bored = shape_bool::boolean(
        &b,
        &c,
        &shape_bool::Boolean {
            op: shape_bool::BoolOp::Cut,
            ..Default::default()
        },
    )
    .unwrap_or_default();
    solid(
        h,
        chamfer(
            &bored,
            &Chamfer {
                distance: 0.5,
                ..side(1)
            },
        ),
    )
}

/// The items §7.4 left untested: a pipe through curved faces, patches of
/// seven and twelve sides, loop slide around a cone, and miters with
/// chamfers of several segments on an outline with arcs
/// (`docs/research.md` §7.5).
fn untested(h: &mut OutputHash) -> Result<(), HashError> {
    let part = |c: &[(f64, f64)]| {
        revolve(
            &shape_2d::Profile::from_contour(shape_2d::Contour::corners(
                c.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
            )),
            &Revolve {
                angle: 2.0,
                axis: Axis::Z,
                ..Default::default()
            },
        )
        .unwrap_or_default()
    };
    let radius = |p: DVec3| (p.x * p.x + p.y * p.y).sqrt();
    let ring = part(&[
        (5.0, 0.0),
        (10.0, 0.0),
        (10.0, 4.0),
        (10.0, 8.0),
        (5.0, 8.0),
    ]);
    let ring = tag(&ring, "seam", |p| {
        (radius(p) - 10.0).abs() < 1e-9 && p.z == 4.0
    });
    let ring = tag(&ring, "post", |p| {
        p.y.abs() < 1e-12 && (radius(p) - 10.0).abs() < 1e-9
    });
    solid(h, fillet(&ring, &round(1.0, &["seam", "post"], 4)))?;
    for k in [7usize, 12] {
        let ngon: Vec<(f64, f64)> = (0..k)
            .map(|i| {
                let a = trig::TAU * i as f64 / k as f64;
                (10.0 * trig::cos(a), 10.0 * trig::sin(a))
            })
            .collect();
        let pyr = prism(
            &ngon,
            &Extrude {
                height: 20.0,
                taper: 1.0,
                ..Default::default()
            },
        );
        solid(h, fillet(&pyr, &round(1.5, &["side"], 4)))?;
    }
    let cone = part(&[(5.0, 0.0), (10.0, 0.0), (7.0, 8.0), (5.0, 8.0)]);
    let cone = tag(&cone, "lid", |p| p.z == 8.0);
    solid(
        h,
        fillet(
            &cone,
            &Fillet {
                loop_slide: true,
                ..round(0.3, &["lid"], 4)
            },
        ),
    )?;
    let curvy = shape_2d::Path::new(0.0, 0.0)
        .line_to(30.0, 0.0)
        .tangent_arc_to(30.0, 10.0)
        .line_to(10.0, 10.0)
        .line_to(10.0, 30.0)
        .arc_to(5.0, 35.0, 0.0, 30.0)
        .close()
        .ok()
        .and_then(|p| {
            extrude(
                &p,
                &Extrude {
                    height: 10.0,
                    ..Default::default()
                },
            )
            .ok()
        })
        .unwrap_or_default();
    solid(
        h,
        chamfer(
            &curvy,
            &Chamfer {
                distance: 2.0,
                edges: names(&["top"]),
                segments: 3,
                outer_miter: Miter::Patch,
                ..Default::default()
            },
        ),
    )
}

/// Bevels wider than the segments of their faces (`docs/research.md`
/// §7.6): a hole's rims by the hole tool, a slice of a cylinder, and a
/// twisted prism's top edges.
fn wide(h: &mut OutputHash) -> Result<(), HashError> {
    let plate = cuboid(&BoxDesc {
        width: 30.0,
        depth: 30.0,
        height: 10.0,
    })
    .unwrap_or_default();
    let holed = hole(
        &plate,
        &Hole {
            diameter: 6.0,
            at: vec![above(0.0, 0.0)],
            ..Default::default()
        },
    )
    .unwrap_or_default();
    solid(h, fillet(&holed, &round(1.5, &["hole"], 4)))?;
    solid(h, fillet(&holed, &round(1.5, &["top"], 4)))?;
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
    )
    .unwrap_or_default();
    solid(
        h,
        chamfer(
            &pie,
            &Chamfer {
                distance: 1.5,
                segments: 3,
                ..Default::default()
            },
        ),
    )?;
    let square = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    let twisted = prism(
        &square,
        &Extrude {
            height: 20.0,
            twist: 1.0,
            ..Default::default()
        },
    );
    solid(h, fillet(&twisted, &round(5.0, &["top"], 4)))
}

/// Results that crossed themselves or were otherwise wrong, and the
/// inputs and selections now refused (`docs/research.md` §7.7).
fn sound(h: &mut OutputHash) -> Result<(), HashError> {
    let plate = cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })
    .unwrap_or_default();
    let pin = |r: f64, height: f64, z: f64| {
        let mut c = cylinder(&CylinderDesc {
            radius: r,
            height,
            ..Default::default()
        })
        .unwrap_or_default();
        c.map_positions(|p| p + DVec3::new(0.0, 0.0, z), false);
        c
    };
    let op = |b: &Solid, op| {
        shape_bool::boolean(
            &plate,
            b,
            &shape_bool::Boolean {
                op,
                ..Default::default()
            },
        )
        .unwrap_or_default()
    };
    // Crossed itself near the bore; the straight-run retry fits it.
    let bore = op(&pin(4.0, 30.0, -10.0), shape_bool::BoolOp::Cut);
    solid(
        h,
        chamfer(
            &bore,
            &Chamfer {
                distance: 1.5,
                edges: names(&["bottom"]),
                ..Default::default()
            },
        ),
    )?;
    // The input crossed itself (a boolean's sliver): welded first.
    let post = op(&pin(3.0, 8.0, 10.0), shape_bool::BoolOp::Join);
    let corner = tag(&post, "pick", |p| p.x == -10.0 && p.y == -10.0);
    solid(h, fillet(&corner, &round(0.5, &["pick"], 4)))?;
    // Four faces on one edge.
    let mut other = plate.clone();
    other.map_positions(|p| p + DVec3::new(20.0, 0.0, 10.0), false);
    let edge = op(&other, shape_bool::BoolOp::Join);
    solid(h, fillet(&edge, &round(1.0, &[], 4)))?;
    // The twisted prism's top at radii its rows' tilt broke.
    let square = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    let twisted = prism(
        &square,
        &Extrude {
            height: 20.0,
            twist: 1.0,
            ..Default::default()
        },
    );
    for r in [0.5, 2.0] {
        solid(h, fillet(&twisted, &round(r, &["top"], 4)))?;
        solid(
            h,
            chamfer(
                &twisted,
                &Chamfer {
                    distance: r,
                    edges: names(&["top"]),
                    ..Default::default()
                },
            ),
        )?;
    }
    // A strip of no width: one piece of a rim.
    let rim = tag(&pin(5.0, 10.0, 0.0), "pick", |p| {
        p.z == 10.0 && p.x > 4.8 && p.y >= 0.0
    });
    solid(h, fillet(&rim, &round(0.5, &["pick"], 4)))
}

/// Lone pieces of a crease, tapering to a tip at both ends
/// (`docs/research.md` §7.8): a short rim piece, a long straight edge
/// meeting arcs tangentially, a piece a boolean split from a rim, and one
/// whose taper does not fit (the error names it).
fn tips(h: &mut OutputHash) -> Result<(), HashError> {
    let pin = cylinder(&CylinderDesc {
        radius: 5.0,
        height: 10.0,
        ..Default::default()
    })
    .unwrap_or_default();
    let rim = tag(&pin, "pick", |p| p.z == 10.0 && p.x > 4.8 && p.y >= 0.0);
    let slot = extrude(
        &shape_2d::rect(&shape_2d::Rect {
            width: 30.0,
            height: 20.0,
            radius: 4.0,
        })
        .unwrap_or_default(),
        &Extrude {
            height: 6.0,
            ..Default::default()
        },
    )
    .unwrap_or_default();
    let straight = tag(&slot, "pick", |p| p.z == 6.0 && p.y == 10.0);
    let plate = cuboid(&BoxDesc {
        width: 30.0,
        depth: 30.0,
        height: 10.0,
    })
    .unwrap_or_default();
    let cb = hole(
        &plate,
        &Hole {
            diameter: 5.0,
            counterbore: (9.0, 3.0),
            at: vec![above(-3.0, 2.0)],
            ..Default::default()
        },
    )
    .unwrap_or_default();
    let near = |p: DVec3, x: f64, y: f64| (p - DVec3::new(x, y, 7.0)).length() < 1e-3;
    let floor = tag(&cb, "pick", |p| {
        near(p, -0.75, -1.8971) || near(p, 0.18198, -1.18198)
    });
    for s in [&rim, &straight, &floor] {
        for d in [0.5, 1.5] {
            solid(h, fillet(s, &round(d, &["pick"], 4)))?;
            solid(
                h,
                chamfer(
                    s,
                    &Chamfer {
                        distance: d,
                        edges: names(&["pick"]),
                        ..Default::default()
                    },
                ),
            )?;
        }
    }
    Ok(())
}

/// Faces whose triangles at a vertex are not one plane, cut on their folds
/// (`docs/research.md` §7.8): a loft's top (run A's toaster housing), the
/// twisted prism's sides, and the apex revolve's start cap.
fn folds(h: &mut OutputHash) -> Result<(), HashError> {
    let rr = |w: f64, d: f64, r: f64| {
        shape_2d::rect(&shape_2d::Rect {
            width: w,
            height: d,
            radius: r,
        })
        .unwrap_or_default()
    };
    let housing = shape_gen::loft::loft(&shape_gen::loft::Loft {
        sections: vec![
            (rr(155.0, 285.0, 30.0), 0.0),
            (rr(151.0, 281.0, 32.0), 61.25),
            (rr(125.0, 275.0, 24.0), 175.0),
        ],
        ..Default::default()
    })
    .unwrap_or_default();
    solid(h, fillet(&housing, &round(14.0, &["top"], 0)))?;
    let square = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    let twisted = prism(
        &square,
        &Extrude {
            height: 20.0,
            twist: 1.0,
            ..Default::default()
        },
    );
    solid(h, fillet(&twisted, &round(0.5, &["side"], 4)))?;
    solid(
        h,
        chamfer(
            &twisted,
            &Chamfer {
                distance: 0.5,
                edges: names(&["side"]),
                ..Default::default()
            },
        ),
    )?;
    let apex = revolve(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(8.0, 0.0),
            DVec2::new(8.0, 4.0),
            DVec2::new(0.0, 10.0),
        ])),
        &Revolve {
            angle: 3.0,
            axis: Axis::Z,
            ..Default::default()
        },
    )
    .unwrap_or_default();
    solid(h, fillet(&apex, &round(0.5, &["start"], 4)))
}

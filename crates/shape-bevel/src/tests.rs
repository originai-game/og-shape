use super::*;
use shape_core::DVec2;
use shape_edit::{Hole, above, hole};
use shape_gen::solids::{BoxDesc, CylinderDesc, Extrude, cuboid, cylinder, extrude};

fn block() -> Solid {
    cuboid(&BoxDesc {
        width: 40.0,
        depth: 30.0,
        height: 20.0,
    })
    .unwrap()
}

fn names(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// Closed, and every triangle named `kind` pairs into a quad.
fn check(s: &Solid, kind: &str) -> QuadPairs {
    let m = s.measure();
    assert!(m.closed && m.degenerate_triangles == 0, "{m:?}");
    let f = s.faces(&[kind]).unwrap();
    let q = quad_pairs(s, &f);
    assert_eq!(q.paired, q.triangles, "{q:?}");
    q
}

/// What an n-segment arc loses against the true quarter circle, per unit
/// length of a square edge of radius r.
fn deficit(r: f64, n: u32) -> f64 {
    r * r * (trig::PI / 4.0 - n as f64 * trig::sin(trig::PI / (2.0 * n as f64)) / 2.0)
}

#[test]
fn a_box_s_top_fillet_meets_mitred_corners_at_the_analytic_volume() {
    let r = 3.0;
    let s = fillet(
        &block(),
        &Fillet {
            radius: r,
            edges: names(&["top"]),
            segments: 32,
            ..Default::default()
        },
    )
    .unwrap();
    check(&s, "fillet");
    // Four quarter-round edges, mitred: A·perimeter − 8·∫a dA.
    let want = 24000.0 - (1.0 - trig::PI / 4.0) * r * r * 140.0
        + 8.0 * r * r * r * (5.0 / 6.0 - trig::PI / 4.0);
    let v = s.measure().volume;
    assert!(v < want && want - v < deficit(r, 32) * 140.0, "{v} {want}");
    // New faces carry only `fillet`; the vertical edges keep `side`.
    for t in s.faces(&["fillet"]).unwrap() {
        assert_eq!(s.face_tags[t as usize].len(), 1);
    }
    assert_eq!(s.edges(&["side"]).unwrap().len(), 4);
    // Default segments: the chord rule at r = 3 over a quarter turn.
    let d = fillet(
        &block(),
        &Fillet {
            radius: r,
            edges: names(&["top"]),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(d.faces(&["fillet"]).unwrap().len(), 4 * 6 * 2);
}

#[test]
fn chamfers_are_exact_and_name_their_edges() {
    let s = chamfer(
        &block(),
        &Chamfer {
            distance: 2.0,
            distance2: 4.0,
            edges: names(&["top"]),
            ..Default::default()
        },
    )
    .unwrap();
    check(&s, "chamfer");
    // 2 on the top, 4 down the walls, mitred: 4·140 − 8·(4·2/3).
    assert!((s.measure().volume - (24000.0 - 560.0 + 64.0 / 3.0)).abs() < 1e-9);
    // The cap edge is `chamfer` and `top`; the four mitres are `chamfer`.
    let top = s.edges(&["top"]).unwrap();
    assert_eq!(top.len(), 4);
    assert_eq!(s.edges(&["chamfer"]).unwrap().len(), 4 + 4 + 4);
    let z = |e: &(u32, u32)| s.positions[e.0 as usize].z;
    assert!(top.iter().all(|e| z(e) == 20.0));
}

#[test]
fn three_edge_corners_are_octants_and_soften_rounds_a_box() {
    let r = 3.0;
    let s = soften(
        &block(),
        &Soften {
            radius: r,
            segments: 32,
            ..Default::default()
        },
    )
    .unwrap();
    let q = check(&s, "fillet");
    assert!(q.max_fold < trig::to_radians(2.0), "{q:?}");
    let (a, b, c) = (34.0, 24.0, 14.0);
    let want = a * b * c
        + 2.0 * r * (a * b + b * c + a * c)
        + trig::PI * r * r * (a + b + c)
        + 4.0 / 3.0 * trig::PI * r * r * r;
    let v = s.measure().volume;
    assert!(v < want && want - v < 1.2 * deficit(r, 32) * 4.0 * (a + b + c));
    // The enclosure corner: bottom and vertical edges, three at a corner.
    let e = fillet(
        &block(),
        &Fillet {
            radius: 4.0,
            edges: names(&["bottom", "side"]),
            ..Default::default()
        },
    )
    .unwrap();
    check(&e, "fillet");
}

#[test]
fn curved_neighbours_cylinder_rims_and_hole_rims() {
    let c = cylinder(&CylinderDesc {
        radius: 15.0,
        radius_top: 0.0,
        height: 20.0,
    })
    .unwrap();
    let s = fillet(
        &c,
        &Fillet {
            radius: 2.0,
            edges: names(&["top"]),
            ..Default::default()
        },
    )
    .unwrap();
    check(&s, "fillet");
    // One smooth fillet surface: strips joined across the rim.
    let f = s.faces(&["fillet"]).unwrap();
    assert!(
        f.iter()
            .all(|&t| s.surfaces[t as usize] == s.surfaces[f[0] as usize])
    );
    // A chamfer at 30° to the top: d2 = 2·tan 30° down the wall.
    let ch = chamfer(
        &c,
        &Chamfer {
            distance: 2.0,
            angle: trig::to_radians(30.0),
            edges: names(&["top"]),
            ..Default::default()
        },
    )
    .unwrap();
    check(&ch, "chamfer");
    let low = ch
        .positions
        .iter()
        .filter(|p| p.z < 20.0 && p.z > 0.0)
        .fold(20.0f64, |m, p| m.min(p.z));
    assert!((20.0 - low - 2.0 * trig::tan(trig::to_radians(30.0))).abs() < 1e-9);
    // sand3d's washer: a plate with a hole, its rims chamfered.
    let p = cuboid(&BoxDesc {
        width: 60.0,
        depth: 40.0,
        height: 8.0,
    })
    .unwrap();
    let p = hole(
        &p,
        &Hole {
            diameter: 10.0,
            at: vec![above(0.0, 0.0)],
            ..Default::default()
        },
    )
    .unwrap();
    let before = p.measure().volume;
    let w = chamfer(
        &p,
        &Chamfer {
            distance: 1.0,
            edges: names(&["hole"]),
            ..Default::default()
        },
    )
    .unwrap();
    check(&w, "chamfer");
    // Pappus on each 24-gon rim, near enough: ½·2π(5 + 1/3) each.
    let lost = before - w.measure().volume;
    assert!(
        (lost - 2.0 * 0.5 * trig::TAU * (5.0 + 1.0 / 3.0)).abs() < 0.1,
        "{lost}"
    );
}

#[test]
fn sand3d_bracket_fillets_its_top_and_hole_rims() {
    let plate = extrude(
        &shape_2d::rect(&shape_2d::Rect {
            width: 80.0,
            height: 40.0,
            radius: 4.0,
        })
        .unwrap(),
        &Extrude {
            height: 6.0,
            ..Default::default()
        },
    )
    .unwrap();
    let plate = hole(
        &plate,
        &Hole {
            diameter: 6.6,
            at: vec![above(-25.0, 0.0), above(25.0, 0.0)],
            ..Default::default()
        },
    )
    .unwrap();
    let s = fillet(
        &plate,
        &Fillet {
            radius: 1.0,
            edges: names(&["top"]),
            ..Default::default()
        },
    )
    .unwrap();
    check(&s, "fillet");
    // The bottom rims of the holes are untouched and still named.
    assert!(!s.edges(&["bottom"]).unwrap().is_empty());
    let lost = plate.measure().volume - s.measure().volume;
    // (1 − π/4)·r² over the outer outline and two rims, ±3 %.
    let length = 2.0 * (80.0 + 40.0) - 32.0 + trig::TAU * 4.0 + 2.0 * trig::TAU * 3.3;
    let want = (1.0 - trig::PI / 4.0) * length;
    assert!((lost / want - 1.0).abs() < 0.03, "{lost} {want}");
}

#[test]
fn a_lone_edge_ends_on_its_end_faces() {
    let b = block();
    let e = b
        .edges(&["top"])
        .unwrap()
        .into_iter()
        .find(|&(x, y)| b.positions[x as usize].y == -15.0 && b.positions[y as usize].y == -15.0)
        .unwrap();
    let mut s = b.clone();
    s.tag_edge(e.0, e.1, &["lip"]);
    let s = fillet(
        &s,
        &Fillet {
            radius: 3.0,
            edges: names(&["lip"]),
            segments: 32,
            ..Default::default()
        },
    )
    .unwrap();
    check(&s, "fillet");
    let want = 24000.0 - (1.0 - trig::PI / 4.0) * 9.0 * 40.0;
    let v = s.measure().volume;
    assert!(v < want && want - v < deficit(3.0, 32) * 40.0 * 1.01);
}

#[test]
fn four_edge_corners_take_the_adj_pattern() {
    let pyramid = extrude(
        &shape_2d::rect(&shape_2d::Rect {
            width: 20.0,
            height: 20.0,
            radius: 0.0,
        })
        .unwrap(),
        &Extrude {
            height: 20.0,
            taper: 1.0,
            ..Default::default()
        },
    )
    .unwrap();
    for n in [4, 6] {
        let s = fillet(
            &pyramid,
            &Fillet {
                radius: 2.0,
                edges: names(&["side"]),
                segments: n,
                ..Default::default()
            },
        )
        .unwrap();
        let m = s.measure();
        assert!(m.closed && m.degenerate_triangles == 0);
        // The apex patch: 4 sectors of (n/2)² quads, all paired.
        let apex: Vec<u32> = s
            .faces(&["fillet"])
            .unwrap()
            .into_iter()
            .filter(|&t| {
                s.triangles[t as usize]
                    .iter()
                    .all(|&v| s.positions[v as usize].z > 15.0)
            })
            .collect();
        let q = quad_pairs(&s, &apex);
        assert!(q.paired >= 2 * 4 * (n / 2 * n / 2) as usize, "{q:?}");
    }
    // An odd count falls back to the cutoff fan.
    let odd = fillet(
        &pyramid,
        &Fillet {
            radius: 2.0,
            edges: names(&["side"]),
            segments: 3,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(odd.measure().closed);
}

#[test]
fn concave_edges_fill_in() {
    let corners = [
        (0.0, 0.0),
        (30.0, 0.0),
        (30.0, 10.0),
        (10.0, 10.0),
        (10.0, 30.0),
        (0.0, 30.0),
    ];
    let l = extrude(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(
            corners.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
        )),
        &Extrude {
            height: 10.0,
            ..Default::default()
        },
    )
    .unwrap();
    let s = fillet(
        &l,
        &Fillet {
            radius: 2.0,
            edges: names(&["side"]),
            segments: 16,
            ..Default::default()
        },
    )
    .unwrap();
    check(&s, "fillet");
    // Five convex vertical edges lose (1 − π/4)·r², the concave one gains
    // it; every edge is 10 tall.
    let want = 5000.0 - 4.0 * (1.0 - trig::PI / 4.0) * 4.0 * 10.0;
    assert!((s.measure().volume - want).abs() < 4.0 * deficit(2.0, 16) * 10.0 * 1.01);
}

#[test]
fn too_large_is_an_error_never_a_clipped_result() {
    // 30 deep: two fillets of 15 meet exactly, 16 overlap.
    for r in [15.0, 16.0] {
        let e = fillet(
            &block(),
            &Fillet {
                radius: r,
                edges: names(&["top"]),
                ..Default::default()
            },
        );
        assert!(matches!(e, Err(BevelError::TooLarge(_))), "{r}: {e:?}");
    }
    // sand3d's fillet on a chamfer: 4 down a wall left 3 tall.
    let disc = extrude(
        &shape_2d::circle(&shape_2d::Circle {
            radius: 28.5,
            segments: 0,
        })
        .unwrap(),
        &Extrude {
            height: 6.0,
            ..Default::default()
        },
    )
    .unwrap();
    let ch = chamfer(
        &disc,
        &Chamfer {
            distance: 9.0,
            distance2: 3.0,
            edges: names(&["top"]),
            ..Default::default()
        },
    )
    .unwrap();
    check(&ch, "chamfer");
    let too = Fillet {
        radius: 4.0,
        edges: names(&["chamfer"]),
        ..Default::default()
    };
    assert!(matches!(fillet(&ch, &too), Err(BevelError::TooLarge(_))));
    let ok = fillet(&ch, &Fillet { radius: 2.0, ..too }).unwrap();
    check(&ok, "fillet");
    // Unknown names list the ones that exist.
    let e = fillet(
        &block(),
        &Fillet {
            radius: 1.0,
            edges: names(&["rim"]),
            ..Default::default()
        },
    );
    assert!(
        matches!(e, Err(BevelError::NoSuchEdge(m, a)) if m == ["rim"] && a.contains(&"top".to_string()))
    );
}

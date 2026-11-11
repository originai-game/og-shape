use super::*;
use crate::bevel;
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
        ..Default::default()
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

/// The square pyramid of `four_edge_corners_take_the_adj_pattern`, its
/// side edges to the base corners with `x > 0` (and `y > 0` for `one`)
/// named `pick`.
fn pyramid(pick: &str, one: bool) -> Solid {
    let mut p = extrude(
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
    for (a, b) in p.edges(&["side"]).unwrap() {
        let base = [a, b]
            .map(|v| p.positions[v as usize])
            .into_iter()
            .find(|q| q.z == 0.0)
            .unwrap();
        if base.x > 0.0 && (!one || base.y > 0.0) {
            p.tag_edge(a, b, &[pick]);
        }
    }
    p
}

/// Every vertex of the result lies inside the convex solid it came from.
fn inside(s: &Solid, of: &Solid) {
    for t in 0..of.triangles.len() {
        let n = bevel::tri_normal(of, t);
        let c = of.positions[of.triangles[t][0] as usize];
        for p in &s.positions {
            assert!(n.dot(*p - c) < 1e-9, "{p} outside");
        }
    }
}

#[test]
fn two_unbevelled_edges_between_bevelled_ones_cut_their_face_on_a_profile() {
    // Two adjacent side edges at the apex: the other two sides are
    // unbevelled, and the face between them is cut on a profile around
    // the apex; the corner is a three-sided ADJ patch.
    let p = pyramid("pair", false);
    let s = fillet(
        &p,
        &Fillet {
            radius: 1.5,
            edges: names(&["pair"]),
            segments: 4,
            ..Default::default()
        },
    )
    .unwrap();
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
    // Three sectors of 2 × 2 quads, and the strips' last rows.
    let q = quad_pairs(&s, &apex);
    assert_eq!((q.paired, q.triangles), (24, 24), "{q:?}");
    assert!(q.max_fold < trig::to_radians(35.0), "{q:?}");
    check_closed(&s);
    inside(&s, &p);
    // The two unbevelled side edges still reach toward the apex, named.
    assert_eq!(s.edges(&["side"]).unwrap().len(), 2);
    // A lone side edge: three unbevelled edges past its end, two faces
    // cut on profiles, a three-sided patch.
    let one = pyramid("one", true);
    let s = fillet(
        &one,
        &Fillet {
            radius: 1.5,
            edges: names(&["one"]),
            segments: 4,
            ..Default::default()
        },
    )
    .unwrap();
    check_closed(&s);
    inside(&s, &one);
    let f = s.faces(&["fillet"]).unwrap();
    let q = quad_pairs(&s, &f);
    // All but the strip's twisted last quad at the base.
    assert!(q.triangles - q.paired <= 2, "{q:?}");
}

fn check_closed(s: &Solid) {
    let m = s.measure();
    assert!(m.closed && m.degenerate_triangles == 0, "{m:?}");
}

#[test]
fn a_lone_edge_whose_faces_meet_again_tapers_to_a_tip() {
    // Part of a cylinder's rim: each end continues into an unbevelled rim
    // edge, between the same two faces.
    let mut c = cylinder(&CylinderDesc {
        radius: 15.0,
        radius_top: 0.0,
        height: 20.0,
        ..Default::default()
    })
    .unwrap();
    let rim = c.edges(&["top"]).unwrap();
    let arc: Vec<_> = rim
        .iter()
        .copied()
        .filter(|&(a, b)| {
            [a, b].iter().all(|&v| {
                let p = c.positions[v as usize];
                p.x > 0.0 && p.y > 0.0
            })
        })
        .collect();
    for &(a, b) in &arc {
        c.tag_edge(a, b, &["arc"]);
    }
    let s = fillet(
        &c,
        &Fillet {
            radius: 1.5,
            edges: names(&["arc"]),
            segments: 4,
            ..Default::default()
        },
    )
    .unwrap();
    check_closed(&s);
    // Each tip's row is 4 triangles; the rest of the strips are quads.
    let f = s.faces(&["fillet"]).unwrap();
    let q = quad_pairs(&s, &f);
    assert_eq!(q.triangles, arc.len() * 8 - 2 * 4);
    assert_eq!(q.triangles - q.paired, 2 * 4, "{q:?}");
    assert!(q.max_fold < 1e-6, "{q:?}");
    // The tips are the arc's end vertices; the rim's other edges keep
    // `top` and their vertices.
    for &(a, b) in &rim {
        if !arc.contains(&(a, b)) {
            for v in [a, b] {
                assert!(s.positions.contains(&c.positions[v as usize]));
            }
        }
    }
    let fillets = s.edges(&["fillet"]).unwrap();
    let plain = s
        .edges(&["top"])
        .unwrap()
        .into_iter()
        .filter(|e| !fillets.contains(e))
        .count();
    assert_eq!(plain, rim.len() - arc.len());
    // Full width along the arc (a 4-segment profile, at the fillet's
    // centroid radius about 14.5), less where the end edges taper.
    let length: f64 = arc
        .iter()
        .map(|&(a, b)| (c.positions[a as usize] - c.positions[b as usize]).length())
        .sum();
    let section = 2.25 * (1.0 - 2.0 * trig::sin(trig::PI / 8.0));
    let lost = c.measure().volume - s.measure().volume;
    let want = section * length * 14.5 / 15.0;
    let end = section * length / arc.len() as f64;
    assert!(lost < want && lost > want - 2.0 * end, "{lost} {want}");
}

fn l_prism() -> Solid {
    let corners = [
        (0.0, 0.0),
        (30.0, 0.0),
        (30.0, 10.0),
        (10.0, 10.0),
        (10.0, 30.0),
        (0.0, 30.0),
    ];
    extrude(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(
            corners.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
        )),
        &Extrude {
            height: 10.0,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn arc_and_patch_miters_round_the_face_corner_and_stay_on_it() {
    let l = l_prism();
    let base = Fillet {
        radius: 2.0,
        edges: names(&["top"]),
        segments: 4,
        ..Default::default()
    };
    let sharp = fillet(&l, &base).unwrap();
    let mut volumes = vec![sharp.measure().volume];
    for (inner, outer) in [
        (Miter::Arc, Miter::Sharp),
        (Miter::Sharp, Miter::Arc),
        (Miter::Sharp, Miter::Patch),
    ] {
        let s = fillet(
            &l,
            &Fillet {
                inner_miter: inner,
                outer_miter: outer,
                ..base.clone()
            },
        )
        .unwrap();
        check_closed(&s);
        // The top stays flat: every arc lies on it.
        for t in s.faces(&["top"]).unwrap() {
            for &v in &s.triangles[t as usize] {
                assert_eq!(s.positions[v as usize].z, 10.0);
            }
        }
        let f = s.faces(&["fillet"]).unwrap();
        let q = quad_pairs(&s, &f);
        assert!(q.paired * 10 >= q.triangles * 9, "{q:?}");
        volumes.push(s.measure().volume);
    }
    // Inner arcs cut the five convex corners of the top; outer miters
    // round the reflex one by keeping more of the top.
    assert!(volumes[1] < volumes[0], "{volumes:?}");
    assert!(
        volumes[2] > volumes[0] && volumes[3] > volumes[0],
        "{volumes:?}"
    );
    // A patch miter needs a reflex corner.
    let e = fillet(
        &l,
        &Fillet {
            inner_miter: Miter::Patch,
            ..base.clone()
        },
    );
    assert!(matches!(e, Err(BevelError::Invalid(_))), "{e:?}");
    // Chamfers and a box's all-edge corners take them too.
    let ch = chamfer(
        &l,
        &Chamfer {
            distance: 2.0,
            edges: names(&["top"]),
            inner_miter: Miter::Arc,
            outer_miter: Miter::Patch,
            segments: 2,
            ..Default::default()
        },
    )
    .unwrap();
    check_closed(&ch);
    let all = fillet(
        &block(),
        &Fillet {
            radius: 3.0,
            segments: 4,
            inner_miter: Miter::Arc,
            ..Default::default()
        },
    )
    .unwrap();
    check(&all, "fillet");
}

#[test]
fn faces_cut_into_slivers_are_rebuilt_from_their_outline() {
    // A join leaves its cut edges in short pieces and its faces in
    // slivers; each run is bevelled as one edge.
    let big = cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })
    .unwrap();
    let mut small = cuboid(&BoxDesc {
        width: 8.0,
        depth: 8.0,
        height: 10.0,
    })
    .unwrap();
    small.map_positions(|p| p + shape_core::DVec3::new(3.0, 2.0, 8.0), false);
    let j = shape_bool::boolean(&big, &small, &shape_bool::Boolean::default()).unwrap();
    let s = fillet(
        &j,
        &Fillet {
            radius: 1.0,
            segments: 4,
            ..Default::default()
        },
    )
    .unwrap();
    check(&s, "fillet");
    // The concave seam's fillet adds what the convex ones remove, about.
    assert!((s.measure().volume - j.measure().volume).abs() < 0.03 * j.measure().volume);
    // 1.5 on either side of a 3 gap meets exactly: refused.
    let e = fillet(
        &j,
        &Fillet {
            radius: 1.5,
            segments: 4,
            ..Default::default()
        },
    );
    assert!(matches!(e, Err(BevelError::TooLarge(_))), "{e:?}");
    // A part-turn revolve's end faces are slivers too.
    let ring = shape_gen::solids::revolve(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(vec![
            DVec2::new(5.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 8.0),
            DVec2::new(5.0, 8.0),
        ])),
        &shape_gen::solids::Revolve {
            angle: 2.0,
            axis: shape_gen::solids::Axis::Z,
            ..Default::default()
        },
    )
    .unwrap();
    let s = fillet(
        &ring,
        &Fillet {
            radius: 0.3,
            segments: 4,
            ..Default::default()
        },
    )
    .unwrap();
    check_closed(&s);
    // Wider than a segment of the bore: `a_bevel_wider_than_the_bore_s_segments`.
}

#[test]
fn a_pipe_joint_stays_on_the_pipe() {
    // A block whose front is two faces in one plane: the seam between
    // them is a real edge under the top's front edge, which runs straight
    // through it.
    let corners = [
        (-20.0, -15.0),
        (0.0, -15.0),
        (20.0, -15.0),
        (20.0, 15.0),
        (-20.0, 15.0),
    ];
    let mut b = extrude(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(
            corners.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
        )),
        &Extrude {
            height: 20.0,
            ..Default::default()
        },
    )
    .unwrap();
    let seam = b
        .real_edges()
        .into_iter()
        .find(|&(x, y)| {
            [x, y].iter().all(|&v| {
                let q = b.positions[v as usize];
                q.x == 0.0 && q.y == -15.0
            })
        })
        .unwrap();
    b.tag_edge(seam.0, seam.1, &["seam"]);
    let r = 3.0;
    let plain = fillet(
        &b,
        &Fillet {
            radius: r,
            edges: names(&["top"]),
            segments: 8,
            ..Default::default()
        },
    )
    .unwrap();
    let s = fillet(
        &b,
        &Fillet {
            radius: r,
            edges: names(&["top", "seam"]),
            segments: 8,
            ..Default::default()
        },
    )
    .unwrap();
    check_closed(&s);
    let q = quad_pairs(&s, &s.faces(&["fillet"]).unwrap());
    assert!(q.triangles - q.paired <= 2, "{q:?}");
    // The seam's faces are one plane, so its bevel is flat and the joint
    // is the top edge's round, unbroken: every new point near the joint
    // lies on the cylinder of radius r about the round's axis.
    let axis = |q: shape_core::DVec3| {
        let (dy, dz) = (q.y - (-15.0 + r), q.z - (20.0 - r));
        (dy * dy + dz * dz).sqrt()
    };
    let mut near = 0;
    for t in s.faces(&["fillet"]).unwrap() {
        for &v in &s.triangles[t as usize] {
            let q = s.positions[v as usize];
            if q.x.abs() < r && q.y < -15.0 + r && q.z > 20.0 - r {
                near += 1;
                assert!((axis(q) - r).abs() < 1e-9, "{q} at {}", axis(q));
            }
        }
    }
    assert!(near > 20);
    let (v0, v1) = (plain.measure().volume, s.measure().volume);
    assert!((v0 - v1).abs() < 1e-6 * v0, "{v0} {v1}");
}

#[test]
fn loop_slide_keeps_each_bevel_s_width_even_along_its_edge() {
    // A tapered, uneven quadrilateral: at each top corner the two walls
    // meet the slanted edge at different angles, so their offset lines
    // meet it at different points.
    let quad = [(0.0, 0.0), (30.0, 0.0), (24.0, 20.0), (-6.0, 14.0)];
    let p = extrude(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(
            quad.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
        )),
        &Extrude {
            height: 10.0,
            taper: 0.3,
            ..Default::default()
        },
    )
    .unwrap();
    let d = Chamfer {
        distance: 2.0,
        edges: names(&["top"]),
        ..Default::default()
    };
    // The width of the front wall's bevel at each end: the distance from
    // the wall's top edge of the points the chamfer leaves on the wall.
    let wall = (0..p.triangles.len())
        .find(|&t| bevel::tri_normal(&p, t).y < -0.5)
        .unwrap();
    let n = bevel::tri_normal(&p, wall);
    let on = |q: shape_core::DVec3, a: shape_core::DVec3| n.dot(q - a).abs() < 1e-9;
    let a = p.positions[p.triangles[wall][0] as usize];
    let top: Vec<_> = p
        .positions
        .iter()
        .filter(|q| q.z == 10.0 && on(**q, a))
        .copied()
        .collect();
    let (a, line) = (top[0], (top[1] - top[0]).normalize());
    let front = |s: &Solid| -> Vec<f64> {
        let mut out: Vec<f64> = s
            .positions
            .iter()
            .filter(|q| q.z < 10.0 && q.z > 5.0 && on(**q, a))
            .map(|&q| ((q - a) - line * (q - a).dot(line)).length())
            .collect();
        out.sort_by(f64::total_cmp);
        out
    };
    let plain = chamfer(&p, &d).unwrap();
    let w = front(&plain);
    assert_eq!(w.len(), 2, "{w:?}");
    assert!((w[0] - w[1]).abs() > 1e-3, "{w:?}");
    let slid = chamfer(
        &p,
        &Chamfer {
            loop_slide: true,
            ..d.clone()
        },
    )
    .unwrap();
    check(&slid, "chamfer");
    let w = front(&slid);
    assert_eq!(w.len(), 2, "{w:?}");
    assert!((w[0] - w[1]).abs() < 1e-6, "{w:?}");
}

/// Two boxes 20 × 20 × 10 joined where they touch at one corner: the
/// corner (10, 10, 10) is a vertex of two cones of faces.
fn corner_touch() -> Solid {
    let a = cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })
    .unwrap();
    let mut b = a.clone();
    b.map_positions(|p| p + shape_core::DVec3::new(20.0, 20.0, 10.0), false);
    shape_bool::boolean(&a, &b, &shape_bool::Boolean::default()).unwrap()
}

#[test]
fn two_cones_at_one_vertex_are_bevelled_as_two_corners() {
    let j = corner_touch();
    let corner = shape_core::DVec3::new(10.0, 10.0, 10.0);
    let d = Fillet {
        radius: 2.0,
        segments: 4,
        ..Default::default()
    };
    let s = fillet(&j, &d).unwrap();
    check(&s, "fillet");
    // Each box rounds its own corner: the same as the two boxes apart.
    let one = fillet(
        &cuboid(&BoxDesc {
            width: 20.0,
            depth: 20.0,
            height: 10.0,
        })
        .unwrap(),
        &d,
    )
    .unwrap();
    let (v, w) = (s.measure().volume, 2.0 * one.measure().volume);
    assert!((v - w).abs() < 1e-9 * w, "{v} {w}");
    assert_eq!(s.measure().components, 2);
    assert!(!s.positions.contains(&corner));
    // Only the lower box's edges: the upper box keeps its corner, and the
    // lower box's copy of it is gone.
    let mut low = j.clone();
    for (a, b) in j.real_edges() {
        let q = [a, b].map(|x| j.positions[x as usize]);
        if q.iter().all(|p| p.x <= 10.0 && p.y <= 10.0 && p.z <= 10.0) {
            low.tag_edge(a, b, &["low"]);
        }
    }
    let s = chamfer(
        &low,
        &Chamfer {
            distance: 1.0,
            edges: names(&["low"]),
            ..Default::default()
        },
    )
    .unwrap();
    check_closed(&s);
    // Strips pair; each of the eight corners is one flat triangle.
    let q = quad_pairs(&s, &s.faces(&["chamfer"]).unwrap());
    assert_eq!(q.triangles - q.paired, 8, "{q:?}");
    assert_eq!(s.positions.iter().filter(|&&p| p == corner).count(), 1);
    // The lower box's corner is cut away, so the boxes no longer touch.
    assert_eq!(s.measure().components, 2);
}

#[test]
fn a_corner_of_zero_angle_holds_no_offset() {
    // A needle: the top corner at the origin is 1e-10 rad, so the two top
    // edges' offset lines meet about 1e10 offsets away.
    let needle = extrude(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(10.0, 0.0),
            DVec2::new(10.0, 1e-9),
        ])),
        &Extrude {
            height: 5.0,
            ..Default::default()
        },
    )
    .unwrap();
    for e in [
        fillet(
            &needle,
            &Fillet {
                radius: 0.5,
                edges: names(&["top"]),
                segments: 2,
                ..Default::default()
            },
        ),
        chamfer(
            &needle,
            &Chamfer {
                distance: 0.5,
                edges: names(&["top"]),
                ..Default::default()
            },
        ),
    ] {
        assert!(
            matches!(e, Err(BevelError::TooLarge(p)) if p.x == 0.0 && p.y == 0.0),
            "{e:?}"
        );
    }
}

/// A block 4 × 2 whose top has a straight real edge along y = 1 at
/// height 1, split at `(2, 1, 1)`: behind it (y > 1) one curved surface
/// rising to a ridge, in front two flat faces split by an edge from the
/// vertex to `(2, 0, 1)`. The edge's left piece (`along`) and the split
/// (`split`) are named; its right piece, in line with `along`, is not.
fn ridge() -> Solid {
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
    let mut s = Solid::default();
    for (x, y, z) in pts {
        s.push_vertex(shape_core::DVec3::new(x, y, z));
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
            s.push_triangle(t, surface, &[name]);
        }
    }
    s.tag_edge(0, 1, &["along"]);
    s.tag_edge(1, 7, &["split"]);
    s
}

#[test]
fn an_edge_in_line_with_its_unbevelled_neighbour_across_a_curved_face() {
    // §7.5 placed the gap's point at the mean of the curved face's point
    // and the flat face's, which lifted the flat `top` face off its plane
    // by 0.11 of the offset at the vertex (refused since §7.7). Cut on the
    // curved face's folds (§7.8), every old face keeps its plane.
    let r = ridge();
    let m = r.measure();
    assert!(m.closed && m.degenerate_triangles == 0, "{m:?}");
    for d in [0.2, 0.3] {
        let s = fillet(
            &r,
            &Fillet {
                radius: d,
                edges: names(&["along", "split"]),
                segments: 4,
                ..Default::default()
            },
        )
        .unwrap();
        let c = chamfer(
            &r,
            &Chamfer {
                distance: d,
                edges: names(&["along", "split"]),
                ..Default::default()
            },
        )
        .unwrap();
        for (out, kind) in [(&s, "fillet"), (&c, "chamfer")] {
            check_closed(out);
            uncrossed(out);
            in_planes(&r, out, kind);
        }
    }
}

/// A regular `k`-gon prism of radius 10, 20 tall.
fn ngon_prism(k: usize, taper: f64) -> Solid {
    let pts: Vec<DVec2> = (0..k)
        .map(|i| {
            let a = trig::TAU * i as f64 / k as f64;
            DVec2::new(10.0 * trig::cos(a), 10.0 * trig::sin(a))
        })
        .collect();
    extrude(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(pts)),
        &Extrude {
            height: 20.0,
            taper,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn straight_profiles_on_end_faces_leave_no_flat_triangle() {
    // A chamfer's profile is straight: where an end face was clipped
    // into a triangle of three of its points, flat in floating point,
    // the result was refused as `TooLarge` though the chamfer fits.
    let side = |d: f64, seg: u32| Chamfer {
        distance: d,
        edges: names(&["side"]),
        segments: seg,
        ..Default::default()
    };
    for (k, taper, seg) in [(11, 0.0, 2), (9, 0.0, 3), (7, 1.0, 2)] {
        let s = chamfer(&ngon_prism(k, taper), &side(1.5, seg)).unwrap();
        check_closed(&s);
    }
    // One bottom edge of an octagonal prism: its end faces are the two
    // walls beside it.
    let p = ngon_prism(8, 0.0);
    let mut one = p.clone();
    let e = p
        .edges(&["bottom"])
        .unwrap()
        .into_iter()
        .find(|&(a, b)| [a, b].iter().all(|&v| p.positions[v as usize].y > 7.0))
        .unwrap();
    one.tag_edge(e.0, e.1, &["one"]);
    let s = chamfer(
        &one,
        &Chamfer {
            distance: 0.5,
            edges: names(&["one"]),
            segments: 2,
            ..Default::default()
        },
    )
    .unwrap();
    check(&s, "chamfer");
    // A block with a bore: its bottom face is rebuilt from its outline,
    // whose bore rim has two points 2e-15 apart from the cut.
    let b = cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })
    .unwrap();
    let mut c = cylinder(&CylinderDesc {
        radius: 4.0,
        height: 30.0,
        ..Default::default()
    })
    .unwrap();
    c.map_positions(|p| p + shape_core::DVec3::new(0.0, 0.0, -10.0), false);
    let bored = shape_bool::boolean(
        &b,
        &c,
        &shape_bool::Boolean {
            op: shape_bool::BoolOp::Cut,
            ..Default::default()
        },
    )
    .unwrap();
    for seg in [1, 2] {
        let s = chamfer(&bored, &side(0.5, seg)).unwrap();
        check(&s, "chamfer");
    }
}

/// A part revolve (2 rad about z) of the section given, `(r, z)` corners.
fn part_ring(section: &[(f64, f64)]) -> Solid {
    shape_gen::solids::revolve(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(
            section.iter().map(|&(x, y)| DVec2::new(x, y)).collect(),
        )),
        &shape_gen::solids::Revolve {
            angle: 2.0,
            axis: shape_gen::solids::Axis::Z,
            ..Default::default()
        },
    )
    .unwrap()
}

/// Names the real edges whose ends both pass `pick`.
fn tag(s: &Solid, name: &str, pick: impl Fn(shape_core::DVec3) -> bool) -> Solid {
    let mut out = s.clone();
    for (a, b) in s.real_edges() {
        if pick(s.positions[a as usize]) && pick(s.positions[b as usize]) {
            out.tag_edge(a, b, &[name]);
        }
    }
    out
}

#[test]
fn a_pipe_through_curved_faces() {
    // The outer wall at radius 10 is two bands split at z = 4 (a seam
    // circle, smooth); the start cap's outer edge is split there too, so
    // at (10, 0, 4) the two pieces are in line and the seam crosses them.
    let ring = part_ring(&[
        (5.0, 0.0),
        (10.0, 0.0),
        (10.0, 4.0),
        (10.0, 8.0),
        (5.0, 8.0),
    ]);
    let radius = |p: shape_core::DVec3| (p.x * p.x + p.y * p.y).sqrt();
    let t = tag(&ring, "seam", |p| {
        (radius(p) - 10.0).abs() < 1e-9 && p.z == 4.0
    });
    let t = tag(&t, "post", |p| {
        p.y.abs() < 1e-12 && (radius(p) - 10.0).abs() < 1e-9
    });
    assert_eq!(t.edges(&["post"]).unwrap().len(), 2);
    for n in [4, 8] {
        let d = |edges: &[&str]| Fillet {
            radius: 1.0,
            edges: names(edges),
            segments: n,
            ..Default::default()
        };
        let s = fillet(&t, &d(&["seam", "post"])).unwrap();
        check_closed(&s);
        let q = quad_pairs(&s, &s.faces(&["fillet"]).unwrap());
        assert!(q.triangles - q.paired <= 2, "{q:?}");
        // The seam's faces are tangent, so its bevel is all but flat and
        // the joint keeps the post's round: the volume is the post's
        // alone, to the faceting of the bands (2e-6 of it).
        let plain = fillet(&t, &d(&["post"])).unwrap();
        let (v0, v1) = (plain.measure().volume, s.measure().volume);
        assert!((v0 - v1).abs() < 5e-6 * v0, "{v0} {v1}");
    }
}

#[test]
fn patches_of_seven_and_more_sides() {
    for k in [7, 9, 12] {
        let p = ngon_prism(k, 1.0);
        for n in [2, 4, 6] {
            let s = fillet(
                &p,
                &Fillet {
                    radius: 1.5,
                    edges: names(&["side"]),
                    segments: n,
                    ..Default::default()
                },
            )
            .unwrap();
            let q = check(&s, "fillet");
            assert!(q.max_fold < trig::to_radians(15.0), "{k} {n}: {q:?}");
            inside(&s, &p);
            // The apex patch: k sectors of (n/2)² quads. Each strip runs
            // from the base (z = 0) to the patch in one row, so the patch
            // is what lies wholly above z = 1.
            let apex = s
                .faces(&["fillet"])
                .unwrap()
                .into_iter()
                .filter(|&t| {
                    s.triangles[t as usize]
                        .iter()
                        .all(|&v| s.positions[v as usize].z > 1.0)
                })
                .count();
            assert_eq!(apex, 2 * k * (n * n / 4) as usize, "{k} {n}");
        }
        let c = chamfer(
            &p,
            &Chamfer {
                distance: 1.5,
                edges: names(&["side"]),
                segments: 2,
                ..Default::default()
            },
        )
        .unwrap();
        check(&c, "chamfer");
        inside(&c, &p);
    }
}

#[test]
fn loop_slide_around_a_curved_face() {
    // A part revolve with a conical outer wall: at the start cap's top
    // edge the cap (flat) and the cone meet the unbevelled post at
    // different angles, so the two offset lines meet it apart.
    let ring = part_ring(&[(5.0, 0.0), (10.0, 0.0), (7.0, 8.0), (5.0, 8.0)]);
    let t = tag(&ring, "lid", |p| p.z == 8.0);
    // The fillet's depth below the top on the start cap (y = 0), at both
    // ends of the cap's top edge.
    let depths = |slide: bool| {
        let s = fillet(
            &t,
            &Fillet {
                radius: 0.3,
                edges: names(&["lid"]),
                segments: 4,
                loop_slide: slide,
                ..Default::default()
            },
        )
        .unwrap();
        check_closed(&s);
        let q = quad_pairs(&s, &s.faces(&["fillet"]).unwrap());
        assert!(q.triangles - q.paired <= 2, "{q:?}");
        let mut d: Vec<f64> = s
            .positions
            .iter()
            .filter(|p| p.y == 0.0 && p.z < 8.0 && p.z > 4.0)
            .map(|p| 8.0 - p.z)
            .collect();
        d.sort_by(f64::total_cmp);
        d
    };
    let d = depths(false);
    assert_eq!(d.len(), 2, "{d:?}");
    assert!(d[1] - d[0] > 5e-3, "{d:?}");
    let d = depths(true);
    assert_eq!(d.len(), 2, "{d:?}");
    assert!(d[1] - d[0] < 1e-9, "{d:?}");
}

#[test]
fn miters_with_chamfers_of_several_segments() {
    // The L's top, and an outline with arcs and a reflex corner.
    let curvy = extrude(
        &shape_2d::Path::new(0.0, 0.0)
            .line_to(30.0, 0.0)
            .tangent_arc_to(30.0, 10.0)
            .line_to(10.0, 10.0)
            .line_to(10.0, 30.0)
            .arc_to(5.0, 35.0, 0.0, 30.0)
            .close()
            .unwrap(),
        &Extrude {
            height: 10.0,
            ..Default::default()
        },
    )
    .unwrap();
    for solid in [l_prism(), curvy.clone()] {
        for segments in [2, 3, 4, 5] {
            for (inner, outer) in [
                (Miter::Arc, Miter::Sharp),
                (Miter::Sharp, Miter::Arc),
                (Miter::Sharp, Miter::Patch),
                (Miter::Arc, Miter::Patch),
            ] {
                for spread in [0.0, 1e-6, 0.5] {
                    let r = chamfer(
                        &solid,
                        &Chamfer {
                            distance: 2.0,
                            edges: names(&["top"]),
                            segments,
                            inner_miter: inner,
                            outer_miter: outer,
                            spread,
                            ..Default::default()
                        },
                    );
                    match r {
                        Ok(s) => {
                            check_closed(&s);
                            let q = quad_pairs(&s, &s.faces(&["chamfer"]).unwrap());
                            assert!(q.paired * 4 >= q.triangles * 3, "{q:?}");
                        }
                        // An inner arc at the curvy outline's corner of
                        // nearly 180° slides its points past the next
                        // arc point, a chord of 0.65: refused, unless
                        // the spread is smaller.
                        Err(BevelError::TooLarge(p)) => {
                            assert!(inner == Miter::Arc && spread != 1e-6, "{p}");
                            assert_eq!(p, shape_core::DVec3::new(0.0, 30.0, 10.0));
                        }
                        Err(e) => panic!("{e:?}"),
                    }
                }
            }
        }
    }
}

// Bevels wider than the segments of their faces (`docs/research.md` §7.6).

/// No two triangles cross.
fn uncrossed(s: &Solid) {
    let c = shape_bool::selfcheck::self_crossings(&s.positions, &s.triangles);
    assert!(c.is_empty(), "{} crossing pairs", c.len());
}

/// Canonical: canonicalizing again changes nothing.
fn canonical(s: &Solid) {
    let mut again = s.clone();
    again.canonicalize().unwrap();
    assert_eq!(&again, s);
}

/// Every new face carries only its kind's name, and some edge is named
/// with it and with each cap name given.
fn named(s: &Solid, kind: &str, caps: &[&str]) {
    for t in s.faces(&[kind]).unwrap() {
        assert_eq!(s.face_tags[t as usize].len(), 1);
    }
    let edges = s.edges(&[kind]).unwrap();
    for cap in caps {
        let tag = s.names.get(cap).unwrap();
        assert!(
            edges.iter().any(|e| s.edge_tags[e].contains(&tag)),
            "no {kind} edge also named {cap}"
        );
    }
}

/// Volume of a 2-radian part revolve of the rectangle r 5–10, z 0–8,
/// every edge rounded by `rho` (a rolling ball: every edge is square, so
/// the result is the eroded body grown back by `rho`; Steiner's formula,
/// the eroded body's area by the midpoint rule).
fn ring_rounded(rho: f64) -> f64 {
    let (a, b, h, ang) = (5.0 + rho, 10.0 - rho, 8.0 - 2.0 * rho, 2.0);
    let al = |r: f64| trig::asin(rho / r);
    let ext = |r: f64| ang - 2.0 * al(r);
    let n = 20_000;
    let step = (b - a) / n as f64;
    let area: f64 = (0..n)
        .map(|i| {
            let r = a + (i as f64 + 0.5) * step;
            r * ext(r) * step
        })
        .sum();
    let side = (b * b - rho * rho).sqrt() - (a * a - rho * rho).sqrt();
    let per = a * ext(a) + b * ext(b) + 2.0 * side;
    let mean = -ext(a) * h / 2.0
        + ext(b) * h / 2.0
        + trig::PI / 2.0 * per
        + h * (trig::PI + al(b) - al(a));
    area * h
        + rho * (2.0 * area + h * per)
        + rho * rho * mean
        + 4.0 * trig::PI / 3.0 * rho * rho * rho
}

#[test]
fn a_bevel_wider_than_the_bore_s_segments() {
    // The part revolve's bore at r 5 has segments 0.91 long; a 1.5
    // fillet on its start and end edges swallows the rims' first
    // vertices, which slide along the rims past the corner's reach.
    let ring = part_ring(&[(5.0, 0.0), (10.0, 0.0), (10.0, 8.0), (5.0, 8.0)]);
    let v0 = ring.measure().volume;
    for (rho, lo, hi) in [(1.0, 1.0, 1.1), (1.5, 1.0, 1.1)] {
        let s = fillet(
            &ring,
            &Fillet {
                radius: rho,
                segments: 4,
                ..Default::default()
            },
        )
        .unwrap();
        check_closed(&s);
        uncrossed(&s);
        canonical(&s);
        named(&s, "fillet", &["start", "end"]);
        // Against the smooth rounded body: the 4-segment profile cuts 9 %
        // more than a circle (as below r 1, where the classic layout fits).
        let removed = v0 - s.measure().volume;
        let want = 600.0 - ring_rounded(rho);
        assert!(
            removed > lo * want && removed < hi * want,
            "{removed} {want}"
        );
        let q = quad_pairs(&s, &s.faces(&["fillet"]).unwrap());
        assert!(q.paired * 10 >= q.triangles * 9, "{q:?}");
    }
    // The same profile's excess where the classic layout fits.
    let s = fillet(
        &ring,
        &Fillet {
            radius: 0.5,
            segments: 4,
            ..Default::default()
        },
    )
    .unwrap();
    let excess = (v0 - s.measure().volume) / (600.0 - ring_rounded(0.5));
    assert!(excess > 1.09 && excess < 1.1, "{excess}");
    let c = chamfer(
        &ring,
        &Chamfer {
            distance: 1.5,
            ..Default::default()
        },
    )
    .unwrap();
    check_closed(&c);
    uncrossed(&c);
    named(&c, "chamfer", &["start", "end"]);
}

/// A 30 × 30 × 10 plate with a 6 mm hole through it: by the hole tool (a
/// boolean, whose rims come in pieces down to 1e-15 long), and extruded
/// from an outline with the same 24-gon hole.
fn plates() -> (Solid, Solid) {
    let plate = cuboid(&BoxDesc {
        width: 30.0,
        depth: 30.0,
        height: 10.0,
    })
    .unwrap();
    let cut = hole(
        &plate,
        &Hole {
            diameter: 6.0,
            at: vec![above(0.0, 0.0)],
            ..Default::default()
        },
    )
    .unwrap();
    let mut ring: Vec<DVec2> = (0..24)
        .map(|k| {
            let (sin, cos) = trig::sin_cos(trig::TAU * k as f64 / 24.0);
            DVec2::new(3.0 * cos, 3.0 * sin)
        })
        .collect();
    ring.reverse();
    let outline = shape_2d::Profile {
        regions: vec![shape_2d::Region {
            outer: shape_2d::Contour::corners(vec![
                DVec2::new(-15.0, -15.0),
                DVec2::new(15.0, -15.0),
                DVec2::new(15.0, 15.0),
                DVec2::new(-15.0, 15.0),
            ]),
            holes: vec![shape_2d::Contour::smooth(ring)],
        }],
    };
    let plain = extrude(
        &outline,
        &Extrude {
            height: 10.0,
            ..Default::default()
        },
    )
    .unwrap();
    let plain = tag(&plain, "hole", |p| p.truncate().length() < 3.0 + 1e-9);
    (cut, plain)
}

#[test]
fn a_hole_s_rims_wider_than_their_pieces() {
    let (cut, plain) = plates();
    // Rims in pieces down to 1e-15 long: needles whose f64 area is zero.
    assert!(cut.triangles.iter().any(|t| {
        let [a, b, c] = t.map(|v| cut.positions[v as usize]);
        (b - a).cross(c - a) == DVec3::ZERO
    }));
    for r in [0.5, 1.5, 2.5] {
        let round = |s: &Solid, edges: &[&str]| {
            fillet(
                s,
                &Fillet {
                    radius: r,
                    segments: 4,
                    edges: names(edges),
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let a = round(&cut, &["hole"]);
        let b = round(&plain, &["hole"]);
        check(&a, "fillet");
        uncrossed(&a);
        canonical(&a);
        named(&a, "fillet", &["top", "bottom"]);
        // The pieces and the points 1e-15 apart are merged on the rims'
        // own lines: the same volume as the plain plate's.
        let (va, vb) = (a.measure().volume, b.measure().volume);
        assert!((va - vb).abs() < 1e-9 * vb, "{va} {vb}");
        // One rim and the plate's edges with it.
        let t = round(&cut, &["top"]);
        check_closed(&t);
        uncrossed(&t);
    }
}

#[test]
fn a_cylinder_s_top_rim_chamfered_over_several_segments() {
    // A 12-gon cylinder, r 3: its wall segments are 1.55 wide. A chamfer
    // 2 deep (three profile segments) fitted before; its volume is the
    // frustum's, P d² / 2 − n tan(π/n) d³ / 3.
    let c = cylinder(&CylinderDesc {
        radius: 3.0,
        height: 10.0,
        segments: 12,
        ..Default::default()
    })
    .unwrap();
    let d = 2.0;
    let s = chamfer(
        &c,
        &Chamfer {
            distance: d,
            segments: 3,
            edges: names(&["top"]),
            ..Default::default()
        },
    )
    .unwrap();
    check(&s, "chamfer");
    let n = 12.0;
    let per = n * 2.0 * 3.0 * trig::sin(trig::PI / n);
    let want = per * d * d / 2.0 - n * trig::tan(trig::PI / n) * d * d * d / 3.0;
    let removed = c.measure().volume - s.measure().volume;
    assert!((removed - want).abs() < 1e-9 * want, "{removed} {want}");
    // A slice of it, 2 radians: the cut faces' vertical edges are wider
    // than a wall segment once their chamfer is 1, and the rims' first
    // vertices slide past it.
    let pie = shape_gen::solids::revolve(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(5.0, 0.0),
            DVec2::new(5.0, 8.0),
            DVec2::new(0.0, 8.0),
        ])),
        &shape_gen::solids::Revolve {
            angle: 2.0,
            axis: shape_gen::solids::Axis::Z,
            segments: 24,
        },
    )
    .unwrap();
    for d in [1.0, 1.5] {
        let s = chamfer(
            &pie,
            &Chamfer {
                distance: d,
                segments: 3,
                ..Default::default()
            },
        )
        .unwrap();
        check_closed(&s);
        uncrossed(&s);
        canonical(&s);
        named(&s, "chamfer", &["start", "end"]);
    }
}

/// The distance from `p` to the nearest triangle of `tris`.
fn to_surface(p: shape_core::DVec3, tris: &[[shape_core::DVec3; 3]]) -> f64 {
    tris.iter()
        .map(|&[a, b, c]| {
            let n = (b - a).cross(c - a).normalize_or_zero();
            let h = (p - a).dot(n);
            let q = p - n * h;
            let inside = [(a, b), (b, c), (c, a)]
                .iter()
                .all(|&(x, y)| (y - x).cross(q - x).dot(n) >= -1e-9);
            if inside {
                h.abs()
            } else {
                [(a, b), (b, c), (c, a)]
                    .iter()
                    .map(|&(x, y)| {
                        let t = ((p - x).dot(y - x) / (y - x).length_squared()).clamp(0.0, 1.0);
                        (p - (x + (y - x) * t)).length()
                    })
                    .fold(f64::INFINITY, f64::min)
            }
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn a_twisted_prism_s_top_edges_cut_across_its_rows() {
    // Each side of a square twisted 1 rad is seven rows of twisted quads,
    // 2.86 high; a 5 fillet on the top edges crosses the first row.
    let square = shape_2d::Profile::from_contour(shape_2d::Contour::corners(vec![
        DVec2::new(-10.0, -10.0),
        DVec2::new(10.0, -10.0),
        DVec2::new(10.0, 10.0),
        DVec2::new(-10.0, 10.0),
    ]));
    let twisted = extrude(
        &square,
        &Extrude {
            height: 20.0,
            twist: 1.0,
            ..Default::default()
        },
    )
    .unwrap();
    let v0 = twisted.measure().volume;
    let sides: Vec<[shape_core::DVec3; 3]> = twisted
        .faces(&["side"])
        .unwrap()
        .iter()
        .map(|&t| twisted.triangles[t as usize].map(|v| twisted.positions[v as usize]))
        .collect();
    let mut per_r2 = Vec::new();
    for r in [4.5, 5.0, 5.5] {
        let s = fillet(
            &twisted,
            &Fillet {
                radius: r,
                segments: 4,
                edges: names(&["top"]),
                ..Default::default()
            },
        )
        .unwrap();
        check_closed(&s);
        uncrossed(&s);
        canonical(&s);
        named(&s, "fillet", &["top"]);
        // The sides keep their own triangles' planes, cut where the
        // offset line crosses them.
        for t in s.faces(&["side"]).unwrap() {
            for v in s.triangles[t as usize] {
                let p = s.positions[v as usize];
                assert!(to_surface(p, &sides) < 1e-9, "{p}");
            }
        }
        let q = quad_pairs(&s, &s.faces(&["fillet"]).unwrap());
        eprintln!("r {r} {q:?} removed {}", v0 - s.measure().volume);
        assert!(q.paired * 10 >= q.triangles * 8, "{q:?}");
        per_r2.push((v0 - s.measure().volume) / (r * r));
        // A fillet cuts less than the chamfer of the same offsets.
        let c = chamfer(
            &twisted,
            &Chamfer {
                distance: r,
                edges: names(&["top"]),
                ..Default::default()
            },
        )
        .unwrap();
        check_closed(&c);
        uncrossed(&c);
        assert!(c.measure().volume < s.measure().volume);
    }
    // The cut grows with r² while it crosses the same row.
    for w in per_r2.windows(2) {
        assert!((w[0] - w[1]).abs() < 0.02 * w[0], "{per_r2:?}");
    }
}

// Results that crossed themselves or were otherwise wrong
// (`docs/research.md` §7.7).

fn moved(s: &Solid, by: shape_core::DVec3) -> Solid {
    let mut out = s.clone();
    out.map_positions(|p| p + by, false);
    out
}

fn op(a: &Solid, b: &Solid, op: shape_bool::BoolOp) -> Solid {
    shape_bool::boolean(
        a,
        b,
        &shape_bool::Boolean {
            op,
            ..Default::default()
        },
    )
    .unwrap()
}

fn plate() -> Solid {
    cuboid(&BoxDesc {
        width: 20.0,
        depth: 20.0,
        height: 10.0,
    })
    .unwrap()
}

fn pin(r: f64, h: f64) -> Solid {
    cylinder(&CylinderDesc {
        radius: r,
        height: h,
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn a_layout_that_crosses_itself_is_never_returned() {
    // A bore's bottom rim: the boolean leaves the plate's bottom edges in
    // pieces, and at 1.5 the classic layout crossed itself near the bore.
    // The final check sends it on to the straight-run retry, which fits.
    let bore = op(
        &plate(),
        &moved(&pin(4.0, 30.0), shape_core::DVec3::new(0.0, 0.0, -10.0)),
        shape_bool::BoolOp::Cut,
    );
    let v0 = bore.measure().volume;
    let removed = |d: f64| {
        let s = chamfer(
            &bore,
            &Chamfer {
                distance: d,
                edges: names(&["bottom"]),
                ..Default::default()
            },
        )
        .unwrap();
        check_closed(&s);
        uncrossed(&s);
        v0 - s.measure().volume
    };
    // Every edge square, the same corners: the cut grows with d².
    let ratio = removed(1.5) / removed(0.5);
    assert!((ratio - 9.0).abs() < 0.2, "{ratio}");
}

#[test]
fn inputs_that_cross_themselves_or_are_not_manifold() {
    // A post joined onto a plate: the boolean leaves two rim points 1e-15
    // apart and a sliver across the plate's top triangle. The weld of a
    // wide bevel's pass 0 takes it out first.
    let post = op(
        &plate(),
        &moved(&pin(3.0, 8.0), shape_core::DVec3::new(0.0, 0.0, 10.0)),
        shape_bool::BoolOp::Join,
    );
    let mut corner = post.clone();
    let (a, b) = post
        .real_edges()
        .into_iter()
        .find(|&(a, b)| {
            let (p, q) = (post.positions[a as usize], post.positions[b as usize]);
            p.x == -10.0 && p.y == -10.0 && q.x == -10.0 && q.y == -10.0
        })
        .unwrap();
    corner.tag_edge(a, b, &["pick"]);
    let s = fillet(
        &corner,
        &Fillet {
            radius: 0.5,
            edges: names(&["pick"]),
            segments: 4,
            ..Default::default()
        },
    )
    .unwrap();
    check_closed(&s);
    uncrossed(&s);
    // Two boxes overlapping, not joined: refused, never bevelled through.
    let mut two = plate();
    let other = moved(&plate(), shape_core::DVec3::new(5.0, 5.0, 5.0));
    let base = two.positions.len() as u32;
    for &p in &other.positions {
        two.push_vertex(p);
    }
    for (t, tri) in other.triangles.iter().enumerate() {
        let names: Vec<&str> = other.face_tags[t]
            .iter()
            .map(|&n| other.names.name(n))
            .collect();
        two.push_triangle(tri.map(|v| v + base), other.surfaces[t] + 10, &names);
    }
    let r = fillet(
        &two,
        &Fillet {
            radius: 1.0,
            ..Default::default()
        },
    );
    assert!(matches!(r, Err(BevelError::SelfCrossing(_))), "{r:?}");
    // Two boxes joined along an edge: four faces on one edge.
    let edge = op(
        &plate(),
        &moved(&plate(), shape_core::DVec3::new(20.0, 0.0, 10.0)),
        shape_bool::BoolOp::Join,
    );
    let r = fillet(
        &edge,
        &Fillet {
            radius: 1.0,
            ..Default::default()
        },
    );
    assert!(
        matches!(r, Err(BevelError::NotManifold(p, q)) if p.x == 10.0 && q.x == 10.0 && p.z == 10.0 && q.z == 10.0),
        "{r:?}"
    );
}

fn twisted() -> Solid {
    let square = shape_2d::Profile::from_contour(shape_2d::Contour::corners(vec![
        DVec2::new(-10.0, -10.0),
        DVec2::new(10.0, -10.0),
        DVec2::new(10.0, 10.0),
        DVec2::new(-10.0, 10.0),
    ]));
    extrude(
        &square,
        &Extrude {
            height: 20.0,
            twist: 1.0,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn a_twisted_prism_s_top_keeps_its_sides_at_every_radius() {
    // The side rows are twisted quads: moving the top corner down the
    // helical edge tilted the row's long triangle, and r 0.5 added 11 mm³.
    // The bands are cut instead, at every radius (§7.7).
    let t = twisted();
    let v0 = t.measure().volume;
    let sides: Vec<[shape_core::DVec3; 3]> = t
        .faces(&["side"])
        .unwrap()
        .iter()
        .map(|&i| t.triangles[i as usize].map(|v| t.positions[v as usize]))
        .collect();
    for r in [0.5, 1.0, 2.0, 3.0] {
        let f = fillet(
            &t,
            &Fillet {
                radius: r,
                segments: 4,
                edges: names(&["top"]),
                ..Default::default()
            },
        )
        .unwrap();
        let c = chamfer(
            &t,
            &Chamfer {
                distance: r,
                edges: names(&["top"]),
                ..Default::default()
            },
        )
        .unwrap();
        for s in [&f, &c] {
            check_closed(s);
            uncrossed(s);
            for i in s.faces(&["side"]).unwrap() {
                for v in s.triangles[i as usize] {
                    let p = s.positions[v as usize];
                    assert!(to_surface(p, &sides) < 1e-9, "r {r}: {p}");
                }
            }
        }
        let per_r2 = (v0 - f.measure().volume) / (r * r);
        assert!(per_r2 > 15.0 && per_r2 < 19.0, "r {r}: {per_r2}");
        assert!(c.measure().volume < f.measure().volume);
    }
}

/// Removed volume, and every vertex and centroid of a triangle not named
/// `kind` on the input's surface (no old face left its plane).
fn on_input(s: &Solid, out: &Solid, kind: &str) -> f64 {
    let tris: Vec<[shape_core::DVec3; 3]> = s
        .triangles
        .iter()
        .map(|t| t.map(|v| s.positions[v as usize]))
        .collect();
    let new: std::collections::BTreeSet<u32> = out.faces(&[kind]).unwrap().into_iter().collect();
    for (i, t) in out.triangles.iter().enumerate() {
        if new.contains(&(i as u32)) {
            continue;
        }
        let p = t.map(|v| out.positions[v as usize]);
        for q in [p[0], p[1], p[2], (p[0] + p[1] + p[2]) / 3.0] {
            assert!(to_surface(q, &tris) < 1e-9, "{q} off the input");
        }
    }
    s.measure().volume - out.measure().volume
}

/// Every triangle not named `kind` lies in the plane of an input triangle
/// with its names (no old face tilted; a concave bevel's end may still
/// fill a corner outside the old face, in its plane).
fn in_planes(s: &Solid, out: &Solid, kind: &str) {
    let new: std::collections::BTreeSet<u32> = out.faces(&[kind]).unwrap().into_iter().collect();
    for (i, t) in out.triangles.iter().enumerate() {
        if new.contains(&(i as u32)) {
            continue;
        }
        let p = t.map(|v| out.positions[v as usize]);
        let fits = s.triangles.iter().enumerate().any(|(j, u)| {
            let [a, b, c] = u.map(|v| s.positions[v as usize]);
            let n = (b - a).cross(c - a).normalize_or_zero();
            let tags = |x: &Solid, k: usize| -> Vec<String> {
                x.face_tags[k]
                    .iter()
                    .map(|&g| x.names.name(g).to_string())
                    .collect()
            };
            tags(s, j) == tags(out, i) && p.iter().all(|&q| (q - a).dot(n).abs() < 1e-9)
        });
        assert!(fits, "{p:?} off its face's planes");
    }
}

#[test]
fn a_lone_piece_of_a_crease_tapers_to_a_tip_at_both_ends() {
    // One piece of a cylinder's rim, shorter than twice the offset: the
    // width peaks at half its length in the middle. The faces meet square,
    // so a chamfer removes two tetrahedra, (L/2)³/3 together.
    let c = pin(5.0, 10.0);
    let (a, b) = c
        .real_edges()
        .into_iter()
        .find(|&(a, b)| c.positions[a as usize].z == 10.0 && c.positions[b as usize].z == 10.0)
        .unwrap();
    let half = (c.positions[a as usize] - c.positions[b as usize]).length() / 2.0;
    assert!(half < 1.5);
    let mut piece = c.clone();
    piece.tag_edge(a, b, &["pick"]);
    let ch = chamfer(
        &piece,
        &Chamfer {
            distance: 1.5,
            edges: names(&["pick"]),
            ..Default::default()
        },
    )
    .unwrap();
    check_closed(&ch);
    uncrossed(&ch);
    let gone = on_input(&piece, &ch, "chamfer");
    assert!((gone - half * half * half / 3.0).abs() < 1e-9, "{gone}");
    let f = fillet(
        &piece,
        &Fillet {
            radius: 0.5,
            edges: names(&["pick"]),
            ..Default::default()
        },
    )
    .unwrap();
    check_closed(&f);
    uncrossed(&f);
    canonical(&f);
    named(&f, "fillet", &["top"]);
    assert!(on_input(&piece, &f, "fillet") > 0.0);
    // A straight edge of a rounded rectangle's top, longer than twice the
    // offset, meets its arcs tangentially: full width between stations
    // one offset in from each tip, d²/2 (L − 2d) + d³/3 removed.
    let rr = extrude(
        &shape_2d::rect(&shape_2d::Rect {
            width: 30.0,
            height: 20.0,
            radius: 4.0,
        })
        .unwrap(),
        &Extrude {
            height: 6.0,
            ..Default::default()
        },
    )
    .unwrap();
    let (a, b) = rr
        .real_edges()
        .into_iter()
        .find(|&(a, b)| {
            let (p, q) = (rr.positions[a as usize], rr.positions[b as usize]);
            p.z == 6.0 && q.z == 6.0 && p.y == q.y && (p.x - q.x).abs() > 20.0
        })
        .unwrap();
    let len = (rr.positions[a as usize] - rr.positions[b as usize]).length();
    let mut straight = rr.clone();
    straight.tag_edge(a, b, &["pick"]);
    let d = 1.5;
    let ch = chamfer(
        &straight,
        &Chamfer {
            distance: d,
            edges: names(&["pick"]),
            ..Default::default()
        },
    )
    .unwrap();
    check_closed(&ch);
    uncrossed(&ch);
    let gone = on_input(&straight, &ch, "chamfer");
    let want = d * d / 2.0 * (len - 2.0 * d) + d * d * d / 3.0;
    assert!((gone - want).abs() < 1e-9, "{gone} {want}");
    // A middle piece of the twisted prism's helical edge, between two
    // twisted rows: the stations' points lie in the planes of the piece's
    // own triangles, so no row tilts.
    let t = twisted();
    let &(a, b) = t
        .real_edges()
        .iter()
        .find(|&&(a, b)| {
            let (p, q) = (t.positions[a as usize], t.positions[b as usize]);
            p.z.max(q.z) >= 10.0 && p.z.min(q.z) < 10.0
        })
        .unwrap();
    let mut one = t.clone();
    one.tag_edge(a, b, &["pick"]);
    for r in [0.5, 1.5] {
        let f = fillet(
            &one,
            &Fillet {
                radius: r,
                edges: names(&["pick"]),
                ..Default::default()
            },
        )
        .unwrap();
        check_closed(&f);
        uncrossed(&f);
        assert!(on_input(&one, &f, "fillet") > 0.0);
    }
    // Where the taper does not fit, the error names the piece and asks for
    // the whole crease: a counterbore's floor rim beside a wall triangle
    // the boolean left as a sliver.
    let wide_plate = cuboid(&BoxDesc {
        width: 30.0,
        depth: 30.0,
        height: 10.0,
    })
    .unwrap();
    let cb = hole(
        &wide_plate,
        &Hole {
            diameter: 5.0,
            counterbore: (9.0, 3.0),
            at: vec![above(-3.0, 2.0)],
            ..Default::default()
        },
    )
    .unwrap();
    let near = |v: u32, x: f64, y: f64| {
        let p = cb.positions[v as usize];
        (p - shape_core::DVec3::new(x, y, 7.0)).length() < 1e-3
    };
    let (a, b) = cb
        .real_edges()
        .into_iter()
        .find(|&(a, b)| {
            near(a, -0.75, -1.8971) && near(b, 0.18198, -1.18198)
                || near(b, -0.75, -1.8971) && near(a, 0.18198, -1.18198)
        })
        .unwrap();
    let mut floor = cb.clone();
    floor.tag_edge(a, b, &["pick"]);
    let e = fillet(
        &floor,
        &Fillet {
            radius: 0.5,
            edges: names(&["pick"]),
            ..Default::default()
        },
    )
    .unwrap_err();
    let BevelError::LonePiece(_, _, n) = e else {
        panic!("{e:?}");
    };
    assert!(n.contains(&"pick".to_string()));
}

#[test]
fn faces_not_in_one_plane_are_cut_on_their_folds() {
    // Where the classic layout would tilt a face whose triangles at a
    // vertex are not one plane, each fold there is cut like a real edge,
    // in each triangle's own plane (`docs/research.md` §7.8).
    let t = twisted();
    let apex = shape_gen::solids::revolve(
        &shape_2d::Profile::from_contour(shape_2d::Contour::corners(vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(8.0, 0.0),
            DVec2::new(8.0, 4.0),
            DVec2::new(0.0, 10.0),
        ])),
        &shape_gen::solids::Revolve {
            angle: 3.0,
            axis: shape_gen::solids::Axis::Z,
            ..Default::default()
        },
    )
    .unwrap();
    // A loft whose walls are twisted quads between sections of different
    // corner radii: run A's toaster housing, `fillet(14, top)`.
    let rr = |w: f64, h: f64, r: f64| {
        shape_2d::rect(&shape_2d::Rect {
            width: w,
            height: h,
            radius: r,
        })
        .unwrap()
    };
    let loft = shape_gen::loft::loft(&shape_gen::loft::Loft {
        sections: vec![
            (rr(155.0, 285.0, 30.0), 0.0),
            (rr(151.0, 281.0, 32.0), 61.25),
            (rr(125.0, 275.0, 24.0), 175.0),
        ],
        ..Default::default()
    })
    .unwrap();
    let cases: [(&Solid, &[&str], f64); 4] = [
        (&t, &[], 1.5),
        (&t, &["side"], 0.5),
        (&apex, &["start"], 0.5),
        (&loft, &["top"], 14.0),
    ];
    for (s, edges, r) in cases {
        let f = fillet(
            s,
            &Fillet {
                radius: r,
                edges: names(edges),
                segments: 4,
                ..Default::default()
            },
        )
        .unwrap();
        let c = chamfer(
            s,
            &Chamfer {
                distance: r,
                edges: names(edges),
                ..Default::default()
            },
        )
        .unwrap();
        for (out, kind) in [(&f, "fillet"), (&c, "chamfer")] {
            check_closed(out);
            uncrossed(out);
            let gone = on_input(s, out, kind);
            assert!(gone > 0.0, "{edges:?} {kind}: {gone}");
        }
        assert!(on_input(s, &f, "fillet") < on_input(s, &c, "chamfer"));
    }
    // Where the cuts do not fit either, the refusal stays: one of the
    // twisted prism's edges at 1.5.
    let e = t.real_edges()[4];
    let mut one = t.clone();
    one.tag_edge(e.0, e.1, &["pick"]);
    let e = fillet(
        &one,
        &Fillet {
            radius: 1.5,
            edges: names(&["pick"]),
            segments: 4,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(
        matches!(&e, BevelError::Unsupported(_, why) if why.contains("not in one plane")),
        "{e:?}"
    );
}

fn rounded(w: f64, h: f64, r: f64) -> shape_2d::Profile {
    shape_2d::rect(&shape_2d::Rect {
        width: w,
        height: h,
        radius: r,
    })
    .unwrap()
}

/// A loft of rounded rectangles `steps` rows high: two sections, 10 high.
fn rows_loft(top: (f64, f64, f64), steps: u32, height: f64) -> Solid {
    shape_gen::loft::loft(&shape_gen::loft::Loft {
        sections: vec![
            (rounded(20.0, 20.0, 4.0), 0.0),
            (rounded(top.0, top.1, top.2), height),
        ],
        steps,
        curve: shape_gen::loft::LoftCurve::Straight,
        ..Default::default()
    })
    .unwrap()
}

#[test]
fn a_wall_whose_rows_are_shallower_than_the_bevel_is_cut_across_them() {
    // The smallest form of run Z2's toaster (`docs/research.md` §7.9): a
    // loft between rounded rectangles of different corner radii, its
    // rows 2.5 high, its top rim bevelled 3 deep. The rows are twisted
    // quads, so the classic layout would tilt them, and the fold layout's
    // points fall beyond the first row: it was the tilt refusal.
    let twisted = rows_loft((16.0, 16.0, 3.0), 4, 10.0);
    // The same with straight walls (one section twice): `TooLarge`.
    let straight = rows_loft((20.0, 20.0, 4.0), 4, 10.0);
    // ... whose single-row twin bevels classically.
    let one_row = rows_loft((20.0, 20.0, 4.0), 1, 10.0);
    let bevel = |s: &Solid, kind: &str, r: f64| {
        if kind == "fillet" {
            fillet(
                s,
                &Fillet {
                    radius: r,
                    edges: names(&["top"]),
                    segments: 4,
                    ..Default::default()
                },
            )
        } else {
            chamfer(
                s,
                &Chamfer {
                    distance: r,
                    edges: names(&["top"]),
                    ..Default::default()
                },
            )
        }
    };
    for kind in ["fillet", "chamfer"] {
        let mut gone = Vec::new();
        for s in [&twisted, &straight, &one_row] {
            let out = bevel(s, kind, 3.0).unwrap();
            check_closed(&out);
            uncrossed(&out);
            canonical(&out);
            in_planes(s, &out, kind);
            gone.push(on_input(s, &out, kind));
        }
        // Straight walls: the cut rows remove exactly what the single row
        // does (the extra profiles are the strip's own interpolations).
        assert!(
            (gone[1] - gone[2]).abs() < 1e-9 * gone[2],
            "{kind}: {gone:?}"
        );
        // The twisted wall leans in (the top is smaller): a little less.
        assert!(
            gone[0] > 0.7 * gone[2] && gone[0] < gone[2],
            "{kind}: {gone:?}"
        );
    }
    // A wall too short for the bevel: `TooLarge` where it does not fit,
    // not the tilt refusal.
    let short = rows_loft((16.0, 16.0, 3.0), 4, 4.0);
    for kind in ["fillet", "chamfer"] {
        let e = bevel(&short, kind, 5.0).unwrap_err();
        assert!(matches!(e, BevelError::TooLarge(_)), "{kind}: {e:?}");
    }
}

/// A round corner of the fillet's own radius: on the bottom the offset
/// line swallows the corner's chords (a polygon of radius 5 is 4.96 to
/// its sides), and the classic layout is refused. The wide bevel fitted
/// by sliding the bottom's vertices along the rim, which twisted the
/// wall's facets over them by 0.21 (`docs/research.md` §37). The run is
/// shrunk to one point instead: the wall keeps its columns.
#[test]
fn a_fillet_as_wide_as_the_round_corner_it_follows_ends_in_a_point() {
    let plate = extrude(
        &shape_2d::rect(&shape_2d::Rect {
            width: 80.0,
            height: 50.0,
            radius: 5.0,
        })
        .unwrap(),
        &Extrude {
            height: 80.0,
            ..Default::default()
        },
    )
    .unwrap();
    let s = fillet(
        &plate,
        &Fillet {
            radius: 5.0,
            edges: names(&["bottom"]),
            ..Default::default()
        },
    )
    .unwrap();
    check_closed(&s);
    uncrossed(&s);
    // Convex still: no vertex outside any triangle's plane.
    for t in &s.triangles {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        let n = (b - a).cross(c - a).normalize();
        let out = s
            .positions
            .iter()
            .map(|&q| n.dot(q - a))
            .fold(0.0, f64::max);
        assert!(out < 1e-9, "{out} at {a:?}");
    }
    // The bottom is the rectangle between the corners' centres, less what
    // each corner's first and last chords still cut off it (a chord of
    // this polygon is 4.96 from the centre, so its offset line passes
    // 0.04 beyond): three points a corner, the chords between gone.
    let low: Vec<DVec3> = s.positions.iter().copied().filter(|p| p.z == 0.0).collect();
    assert_eq!(low.len(), 12, "{low:?}");
    assert!(low.iter().all(|p| {
        let d = DVec2::new(p.x.abs() - 35.0, p.y.abs() - 20.0);
        d.x < 1e-9 && d.y < 1e-9 && d.length() < 0.33
    }));
    // The wall's columns stand where they stood: its rim at z = 5 is on
    // the corner's own vertices.
    let rim = s
        .positions
        .iter()
        .filter(|p| (p.z - 5.0).abs() < 1e-9 && p.x > 35.0 && p.y > 20.0)
        .count();
    let top = s
        .positions
        .iter()
        .filter(|p| p.z == 80.0 && p.x > 35.0 && p.y > 20.0)
        .count();
    assert_eq!(rim, top);
    assert!(s.positions.iter().all(|p| {
        p.z != 5.0 || p.x.abs() < 35.0 || p.y.abs() < 20.0 || {
            let r = DVec2::new(p.x.abs() - 35.0, p.y.abs() - 20.0).length();
            (r - 5.0).abs() < 1e-9
        }
    }));
    // Two fillets whose offsets meet across a face are still too large.
    let e = fillet(
        &block(),
        &Fillet {
            radius: 15.0,
            edges: names(&["top"]),
            ..Default::default()
        },
    );
    assert!(matches!(e, Err(BevelError::TooLarge(_))), "{e:?}");
}

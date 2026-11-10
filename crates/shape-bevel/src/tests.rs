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
    // Wider than a segment of the bore: the offset would cross the bore's
    // own edges, which this version does not do.
    let e = fillet(
        &ring,
        &Fillet {
            radius: 1.5,
            segments: 4,
            ..Default::default()
        },
    );
    assert!(matches!(e, Err(BevelError::TooLarge(_))), "{e:?}");
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
    let r = ridge();
    let m = r.measure();
    assert!(m.closed && m.degenerate_triangles == 0, "{m:?}");
    let v = shape_core::DVec3::new(2.0, 1.0, 1.0);
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
        check(&s, "fillet");
        let c = chamfer(
            &r,
            &Chamfer {
                distance: d,
                edges: names(&["along", "split"]),
                ..Default::default()
            },
        )
        .unwrap();
        check(&c, "chamfer");
        // The curved face's corner at the vertex moves along its spoke to
        // the ridge, square to the edge, by half the offset; the flat
        // face's point on the unbevelled piece is at the offset along it,
        // and the gap takes their mean.
        let a = v + shape_core::DVec3::new(0.0, 1.0, 0.5).normalize() * (d / 2.0);
        let b = v + shape_core::DVec3::new(d, 0.0, 0.0);
        let want = (a + b) / 2.0;
        for out in [&s, &c] {
            assert!(
                out.positions.iter().any(|&p| (p - want).length() < 1e-12),
                "{want}"
            );
            // The unbevelled piece's far end and its name survive.
            assert!(
                out.positions
                    .contains(&shape_core::DVec3::new(4.0, 1.0, 1.0))
            );
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

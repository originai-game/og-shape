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

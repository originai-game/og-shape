#![allow(missing_docs)]
//! Booleans on operands that only touch (`docs/research.md` §5.4, item 5;
//! §13.6, items 1, 2 and 5): faces lying on faces, vertices a rounding
//! error off a face, edges on edges, and a tool wholly inside the target.
//! Each result is closed by `measure`, or a non-manifold union where the
//! operands touch along an edge, which the report counts; volumes keep
//! V(A ∪ B) + V(A ∩ B) = V(A) + V(B) and V(A − B) = V(A) − V(A ∩ B).

use shape_bool::{BoolError, BoolOp, BoolReport, Boolean, boolean, boolean_report};
use shape_core::DVec3;
use shape_gen::loft::{Loft, loft};
use shape_gen::solids::{BoxDesc, cuboid, torus, tube};
use shape_mesh::Solid;

fn moved(mut s: Solid, d: DVec3) -> Solid {
    s.map_positions(|p| p + d, false);
    s
}

fn block(w: f64, d: f64, h: f64) -> Solid {
    cuboid(&BoxDesc {
        width: w,
        depth: d,
        height: h,
    })
    .unwrap()
}

fn circle(r: f64) -> shape_2d::Profile {
    shape_2d::circle(&shape_2d::Circle {
        radius: r,
        segments: 0,
    })
    .unwrap()
}

/// Two lofts stacked on a shared face of different samplings (40 and 32
/// points on one 32-gon): the faces lie on one another, and many of one's
/// vertices lie within an ulp of the other's edges.
fn lofts() -> (Solid, Solid) {
    let a = loft(&Loft {
        sections: vec![(circle(10.0), 0.0), (circle(8.0), 20.0)],
        ..Default::default()
    })
    .unwrap();
    let b = loft(&Loft {
        sections: vec![(circle(8.0), 0.0), (circle(5.0), 10.0)],
        ..Default::default()
    })
    .unwrap();
    (a, moved(b, DVec3::new(0.0, 0.0, 20.0)))
}

/// A torus about y and a box over its x ≥ 0, z ≥ 0 quarter: the torus's
/// ring of vertices at 90° lies 8·10⁻¹⁴ from the box's face x = 0.
fn torus_quarter() -> (Solid, Solid) {
    let t = torus(351.0, 127.0).unwrap();
    let q = moved(block(600.0, 400.0, 600.0), DVec3::new(300.0, 0.0, 0.0));
    (t, q)
}

fn op(o: BoolOp) -> Boolean<'static> {
    Boolean {
        op: o,
        ..Default::default()
    }
}

/// Every operation closed (or empty), with the volume identities to 1e-9
/// of the operands' volume; returns the three volumes.
fn identities(a: &Solid, b: &Solid) -> [f64; 3] {
    let vol = |o: BoolOp| match boolean_report(a, b, &op(o)) {
        Ok((s, r)) => {
            let m = s.measure();
            assert!(m.closed || r.touching_edges > 0, "{o:?}: {m:?} {r:?}");
            assert_eq!(m.boundary_edges, 0, "{o:?}");
            m.volume
        }
        Err(BoolError::Empty) => 0.0,
        Err(e) => panic!("{o:?}: {e}"),
    };
    let (va, vb) = (a.measure().volume, b.measure().volume);
    let [u, i, c] = [BoolOp::Join, BoolOp::Intersect, BoolOp::Cut].map(vol);
    let tol = 1e-9 * (va + vb);
    assert!((u + i - va - vb).abs() < tol, "∪ {u} ∩ {i} a {va} b {vb}");
    assert!((c - (va - i)).abs() < tol, "− {c} ∩ {i} a {va}");
    [u, i, c]
}

#[test]
fn lofts_stacked_on_a_shared_face_join_closed() {
    let (a, b) = lofts();
    let [u, i, _] = identities(&a, &b);
    assert_eq!(i, 0.0);
    let (s, r) = boolean_report(&a, &b, &op(BoolOp::Join)).unwrap();
    let m = s.measure();
    assert!(m.closed && m.components == 1, "{m:?}");
    assert_eq!(r.touching_edges, 0);
    assert!(r.touching);
    assert!((u - a.measure().volume - b.measure().volume).abs() < 1e-9 * u);
}

#[test]
fn a_box_through_a_torus_s_ring_of_vertices_is_closed() {
    let (t, q) = torus_quarter();
    let [_, i, _] = identities(&t, &q);
    // A quarter of the torus, less the ring's sliver: 2π²Rr² / 4.
    let quarter = std::f64::consts::PI * std::f64::consts::PI * 351.0 * 127.0 * 127.0 / 2.0;
    assert!((i - quarter).abs() / quarter < 0.01, "{i} {quarter}");
}

#[test]
fn boxes_sharing_only_an_edge_are_a_non_manifold_union() {
    let a = block(10.0, 10.0, 10.0);
    let b = moved(block(10.0, 10.0, 10.0), DVec3::new(10.0, 10.0, 0.0));
    let (s, r) = boolean_report(&a, &b, &op(BoolOp::Join)).unwrap();
    let m = s.measure();
    assert!(!m.closed && m.boundary_edges == 0, "{m:?}");
    assert_eq!(r.touching_edges, m.non_manifold_edges);
    assert!(r.touching_edges > 0);
    assert!((m.volume - 2000.0).abs() < 1e-9);
    assert_eq!(
        boolean(&a, &b, &op(BoolOp::Intersect)),
        Err(BoolError::Empty)
    );
    let c = boolean(&a, &b, &op(BoolOp::Cut)).unwrap().measure();
    assert!(c.closed && (c.volume - 1000.0).abs() < 1e-9, "{c:?}");
}

#[test]
fn a_box_face_tangent_to_a_tube_joins() {
    // The tube's vertex column at (10, 0) lies on the box's face x = 10.
    let t = tube(10.0, 8.0, 30.0).unwrap();
    let b = moved(block(20.0, 20.0, 20.0), DVec3::new(20.0, 0.0, 5.0));
    identities(&t, &b);
    // Turned so that no vertex lies on the face: the face touches no
    // vertex, the tube's facets cross it by rounding or not at all.
    for k in 1..8 {
        let a = std::f64::consts::PI * k as f64 / 37.0;
        let (s, c) = (shape_core::trig::sin(a), shape_core::trig::cos(a));
        let mut b = block(20.0, 20.0, 20.0);
        b.map_positions(|p| p + DVec3::new(20.0, 0.0, 5.0), false);
        b.map_positions(
            |p| DVec3::new(c * p.x - s * p.y, s * p.x + c * p.y, p.z),
            false,
        );
        identities(&t, &b);
    }
}

#[test]
fn a_tool_wholly_inside_leaves_a_void_and_says_so() {
    let a = block(40.0, 40.0, 40.0);
    let b = moved(block(10.0, 10.0, 10.0), DVec3::new(0.0, 0.0, 15.0));
    let (s, r) = boolean_report(&a, &b, &op(BoolOp::Cut)).unwrap();
    let m = s.measure();
    assert!(m.closed && m.components == 2, "{m:?}");
    assert!((m.volume - (64_000.0 - 1000.0)).abs() < 1e-9);
    assert_eq!(
        r,
        BoolReport {
            voids: 1,
            ..Default::default()
        }
    );
    // A later boolean keeps the void.
    let c = moved(block(10.0, 10.0, 60.0), DVec3::new(15.0, 15.0, -10.0));
    let s2 = boolean(&s, &c, &op(BoolOp::Cut)).unwrap().measure();
    assert!(s2.closed, "{s2:?}");
    // The bar takes 10 × 10 × 40 from the block; the void stays.
    assert!(
        (s2.volume - (63_000.0 - 4000.0)).abs() < 1e-6,
        "{}",
        s2.volume
    );
    assert_eq!(s2.components, 2);
    // A join of the tool into the void fills it.
    let f = boolean(&s, &b, &op(BoolOp::Join)).unwrap().measure();
    assert!(
        f.closed && f.components == 1 && (f.volume - 64_000.0).abs() < 1e-9,
        "{f:?}"
    );
}

#[test]
fn a_plate_along_a_tube_s_vertex_columns_with_flush_ends_joins() {
    // Phase 6's hinged strap: the plate's top edges lie on the tube's
    // vertex columns, and the tube's ends are flush with the plate's.
    let plate = moved(block(914.4, 1.2, 38.14), DVec3::new(0.0, -2.26, 0.0));
    let mut t = tube(2.86, 1.66, 914.4).unwrap();
    t.map_positions(|p| DVec3::new(-457.2 + p.z, p.x, 38.14 + p.y), false);
    identities(&plate, &t);
    let s = boolean(&plate, &t, &op(BoolOp::Join)).unwrap().measure();
    assert!(s.closed && s.components == 1, "{s:?}");
}

#[test]
fn an_open_result_names_what_touches() {
    let e = BoolError::OpenTouching {
        boundary: 8,
        target: vec!["bottom".into(), "side".into()],
        tool: vec!["top".into()],
    };
    let m = e.to_string();
    assert!(m.contains("touch along coplanar or tangent faces"), "{m}");
    assert!(m.contains("target: bottom, side; tool: top"), "{m}");
    assert!(
        !BoolError::OpenResult { boundary: 3 }
            .to_string()
            .contains("most likely passes through itself")
    );
}

#[test]
fn polygon_lofts_stacked_on_a_shared_face_join_manifold() {
    // On main these joined with four to six edges used four times, which
    // the result's directed check let through: `closed: false`, no error.
    for sides in [6, 8] {
        let p = |r: f64| {
            shape_2d::polygon(&shape_2d::Polygon {
                sides,
                radius: r,
                ..Default::default()
            })
            .unwrap()
        };
        let a = loft(&Loft {
            sections: vec![(p(10.0), 0.0), (p(8.0), 20.0)],
            ..Default::default()
        })
        .unwrap();
        let b = loft(&Loft {
            sections: vec![(p(8.0), 0.0), (p(5.0), 15.0)],
            ..Default::default()
        })
        .unwrap();
        let b = moved(b, DVec3::new(0.0, 0.0, 20.0));
        identities(&a, &b);
        let (s, r) = boolean_report(&a, &b, &op(BoolOp::Join)).unwrap();
        let m = s.measure();
        assert!(m.closed && m.components == 1, "{sides}: {m:?}");
        assert_eq!(r.touching_edges, 0);
    }
}

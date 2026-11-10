#![allow(missing_docs)]
//! Boolean outputs that cross themselves by a rounding error, repaired by
//! `uncross_output`, and solids stacked a few ulps apart
//! (`docs/research.md` §23).

use shape_bool::{
    BoolOp, Boolean, CsgOp, CsgOutput, boolean, components, crossing_pairs, csg_points_traced,
    selfcheck, uncross_output,
};
use shape_core::DVec3;
use shape_gen::solids::{BoxDesc, CylinderDesc, cuboid, cylinder, torus};
use shape_mesh::Solid;
use std::collections::BTreeMap;

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

/// A cone frustum of 24 segments from radius `r0` at `z0` to `r1`, `h` up.
fn frustum(r0: f64, r1: f64, z0: f64, h: f64) -> Solid {
    let c = shape_2d::circle(&shape_2d::Circle {
        radius: r0,
        segments: 24,
    })
    .unwrap();
    let s = shape_gen::solids::extrude(
        &c,
        &shape_gen::solids::Extrude {
            height: h,
            taper: 1.0 - r1 / r0,
            ..Default::default()
        },
    )
    .unwrap();
    moved(s, DVec3::new(0.0, 0.0, z0))
}

/// Crossing pairs of an output, welded by exact position.
fn pairs(o: &CsgOutput) -> usize {
    let mut id: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    let mut pts = Vec::new();
    let w: Vec<u32> = o
        .points
        .iter()
        .map(|p| {
            *id.entry(p.to_array().map(|x| (x + 0.0).to_bits()))
                .or_insert_with(|| {
                    pts.push(*p);
                    pts.len() as u32 - 1
                })
        })
        .collect();
    let tris: Vec<[u32; 3]> = o.tris.iter().map(|t| t.map(|v| w[v as usize])).collect();
    selfcheck::self_crossings(&pts, &tris).len()
}

fn volume(o: &CsgOutput) -> f64 {
    o.tris
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|v| o.points[v as usize]);
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

/// The output as built crosses itself; repaired, it does not, is closed,
/// and keeps its volume. Returns what the repair did.
fn repaired(a: &Solid, b: &Solid, op: CsgOp) -> shape_bool::Uncrossed {
    let o = csg_points_traced(
        (&a.positions, &a.triangles),
        (&b.positions, &b.triangles),
        op,
    )
    .unwrap();
    assert!(pairs(&o) > 0, "the case no longer crosses itself");
    let (u, r) = uncross_output(&a.positions, &b.positions, o.clone());
    assert_eq!(pairs(&u), 0, "{r:?}");
    assert_eq!((u.unmatched, u.non_manifold, u.touching_edges), (0, 0, 0));
    let (v0, v1) = (volume(&o), volume(&u));
    assert!((v0 - v1).abs() <= 1e-9 * v0.abs(), "{v0} {v1}");
    r
}

/// A pin standing on a plate, its vertex column an ulp off the plate's
/// diagonal (the post of `docs/research.md` §7.7): two points 3·10⁻¹⁵
/// apart, merged.
#[test]
fn the_post_s_points_an_ulp_apart_are_merged() {
    let plate = block(20.0, 20.0, 10.0);
    let pin = moved(
        cylinder(&CylinderDesc {
            radius: 3.0,
            height: 8.0,
            ..Default::default()
        })
        .unwrap(),
        DVec3::new(0.0, 0.0, 10.0),
    );
    let r = repaired(&plate, &pin, CsgOp::Union);
    assert!(r.collapsed > 0, "{r:?}");
}

/// A countersink whose narrow end is the bore's own polygon: needles
/// along its ring edges, split across their long edge.
#[test]
fn the_countersink_ring_s_needles_are_split() {
    let plate = block(40.0, 40.0, 10.0);
    let bored = boolean(
        &plate,
        &frustum(2.0, 2.0, -30.0, 40.06),
        &Boolean {
            op: BoolOp::Cut,
            ..Default::default()
        },
    )
    .unwrap();
    let r = repaired(&bored, &frustum(2.0, 4.06, 8.0, 2.06), CsgOp::Subtract);
    assert!(r.split > 0, "{r:?}");
}

/// The torus's ring of vertices 8·10⁻¹⁴ off the box face.
#[test]
fn the_torus_ring_on_a_box_face_does_not_cross_itself() {
    let t = torus(351.0, 127.0).unwrap();
    let q = moved(block(600.0, 400.0, 600.0), DVec3::new(300.0, 0.0, 0.0));
    repaired(&t, &q, CsgOp::Intersect);
}

/// An output that does not cross itself comes back byte for byte.
#[test]
fn a_clean_output_is_unchanged() {
    let a = block(20.0, 20.0, 10.0);
    let b = moved(block(5.0, 7.0, 30.0), DVec3::new(1.25, -2.5, -10.0));
    let o = csg_points_traced(
        (&a.positions, &a.triangles),
        (&b.positions, &b.triangles),
        CsgOp::Subtract,
    )
    .unwrap();
    let (u, r) = uncross_output(&a.positions, &b.positions, o.clone());
    assert_eq!(u, o);
    assert_eq!((r.before, r.after), (0, 0));
}

/// A slab on a slab, its bottom two ulps above the other's top: the probe
/// a millionth of the size off the slab's face reached into the other and
/// the union lost a slab. A region that meets nothing is classified
/// exactly, and the snap makes the two one shell.
#[test]
fn slabs_stacked_two_ulps_apart_join_whole() {
    let a = block(10.0, 10.0, 1.0);
    let top = a.positions.iter().map(|p| p.z).fold(f64::MIN, f64::max);
    let gap = top.next_up().next_up();
    let low = a.positions.iter().map(|p| p.z).fold(f64::MAX, f64::min);
    let mut b = moved(a.clone(), DVec3::new(0.0, 0.0, top - low));
    let bottom = b.positions.iter().map(|p| p.z).fold(f64::MAX, f64::min);
    assert_eq!(bottom, top);
    b.map_positions(
        |p| {
            if p.z == top {
                DVec3::new(p.x, p.y, gap)
            } else {
                p
            }
        },
        false,
    );
    let s = boolean(&a, &b, &Boolean::default()).unwrap();
    let m = s.measure();
    assert!(m.closed);
    assert!((m.volume - 200.0).abs() < 1e-9, "{}", m.volume);
    assert_eq!(components(&s).len(), 1);
    assert_eq!(crossing_pairs(&s), 0);
}

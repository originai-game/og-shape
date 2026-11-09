#![allow(missing_docs)]
use shape_bool::{CsgOp, csg_points};
use shape_core::DVec3;
use shape_gen::solids::{BoxDesc, cuboid, torus};
use shape_mesh::Solid;
use std::collections::BTreeMap;

fn moved(mut s: Solid, d: DVec3) -> Solid { s.map_positions(|p| p + d, false); s }

fn lofts() -> (Solid, Solid) {
    use shape_gen::loft::{Loft, loft};
    let c = |r: f64| shape_2d::circle(&shape_2d::Circle { radius: r, segments: 0 }).unwrap();
    let a = loft(&Loft { sections: vec![(c(10.0), 0.0), (c(8.0), 20.0)], ..Default::default() }).unwrap();
    let b = loft(&Loft { sections: vec![(c(8.0), 0.0), (c(5.0), 10.0)], ..Default::default() }).unwrap();
    (a, moved(b, DVec3::new(0.0, 0.0, 20.0)))
}

fn report(name: &str, a: &Solid, b: &Solid, op: CsgOp) {
    let (pts, tris, _) = csg_points((&a.positions, &a.triangles), (&b.positions, &b.triangles), op).unwrap();
    let mut d: BTreeMap<(u32, u32), i64> = BTreeMap::new();
    let mut u: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for t in &tris { for k in 0..3 { let (x, y) = (t[k], t[(k+1)%3]); *d.entry((x.min(y), x.max(y))).or_default() += if x<y {1} else {-1}; *u.entry((x.min(y), x.max(y))).or_default() += 1; } }
    let open: Vec<_> = d.iter().filter(|(_, n)| **n != 0).collect();
    let nm = u.values().filter(|&&n| n > 2).count();
    println!("{name} {op:?}: tris {} open {} nonmanifold {}", tris.len(), open.len(), nm);
    for (e, n) in open.iter().take(12) { println!("  {:?} {:?} {}", pts[e.0 as usize], pts[e.1 as usize], n); }
}

#[test]
fn dbg() {
    let (a, b) = lofts();
    if std::env::var("CASE").map_or(true, |c| c == "lofts") { for op in [CsgOp::Union, CsgOp::Intersect, CsgOp::Subtract] { report("lofts", &a, &b, op); } }
    let t = torus(351.0, 127.0).unwrap();
    let q = moved(cuboid(&BoxDesc { width: 600.0, depth: 400.0, height: 600.0 }).unwrap(), DVec3::new(300.0, 0.0, 0.0));
    if std::env::var("CASE").map_or(true, |c| c == "torus") { for op in [CsgOp::Union, CsgOp::Intersect, CsgOp::Subtract] { report("torus", &t, &q, op); } }
}

fn nm_report(name: &str, s: &Solid) {
    let m = s.measure();
    println!("{name}: closed {} boundary {} nm {} degen {} vol {}", m.closed, m.boundary_edges, m.non_manifold_edges, m.degenerate_triangles, m.volume);
    let mut u: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (i, t) in s.triangles.iter().enumerate() { for k in 0..3 { let (x, y) = (t[k], t[(k+1)%3]); u.entry((x.min(y), x.max(y))).or_default().push(i); } }
    for (e, ts) in &u { if ts.len() > 2 { println!("  nm edge {:?} {:?} tris {}", s.positions[e.0 as usize], s.positions[e.1 as usize], ts.len()); for &t in ts { let tt = s.triangles[t]; println!("     {:?}", tt.map(|v| s.positions[v as usize])); } } }
}

#[test]
fn dbg_named() {
    use shape_bool::{BoolOp, Boolean, boolean};
    let (a, b) = lofts();
    if std::env::var("CASE").map_or(true, |c| c == "lofts") {
        for op in [BoolOp::Join, BoolOp::Cut] {
            match boolean(&a, &b, &Boolean { op, ..Default::default() }) { Ok(s) => nm_report(&format!("lofts {op:?}"), &s), Err(e) => println!("lofts {op:?}: {e}") }
        }
    }
}

fn straps(py: f64) -> (Solid, Solid) {
    let plate = moved(cuboid(&BoxDesc { width: 914.4, depth: 1.2, height: 38.14 }).unwrap(), DVec3::new(0.0, -py, 0.0));
    let mut t = shape_gen::solids::tube(2.86, 1.66, 914.4).unwrap();
    t.map_positions(|p| DVec3::new(-457.2, 0.0, 38.14) + DVec3::new(0.0, 1.0, 0.0) * p.x + DVec3::new(0.0, 0.0, 1.0) * p.y + DVec3::new(1.0, 0.0, 0.0) * p.z, false);
    (plate, t)
}

#[test]
fn dbg_straps() {
    use shape_bool::{BoolOp, Boolean, boolean};
    for py in [2.86 - 0.6] {
        let (a, b) = straps(py);
        for op in [BoolOp::Join, BoolOp::Intersect, BoolOp::Cut] {
            match boolean(&a, &b, &Boolean { op, ..Default::default() }) { Ok(s) => nm_report(&format!("straps {py} {op:?}"), &s), Err(e) => println!("straps {py} {op:?}: {e}") }
        }
    }
}

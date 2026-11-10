//! Boolean operations on two closed meshes (union, subtract, intersect),
//! each placed by a transform first. The result is a closed mesh when
//! both inputs are (every edge used twice, in opposite directions).
//!
//! The method, in four steps:
//!
//! 1. Each mesh is welded by exact position, and every pair of triangles
//!    whose boxes overlap is intersected. Which side of a plane a vertex
//!    lies on, whether an edge crosses a triangle and through which of its
//!    features, are all decided by Shewchuk's exact predicates (`robust`'s
//!    `orient3d` and `orient2d`), so two hosts make the same decisions.
//!    A crossing point is computed once, from inputs in a fixed order, and
//!    named by what made it (an edge and a triangle, or two edges), so the
//!    triangles that share it share the same point, bit for bit. Two that
//!    round to one position are one point.
//! 2. Each triangle crossed is cut along its intersection segments by a
//!    constrained Delaunay triangulation (spade) in its own plane, with its
//!    edges, and the points on them, as the outline.
//! 3. The pieces of each mesh are grouped into regions, pieces joined by an
//!    edge no intersection runs along, and each region is classified once:
//!    the other mesh's winding number just in front of and just behind its
//!    largest piece. Two coincident faces see the other surface change
//!    across them; the operation keeps one of them or neither.
//! 4. The kept pieces are emitted, a subtracted mesh's turned inside out,
//!    with the normals, uvs and colours of the triangles they came from.
//!
//! Crossing points and winding numbers are computed in f64 with `+ - * /`,
//! `sqrt` and [`crate::trig::atan2`]: the same on every host.
//!
//! [`csg_points`] runs these steps twice when the first result is open
//! (operands that touch: faces on faces, vertices within an ulp of a face).
//! The second pass orders points on an edge exactly (`exact`), lays a
//! triangle out exactly where its rounded outline is not simple, makes
//! points at one position one point, and classifies faces lying on the
//! other operand by orientation (`docs/research.md` §5.4, item 5). When
//! both leave it open, the tool's vertices within 2⁻⁴⁰ of the operands'
//! size of a target vertex are snapped onto it and both passes run again
//! (faces a few ulps apart, placed by different rounded motions;
//! `docs/research.md` §18).

use glam::{DVec2, DVec3};
use robust::{Coord, Coord3D, orient2d, orient3d};
use shape_gen::{Mesh, Placement, normalize, place, trig};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use std::collections::{BTreeMap, BTreeSet};

use crate::{exact, selfcheck, sliver};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CsgOp {
    Union,
    Subtract,
    Intersect,
}

/// The attributes at a triangle's corner.
#[derive(Clone, Copy)]
struct Corner {
    normal: DVec3,
    uv: [f64; 2],
    color: [f64; 4],
}

/// A mesh welded by exact position: `tris` hold global point ids.
struct Solid {
    tris: Vec<[u32; 3]>,
    corners: Vec<[Corner; 3]>,
    /// The input triangle each one is (degenerate ones are dropped).
    source: Vec<u32>,
    min: DVec3,
    max: DVec3,
}

/// Two triangles, each `(mesh, triangle)`.
type TriPair = ((u8, u32), (u8, u32));

/// A region's boundary: its edges by sorted ids, each with how many times
/// more it runs low to high than back.
type Boundary = Vec<((u32, u32), i32)>;

/// An edge of mesh `0` (A) or `1` (B), by its sorted point ids.
type EdgeKey = (u8, u32, u32);

fn edge_key(mesh: u8, a: u32, b: u32) -> EdgeKey {
    (mesh, a.min(b), a.max(b))
}

/// What names a crossing point.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Origin {
    /// An edge through a triangle's inside (mesh, triangle index).
    EdgeTri(EdgeKey, u8, u32),
    /// Two edges through each other (A's first).
    EdgeEdge(EdgeKey, EdgeKey),
    /// Three triangles of one mesh through one point (a self-union), in
    /// increasing order.
    Triple(u32, u32, u32),
}

/// Where a point lies in a triangle.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Loc {
    Inside,
    Edge(usize),
    Vertex(usize),
}

#[derive(Default)]
struct Cut {
    points: Vec<u32>,
    segments: Vec<[u32; 2]>,
    /// The triangle (of the other mesh, or of this one in a self-union)
    /// whose crossing made each segment.
    src: Vec<u32>,
}

struct Csg {
    points: Vec<DVec3>,
    /// The edges each point lies on (beyond its own mesh's corners).
    on: Vec<Vec<EdgeKey>>,
    made: BTreeMap<Origin, u32>,
    /// What made each point (None for the operands' own vertices); a point
    /// two crossings rounded to keeps the first.
    origin_of: Vec<Option<Origin>>,
    /// Each made point by its exact position.
    made_at: BTreeMap<[u64; 3], u32>,
    solids: [Solid; 2],
    cuts: [BTreeMap<u32, Cut>; 2],
    crossings: BTreeSet<(u32, u32)>,
    /// Pairs of triangles cut in one plane (`coplanar`), each as
    /// `((mesh, tri), (mesh, tri))`: where one surface lies on another.
    coplanar_pairs: Vec<TriPair>,
    /// Pairs of triangles that meet without crossing: one lies on one side
    /// of the other's plane and touches it (a vertex or an edge on it).
    touch_pairs: Vec<TriPair>,
    /// The second pass, for operands that touch (`docs/research.md` §5.4,
    /// item 5): crossings that round to one position stay two points,
    /// points on an edge are ordered exactly, and a triangle whose rounded
    /// outline spade cannot keep is triangulated in an exact layout.
    robust: bool,
    /// Second pass: pairs of points that one triangle's layout could not
    /// tell apart; they are made one point everywhere ([`merge_points`]).
    merges: std::cell::RefCell<Vec<(u32, u32)>>,
    /// Second pass: each cut triangle's outline, in order.
    chains: std::cell::RefCell<Vec<Vec<u32>>>,
}

fn c3(p: DVec3) -> Coord3D<f64> {
    Coord3D {
        x: p.x,
        y: p.y,
        z: p.z,
    }
}

fn sgn(v: f64) -> i32 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

/// The axis a triangle's normal leans along most: dropping it projects the
/// triangle without folding it (and exactly: no arithmetic is done).
fn drop_axis(n: DVec3) -> usize {
    let a = n.abs();
    if a.x >= a.y && a.x >= a.z {
        0
    } else if a.y >= a.z {
        1
    } else {
        2
    }
}

fn project(p: DVec3, axis: usize) -> DVec2 {
    match axis {
        0 => DVec2::new(p.y, p.z),
        1 => DVec2::new(p.z, p.x),
        _ => DVec2::new(p.x, p.y),
    }
}

fn o2(a: DVec2, b: DVec2, c: DVec2) -> i32 {
    sgn(orient2d(
        Coord { x: a.x, y: a.y },
        Coord { x: b.x, y: b.y },
        Coord { x: c.x, y: c.y },
    ))
}

impl Csg {
    fn p(&self, i: u32) -> DVec3 {
        self.points[i as usize]
    }

    /// The triangle's normal, each component the triangle's signed area
    /// in one axis plane by `orient2d`: its sign is exact, so a sliver
    /// whose cross product cancels to zero in f64 still has a normal.
    fn tri_normal(&self, t: [u32; 3]) -> DVec3 {
        let [a, b, c] = t.map(|i| self.p(i));
        let area = |axis: usize| {
            let [a, b, c] = [a, b, c].map(|p| project(p, axis));
            orient2d(
                Coord { x: a.x, y: a.y },
                Coord { x: b.x, y: b.y },
                Coord { x: c.x, y: c.y },
            )
        };
        DVec3::new(area(0), area(1), area(2))
    }

    fn add_point(&mut self, origin: Origin, p: impl FnOnce(&Self) -> DVec3, on: &[EdgeKey]) -> u32 {
        if let Some(&id) = self.made.get(&origin) {
            return id;
        }
        let pos = p(self);
        // Two crossings a rounding error apart can round to one position.
        // They are made one point, on the edges of both, so the triangles
        // of both meshes cut at it agree on it.
        let key = pos.to_array().map(|x| (x + 0.0).to_bits());
        if !self.robust
            && let Some(&id) = self.made_at.get(&key)
        {
            for &e in on {
                self.note_on(id, e);
            }
            self.made.insert(origin, id);
            return id;
        }
        let id = self.points.len() as u32;
        self.points.push(pos);
        self.on.push(on.to_vec());
        self.origin_of.push(Some(origin));
        self.made.insert(origin, id);
        self.made_at.insert(key, id);
        id
    }

    fn note_on(&mut self, id: u32, e: EdgeKey) {
        let v = &mut self.on[id as usize];
        if !v.contains(&e) {
            v.push(e);
        }
    }

    /// Where `p` (exactly in the triangle's plane) lies in triangle `t`.
    fn locate(&self, t: [u32; 3], p: DVec3) -> Option<Loc> {
        let axis = drop_axis(self.tri_normal(t));
        let q = project(p, axis);
        let v = t.map(|i| project(self.p(i), axis));
        let o = [o2(v[0], v[1], q), o2(v[1], v[2], q), o2(v[2], v[0], q)];
        if o.iter().any(|&s| s > 0) && o.iter().any(|&s| s < 0) {
            return None;
        }
        let zeros: Vec<usize> = (0..3).filter(|&k| o[k] == 0).collect();
        Some(match zeros.as_slice() {
            [] => Loc::Inside,
            [k] => Loc::Edge(*k),
            [a, b] => Loc::Vertex(if (*a + 1) % 3 == *b { *b } else { *a }),
            _ => return None,
        })
    }

    /// The point where edge `(p, q)` of mesh `m` crosses triangle `tri` of
    /// mesh `other`, whose plane it crosses strictly. None when it misses.
    fn edge_cross(&mut self, m: u8, p: u32, q: u32, other: u8, tri: u32) -> Option<u32> {
        let t = self.solids[other as usize].tris[tri as usize];
        let e = edge_key(m, p, q);
        let (p, q) = (e.1, e.2);
        let (pp, pq) = (self.p(p), self.p(q));
        let s: [i32; 3] = std::array::from_fn(|k| {
            sgn(orient3d(
                c3(pp),
                c3(pq),
                c3(self.p(t[k])),
                c3(self.p(t[(k + 1) % 3])),
            ))
        });
        if s.iter().any(|&x| x > 0) && s.iter().any(|&x| x < 0) {
            return None;
        }
        let zeros: Vec<usize> = (0..3).filter(|&k| s[k] == 0).collect();
        match zeros.as_slice() {
            [] => {
                let id = self.add_point(
                    Origin::EdgeTri(e, other, tri),
                    |c| {
                        let (a, b, cc) = (c.p(t[0]), c.p(t[1]), c.p(t[2]));
                        let n = (b - a).cross(cc - a);
                        let (dp, dq) = (n.dot(pp - a), n.dot(pq - a));
                        pp + (pq - pp) * (dp / (dp - dq))
                    },
                    &[e],
                );
                Some(id)
            }
            [k] => {
                let f = edge_key(other, t[*k], t[(*k + 1) % 3]);
                Some(self.edge_edge(e, f))
            }
            [a, b] => {
                // Through the vertex the two edges share.
                let v = t[if (*a + 1) % 3 == *b { *b } else { *a }];
                self.note_on(v, e);
                Some(v)
            }
            _ => None,
        }
    }

    /// The point where two edges cross (between meshes: A's first).
    fn edge_edge(&mut self, e: EdgeKey, f: EdgeKey) -> u32 {
        let (e, f) = if e <= f { (e, f) } else { (f, e) };
        self.add_point(
            Origin::EdgeEdge(e, f),
            |c| {
                let (p1, q1, p2, q2) = (c.p(e.1), c.p(e.2), c.p(f.1), c.p(f.2));
                let (d1, d2, r) = (q1 - p1, q2 - p2, p1 - p2);
                let (a, ee, ff) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
                let (cc, b) = (d1.dot(r), d1.dot(d2));
                let den = a * ee - b * b;
                let s = if den != 0.0 {
                    ((b * ff - cc * ee) / den).clamp(0.0, 1.0)
                } else {
                    0.5
                };
                p1 + d1 * s
            },
            &[e, f],
        )
    }

    fn cut(&mut self, m: u8, tri: u32) -> &mut Cut {
        self.cuts[m as usize].entry(tri).or_default()
    }

    fn add_points(&mut self, m: u8, tri: u32, ids: &[u32]) {
        let c = self.cut(m, tri);
        for &i in ids {
            if !c.points.contains(&i) {
                c.points.push(i);
            }
        }
    }

    fn add_segment(&mut self, m: u8, tri: u32, a: u32, b: u32, src: u32) {
        if a == b {
            return;
        }
        self.add_points(m, tri, &[a, b]);
        let c = self.cut(m, tri);
        c.segments.push([a, b]);
        c.src.push(src);
        self.crossings.insert((a.min(b), a.max(b)));
    }

    /// Intersects triangle `ia` of mesh `ma` with triangle `ib` of mesh
    /// `mb` (A with B, or two of one mesh's for a self-union).
    fn pair(&mut self, (ma, ia): (u8, u32), (mb, ib): (u8, u32)) {
        let ta = self.solids[ma as usize].tris[ia as usize];
        let tb = self.solids[mb as usize].tris[ib as usize];
        let side = |c: &Self, plane: [u32; 3], v: u32| {
            sgn(orient3d(
                c3(c.p(plane[0])),
                c3(c.p(plane[1])),
                c3(c.p(plane[2])),
                c3(c.p(v)),
            ))
        };
        let oa = ta.map(|v| side(self, tb, v));
        let ob = tb.map(|v| side(self, ta, v));
        let one_side = |o: [i32; 3]| o.iter().all(|&s| s > 0) || o.iter().all(|&s| s < 0);
        if one_side(oa) || one_side(ob) {
            return;
        }
        if oa == [0; 3] {
            self.coplanar((ma, ia), (mb, ib));
            return;
        }
        let touch = |o: [i32; 3]| o.iter().all(|&s| s >= 0) || o.iter().all(|&s| s <= 0);
        if touch(oa) || touch(ob) {
            self.touch_pairs.push(((ma, ia), (mb, ib)));
        }
        let mut found: Vec<u32> = Vec::new();
        // Vertices of one triangle in the other's plane and inside it.
        for (t, o, ot, om) in [(ta, oa, tb, mb), (tb, ob, ta, ma)] {
            for k in 0..3 {
                if o[k] != 0 {
                    continue;
                }
                let v = t[k];
                match self.locate(ot, self.p(v)) {
                    None => {}
                    Some(Loc::Edge(j)) => {
                        self.note_on(v, edge_key(om, ot[j], ot[(j + 1) % 3]));
                        found.push(v);
                    }
                    // At the other's vertex (two meshes' vertices at one
                    // position keep their own ids) or inside it.
                    Some(Loc::Vertex(_) | Loc::Inside) => found.push(v),
                }
            }
        }
        // Edges of one triangle through the other.
        for (m, t, o, om, other) in [(ma, ta, oa, mb, ib), (mb, tb, ob, ma, ia)] {
            for k in 0..3 {
                let (i, j) = (k, (k + 1) % 3);
                if o[i] * o[j] < 0
                    && let Some(id) = self.edge_cross(m, t[i], t[j], om, other)
                {
                    found.push(id);
                }
            }
        }
        let mut uniq: Vec<u32> = Vec::new();
        for id in found {
            if !uniq.contains(&id) {
                uniq.push(id);
            }
        }
        let dir = self.tri_normal(ta).cross(self.tri_normal(tb));
        shape_core::sort::sort_by(&mut uniq, |&a, &b| {
            self.p(a)
                .dot(dir)
                .total_cmp(&self.p(b).dot(dir))
                .then(a.cmp(&b))
        });
        self.add_points(ma, ia, &uniq);
        self.add_points(mb, ib, &uniq);
        for w in uniq.windows(2) {
            self.add_segment(ma, ia, w[0], w[1], ib);
            self.add_segment(mb, ib, w[0], w[1], ia);
        }
    }

    /// Two triangles in one plane: each one's edges inside the other are
    /// cut into it, and the points where their edges cross.
    fn coplanar(&mut self, (ma, ia): (u8, u32), (mb, ib): (u8, u32)) {
        self.coplanar_pairs.push(((ma, ia), (mb, ib)));
        let ta = self.solids[ma as usize].tris[ia as usize];
        let tb = self.solids[mb as usize].tris[ib as usize];
        let axis = drop_axis(self.tri_normal(ta));
        let pr = |c: &Self, i: u32| project(c.p(i), axis);
        // Edge crossings, for each triangle (slot 0: `ta`, 1: `tb`).
        let mut on_edge: [Vec<(u32, EdgeKey)>; 2] = [Vec::new(), Vec::new()];
        for i in 0..3 {
            let (a0, a1) = (ta[i], ta[(i + 1) % 3]);
            for j in 0..3 {
                let (b0, b1) = (tb[j], tb[(j + 1) % 3]);
                let s1 = o2(pr(self, a0), pr(self, a1), pr(self, b0));
                let s2 = o2(pr(self, a0), pr(self, a1), pr(self, b1));
                let s3 = o2(pr(self, b0), pr(self, b1), pr(self, a0));
                let s4 = o2(pr(self, b0), pr(self, b1), pr(self, a1));
                if s1 * s2 < 0 && s3 * s4 < 0 {
                    let (e, f) = (edge_key(ma, a0, a1), edge_key(mb, b0, b1));
                    let id = self.edge_edge(e, f);
                    self.add_points(ma, ia, &[id]);
                    self.add_points(mb, ib, &[id]);
                    on_edge[0].push((id, e));
                    on_edge[1].push((id, f));
                }
            }
        }
        // Vertices of each inside the other.
        for (slot, m, t, ot, om, oi) in [(0, ma, ta, tb, mb, ib), (1, mb, tb, ta, ma, ia)] {
            for &v in &t {
                if let Some(loc) = self.locate(ot, self.p(v)) {
                    if let Loc::Edge(j) = loc {
                        self.note_on(v, edge_key(om, ot[j], ot[(j + 1) % 3]));
                    }
                    self.add_points(om, oi, &[v]);
                    // The vertex lies on its own triangle's two edges.
                    for k in 0..3 {
                        if t[k] == v {
                            on_edge[slot].push((v, edge_key(m, t[k], t[(k + 1) % 3])));
                            on_edge[slot].push((v, edge_key(m, t[(k + 2) % 3], t[k])));
                        }
                    }
                }
            }
        }
        // Each edge's pieces inside the other triangle become its cuts.
        for (slot, m, t, ot, om, oi, ti) in
            [(0, ma, ta, tb, mb, ib, ia), (1, mb, tb, ta, ma, ia, ib)]
        {
            for k in 0..3 {
                let e = edge_key(m, t[k], t[(k + 1) % 3]);
                let (p0, p1) = (self.p(e.1), self.p(e.2));
                let mut along: Vec<u32> = on_edge[slot]
                    .iter()
                    .filter(|x| x.1 == e)
                    .map(|x| x.0)
                    .collect();
                // Vertices of the other triangle lying on this edge.
                for &v in &ot {
                    if self.on[v as usize].contains(&e) {
                        along.push(v);
                    }
                }
                shape_core::sort::sort_by(&mut along, |&a, &b| {
                    (self.p(a) - p0)
                        .dot(p1 - p0)
                        .total_cmp(&(self.p(b) - p0).dot(p1 - p0))
                        .then(a.cmp(&b))
                });
                along.dedup();
                for w in along.windows(2) {
                    let mid = (self.p(w[0]) + self.p(w[1])) * 0.5;
                    let q = project(mid, axis);
                    let v = ot.map(|i| project(self.p(i), axis));
                    let o = [o2(v[0], v[1], q), o2(v[1], v[2], q), o2(v[2], v[0], q)];
                    if !(o.iter().any(|&s| s > 0) && o.iter().any(|&s| s < 0)) {
                        self.add_segment(om, oi, w[0], w[1], ti);
                        self.crossings.insert((w[0].min(w[1]), w[0].max(w[1])));
                    }
                }
            }
        }
    }

    /// The point where the planes of three triangles of mesh 0 meet
    /// (Cramer's rule, in a fixed order); None if two are parallel.
    fn three_planes(&self, t: [u32; 3]) -> Option<DVec3> {
        let plane = |i: u32| {
            let [a, b, c] = self.solids[0].tris[i as usize].map(|v| self.p(v));
            let n = (b - a).cross(c - a);
            (n, n.dot(a))
        };
        let [(n1, d1), (n2, d2), (n3, d3)] = t.map(plane);
        let det = n1.dot(n2.cross(n3));
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let p = (n2.cross(n3) * d1 + n3.cross(n1) * d2 + n1.cross(n2) * d3) / det;
        p.is_finite().then_some(p)
    }

    /// A self-union's triple points. Where two crossing segments inside one
    /// triangle, made by two other triangles, cross each other, the three
    /// triangles meet in a point. It is made once, named by the three, and
    /// each of them splits its two segments there, so all three are cut
    /// alike whichever of them saw the crossing (two meshes never need
    /// this: neither crosses itself).
    fn triples(&mut self) {
        let mut found: BTreeSet<[u32; 3]> = BTreeSet::new();
        for (&t, cut) in &self.cuts[0] {
            let axis = drop_axis(self.tri_normal(self.solids[0].tris[t as usize]));
            let pr = |k: u32| project(self.p(k), axis);
            for i in 0..cut.segments.len() {
                for j in i + 1..cut.segments.len() {
                    let (u, v) = (cut.src[i], cut.src[j]);
                    let ([a, b], [p, q]) = (cut.segments[i], cut.segments[j]);
                    if u == v || a == p || a == q || b == p || b == q {
                        continue;
                    }
                    let (s1, s2) = (o2(pr(a), pr(b), pr(p)), o2(pr(a), pr(b), pr(q)));
                    let (s3, s4) = (o2(pr(p), pr(q), pr(a)), o2(pr(p), pr(q), pr(b)));
                    if s1 * s2 < 0 && s3 * s4 < 0 {
                        let mut k = [t, u, v];
                        k.sort_unstable();
                        found.insert(k);
                    }
                }
            }
        }
        for k in found {
            let Some(pos) = self.three_planes(k) else {
                continue;
            };
            let id = self.add_point(Origin::Triple(k[0], k[1], k[2]), |_| pos, &[]);
            for (x, y, z) in [(k[0], k[1], k[2]), (k[1], k[0], k[2]), (k[2], k[0], k[1])] {
                self.split(x, y, id);
                self.split(x, z, id);
            }
        }
    }

    /// Splits at point `id` the segment of triangle `tri` (mesh 0) made by
    /// triangle `src`: of several, the one `id` lies nearest.
    fn split(&mut self, tri: u32, src: u32, id: u32) {
        let Some(cut) = self.cuts[0].get(&tri) else {
            return;
        };
        let p = self.p(id);
        let detour = |s: [u32; 2]| {
            let (a, b) = (self.p(s[0]), self.p(s[1]));
            (a - p).length() + (p - b).length() - (a - b).length()
        };
        let mut best: Option<(usize, f64)> = None;
        for (i, (&s, &from)) in cut.segments.iter().zip(&cut.src).enumerate() {
            if from != src || s.contains(&id) {
                continue;
            }
            let d = detour(s);
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((i, d));
            }
        }
        let Some((i, _)) = best else {
            return;
        };
        let cut = self.cuts[0].get_mut(&tri).expect("cut");
        let [a, b] = cut.segments[i];
        cut.segments[i] = [a, id];
        cut.segments.push([id, b]);
        cut.src.push(src);
        if !cut.points.contains(&id) {
            cut.points.push(id);
        }
        self.crossings.insert((a.min(id), a.max(id)));
        self.crossings.insert((id.min(b), id.max(b)));
    }

    /// [`Csg::pieces`] in the first pass: positions as rounded, and spade
    /// keeps what it can (the result's check refuses an open result).
    fn pieces_plain(&self, m: u8, tri: u32) -> Vec<[u32; 3]> {
        let t = self.solids[m as usize].tris[tri as usize];
        let Some(cut) = self.cuts[m as usize].get(&tri) else {
            return vec![t];
        };
        let normal = self.tri_normal(t);
        let axis = drop_axis(normal);
        // The outline: each edge with the points on it, in order.
        let mut chain: Vec<u32> = Vec::new();
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let e = edge_key(m, a, b);
            let (pa, pb) = (self.p(a), self.p(b));
            let mut along: Vec<u32> = cut
                .points
                .iter()
                .copied()
                .filter(|&i| i != a && i != b && self.on[i as usize].contains(&e))
                .collect();
            shape_core::sort::sort_by(&mut along, |&x, &y| {
                (self.p(x) - pa)
                    .dot(pb - pa)
                    .total_cmp(&(self.p(y) - pa).dot(pb - pa))
                    .then(x.cmp(&y))
            });
            chain.push(a);
            chain.extend(along);
        }
        // A point made inside the triangle (an edge through it) can round to
        // a position on or past its outline, as seen in the projection: past
        // a corner's edge, or past a point rounded off an edge. Its segments
        // would then run along the outline or through an outline point, and
        // spade would merge or drop them, so regions would leak or pieces be
        // lost. It is placed in the triangulation just inside the outline
        // instead; its own position (shared with the other mesh) is
        // unchanged, as are which segments end at it (`docs/research.md`
        // §5.4, follow-up 4).
        let outline: Vec<DVec2> = chain.iter().map(|&i| project(self.p(i), axis)).collect();
        let v = t.map(|i| project(self.p(i), axis));
        let turn = o2(v[0], v[1], v[2]);
        let centre = (v[0] + v[1] + v[2]) / 3.0;
        let at = |i: u32| {
            let q = project(self.p(i), axis);
            // A point of the other mesh's at one of ours keeps its place, so
            // spade merges the two.
            let shared = chain.iter().any(|&j| self.p(j) == self.p(i));
            if turn == 0 || shared || chain.contains(&i) || strictly_inside(&outline, q) {
                return q;
            }
            // The smallest step towards the centre, by powers of two, that
            // puts it strictly inside.
            let mut step = f64::EPSILON;
            while step < 0.5 {
                let r = q + (centre - q) * step;
                if strictly_inside(&outline, r) {
                    return r;
                }
                step *= 2.0;
            }
            centre
        };
        let mut cdt: ConstrainedDelaunayTriangulation<Point2<f64>> =
            ConstrainedDelaunayTriangulation::new();
        let mut id_of: BTreeMap<usize, u32> = BTreeMap::new();
        let mut handle_of: BTreeMap<u32, spade::handles::FixedVertexHandle> = BTreeMap::new();
        for &i in chain.iter().chain(cut.points.iter()) {
            if handle_of.contains_key(&i) {
                continue;
            }
            let q = at(i);
            let Ok(h) = cdt.insert(Point2::new(q.x, q.y)) else {
                continue;
            };
            id_of.entry(h.index()).or_insert(i);
            handle_of.insert(i, h);
        }
        let mut boundary: BTreeSet<(usize, usize)> = BTreeSet::new();
        for k in 0..chain.len() {
            let (a, b) = (chain[k], chain[(k + 1) % chain.len()]);
            let (Some(&ha), Some(&hb)) = (handle_of.get(&a), handle_of.get(&b)) else {
                continue;
            };
            if ha != hb {
                cdt.try_add_constraint(ha, hb);
                boundary.insert((ha.index().min(hb.index()), ha.index().max(hb.index())));
            }
        }
        for s in &cut.segments {
            let (Some(&ha), Some(&hb)) = (handle_of.get(&s[0]), handle_of.get(&s[1])) else {
                continue;
            };
            if ha != hb {
                cdt.try_add_constraint(ha, hb);
            }
        }
        let faces: Vec<[usize; 3]> = cdt
            .inner_faces()
            .map(|f| f.vertices().map(|v| v.fix().index()))
            .collect();
        // Flood from the hull through every edge that is not the outline:
        // what it reaches lies outside the triangle.
        let key = |a: usize, b: usize| (a.min(b), a.max(b));
        let mut by_edge: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
        for (fi, f) in faces.iter().enumerate() {
            for k in 0..3 {
                by_edge
                    .entry(key(f[k], f[(k + 1) % 3]))
                    .or_default()
                    .push(fi);
            }
        }
        let mut outside = vec![false; faces.len()];
        let mut queue: Vec<usize> = Vec::new();
        for (fi, f) in faces.iter().enumerate() {
            for k in 0..3 {
                let e = key(f[k], f[(k + 1) % 3]);
                if by_edge[&e].len() == 1 && !boundary.contains(&e) && !outside[fi] {
                    outside[fi] = true;
                    queue.push(fi);
                }
            }
        }
        while let Some(fi) = queue.pop() {
            let f = faces[fi];
            for k in 0..3 {
                let e = key(f[k], f[(k + 1) % 3]);
                if boundary.contains(&e) {
                    continue;
                }
                for &g in &by_edge[&e] {
                    if !outside[g] {
                        outside[g] = true;
                        queue.push(g);
                    }
                }
            }
        }
        // spade's faces turn counter-clockwise in the projection; the
        // triangle's own winding there decides whether they turn back.
        let flip = turn < 0;
        let mut out = Vec::new();
        for (fi, f) in faces.iter().enumerate() {
            if outside[fi] {
                continue;
            }
            let ids = f.map(|h| id_of[&h]);
            if ids[0] == ids[1] || ids[1] == ids[2] || ids[0] == ids[2] {
                continue;
            }
            out.push(if flip { [ids[0], ids[2], ids[1]] } else { ids });
        }
        out
    }

    /// The points `ids` on edge `e`, in order from its end `from`: sorted
    /// by their rounded positions, and where two of those lie within 2⁻³⁰
    /// of the edge of each other, by their exact places on it
    /// ([`Csg::param`]), so the two triangles that share the edge put them
    /// in one order, and in the order the operands give them.
    fn along_edge(&self, e: EdgeKey, mut ids: Vec<u32>, from: u32) -> Vec<u32> {
        let (p, q) = (self.p(e.1), self.p(e.2));
        let d = q - p;
        let u = |i: u32| (self.p(i) - p).dot(d);
        shape_core::sort::sort_by(&mut ids, |&x, &y| u(x).total_cmp(&u(y)).then(x.cmp(&y)));
        let tol = d.length_squared() * (1.0 / (1u64 << 30) as f64);
        let mut start = 0;
        while start < ids.len() {
            let mut end = start + 1;
            while end < ids.len() && u(ids[end]) - u(ids[end - 1]) <= tol {
                end += 1;
            }
            if end - start > 1 {
                let params: Vec<(u32, Option<exact::Fraction>)> = ids[start..end]
                    .iter()
                    .map(|&i| (i, self.param(i, e)))
                    .collect();
                let cmp = |a: &(u32, Option<exact::Fraction>),
                           b: &(u32, Option<exact::Fraction>)| {
                    match (&a.1, &b.1) {
                        (Some(x), Some(y)) => x.cmp(y),
                        _ => u(a.0).total_cmp(&u(b.0)),
                    }
                    .then(a.0.cmp(&b.0))
                };
                // Insertion sort: it never needs the order to be total.
                let mut run = params;
                for i in 1..run.len() {
                    let mut j = i;
                    while j > 0 && cmp(&run[j - 1], &run[j]) == std::cmp::Ordering::Greater {
                        run.swap(j - 1, j);
                        j -= 1;
                    }
                }
                for (k, (i, _)) in run.into_iter().enumerate() {
                    ids[start + k] = i;
                }
            }
            start = end;
        }
        if from != e.1 {
            ids.reverse();
        }
        ids
    }

    /// Where point `x` lies along edge `e` (from its lower id), exactly:
    /// for an operand's vertex on the edge, a crossing of the edge with a
    /// triangle's plane, or with another edge. None for a point that only
    /// rounded onto the edge, or a self-union's triple point.
    fn param(&self, x: u32, e: EdgeKey) -> Option<exact::Fraction> {
        let arr = |i: u32| self.p(i).to_array();
        let (p, q) = (arr(e.1), arr(e.2));
        match self.origin_of[x as usize] {
            None => {
                let d = self.p(e.2) - self.p(e.1);
                let k = drop_axis(d);
                let v = arr(x);
                exact::Fraction::new(exact::Exp::diff(v[k], p[k]), exact::Exp::diff(q[k], p[k]))
            }
            Some(Origin::EdgeTri(f, om, tri)) if f == e => {
                let t = self.solids[om as usize].tris[tri as usize].map(arr);
                let dp = exact::orient3d(t[0], t[1], t[2], p);
                let dq = exact::orient3d(t[0], t[1], t[2], q);
                let den = dp.sub(&dq);
                exact::Fraction::new(dp, den)
            }
            Some(Origin::EdgeEdge(f, g)) if f == e || g == e => {
                let o = if f == e { g } else { f };
                let n = (self.p(e.2) - self.p(e.1)).cross(self.p(o.2) - self.p(o.1));
                let axis = drop_axis(n);
                let pr = |i: u32| project(self.p(i), axis).to_array();
                let dp = exact::orient2d(pr(o.1), pr(o.2), pr(e.1));
                let dq = exact::orient2d(pr(o.1), pr(o.2), pr(e.2));
                let den = dp.sub(&dq);
                exact::Fraction::new(dp, den)
            }
            _ => None,
        }
    }

    /// Triangle `tri` of mesh `m` cut into pieces (global ids, the
    /// triangle's own winding).
    fn pieces(&self, m: u8, tri: u32) -> Vec<[u32; 3]> {
        if !self.robust {
            return self.pieces_plain(m, tri);
        }
        let t = self.solids[m as usize].tris[tri as usize];
        let Some(cut) = self.cuts[m as usize].get(&tri) else {
            return vec![t];
        };
        let normal = self.tri_normal(t);
        let axis = drop_axis(normal);
        // The outline: each edge with the points on it, in order.
        let mut chain: Vec<u32> = Vec::new();
        let mut sides: Vec<usize> = Vec::new();
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let e = edge_key(m, a, b);
            let along: Vec<u32> = cut
                .points
                .iter()
                .copied()
                .filter(|&i| i != a && i != b && self.on[i as usize].contains(&e))
                .collect();
            let along = self.along_edge(e, along, a);
            chain.push(a);
            sides.push(3);
            sides.extend(along.iter().map(|_| k));
            chain.extend(along);
        }
        // A point made inside the triangle (an edge through it) can round to
        // a position on or past its outline, as seen in the projection: past
        // a corner's edge, or past a point rounded off an edge. Its segments
        // would then run along the outline or through an outline point, and
        // spade would merge or drop them, so regions would leak or pieces be
        // lost. It is placed in the triangulation just inside the outline
        // instead; its own position (shared with the other mesh) is
        // unchanged, as are which segments end at it (`docs/research.md`
        // §5.4, follow-up 4).
        self.chains.borrow_mut().push(chain.clone());
        let outline: Vec<DVec2> = chain.iter().map(|&i| project(self.p(i), axis)).collect();
        let v = t.map(|i| project(self.p(i), axis));
        let turn = o2(v[0], v[1], v[2]);
        let at = |i: u32| {
            let q = project(self.p(i), axis);
            // A point of the other mesh's at one of ours keeps its place, so
            // spade merges the two.
            let shared = chain.iter().any(|&j| self.p(j) == self.p(i));
            if turn == 0 || shared || chain.contains(&i) {
                return q;
            }
            inward(&outline, (v[0] + v[1] + v[2]) / 3.0, q)
        };
        if let Some(out) = self.triangulate(&chain, cut, at, turn < 0) {
            return out;
        }
        // The rounded outline is not a simple polygon, or a segment was lost
        // or merged into another: points made a rounding error from a corner
        // or from each other (operands that touch, `docs/research.md` §5.4,
        // item 5). The triangle is triangulated again in a layout of its
        // own where the outline is exact: the corners at (0, 0), (1, 0) and
        // (0, 1), and each point on an edge at a distinct multiple of 2⁻⁵²
        // along it, in its order. Points inside are placed by their
        // barycentric coordinates, just inside the outline. Only which
        // points the pieces join comes from the layout; every position is
        // unchanged.
        let layout = self.exact_layout(t, axis, &chain, &sides);
        let corners = [DVec2::ZERO, DVec2::X, DVec2::Y];
        let square: Vec<DVec2> = chain.iter().map(|i| layout[i]).collect();
        let centre = (corners[0] + corners[1] + corners[2]) / 3.0;
        let mut place: BTreeMap<u32, DVec2> = layout.clone();
        // Where each place is taken, and by which position.
        let mut taken: BTreeMap<[u64; 2], u32> = BTreeMap::new();
        for (&i, q) in &layout {
            taken.insert(q.to_array().map(f64::to_bits), i);
        }
        for &i in &cut.points {
            if place.contains_key(&i) {
                continue;
            }
            if let Some(&j) = chain.iter().find(|&&j| self.p(j) == self.p(i)) {
                place.insert(i, layout[&j]);
                continue;
            }
            let q = project(self.p(i), axis);
            let den = (v[1] - v[0]).perp_dot(v[2] - v[0]);
            let b = if den != 0.0 {
                DVec2::new(
                    (q - v[0]).perp_dot(v[2] - v[0]) / den,
                    (v[1] - v[0]).perp_dot(q - v[0]) / den,
                )
            } else {
                centre
            };
            let mut q = inward(&square, centre, b);
            // Two points at different positions that this layout cannot
            // tell apart: the later one moves along one of its segments
            // (towards the point at its other end, as rounded) by the
            // smallest step that frees it, staying inside.
            let busy = |q: DVec2, taken: &BTreeMap<[u64; 2], u32>| {
                taken
                    .get(&q.to_array().map(f64::to_bits))
                    .is_some_and(|&j| self.p(j) != self.p(i))
            };
            if busy(q, &taken) {
                let toward = cut
                    .segments
                    .iter()
                    .find_map(|s| {
                        let k = if s[0] == i {
                            s[1]
                        } else if s[1] == i {
                            s[0]
                        } else {
                            return None;
                        };
                        Some(place.get(&k).copied().unwrap_or(centre))
                    })
                    .unwrap_or(centre);
                let mut step = f64::EPSILON;
                while step < 0.5 {
                    let r = q + (toward - q) * step;
                    if !busy(r, &taken) && strictly_inside(&square, r) {
                        q = r;
                        break;
                    }
                    step *= 2.0;
                }
            }
            taken.entry(q.to_array().map(f64::to_bits)).or_insert(i);
            place.insert(i, q);
        }
        let at = |i: u32| place.get(&i).copied().unwrap_or(centre);
        self.triangulate(&chain, cut, at, false)
            .unwrap_or_else(|| self.triangulate_lossy(&chain, cut, at, false))
    }

    /// Each outline point's place in [`Csg::pieces`]' exact layout: the
    /// corners at (0, 0), (1, 0), (0, 1), and the points of edge `k` at
    /// distinct multiples of 2⁻⁵² along it, in chain order, as near their
    /// own fraction of the edge as that allows.
    fn exact_layout(
        &self,
        t: [u32; 3],
        axis: usize,
        chain: &[u32],
        sides: &[usize],
    ) -> BTreeMap<u32, DVec2> {
        const N: i64 = 1 << 52;
        let mut layout: BTreeMap<u32, DVec2> = BTreeMap::new();
        let corners = [DVec2::ZERO, DVec2::X, DVec2::Y];
        for k in 0..3 {
            layout.insert(t[k], corners[k]);
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let ids: Vec<u32> = chain
                .iter()
                .zip(sides)
                .filter(|&(_, &s)| s == k)
                .map(|(&i, _)| i)
                .collect();
            if ids.is_empty() {
                continue;
            }
            let (pa, pb) = (project(self.p(a), axis), project(self.p(b), axis));
            let d = pb - pa;
            let l2 = d.length_squared();
            let raw: Vec<i64> = ids
                .iter()
                .map(|&i| {
                    let u = if l2 > 0.0 {
                        (project(self.p(i), axis) - pa).dot(d) / l2
                    } else {
                        0.5
                    };
                    (u.clamp(0.0, 1.0) * N as f64) as i64
                })
                .collect();
            // Points at one position share a place, and so does every
            // point between two of them (they are made one point later,
            // `merge_points`); so do points at a corner's position, with
            // the corner.
            let key = |i: u32| self.p(i).to_array().map(|x| (x + 0.0).to_bits());
            let mut group: Vec<usize> = (0..ids.len()).collect();
            let mut first: BTreeMap<[u64; 3], usize> = BTreeMap::new();
            for (k, &i) in ids.iter().enumerate() {
                match first.get(&key(i)) {
                    Some(&j) => {
                        let gj = group[j];
                        for g in &mut group[j..=k] {
                            *g = gj;
                        }
                    }
                    None => {
                        first.insert(key(i), k);
                    }
                }
            }
            let at_a = |i: u32| key(i) == key(a);
            let at_b = |i: u32| key(i) == key(b);
            let mut ks: Vec<i64> = vec![0; ids.len()];
            let mut prev = 0;
            for k in 0..ids.len() {
                ks[k] = if k > 0 && group[k] == group[k - 1] {
                    ks[k - 1]
                } else if group[..=k]
                    .iter()
                    .zip(&ids)
                    .any(|(&g, &i)| g == group[k] && at_a(i))
                {
                    0
                } else {
                    raw[k].max(prev + 1)
                };
                prev = ks[k];
            }
            let mut next = N;
            for k in (0..ids.len()).rev() {
                let same = k + 1 < ids.len() && group[k] == group[k + 1];
                let tail = group[k..]
                    .iter()
                    .zip(&ids[k..])
                    .any(|(&g, &i)| g == group[k] && at_b(i));
                ks[k] = if same {
                    ks[k + 1]
                } else if tail {
                    N
                } else if ks[k] == 0 && at_a(ids[k]) {
                    0
                } else {
                    ks[k].min(next - 1)
                };
                next = ks[k];
            }
            for (&i, &x) in ids.iter().zip(&ks) {
                let (f, g) = (x as f64 / N as f64, (N - x) as f64 / N as f64);
                let q = match k {
                    0 => DVec2::new(f, 0.0),
                    1 => DVec2::new(g, f),
                    _ => DVec2::new(0.0, g),
                };
                layout.insert(i, q);
            }
        }
        layout
    }

    /// The pieces of a cut triangle, from a constrained Delaunay
    /// triangulation of its outline (`chain`) and segments with each point
    /// at `at`; None unless every outline edge and segment came out as one
    /// edge and no two points at different positions merged.
    fn triangulate(
        &self,
        chain: &[u32],
        cut: &Cut,
        at: impl Fn(u32) -> DVec2,
        flip: bool,
    ) -> Option<Vec<[u32; 3]>> {
        self.cdt_pieces(chain, cut, at, flip, true)
    }

    /// [`Csg::triangulate`], keeping what it can when the triangulation is
    /// not faithful (the result's check then refuses an open result).
    fn triangulate_lossy(
        &self,
        chain: &[u32],
        cut: &Cut,
        at: impl Fn(u32) -> DVec2,
        flip: bool,
    ) -> Vec<[u32; 3]> {
        self.cdt_pieces(chain, cut, at, flip, false)
            .unwrap_or_default()
    }

    fn cdt_pieces(
        &self,
        chain: &[u32],
        cut: &Cut,
        at: impl Fn(u32) -> DVec2,
        flip: bool,
        strict: bool,
    ) -> Option<Vec<[u32; 3]>> {
        let mut cdt: ConstrainedDelaunayTriangulation<Point2<f64>> =
            ConstrainedDelaunayTriangulation::new();
        let mut id_of: BTreeMap<usize, u32> = BTreeMap::new();
        let mut handle_of: BTreeMap<u32, spade::handles::FixedVertexHandle> = BTreeMap::new();
        let mut faithful = true;
        for &i in chain.iter().chain(cut.points.iter()) {
            if handle_of.contains_key(&i) {
                continue;
            }
            let q = at(i);
            let Ok(h) = cdt.insert(Point2::new(q.x, q.y)) else {
                faithful = false;
                continue;
            };
            match id_of.get(&h.index()) {
                Some(&j) if self.p(j) != self.p(i) => {
                    if strict {
                        faithful = false;
                    } else if self.robust {
                        self.merges.borrow_mut().push((j, i));
                    }
                }
                Some(&j) => {
                    if self.robust {
                        self.merges.borrow_mut().push((j, i));
                    }
                }
                None => {
                    id_of.insert(h.index(), i);
                }
            }
            handle_of.insert(i, h);
        }
        let mut boundary: BTreeSet<(usize, usize)> = BTreeSet::new();
        for k in 0..chain.len() {
            let (a, b) = (chain[k], chain[(k + 1) % chain.len()]);
            let (Some(&ha), Some(&hb)) = (handle_of.get(&a), handle_of.get(&b)) else {
                continue;
            };
            if ha != hb {
                if cdt.try_add_constraint(ha, hb).len() != 1 {
                    faithful = false;
                }
                boundary.insert((ha.index().min(hb.index()), ha.index().max(hb.index())));
            }
        }
        for s in &cut.segments {
            let (Some(&ha), Some(&hb)) = (handle_of.get(&s[0]), handle_of.get(&s[1])) else {
                continue;
            };
            if ha != hb
                && cdt.try_add_constraint(ha, hb).len() != 1
                && !cdt.exists_constraint(ha, hb)
            {
                faithful = false;
            }
        }
        if strict && !faithful {
            return None;
        }
        let faces: Vec<[usize; 3]> = cdt
            .inner_faces()
            .map(|f| f.vertices().map(|v| v.fix().index()))
            .collect();
        // Flood from the hull through every edge that is not the outline:
        // what it reaches lies outside the triangle.
        let key = |a: usize, b: usize| (a.min(b), a.max(b));
        let mut by_edge: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
        for (fi, f) in faces.iter().enumerate() {
            for k in 0..3 {
                by_edge
                    .entry(key(f[k], f[(k + 1) % 3]))
                    .or_default()
                    .push(fi);
            }
        }
        let mut outside = vec![false; faces.len()];
        let mut queue: Vec<usize> = Vec::new();
        for (fi, f) in faces.iter().enumerate() {
            for k in 0..3 {
                let e = key(f[k], f[(k + 1) % 3]);
                if by_edge[&e].len() == 1 && !boundary.contains(&e) && !outside[fi] {
                    outside[fi] = true;
                    queue.push(fi);
                }
            }
        }
        while let Some(fi) = queue.pop() {
            let f = faces[fi];
            for k in 0..3 {
                let e = key(f[k], f[(k + 1) % 3]);
                if boundary.contains(&e) {
                    continue;
                }
                for &g in &by_edge[&e] {
                    if !outside[g] {
                        outside[g] = true;
                        queue.push(g);
                    }
                }
            }
        }
        // spade's faces turn counter-clockwise in the layout; the
        // triangle's own winding there decides whether they turn back.
        let mut out = Vec::new();
        for (fi, f) in faces.iter().enumerate() {
            if outside[fi] {
                continue;
            }
            let ids = f.map(|h| id_of[&h]);
            if ids[0] == ids[1] || ids[1] == ids[2] || ids[0] == ids[2] {
                continue;
            }
            out.push(if flip { [ids[0], ids[2], ids[1]] } else { ids });
        }
        if strict {
            // Every outline edge borders one piece, and every segment
            // inside two: the rounded outline was a simple polygon and the
            // segments lie inside it.
            let mut uses: BTreeMap<(u32, u32), u32> = BTreeMap::new();
            for t in &out {
                for k in 0..3 {
                    let (a, b) = (t[k], t[(k + 1) % 3]);
                    *uses.entry((a.min(b), a.max(b))).or_default() += 1;
                }
            }
            let id = |a: u32| {
                handle_of
                    .get(&a)
                    .and_then(|h| id_of.get(&h.index()))
                    .copied()
                    .unwrap_or(a)
            };
            let n = |a: u32, b: u32| {
                let (a, b) = (id(a), id(b));
                uses.get(&(a.min(b), a.max(b))).copied().unwrap_or(0)
            };
            let mut outline: BTreeSet<(u32, u32)> = BTreeSet::new();
            for k in 0..chain.len() {
                let (a, b) = (chain[k], chain[(k + 1) % chain.len()]);
                outline.insert((id(a).min(id(b)), id(a).max(id(b))));
                if id(a) != id(b) && n(a, b) != 1 {
                    return None;
                }
            }
            for s in &cut.segments {
                let (a, b) = (s[0], s[1]);
                if id(a) != id(b)
                    && !outline.contains(&(id(a).min(id(b)), id(a).max(id(b))))
                    && n(a, b) != 2
                {
                    return None;
                }
            }
        }
        Some(out)
    }
}

/// `q`, or if it is not strictly inside `outline`, the smallest step from
/// it towards `centre`, by powers of two, that is.
fn inward(outline: &[DVec2], centre: DVec2, q: DVec2) -> DVec2 {
    if strictly_inside(outline, q) {
        return q;
    }
    let mut step = f64::EPSILON;
    while step < 0.5 {
        let r = q + (centre - q) * step;
        if strictly_inside(outline, r) {
            return r;
        }
        step *= 2.0;
    }
    centre
}

/// Whether `q` lies inside the closed polygon and on none of its edges,
/// decided by exact predicates (a winding number, Sunday's crossing rule).
fn strictly_inside(poly: &[DVec2], q: DVec2) -> bool {
    let mut wn = 0i32;
    for k in 0..poly.len() {
        let (a, b) = (poly[k], poly[(k + 1) % poly.len()]);
        let o = o2(a, b, q);
        if o == 0 && q.cmpge(a.min(b)).all() && q.cmple(a.max(b)).all() {
            return false;
        }
        if a.y <= q.y {
            if b.y > q.y && o > 0 {
                wn += 1;
            }
        } else if b.y <= q.y && o < 0 {
            wn -= 1;
        }
    }
    wn != 0
}

/// The winding number of a closed mesh round `p` (solid angles, Van
/// Oosterom and Strackee): 1 inside, 0 outside.
pub(crate) fn winding(points: &[DVec3], tris: &[[u32; 3]], p: DVec3) -> f64 {
    let mut sum = 0.0;
    for t in tris {
        let a = points[t[0] as usize] - p;
        let b = points[t[1] as usize] - p;
        let c = points[t[2] as usize] - p;
        let (la, lb, lc) = (a.length(), b.length(), c.length());
        let det = a.dot(b.cross(c));
        let den = la * lb * lc + a.dot(b) * lc + a.dot(c) * lb + b.dot(c) * la;
        sum += 2.0 * trig::atan2(det, den);
    }
    sum / (4.0 * trig::PI)
}

/// [`winding`] without triangle `skip`.
fn winding_except(points: &[DVec3], tris: &[[u32; 3]], skip: usize, p: DVec3) -> f64 {
    let mut sum = 0.0;
    for (i, t) in tris.iter().enumerate() {
        if i == skip {
            continue;
        }
        let a = points[t[0] as usize] - p;
        let b = points[t[1] as usize] - p;
        let c = points[t[2] as usize] - p;
        let (la, lb, lc) = (a.length(), b.length(), c.length());
        let det = a.dot(b.cross(c));
        let den = la * lb * lc + a.dot(b) * lc + a.dot(c) * lb + b.dot(c) * la;
        sum += 2.0 * trig::atan2(det, den);
    }
    sum / (4.0 * trig::PI)
}

fn weld(
    mesh: &Mesh,
    placement: Placement,
    points: &mut Vec<DVec3>,
    at: &mut BTreeMap<[u64; 3], u32>,
) -> Result<Solid, String> {
    let placed = place(mesh, placement.matrix())?;
    if placed.positions.iter().any(|p| !p.is_finite()) {
        return Err("it holds a position that is not finite".into());
    }
    let mut ids = Vec::with_capacity(placed.positions.len());
    let start = points.len() as u32;
    for p in &placed.positions {
        // + 0.0 makes -0 and 0 one point.
        let key = [
            (p.x + 0.0).to_bits(),
            (p.y + 0.0).to_bits(),
            (p.z + 0.0).to_bits(),
        ];
        let id = match at.get(&key) {
            Some(&id) if id >= start => id,
            _ => {
                let id = points.len() as u32;
                points.push(*p);
                at.insert(key, id);
                id
            }
        };
        ids.push(id);
    }
    let mut tris = Vec::new();
    let mut corners = Vec::new();
    let mut source = Vec::new();
    let (mut min, mut max) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
    for (ti, t) in placed.indices.as_chunks::<3>().0.iter().enumerate() {
        let w = [ids[t[0] as usize], ids[t[1] as usize], ids[t[2] as usize]];
        if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
            continue;
        }
        tris.push(w);
        source.push(ti as u32);
        corners.push([0, 1, 2].map(|k| {
            let i = t[k] as usize;
            Corner {
                normal: placed.normals[i],
                uv: placed.uvs[i],
                color: placed.colors[i],
            }
        }));
        for &i in &w {
            min = min.min(points[i as usize]);
            max = max.max(points[i as usize]);
        }
    }
    Ok(Solid {
        tris,
        corners,
        source,
        min,
        max,
    })
}

fn bounds(points: &[DVec3], t: [u32; 3]) -> (DVec3, DVec3) {
    let p = t.map(|i| points[i as usize]);
    (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]))
}

fn overlap(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> bool {
    a.0.cmple(b.1).all() && b.0.cmple(a.1).all()
}

/// `a op b`, each placed first. Refused for a singular placement or a
/// position that is not finite.
pub fn csg(a: &Mesh, pa: Placement, b: &Mesh, pb: Placement, op: CsgOp) -> Result<Mesh, String> {
    csg_traced(a, pa, b, pb, op).map(|(m, _)| m)
}

/// [`csg`], with where each output triangle came from: `(0, t)` a piece of
/// triangle `t` of `a`, `(1, t)` of `b` (one entry per output triangle, in
/// order). A fracture tells its outer faces from its cut faces by it.
pub fn csg_traced(
    a: &Mesh,
    pa: Placement,
    b: &Mesh,
    pb: Placement,
    op: CsgOp,
) -> Result<(Mesh, Vec<(u8, u32)>), String> {
    let mut points = Vec::new();
    let mut at = BTreeMap::new();
    let sa = weld(a, pa, &mut points, &mut at).map_err(|e| format!("a: {e}"))?;
    let sb = weld(b, pb, &mut points, &mut at).map_err(|e| format!("b: {e}"))?;
    let (c, kept) = arrange(points, sa, sb, op, false);
    // 4. What the operation keeps.
    let colors = !a.colors.is_empty() || !b.colors.is_empty();
    let mut out = Mesh::default();
    let mut origins: Vec<(u8, u32)> = Vec::new();
    let mut vertex_at: BTreeMap<Vec<u32>, u32> = BTreeMap::new();
    for p in &kept {
        let flip = p.flip;
        let solid = &c.solids[p.mesh as usize];
        origins.push((p.mesh, solid.source[p.tri as usize]));
        let t = solid.tris[p.tri as usize];
        let corners = &solid.corners[p.tri as usize];
        let axis = drop_axis(c.tri_normal(t));
        let tv = t.map(|i| project(c.points[i as usize], axis));
        let den = (tv[1] - tv[0]).perp_dot(tv[2] - tv[0]);
        let ids = if flip {
            [p.ids[0], p.ids[2], p.ids[1]]
        } else {
            p.ids
        };
        for id in ids {
            let pos = c.points[id as usize];
            let q = project(pos, axis);
            let (w1, w2) = if den != 0.0 {
                (
                    (q - tv[0]).perp_dot(tv[2] - tv[0]) / den,
                    (tv[1] - tv[0]).perp_dot(q - tv[0]) / den,
                )
            } else {
                (0.0, 0.0)
            };
            let w0 = 1.0 - w1 - w2;
            let mut normal =
                normalize(corners[0].normal * w0 + corners[1].normal * w1 + corners[2].normal * w2);
            if flip {
                normal = -normal;
            }
            let uv = [0, 1]
                .map(|k| corners[0].uv[k] * w0 + corners[1].uv[k] * w1 + corners[2].uv[k] * w2);
            let color = [0, 1, 2, 3].map(|k| {
                corners[0].color[k] * w0 + corners[1].color[k] * w1 + corners[2].color[k] * w2
            });
            let pf = pos.as_vec3().to_array();
            let nf = normal.as_vec3().to_array();
            let uf = uv.map(|x| x as f32);
            let cf = color.map(|x| x as f32);
            let mut key: Vec<u32> = pf
                .iter()
                .chain(&nf)
                .chain(&uf)
                .map(|x| (x + 0.0).to_bits())
                .collect();
            if colors {
                key.extend(cf.map(|x| x.to_bits()));
            }
            let index = *vertex_at.entry(key).or_insert_with(|| {
                out.positions.push(pf);
                out.normals.push(nf);
                out.uvs.push(uf);
                if colors {
                    out.colors.push(cf);
                }
                (out.positions.len() - 1) as u32
            });
            out.indices.push(index);
        }
    }
    Ok((out, origins))
}

/// A piece the operation keeps: of mesh 0 (A) or 1 (B), from its welded
/// triangle `tri`, with point ids, turned inside out where `flip`.
struct Kept {
    mesh: u8,
    tri: u32,
    ids: [u32; 3],
    flip: bool,
}

/// A piece of a cut triangle: of mesh `mesh`, from its welded triangle
/// `tri`, with point ids in the triangle's winding.
struct Piece {
    mesh: u8,
    tri: u32,
    ids: [u32; 3],
}

/// The pieces' regions: each piece's root, and each root's largest piece.
struct Regions {
    root: Vec<usize>,
    best: BTreeMap<usize, usize>,
}

impl Csg {
    fn new(points: Vec<DVec3>, solids: [Solid; 2]) -> Csg {
        let on = vec![Vec::new(); points.len()];
        let origin_of = vec![None; points.len()];
        Csg {
            points,
            on,
            made: BTreeMap::new(),
            origin_of,
            made_at: BTreeMap::new(),
            solids,
            cuts: [BTreeMap::new(), BTreeMap::new()],
            crossings: BTreeSet::new(),
            coplanar_pairs: Vec::new(),
            touch_pairs: Vec::new(),
            robust: false,
            merges: std::cell::RefCell::new(Vec::new()),
            chains: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// Step 2: every triangle of meshes `0..meshes`, cut into pieces.
    fn all_pieces(&self, meshes: u8) -> Vec<Piece> {
        let mut pieces = Vec::new();
        for m in 0..meshes {
            for tri in 0..self.solids[m as usize].tris.len() as u32 {
                for ids in self.pieces(m, tri) {
                    pieces.push(Piece { mesh: m, tri, ids });
                }
            }
        }
        pieces
    }

    /// Step 3's regions: pieces of one mesh joined across edges no
    /// crossing runs along.
    fn regions(&self, pieces: &[Piece]) -> Regions {
        let mut parent: Vec<usize> = (0..pieces.len()).collect();
        fn find(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let mut first: BTreeMap<(u8, u32, u32), usize> = BTreeMap::new();
        for (pi, p) in pieces.iter().enumerate() {
            for k in 0..3 {
                let (x, y) = (p.ids[k], p.ids[(k + 1) % 3]);
                let e = (x.min(y), x.max(y));
                if self.crossings.contains(&e) {
                    continue;
                }
                match first.get(&(p.mesh, e.0, e.1)) {
                    Some(&q) => {
                        let (r1, r2) = (find(&mut parent, pi), find(&mut parent, q));
                        if r1 != r2 {
                            parent[r1.max(r2)] = r1.min(r2);
                        }
                    }
                    None => {
                        first.insert((p.mesh, e.0, e.1), pi);
                    }
                }
            }
        }
        let area = |p: &Piece| {
            let [a, b, cc] = p.ids.map(|i| self.points[i as usize]);
            (b - a).cross(cc - a).length() * 0.5
        };
        // Each region's largest piece (the first of equals).
        let mut best: BTreeMap<usize, usize> = BTreeMap::new();
        let mut root = Vec::with_capacity(pieces.len());
        for pi in 0..pieces.len() {
            let r = find(&mut parent, pi);
            root.push(r);
            match best.get(&r) {
                Some(&q) if area(&pieces[q]) >= area(&pieces[pi]) => {}
                _ => {
                    best.insert(r, pi);
                }
            }
        }
        Regions { root, best }
    }

    /// Points just in front of and just behind a piece, at its centre: a
    /// millionth of `scale`, or half its inradius if that is less.
    fn probes(&self, p: &Piece, scale: f64) -> (DVec3, DVec3) {
        let [a, b, cc] = p.ids.map(|i| self.points[i as usize]);
        let n = (b - a).cross(cc - a);
        let perimeter = (b - a).length() + (cc - b).length() + (a - cc).length();
        let inradius = n.length() / perimeter.max(1e-300);
        let eps = (scale * 1e-6).min(inradius * 0.5);
        let n = normalize(n);
        let centre = (a + b + cc) / 3.0;
        (centre + n * eps, centre - n * eps)
    }

    /// The diagonal of both meshes' box.
    fn scale(&self) -> f64 {
        let lo = self.solids[0].min.min(self.solids[1].min);
        let hi = self.solids[0].max.max(self.solids[1].max);
        (hi - lo).length().max(1e-30)
    }
}

/// Steps 1 to 3 and the choice of step 4: which pieces `op` keeps.
fn arrange(points: Vec<DVec3>, sa: Solid, sb: Solid, op: CsgOp, robust: bool) -> (Csg, Vec<Kept>) {
    let mut c = Csg::new(points, [sa, sb]);
    c.robust = robust;
    // 1. Every pair of triangles whose boxes overlap.
    let box_a = (c.solids[0].min, c.solids[0].max);
    let box_b = (c.solids[1].min, c.solids[1].max);
    let near = |c: &Csg, m: usize, other: (DVec3, DVec3)| -> Vec<(u32, (DVec3, DVec3))> {
        c.solids[m]
            .tris
            .iter()
            .enumerate()
            .map(|(i, &t)| (i as u32, bounds(&c.points, t)))
            .filter(|(_, bb)| overlap(*bb, other))
            .collect()
    };
    let near_a = near(&c, 0, box_b);
    let near_b = near(&c, 1, box_a);
    for (ia, ib) in candidate_pairs(&near_a, &near_b) {
        c.pair((0, ia), (1, ib));
    }
    // 2. The pieces.
    let mut pieces = c.all_pieces(2);
    if robust {
        pieces = merge_points(&mut c, pieces);
    }
    // 3. Regions, each classified by the other mesh's winding number.
    let regions = c.regions(&pieces);
    let scale = c.scale();
    // (inside the other mesh in front, behind).
    let mut sides: BTreeMap<usize, (bool, bool)> = BTreeMap::new();
    let twins = if robust {
        coincident_regions(&c, &pieces, &regions)
    } else {
        BTreeMap::new()
    };
    for (&r, &b) in &regions.best {
        if let Some(&same) = twins.get(&r) {
            // On the other surface: the other solid lies behind a face
            // facing the same way, in front of one facing the other way.
            sides.insert(r, (!same, same));
            continue;
        }
        let p = &pieces[b];
        let (front, back) = c.probes(p, scale);
        let other = &c.solids[1 - p.mesh as usize].tris;
        let inside = |q: DVec3| winding(&c.points, other, q) > 0.5;
        sides.insert(r, (inside(front), inside(back)));
    }
    let mut choice: Vec<(bool, bool)> = Vec::with_capacity(pieces.len());
    for (pi, p) in pieces.iter().enumerate() {
        let (front, back) = sides[&regions.root[pi]];
        let (keep, flip) = if p.mesh == 0 {
            match op {
                CsgOp::Union => (!front, false),
                CsgOp::Intersect => (back, false),
                CsgOp::Subtract => (!back, false),
            }
        } else if front != back {
            // On A's surface: A's piece answers for it.
            (false, false)
        } else {
            match op {
                CsgOp::Union => (!front, false),
                CsgOp::Intersect => (back, false),
                CsgOp::Subtract => (front, true),
            }
        };
        choice.push((keep, flip));
    }
    if robust {
        close_slivers(&c, &pieces, &regions, op, scale, &mut choice);
    }
    let mut kept = Vec::new();
    for (p, &(keep, flip)) in pieces.iter().zip(&choice) {
        if keep {
            kept.push(Kept {
                mesh: p.mesh,
                tri: p.tri,
                ids: p.ids,
                flip,
            });
        }
    }
    (c, kept)
}

/// Makes points that must be one point one: each pair a triangle's layout
/// merged, every set of points at one position (the result is welded by
/// position), and along each cut triangle's outline, the run between two
/// places where one position comes back (a crossing rounded onto a point
/// past others that lie, in exact order, between them: a run a few ulps
/// long). Repeated until nothing changes. The lowest id stands for each
/// set, in the pieces and in the crossings; pieces left with two equal
/// corners are dropped. Each step collapses edges of the arrangement, so
/// a closed surface stays closed (`docs/research.md` §5.4, item 5).
fn merge_points(c: &mut Csg, pieces: Vec<Piece>) -> Vec<Piece> {
    let merges = std::mem::take(&mut *c.merges.borrow_mut());
    let chains = std::mem::take(&mut *c.chains.borrow_mut());
    let mut parent: BTreeMap<u32, u32> = BTreeMap::new();
    fn find(parent: &BTreeMap<u32, u32>, mut i: u32) -> u32 {
        while let Some(&p) = parent.get(&i) {
            if p == i {
                break;
            }
            i = p;
        }
        i
    }
    fn union(parent: &mut BTreeMap<u32, u32>, a: u32, b: u32) -> bool {
        let (ra, rb) = (find(parent, a), find(parent, b));
        if ra == rb {
            return false;
        }
        parent.insert(ra.max(rb), ra.min(rb));
        true
    }
    for (a, b) in merges {
        union(&mut parent, a, b);
    }
    // Every point a piece uses, by position.
    let mut used: BTreeSet<u32> = BTreeSet::new();
    for p in &pieces {
        used.extend(p.ids);
    }
    let key = |c: &Csg, i: u32| c.points[i as usize].to_array().map(|x| (x + 0.0).to_bits());
    let tiny = c.scale() * (1.0 / (1u64 << 30) as f64);
    loop {
        let mut changed = false;
        let mut at: BTreeMap<[u64; 3], u32> = BTreeMap::new();
        for &i in &used {
            let r = find(&parent, i);
            let k = key(c, r);
            match at.get(&k) {
                Some(&j) => changed |= union(&mut parent, j, r),
                None => {
                    at.insert(k, r);
                }
            }
        }
        for chain in &chains {
            let mut seq: Vec<u32> = Vec::new();
            for &i in chain {
                let r = find(&parent, i);
                if seq.last() != Some(&r) {
                    seq.push(r);
                }
            }
            while seq.len() > 1 && seq.first() == seq.last() {
                seq.pop();
            }
            let n = seq.len();
            let mut first: BTreeMap<u32, usize> = BTreeMap::new();
            for (k, &r) in seq.iter().enumerate() {
                if let Some(&j) = first.get(&r) {
                    // The shorter way round from j to k collapses.
                    let run: Vec<u32> = if k - j <= n - (k - j) {
                        seq[j..=k].to_vec()
                    } else {
                        seq[k..].iter().chain(&seq[..=j]).copied().collect()
                    };
                    // Only a run a rounding error long: a longer one is
                    // left, and the result's check refuses what follows.
                    let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
                    for &i in &run {
                        lo = lo.min(c.points[i as usize]);
                        hi = hi.max(c.points[i as usize]);
                    }
                    if (hi - lo).length() > tiny {
                        continue;
                    }
                    for w in run.windows(2) {
                        changed |= union(&mut parent, w[0], w[1]);
                    }
                } else {
                    first.insert(r, k);
                }
            }
        }
        if !changed {
            break;
        }
    }
    if parent.is_empty() {
        return pieces;
    }
    let crossings = std::mem::take(&mut c.crossings);
    c.crossings = crossings
        .into_iter()
        .map(|(a, b)| {
            let (a, b) = (find(&parent, a), find(&parent, b));
            (a.min(b), a.max(b))
        })
        .filter(|(a, b)| a != b)
        .collect();
    pieces
        .into_iter()
        .filter_map(|p| {
            let ids = p.ids.map(|i| find(&parent, i));
            (ids[0] != ids[1] && ids[1] != ids[2] && ids[0] != ids[2]).then_some(Piece { ids, ..p })
        })
        .collect()
}

/// The second pass's last resort for regions no probe can classify: a
/// region whose largest piece has an inradius under 2⁻³⁰ of the operands'
/// size (a sliver between surfaces a rounding error apart) keeps its
/// probe's answer unless the result is open along it. Then, for each
/// cluster of such regions on the open edges (at most 12, joined by
/// edges), the choice of which to keep that leaves the fewest unmatched
/// edges is taken: fewest changes first, then the lowest-numbered.
/// Regions a probe can resolve are never changed, and a kept piece faces
/// as the operation turns it (`docs/research.md` §5.4, item 5).
fn close_slivers(
    c: &Csg,
    pieces: &[Piece],
    regions: &Regions,
    op: CsgOp,
    scale: f64,
    choice: &mut [(bool, bool)],
) {
    let directed = |p: &Piece, flip: bool| {
        let ids = if flip {
            [p.ids[0], p.ids[2], p.ids[1]]
        } else {
            p.ids
        };
        (0..3).map(move |k| (ids[k], ids[(k + 1) % 3]))
    };
    let balance = |choice: &[(bool, bool)]| {
        let mut b: BTreeMap<(u32, u32), i64> = BTreeMap::new();
        for (p, &(keep, flip)) in pieces.iter().zip(choice) {
            if keep {
                for (x, y) in directed(p, flip) {
                    *b.entry((x.min(y), x.max(y))).or_default() += if x < y { 1 } else { -1 };
                }
            }
        }
        b
    };
    let base = balance(choice);
    let open: BTreeSet<(u32, u32)> = base
        .iter()
        .filter(|(_, n)| **n != 0)
        .map(|(&e, _)| e)
        .collect();
    if open.is_empty() {
        return;
    }
    let inradius = |p: &Piece| {
        let [a, b, cc] = p.ids.map(|i| c.points[i as usize]);
        let perimeter = (b - a).length() + (cc - b).length() + (a - cc).length();
        (b - a).cross(cc - a).length() / perimeter.max(1e-300)
    };
    let tiny = scale * (1.0 / (1u64 << 30) as f64);
    // Unreliable regions on an open edge, with their pieces.
    let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (pi, &r) in regions.root.iter().enumerate() {
        members.entry(r).or_default().push(pi);
    }
    let edges_of = |r: usize| -> BTreeSet<(u32, u32)> {
        members[&r]
            .iter()
            .flat_map(|&pi| directed(&pieces[pi], false).map(|(x, y)| (x.min(y), x.max(y))))
            .collect()
    };
    let unsure: Vec<usize> = regions
        .best
        .iter()
        .filter(|&(&r, &b)| {
            inradius(&pieces[b]) < tiny && edges_of(r).iter().any(|e| open.contains(e))
        })
        .map(|(&r, _)| r)
        .collect();
    // Clusters: regions sharing an edge.
    let mut cluster: BTreeMap<usize, usize> = unsure.iter().map(|&r| (r, r)).collect();
    fn find(c: &mut BTreeMap<usize, usize>, mut r: usize) -> usize {
        while c[&r] != r {
            r = c[&r];
        }
        r
    }
    for (i, &a) in unsure.iter().enumerate() {
        let ea = edges_of(a);
        for &b in &unsure[i + 1..] {
            if edges_of(b).iter().any(|e| ea.contains(e)) {
                let (x, y) = (find(&mut cluster, a), find(&mut cluster, b));
                if x != y {
                    cluster.insert(x.max(y), x.min(y));
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &r in &unsure {
        let k = find(&mut cluster, r);
        groups.entry(k).or_default().push(r);
    }
    // How a region's pieces face when kept.
    let flip_of = |m: u8| m == 1 && op == CsgOp::Subtract;
    let mut base = base;
    for rs in groups.values() {
        if rs.len() > 12 {
            continue;
        }
        let add = |b: &mut BTreeMap<(u32, u32), i64>, pi: usize, flip: bool, sign: i64| {
            for (x, y) in directed(&pieces[pi], flip) {
                *b.entry((x.min(y), x.max(y))).or_default() += sign * if x < y { 1 } else { -1 };
            }
        };
        // The rest of the result, without the cluster.
        let mut rest = base.clone();
        for &r in rs {
            for &pi in &members[&r] {
                if choice[pi].0 {
                    add(&mut rest, pi, choice[pi].1, -1);
                }
            }
        }
        let touched: BTreeSet<(u32, u32)> = rs.iter().flat_map(|&r| edges_of(r)).collect();
        let now: u32 = rs
            .iter()
            .enumerate()
            .filter(|&(_, &r)| choice[members[&r][0]].0)
            .map(|(i, _)| 1 << i)
            .sum();
        let mut best: Option<(u64, u32, u32)> = None;
        for mask in 0u32..(1 << rs.len()) {
            let mut b = rest.clone();
            for (i, &r) in rs.iter().enumerate() {
                if mask & (1 << i) != 0 {
                    for &pi in &members[&r] {
                        add(&mut b, pi, flip_of(pieces[pi].mesh), 1);
                    }
                }
            }
            let cost: u64 = touched
                .iter()
                .map(|e| b.get(e).map_or(0, |n| n.unsigned_abs()))
                .sum();
            let key = (cost, (mask ^ now).count_ones(), mask);
            if best.is_none_or(|bk| key < bk) {
                best = Some(key);
            }
        }
        let Some((_, _, mask)) = best else {
            continue;
        };
        for (i, &r) in rs.iter().enumerate() {
            let keep = mask & (1 << i) != 0;
            for &pi in &members[&r] {
                choice[pi] = (keep, keep && flip_of(pieces[pi].mesh));
            }
        }
        // The cluster's choice is part of the result the next one sees.
        for &r in rs {
            for &pi in &members[&r] {
                if choice[pi].0 {
                    add(&mut rest, pi, choice[pi].1, 1);
                }
            }
        }
        base = rest;
    }
}

/// The second pass's regions that lie on the other mesh's surface,
/// decided symbolically: a region of one mesh whose boundary is the
/// boundary of a region of the other, edge for edge (by point ids, after
/// [`merge_points`]), every triangle of each lying in one plane with a
/// triangle of the other (`coplanar`, exact). Each maps to whether the two
/// face the same way. A sliver a rounding error wide, which no probe can
/// classify, is decided so (`docs/research.md` §5.4, item 5).
fn coincident_regions(c: &Csg, pieces: &[Piece], regions: &Regions) -> BTreeMap<usize, bool> {
    let mut partners: BTreeMap<(u8, u32), BTreeSet<(u8, u32)>> = BTreeMap::new();
    for &(a, b) in &c.coplanar_pairs {
        partners.entry(a).or_default().insert(b);
        partners.entry(b).or_default().insert(a);
    }
    // Each region's triangles, but for pieces whose corners are exactly
    // collinear (they bound nothing).
    let mut tris: BTreeMap<usize, BTreeSet<(u8, u32)>> = BTreeMap::new();
    for (pi, p) in pieces.iter().enumerate() {
        let e = tris.entry(regions.root[pi]).or_default();
        if c.tri_normal(p.ids) != DVec3::ZERO {
            e.insert((p.mesh, p.tri));
        }
    }
    let on = |r: usize, o: usize| {
        tris[&r].iter().all(|t| {
            partners
                .get(t)
                .is_some_and(|ps| ps.iter().any(|q| tris[&o].contains(q)))
        })
    };
    // Each region's boundary: directed edges not cancelled within it.
    let mut edges: BTreeMap<usize, BTreeMap<(u32, u32), i32>> = BTreeMap::new();
    for (pi, p) in pieces.iter().enumerate() {
        let e = edges.entry(regions.root[pi]).or_default();
        for k in 0..3 {
            let (a, b) = (p.ids[k], p.ids[(k + 1) % 3]);
            *e.entry((a.min(b), a.max(b))).or_default() += if a < b { 1 } else { -1 };
        }
    }
    let mut by_boundary: BTreeMap<Boundary, Vec<usize>> = BTreeMap::new();
    let mut mesh_of: BTreeMap<usize, u8> = BTreeMap::new();
    for (&r, e) in &edges {
        let b: Vec<((u32, u32), i32)> = e
            .iter()
            .filter(|(_, n)| **n != 0)
            .map(|(&k, &n)| (k, n))
            .collect();
        if b.is_empty() {
            continue;
        }
        mesh_of.insert(r, pieces[regions.best[&r]].mesh);
        by_boundary.entry(b).or_default().push(r);
    }
    let mut twins: BTreeMap<usize, bool> = BTreeMap::new();
    for (b, rs) in &by_boundary {
        let flipped: Vec<((u32, u32), i32)> = b.iter().map(|&(k, n)| (k, -n)).collect();
        for &r in rs {
            let m = mesh_of[&r];
            let twin = |o: usize| mesh_of[&o] != m && on(r, o) && on(o, r);
            if rs.iter().any(|&o| twin(o)) {
                twins.insert(r, true);
            } else if by_boundary
                .get(&flipped)
                .is_some_and(|os| os.iter().any(|&o| twin(o)))
            {
                twins.insert(r, false);
            }
        }
    }
    twins
}

/// The self-union's steps: one mesh's arrangement with itself, keeping the
/// pieces between winding number below 1 and at least 1.
fn arrange_self(points: Vec<DVec3>, s: Solid) -> (Csg, Vec<Kept>) {
    let empty = Solid {
        tris: Vec::new(),
        corners: Vec::new(),
        source: Vec::new(),
        min: DVec3::INFINITY,
        max: DVec3::NEG_INFINITY,
    };
    let mut c = Csg::new(points, [s, empty]);
    // 1. Pairs whose boxes overlap: those sharing no vertex, and those
    // sharing one or an edge that cross improperly (a shared corner or
    // edge alone is not a crossing).
    let tris = c.solids[0].tris.clone();
    for (i, j) in selfcheck::box_pairs(&c.points, &tris) {
        let (ti, tj) = (tris[i as usize], tris[j as usize]);
        if ti.iter().any(|v| tj.contains(v))
            && !selfcheck::crossing(ti, ti.map(|v| c.p(v)), tj, tj.map(|v| c.p(v)))
        {
            continue;
        }
        c.pair((0, i), (0, j));
    }
    c.triples();
    // 2. The pieces.
    let pieces = c.all_pieces(1);
    // 3. Regions, each classified by the mesh's own winding number.
    let regions = c.regions(&pieces);
    let scale = c.scale();
    // Where triangles lie on one another, the lowest-numbered one answers.
    let mut partners: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for &((_, a), (_, b)) in &c.coplanar_pairs {
        partners.entry(a).or_default().push(b);
        partners.entry(b).or_default().push(a);
    }
    // The winding number just in front of a piece and just behind it differ
    // by one, its own surface. At the piece's centre, the winding number of
    // every other triangle is halfway between: it needs no step off the
    // surface, so a sliver is classified as surely as a large piece. Where
    // another face lies on the piece, that does not hold; points a small
    // step in front and behind decide there.
    let mut sides: BTreeMap<usize, (bool, bool)> = BTreeMap::new();
    for (&r, &b) in &regions.best {
        let p = &pieces[b];
        let side = if partners.contains_key(&p.tri) {
            let (front, back) = c.probes(p, scale);
            let inside = |q: DVec3| winding(&c.points, &tris, q) > 0.5;
            (inside(front), inside(back))
        } else {
            let [a, bb, cc] = p.ids.map(|i| c.p(i));
            let rest = winding_except(&c.points, &tris, p.tri as usize, (a + bb + cc) / 3.0);
            (rest - 0.5 > 0.5, rest + 0.5 > 0.5)
        };
        sides.insert(r, side);
    }
    let covered_by_lower = |p: &Piece| {
        let Some(list) = partners.get(&p.tri) else {
            return false;
        };
        let axis = drop_axis(c.tri_normal(tris[p.tri as usize]));
        let [a, b, cc] = p.ids.map(|i| c.p(i));
        let q = project((a + b + cc) / 3.0, axis);
        list.iter().any(|&u| {
            u < p.tri && {
                let poly: Vec<DVec2> = tris[u as usize]
                    .iter()
                    .map(|&v| project(c.p(v), axis))
                    .collect();
                strictly_inside(&poly, q)
            }
        })
    };
    let mut kept = Vec::new();
    for (pi, p) in pieces.iter().enumerate() {
        let (front, back) = sides[&regions.root[pi]];
        if front == back || covered_by_lower(p) {
            continue;
        }
        kept.push(Kept {
            mesh: 0,
            tri: p.tri,
            ids: p.ids,
            flip: front,
        });
    }
    (c, kept)
}

/// Why [`csg_points`] refused its operands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OperandError {
    /// A position was NaN or infinite (operand 0 = A, 1 = B).
    NotFinite(u8),
    /// After exact welding, an edge is used other than once each way:
    /// the operand is open, not oriented, or not manifold.
    NotClosed(u8),
}

/// Welds an f64 operand (`m`: 0 = A, 1 = B) by exact position into
/// `points`, refusing it unless it is closed and oriented: every directed
/// edge used once and its reverse once, or with `balanced`, as often as
/// its reverse (faces lying on one another, as a self-union takes).
fn weld64(
    m: u8,
    balanced: bool,
    (pos, tris): (&[DVec3], &[[u32; 3]]),
    points: &mut Vec<DVec3>,
    at: &mut BTreeMap<[u64; 3], u32>,
) -> Result<Solid, OperandError> {
    if pos.iter().any(|p| !p.is_finite()) {
        return Err(OperandError::NotFinite(m));
    }
    let start = points.len() as u32;
    let ids: Vec<u32> = pos
        .iter()
        .map(|p| {
            let key = [
                (p.x + 0.0).to_bits(),
                (p.y + 0.0).to_bits(),
                (p.z + 0.0).to_bits(),
            ];
            match at.get(&key) {
                Some(&id) if id >= start => id,
                _ => {
                    let id = points.len() as u32;
                    points.push(*p + DVec3::ZERO);
                    at.insert(key, id);
                    id
                }
            }
        })
        .collect();
    let mut s = Solid {
        tris: Vec::new(),
        corners: Vec::new(),
        source: Vec::new(),
        min: DVec3::INFINITY,
        max: DVec3::NEG_INFINITY,
    };
    let mut directed: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let flat = Corner {
        normal: DVec3::Y,
        uv: [0.0; 2],
        color: [1.0; 4],
    };
    for (ti, t) in tris.iter().enumerate() {
        let w = t.map(|i| ids[i as usize]);
        if w[0] == w[1] || w[1] == w[2] || w[0] == w[2] {
            continue;
        }
        for k in 0..3 {
            *directed.entry((w[k], w[(k + 1) % 3])).or_default() += 1;
        }
        s.tris.push(w);
        s.source.push(ti as u32);
        s.corners.push([flat; 3]);
        for &i in &w {
            s.min = s.min.min(points[i as usize]);
            s.max = s.max.max(points[i as usize]);
        }
    }
    let closed = !s.tris.is_empty()
        && directed
            .iter()
            .all(|(&(x, y), &n)| (balanced || n == 1) && directed.get(&(y, x)) == Some(&n));
    if !closed {
        return Err(OperandError::NotClosed(m));
    }
    Ok(s)
}

/// The kept pieces as points (only those used, in first-use order),
/// triangles and each one's source `(operand, triangle)`, plus the map
/// from arrangement point ids to output ids.
#[allow(clippy::type_complexity)]
fn emit(
    c: &Csg,
    kept: &[Kept],
) -> (
    Vec<DVec3>,
    Vec<[u32; 3]>,
    Vec<(u8, u32)>,
    BTreeMap<u32, u32>,
) {
    let mut remap: BTreeMap<u32, u32> = BTreeMap::new();
    let mut out_points = Vec::new();
    let mut out_tris = Vec::with_capacity(kept.len());
    let mut origins = Vec::with_capacity(kept.len());
    for p in kept {
        let ids = if p.flip {
            [p.ids[0], p.ids[2], p.ids[1]]
        } else {
            p.ids
        };
        let t = ids.map(|id| {
            *remap.entry(id).or_insert_with(|| {
                out_points.push(c.points[id as usize]);
                out_points.len() as u32 - 1
            })
        });
        out_tris.push(t);
        let solid = &c.solids[p.mesh as usize];
        origins.push((p.mesh, solid.source[p.tri as usize]));
    }
    (out_points, out_tris, origins, remap)
}

/// The f64 core of [`csg`] on welded operands: points and triangles in,
/// points, triangles and each output triangle's source `(operand,
/// triangle)` out. Unlike [`csg`], an operand that is not closed and
/// oriented after exact welding is refused (`docs/research.md` §5.4).
#[allow(clippy::type_complexity)]
pub fn csg_points(
    a: (&[DVec3], &[[u32; 3]]),
    b: (&[DVec3], &[[u32; 3]]),
    op: CsgOp,
) -> Result<(Vec<DVec3>, Vec<[u32; 3]>, Vec<(u8, u32)>), OperandError> {
    csg_points_traced(a, b, op).map(|r| (r.points, r.tris, r.origins))
}

/// What [`csg_points_traced`] returns.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CsgOutput {
    /// The points the triangles use.
    pub points: Vec<DVec3>,
    /// The result's triangles.
    pub tris: Vec<[u32; 3]>,
    /// Each triangle's source `(operand, input triangle)`.
    pub origins: Vec<(u8, u32)>,
    /// Pairs `(A's triangle, B's triangle)`, input indices, that lie in one
    /// plane and overlap, in the order found.
    pub coplanar: Vec<(u32, u32)>,
    /// Pairs that meet without crossing: one lies on one side of the
    /// other's plane, touching it (tangent faces, a vertex on a face).
    pub touching: Vec<(u32, u32)>,
    /// Whether the second pass (`docs/research.md` §5.4, item 5) built the
    /// result: the first came out open.
    pub second_pass: bool,
    /// Directed edges, after welding by exact position, used more often
    /// one way than the other: 0 for a closed result.
    pub unmatched: usize,
    /// Edges used four times, by two faces of each operand, one each way:
    /// where the operands touch along an edge.
    pub touching_edges: usize,
    /// Other edges used other than once each way.
    pub non_manifold: usize,
    /// Whether the result was built with the tool's vertices snapped onto
    /// the target's within a rounding error (both passes on the operands
    /// as given came out open; `docs/research.md` §18).
    pub snapped: bool,
}

/// [`csg_points`], with where the operands touch.
///
/// The first pass is og-geom's arrangement. When its result is open
/// (operands that touch: faces on faces, a vertex an ulp off a face), the
/// arrangement is built again by a second pass, and its result is taken
/// if it has fewer unmatched edges. The second pass keeps crossings that
/// round to one position apart, orders points on an edge exactly, lays a
/// triangle out exactly where its rounded outline is not simple, and then
/// makes points at one position one point, collapsing the runs between
/// them (`docs/research.md` §5.4, item 5). Every closed first-pass result
/// is returned as before. If both are open, the tool's vertices within a
/// rounding error of the target's are snapped onto them ([`snap_onto`])
/// and both passes run again; the result with the fewest unmatched edges
/// is taken (a union is never empty). [`CsgOutput::snapped`] says so.
pub fn csg_points_traced(
    a: (&[DVec3], &[[u32; 3]]),
    b: (&[DVec3], &[[u32; 3]]),
    op: CsgOp,
) -> Result<CsgOutput, OperandError> {
    let pass = |robust: bool, b: (&[DVec3], &[[u32; 3]])| -> Result<CsgOutput, OperandError> {
        let mut points = Vec::new();
        let mut at = BTreeMap::new();
        let sa = weld64(0, false, a, &mut points, &mut at)?;
        let sb = weld64(1, false, b, &mut points, &mut at)?;
        let (c, kept) = arrange(points, sa, sb, op, robust);
        let (points, tris, origins, _) = emit(&c, &kept);
        let w = weld_ids(&points);
        // Zero-area triangles on one line (`sliver`): flipped away.
        let (tris, origins): (Vec<[u32; 3]>, Vec<(u8, u32)>) = sliver::flip(&points, &w, &tris)
            .into_iter()
            .map(|(t, src)| (t, origins[src]))
            .unzip();
        let mut counts = edge_counts(&w, &tris, &origins);
        let (tris, origins) = if counts.non_manifold + counts.touching > 0 {
            let (t, o) = cancel_fins(&w, tris, origins);
            counts = edge_counts(&w, &t, &o);
            (t, o)
        } else {
            (tris, origins)
        };
        let source = |pairs: &[TriPair]| {
            let mut v: Vec<(u32, u32)> = Vec::new();
            for &((ma, ia), (_, ib)) in pairs {
                let (sa, sb) = (
                    c.solids[0].source[ia as usize],
                    c.solids[1].source[ib as usize],
                );
                if ma == 0 && !v.contains(&(sa, sb)) {
                    v.push((sa, sb));
                }
            }
            v
        };
        Ok(CsgOutput {
            coplanar: source(&c.coplanar_pairs),
            touching: source(&c.touch_pairs),
            points,
            tris,
            origins,
            second_pass: robust,
            unmatched: counts.unmatched,
            touching_edges: counts.touching,
            non_manifold: counts.non_manifold,
            snapped: false,
        })
    };
    // How far a result is from closed: its unmatched edges, and a union of
    // two closed solids is never empty (a region that leaked across the
    // operands' seam can drop every piece and balance).
    let open = |o: &CsgOutput| {
        if op == CsgOp::Union && o.tris.is_empty() {
            usize::MAX
        } else {
            o.unmatched
        }
    };
    let first = pass(false, b)?;
    // Operands that touch can make crossings a rounding error apart that
    // the first pass cannot keep apart; an open result there is built
    // again by the second pass. Every closed result is the first pass's.
    if open(&first) == 0 {
        return Ok(first);
    }
    let second = pass(true, b)?;
    let best = if open(&second) < open(&first) {
        second
    } else {
        first
    };
    if open(&best) == 0 {
        return Ok(best);
    }
    // Surfaces a rounding error apart that are not exactly one (faces
    // placed by different rounded motions): the tool's vertices within a
    // rounding error of the target's are snapped onto them, and both
    // passes run again on the snapped tool (`docs/research.md` §18).
    let Some(snapped) = snap_onto(a.0, b.0) else {
        return Ok(best);
    };
    let Ok(third) = pass(false, (&snapped, b.1)) else {
        return Ok(best);
    };
    let third = if open(&third) == 0 {
        third
    } else {
        match pass(true, (&snapped, b.1)) {
            Ok(fourth) if open(&fourth) < open(&third) => fourth,
            _ => third,
        }
    };
    Ok(if open(&third) < open(&best) {
        CsgOutput {
            snapped: true,
            ..third
        }
    } else {
        best
    })
}

/// The tool's positions `b` with each one that lies within 2⁻⁴⁰ of the
/// operands' size of a position of the target `a`, but not on one, moved
/// onto the nearest (the first of equals); None when none moves. The
/// design's snap before a boolean (`docs/design.md` §6), taken only when
/// both passes leave the result open.
fn snap_onto(a: &[DVec3], b: &[DVec3]) -> Option<Vec<DVec3>> {
    let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
    for &p in a.iter().chain(b) {
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let tol = (hi - lo).length() * (1.0 / (1u64 << 40) as f64);
    let tol2 = tol * tol;
    // The target's positions by x, for a window search.
    let mut by_x: Vec<u32> = (0..a.len() as u32).collect();
    shape_core::sort::sort_by(&mut by_x, |&i, &j| {
        a[i as usize].x.total_cmp(&a[j as usize].x).then(i.cmp(&j))
    });
    let mut out = b.to_vec();
    let mut moved = false;
    for p in &mut out {
        let start = by_x.partition_point(|&i| a[i as usize].x < p.x - tol);
        let mut best: Option<(f64, u32)> = None;
        for &i in &by_x[start..] {
            let q = a[i as usize];
            if q.x > p.x + tol {
                break;
            }
            let d2 = (q - *p).length_squared();
            if d2 <= tol2 && best.is_none_or(|(bd, bi)| d2 < bd || (d2 == bd && i < bi)) {
                best = Some((d2, i));
            }
        }
        if let Some((d2, i)) = best
            && d2 > 0.0
        {
            *p = a[i as usize];
            moved = true;
        }
    }
    moved.then_some(out)
}

/// Drops pairs of kept triangles on the same three positions facing
/// opposite ways: a piece of each operand where their surfaces lie on one
/// another, both kept because a sliver a rounding error wide was
/// classified on each side. Such a pair bounds no volume, and its edges
/// cancel, so a closed result stays closed; without it, its edges are
/// used four times (`docs/research.md` §5.4, item 5). Pairs are taken in
/// output order.
fn cancel_fins(
    w: &[u32],
    tris: Vec<[u32; 3]>,
    origins: Vec<(u8, u32)>,
) -> (Vec<[u32; 3]>, Vec<(u8, u32)>) {
    // Each triangle by its sorted corners, and whether its winding is the
    // sorted order's (an even permutation).
    let key = |t: [u32; 3]| {
        let t = t.map(|v| w[v as usize]);
        let mut s = t;
        s.sort_unstable();
        let r = (0..3).find(|&k| t[k] == s[0]).unwrap_or(0);
        (s, t[(r + 1) % 3] == s[1])
    };
    let mut by: BTreeMap<[u32; 3], (Vec<usize>, Vec<usize>)> = BTreeMap::new();
    for (i, &t) in tris.iter().enumerate() {
        let (s, even) = key(t);
        if s[0] == s[1] || s[1] == s[2] {
            continue;
        }
        let e = by.entry(s).or_default();
        if even { e.0.push(i) } else { e.1.push(i) }
    }
    let mut drop = vec![false; tris.len()];
    for (pos, neg) in by.values() {
        for (&i, &j) in pos.iter().zip(neg) {
            drop[i] = true;
            drop[j] = true;
        }
    }
    if !drop.contains(&true) {
        return (tris, origins);
    }
    tris.into_iter()
        .zip(origins)
        .zip(drop)
        .filter(|(_, d)| !d)
        .map(|(x, _)| x)
        .unzip()
}

/// Each point's id after welding by exact position (first-seen order).
fn weld_ids(points: &[DVec3]) -> Vec<u32> {
    let mut id: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    points
        .iter()
        .map(|p| {
            let n = id.len() as u32;
            *id.entry(p.to_array().map(|x| (x + 0.0).to_bits()))
                .or_insert(n)
        })
        .collect()
}

/// [`CsgOutput`]'s edge counts.
struct EdgeCounts {
    unmatched: usize,
    touching: usize,
    non_manifold: usize,
}

/// How a result's edges are used, after welding (`w`), zero-area
/// triangles skipped: directed edges that do not balance, edges where the
/// operands touch (two faces of each, one each way), and other edges not
/// used once each way.
fn edge_counts(w: &[u32], tris: &[[u32; 3]], origins: &[(u8, u32)]) -> EdgeCounts {
    // Per edge, low id first: uses by (operand, low to high).
    let mut uses: BTreeMap<(u32, u32), [u32; 4]> = BTreeMap::new();
    for (t, &(m, _)) in tris.iter().zip(origins) {
        let t = t.map(|v| w[v as usize]);
        if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
            continue;
        }
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            let slot = 2 * (m.min(1) as usize) + usize::from(a > b);
            uses.entry((a.min(b), a.max(b))).or_default()[slot] += 1;
        }
    }
    let mut c = EdgeCounts {
        unmatched: 0,
        touching: 0,
        non_manifold: 0,
    };
    for u in uses.values() {
        let (fwd, back) = (u[0] + u[2], u[1] + u[3]);
        c.unmatched += fwd.abs_diff(back) as usize;
        if fwd == 1 && back == 1 {
            continue;
        }
        if *u == [1, 1, 1, 1] {
            c.touching += 1;
        } else {
            c.non_manifold += 1;
        }
    }
    c
}

/// A self-union's output ([`self_union_points`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelfUnion {
    /// The points the triangles use.
    pub points: Vec<DVec3>,
    /// The triangles, facing out.
    pub tris: Vec<[u32; 3]>,
    /// Each triangle's source triangle in the input.
    pub source: Vec<u32>,
    /// Edges `[a, b]`, `a < b`, along which the surface crossed itself.
    pub seams: Vec<[u32; 2]>,
}

/// The outside surface of a closed, oriented mesh that may pass through
/// itself: the arrangement [`csg_points`] builds for two operands, built
/// for one. Every pair of its triangles that crosses ([`selfcheck`]) is
/// intersected and cut, and each region is classified by the mesh's own
/// winding number. What is kept bounds the space of winding number at
/// least 1, facing out of it; a surface where the winding goes from 1 to 2,
/// or from -1 to 0, is dropped. Where faces lie on one another, the one
/// from the lowest-numbered triangle stands for them.
///
/// The operand is refused as [`csg_points`] refuses one (operand 0),
/// except that a directed edge may be used more than once if its reverse
/// is used as often (two copies of a face).
pub fn self_union_points(mesh: (&[DVec3], &[[u32; 3]])) -> Result<SelfUnion, OperandError> {
    let mut points = Vec::new();
    let mut at = BTreeMap::new();
    let s = weld64(0, true, mesh, &mut points, &mut at)?;
    let (c, kept) = arrange_self(points, s);
    let (points, tris, origins, remap) = emit(&c, &kept);
    let seams = c
        .crossings
        .iter()
        .filter_map(|&(a, b)| {
            let (x, y) = (*remap.get(&a)?, *remap.get(&b)?);
            Some([x.min(y), x.max(y)])
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(SelfUnion {
        points,
        tris,
        source: origins.into_iter().map(|o| o.1).collect(),
        seams,
    })
}

/// Every pair of triangles whose boxes overlap, in the order the
/// all-pairs loop visited them (A's index, then B's), so crossing points
/// are made in the same order and the output is unchanged. Found by a
/// sweep along x over the boxes sorted by their low x.
fn candidate_pairs(a: &[(u32, (DVec3, DVec3))], b: &[(u32, (DVec3, DVec3))]) -> Vec<(u32, u32)> {
    let by_min_x = |v: &[(u32, (DVec3, DVec3))]| {
        let mut v = v.to_vec();
        v.sort_by(|x, y| x.1.0.x.total_cmp(&y.1.0.x).then(x.0.cmp(&y.0)));
        v
    };
    let (sa, sb) = (by_min_x(a), by_min_x(b));
    let (mut ia, mut ib) = (0, 0);
    let mut active_a: Vec<(u32, (DVec3, DVec3))> = Vec::new();
    let mut active_b: Vec<(u32, (DVec3, DVec3))> = Vec::new();
    let mut pairs = Vec::new();
    while ia < sa.len() || ib < sb.len() {
        let take_a = ib >= sb.len() || (ia < sa.len() && sa[ia].1.0.x <= sb[ib].1.0.x);
        if take_a {
            let cur = sa[ia];
            ia += 1;
            active_b.retain(|o| o.1.1.x >= cur.1.0.x);
            pairs.extend(
                active_b
                    .iter()
                    .filter(|o| overlap(cur.1, o.1))
                    .map(|o| (cur.0, o.0)),
            );
            active_a.push(cur);
        } else {
            let cur = sb[ib];
            ib += 1;
            active_a.retain(|o| o.1.1.x >= cur.1.0.x);
            pairs.extend(
                active_a
                    .iter()
                    .filter(|o| overlap(o.1, cur.1))
                    .map(|o| (o.0, cur.0)),
            );
            active_b.push(cur);
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    pairs
}

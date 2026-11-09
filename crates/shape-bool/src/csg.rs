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

use glam::{DVec2, DVec3};
use robust::{Coord, Coord3D, orient2d, orient3d};
use shape_gen::{Mesh, Placement, normalize, place, trig};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};
use std::collections::{BTreeMap, BTreeSet};

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
}

struct Csg {
    points: Vec<DVec3>,
    /// The edges each point lies on (beyond its own mesh's corners).
    on: Vec<Vec<EdgeKey>>,
    made: BTreeMap<Origin, u32>,
    /// Each made point by its exact position.
    made_at: BTreeMap<[u64; 3], u32>,
    solids: [Solid; 2],
    cuts: [BTreeMap<u32, Cut>; 2],
    crossings: BTreeSet<(u32, u32)>,
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
        if let Some(&id) = self.made_at.get(&key) {
            for &e in on {
                self.note_on(id, e);
            }
            self.made.insert(origin, id);
            return id;
        }
        let id = self.points.len() as u32;
        self.points.push(pos);
        self.on.push(on.to_vec());
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
    /// the other mesh, whose plane it crosses strictly. None when it misses.
    fn edge_cross(&mut self, m: u8, p: u32, q: u32, tri: u32) -> Option<u32> {
        let other = 1 - m;
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

    /// The point where two edges (one of each mesh) cross.
    fn edge_edge(&mut self, e: EdgeKey, f: EdgeKey) -> u32 {
        let (e, f) = if e.0 == 0 { (e, f) } else { (f, e) };
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

    fn add_segment(&mut self, m: u8, tri: u32, a: u32, b: u32) {
        if a == b {
            return;
        }
        self.add_points(m, tri, &[a, b]);
        self.cut(m, tri).segments.push([a, b]);
        self.crossings.insert((a.min(b), a.max(b)));
    }

    /// Intersects triangle `ia` of A with triangle `ib` of B.
    fn pair(&mut self, ia: u32, ib: u32) {
        let ta = self.solids[0].tris[ia as usize];
        let tb = self.solids[1].tris[ib as usize];
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
            self.coplanar(ia, ib);
            return;
        }
        let mut found: Vec<u32> = Vec::new();
        // Vertices of one triangle in the other's plane and inside it.
        for (m, t, o, ot) in [(0u8, ta, oa, tb), (1u8, tb, ob, ta)] {
            for k in 0..3 {
                if o[k] != 0 {
                    continue;
                }
                let v = t[k];
                match self.locate(ot, self.p(v)) {
                    None => {}
                    Some(Loc::Edge(j)) => {
                        self.note_on(v, edge_key(1 - m, ot[j], ot[(j + 1) % 3]));
                        found.push(v);
                    }
                    // At the other's vertex (two meshes' vertices at one
                    // position keep their own ids) or inside it.
                    Some(Loc::Vertex(_) | Loc::Inside) => found.push(v),
                }
            }
        }
        // Edges of one triangle through the other.
        for (m, t, o, other) in [(0u8, ta, oa, ib), (1u8, tb, ob, ia)] {
            for k in 0..3 {
                let (i, j) = (k, (k + 1) % 3);
                if o[i] * o[j] < 0
                    && let Some(id) = self.edge_cross(m, t[i], t[j], other)
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
        self.add_points(0, ia, &uniq);
        self.add_points(1, ib, &uniq);
        for w in uniq.windows(2) {
            self.add_segment(0, ia, w[0], w[1]);
            self.add_segment(1, ib, w[0], w[1]);
        }
    }

    /// Two triangles in one plane: each one's edges inside the other are
    /// cut into it, and the points where their edges cross.
    fn coplanar(&mut self, ia: u32, ib: u32) {
        let ta = self.solids[0].tris[ia as usize];
        let tb = self.solids[1].tris[ib as usize];
        let axis = drop_axis(self.tri_normal(ta));
        let pr = |c: &Self, i: u32| project(c.p(i), axis);
        // Edge crossings.
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
                    let (e, f) = (edge_key(0, a0, a1), edge_key(1, b0, b1));
                    let id = self.edge_edge(e, f);
                    self.add_points(0, ia, &[id]);
                    self.add_points(1, ib, &[id]);
                    on_edge[0].push((id, e));
                    on_edge[1].push((id, f));
                }
            }
        }
        // Vertices of each inside the other.
        for (m, t, ot, oi) in [(0u8, ta, tb, ib), (1u8, tb, ta, ia)] {
            for &v in &t {
                if let Some(loc) = self.locate(ot, self.p(v)) {
                    if let Loc::Edge(j) = loc {
                        self.note_on(v, edge_key(1 - m, ot[j], ot[(j + 1) % 3]));
                    }
                    self.add_points(1 - m, oi, &[v]);
                    // The vertex lies on its own triangle's two edges.
                    for k in 0..3 {
                        if t[k] == v {
                            on_edge[m as usize].push((v, edge_key(m, t[k], t[(k + 1) % 3])));
                            on_edge[m as usize].push((v, edge_key(m, t[(k + 2) % 3], t[k])));
                        }
                    }
                }
            }
        }
        // Each edge's pieces inside the other triangle become its cuts.
        for (m, t, ot, oi) in [(0u8, ta, tb, ib), (1u8, tb, ta, ia)] {
            for k in 0..3 {
                let e = edge_key(m, t[k], t[(k + 1) % 3]);
                let (p0, p1) = (self.p(e.1), self.p(e.2));
                let mut along: Vec<u32> = on_edge[m as usize]
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
                        self.add_segment(1 - m, oi, w[0], w[1]);
                        self.crossings.insert((w[0].min(w[1]), w[0].max(w[1])));
                    }
                }
            }
        }
    }

    /// Triangle `tri` of mesh `m` cut into pieces (global ids, the
    /// triangle's own winding).
    fn pieces(&self, m: u8, tri: u32) -> Vec<[u32; 3]> {
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
    let (c, kept) = arrange(points, sa, sb, op);
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

/// Steps 1 to 3 and the choice of step 4: which pieces `op` keeps.
fn arrange(points: Vec<DVec3>, sa: Solid, sb: Solid, op: CsgOp) -> (Csg, Vec<Kept>) {
    let on = vec![Vec::new(); points.len()];
    let mut c = Csg {
        points,
        on,
        made: BTreeMap::new(),
        made_at: BTreeMap::new(),
        solids: [sa, sb],
        cuts: [BTreeMap::new(), BTreeMap::new()],
        crossings: BTreeSet::new(),
    };
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
        c.pair(ia, ib);
    }
    // 2. The pieces.
    struct Piece {
        mesh: u8,
        tri: u32,
        ids: [u32; 3],
    }
    let mut pieces: Vec<Piece> = Vec::new();
    for m in 0..2u8 {
        for tri in 0..c.solids[m as usize].tris.len() as u32 {
            for ids in c.pieces(m, tri) {
                pieces.push(Piece { mesh: m, tri, ids });
            }
        }
    }
    // 3. Regions: pieces of one mesh joined across edges no crossing runs along.
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
            if c.crossings.contains(&e) {
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
        let [a, b, cc] = p.ids.map(|i| c.points[i as usize]);
        (b - a).cross(cc - a).length() * 0.5
    };
    // Each region's largest piece (the first of equals).
    let mut best: BTreeMap<usize, usize> = BTreeMap::new();
    for pi in 0..pieces.len() {
        let r = find(&mut parent, pi);
        match best.get(&r) {
            Some(&q) if area(&pieces[q]) >= area(&pieces[pi]) => {}
            _ => {
                best.insert(r, pi);
            }
        }
    }
    let lo = c.solids[0].min.min(c.solids[1].min);
    let hi = c.solids[0].max.max(c.solids[1].max);
    let scale = (hi - lo).length().max(1e-30);
    // (inside the other mesh in front, behind).
    let mut sides: BTreeMap<usize, (bool, bool)> = BTreeMap::new();
    let mut roots: Vec<usize> = best.keys().copied().collect();
    roots.sort_unstable();
    for r in roots {
        let p = &pieces[best[&r]];
        let [a, b, cc] = p.ids.map(|i| c.points[i as usize]);
        let n = (b - a).cross(cc - a);
        let perimeter = (b - a).length() + (cc - b).length() + (a - cc).length();
        let inradius = n.length() / perimeter.max(1e-300);
        let eps = (scale * 1e-6).min(inradius * 0.5);
        let n = normalize(n);
        let centre = (a + b + cc) / 3.0;
        let other = &c.solids[1 - p.mesh as usize].tris;
        let inside = |q: DVec3| winding(&c.points, other, q) > 0.5;
        sides.insert(r, (inside(centre + n * eps), inside(centre - n * eps)));
    }
    let mut kept = Vec::new();
    for (pi, p) in pieces.iter().enumerate() {
        let r = find(&mut parent, pi);
        let (front, back) = sides[&r];
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
        if !keep {
            continue;
        }
        kept.push(Kept {
            mesh: p.mesh,
            tri: p.tri,
            ids: p.ids,
            flip,
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
    /// Two of the operand's triangles that share no vertex meet: the
    /// surface folds through or touches itself.
    SelfIntersecting(u8),
}

/// The f64 core of [`csg`] on welded operands: points and triangles in,
/// points, triangles and each output triangle's source `(operand,
/// triangle)` out. Unlike [`csg`], an operand that is not closed and
/// oriented after exact welding, or whose triangles meet other than at a
/// shared vertex (points within 1e-9 of its size count as one), is refused (`docs/research.md` §5.4).
#[allow(clippy::type_complexity)]
pub fn csg_points(
    a: (&[DVec3], &[[u32; 3]]),
    b: (&[DVec3], &[[u32; 3]]),
    op: CsgOp,
) -> Result<(Vec<DVec3>, Vec<[u32; 3]>, Vec<(u8, u32)>), OperandError> {
    let mut points = Vec::new();
    let mut at = BTreeMap::new();
    let mut weld64 = |m: u8, (pos, tris): (&[DVec3], &[[u32; 3]])| -> Result<Solid, OperandError> {
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
                .all(|(&(x, y), &n)| n == 1 && directed.get(&(y, x)) == Some(&1));
        if !closed {
            return Err(OperandError::NotClosed(m));
        }
        // Points closer than 1e-9 of the operand's size count as one: the
        // CSG's own output can leave such pairs unwelded (`docs/research.md`
        // §5.4), and the classification of step 4 probes at 1e-6.
        let weld = (s.max - s.min).length() * 1e-9;
        if !shape_mesh::self_intersections(&points, &s.tris, weld).is_empty() {
            return Err(OperandError::SelfIntersecting(m));
        }
        Ok(s)
    };
    let sa = weld64(0, a)?;
    let sb = weld64(1, b)?;
    let (c, kept) = arrange(points, sa, sb, op);
    // Only the points the kept pieces use, in first-use order.
    let mut remap: BTreeMap<u32, u32> = BTreeMap::new();
    let mut out_points = Vec::new();
    let mut out_tris = Vec::with_capacity(kept.len());
    let mut origins = Vec::with_capacity(kept.len());
    for p in &kept {
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
    Ok((out_points, out_tris, origins))
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

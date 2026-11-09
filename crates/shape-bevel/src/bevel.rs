//! The bevel core: offsets, boundary points, profiles, strips and the
//! rebuilt faces (`docs/research.md` §7).
//!
//! Vocabulary (after Blender's published design, §2.2):
//! - Around a vertex the triangles form a fan; a **spoke** is an edge out
//!   of the vertex. Bevelled spokes split the fan into **gaps**; real
//!   unbevelled spokes split a gap into **wedges** (one per face).
//! - Each gap gets one **boundary point** (a terminal vertex, with one
//!   bevelled edge, gets two). Every triangle corner at the vertex is
//!   replaced by its gap's point, or, on the end face of a terminal
//!   edge, by a piece of the edge's profile.
//! - Each bevelled edge end gets a **chain**: its profile, from the gap
//!   after the edge to the gap before it (counter-clockwise seen from
//!   outside). An edge's **strip** joins the chains at its two ends.
//! - Where three or more bevelled edges meet, the chains bound a hole
//!   filled by a corner **patch** (`patch.rs`).

use std::collections::{BTreeMap, BTreeSet};

use shape_core::{DVec2, DVec3, predicates, trig};
use shape_mesh::{EdgeKey, Solid, edge_key};

use crate::BevelError;
use crate::patch::{self, Corner};
use crate::profile;

/// What the new faces are called.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Fillet,
    Chamfer,
}

impl Kind {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Kind::Fillet => "fillet",
            Kind::Chamfer => "chamfer",
        }
    }
}

/// How far the offset lines sit from each edge, on its two faces.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Offsets {
    /// The same on both faces.
    Equal(f64),
    /// The first on the face that carries one of the edge's names (the
    /// cap), the second on the other.
    Two(f64, f64),
    /// The first on the cap face; the chamfer face meets the cap at this
    /// angle (radians).
    Angle(f64, f64),
}

/// A resolved bevel request.
pub(crate) struct Spec {
    pub edges: Vec<EdgeKey>,
    pub offsets: Offsets,
    /// 0: from the radius (`shape_2d::curve::segments`).
    pub segments: u32,
    /// The superellipse exponent (2 a circle, 1 straight).
    pub exponent: f64,
    pub kind: Kind,
}

/// Faces closer to coplanar than this (on unit normals) are one plane
/// when placing a boundary point.
const PLANAR_DOT: f64 = 1.0 - 1e-12;
/// Below this sine two edges count as collinear.
const COLLINEAR_SIN: f64 = 1e-9;

pub(crate) fn tri_normal(s: &Solid, t: usize) -> DVec3 {
    let [a, b, c] = s.triangles[t].map(|v| s.positions[v as usize]);
    (b - a).cross(c - a).normalize_or_zero()
}

/// The turn between the two faces of an edge, radians (0 flat, π/2 a box
/// edge).
pub(crate) fn turn(s: &Solid, ta: usize, tb: usize) -> f64 {
    trig::acos(tri_normal(s, ta).dot(tri_normal(s, tb)).clamp(-1.0, 1.0))
}

/// Directed edge → triangle, and triangles per vertex.
pub(crate) struct Topology {
    pub dir: BTreeMap<(u32, u32), usize>,
    pub at: Vec<Vec<usize>>,
}

pub(crate) fn topology(s: &Solid) -> Topology {
    let mut dir = BTreeMap::new();
    let mut at = vec![Vec::new(); s.positions.len()];
    for (i, t) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            dir.insert((t[k], t[(k + 1) % 3]), i);
            at[t[k] as usize].push(i);
        }
    }
    Topology { dir, at }
}

/// The fan around a vertex: triangle `i` is `(v, spokes[i], spokes[i+1])`.
struct Fan {
    tris: Vec<usize>,
    spokes: Vec<u32>,
}

fn rotate_to(t: [u32; 3], v: u32) -> [u32; 3] {
    let r = (0..3).find(|&k| t[k] == v).unwrap_or(0);
    [t[r], t[(r + 1) % 3], t[(r + 2) % 3]]
}

fn fan(s: &Solid, topo: &Topology, v: u32) -> Result<Fan, BevelError> {
    let here = &topo.at[v as usize];
    let p = s.positions[v as usize];
    let start = *here.iter().min().ok_or(BevelError::NotClosed)?;
    let mut f = Fan {
        tris: Vec::new(),
        spokes: Vec::new(),
    };
    let mut cur = start;
    loop {
        let t = rotate_to(s.triangles[cur], v);
        f.tris.push(cur);
        f.spokes.push(t[1]);
        cur = *topo.dir.get(&(v, t[2])).ok_or(BevelError::NotClosed)?;
        if cur == start {
            break;
        }
        if f.tris.len() > here.len() {
            return Err(BevelError::Unsupported(
                p,
                "a vertex whose fan does not close",
            ));
        }
    }
    if f.tris.len() != here.len() {
        return Err(BevelError::Unsupported(
            p,
            "two cones of faces meet at one vertex",
        ));
    }
    Ok(f)
}

/// Per selected edge: its triangles and four offsets
/// `[a at lo, b at lo, a at hi, b at hi]`, where face `a` holds the
/// directed edge lo → hi.
struct EdgeInfo {
    off: [f64; 4],
}

impl EdgeInfo {
    /// Offset on the gap after the edge (counter-clockwise) at `v`.
    fn after(&self, key: EdgeKey, v: u32) -> f64 {
        if v == key.0 { self.off[0] } else { self.off[3] }
    }
    /// Offset on the gap before the edge at `v`.
    fn before(&self, key: EdgeKey, v: u32) -> f64 {
        if v == key.0 { self.off[1] } else { self.off[2] }
    }
}

struct Build<'a> {
    s: &'a Solid,
    topo: Topology,
    edges: BTreeMap<EdgeKey, EdgeInfo>,
    pos: Vec<DVec3>,
    /// (triangle, vertex) → what replaces the vertex in that triangle,
    /// from the side of the corner after it to the side before it.
    repl: BTreeMap<(usize, u32), Vec<u32>>,
    /// (vertex, other end) → the edge end's chain, after → before.
    chains: BTreeMap<(u32, u32), Vec<u32>>,
    /// New triangles and the unit (strip or patch) each belongs to.
    new_tris: Vec<([u32; 3], usize)>,
    units: usize,
    /// Units to make one surface.
    joins: Vec<(usize, usize)>,
    /// Pairs of edges whose strips meet smoothly.
    smooth: Vec<(EdgeKey, EdgeKey)>,
    /// Patch unit per vertex, with the edges around it.
    patches: Vec<(usize, Vec<EdgeKey>)>,
    samples: BTreeMap<u32, Vec<(f64, f64)>>,
    n: u32,
    exponent: f64,
    kind: Kind,
}

impl Build<'_> {
    fn push(&mut self, p: DVec3) -> u32 {
        self.pos.push(p);
        self.pos.len() as u32 - 1
    }

    fn samples(&mut self, n: u32) -> Vec<(f64, f64)> {
        let p = self.exponent;
        self.samples
            .entry(n)
            .or_insert_with(|| profile::samples(p, n))
            .clone()
    }

    fn unit(&mut self) -> usize {
        self.units += 1;
        self.units - 1
    }

    /// A chain of `n` segments from the existing points `a` to `b` around
    /// `c`, its inner points new.
    fn chain(&mut self, a: u32, b: u32, c: DVec3) -> Vec<u32> {
        let s = self.samples(self.n);
        let pts = profile::chain(self.pos[a as usize], self.pos[b as usize], c, &s);
        let last = pts.len() - 1;
        pts.iter()
            .enumerate()
            .map(|(k, &p)| match k {
                0 => a,
                k if k == last => b,
                _ => self.push(p),
            })
            .collect()
    }

    /// The boundary point of the wedge between spokes `lo` and `hi` (fan
    /// indices, `hi` may be `k` for spoke 0), each edge's offset line at
    /// `dl` and `dr` (0 for an unbevelled edge, whose line the point then
    /// lies on).
    fn wedge_point(
        &self,
        v: u32,
        f: &Fan,
        lo: usize,
        hi: usize,
        dl: f64,
        dr: f64,
    ) -> Result<DVec3, BevelError> {
        let k = f.spokes.len();
        let p = self.s.positions[v as usize];
        let dir = |i: usize| (self.s.positions[f.spokes[i % k] as usize] - p).normalize_or_zero();
        let (l, r) = (dir(lo), dir(hi));
        let normals: Vec<DVec3> = (lo..hi).map(|i| tri_normal(self.s, f.tris[i])).collect();
        let planar = normals.len() == 1 || normals.iter().all(|n| n.dot(normals[0]) > PLANAR_DOT);
        let out = if planar {
            let nrm = normals
                .iter()
                .fold(DVec3::ZERO, |a, &b| a + b)
                .normalize_or_zero();
            let e2 = nrm.cross(l).normalize_or_zero();
            let (rx, ry) = (r.dot(l), r.dot(e2));
            if ry.abs() < COLLINEAR_SIN {
                if rx > 0.0 {
                    return Err(BevelError::Unsupported(p, "a face corner of zero angle"));
                }
                p + e2 * ((dl + dr) / 2.0)
            } else {
                p + l * ((dr + rx * dl) / ry) + e2 * dl
            }
        } else {
            let sin = |a: DVec3, b: DVec3| a.cross(b).length();
            if dl == 0.0 || dr == 0.0 {
                // Slide along the unbevelled edge.
                let (along, d) = if dl == 0.0 { (l, dr) } else { (r, dl) };
                let s = sin(l, r);
                if s < COLLINEAR_SIN {
                    return Err(BevelError::Unsupported(
                        p,
                        "an edge in line with its neighbour",
                    ));
                }
                p + along * (d / s)
            } else {
                // Slide along the inner edge most across the bevelled ones.
                let mut best: Option<(f64, DVec3)> = None;
                for i in lo + 1..hi {
                    let e = dir(i);
                    let w = sin(e, l) + sin(e, r);
                    if best.is_none_or(|(b, _)| w > b) {
                        best = Some((w, e));
                    }
                }
                let (w, e) = best.ok_or(BevelError::Unsupported(p, "a curved face corner"))?;
                if w < COLLINEAR_SIN {
                    return Err(BevelError::TooLarge(p));
                }
                p + e * ((dl + dr) / w)
            }
        };
        if out.is_finite() {
            Ok(out)
        } else {
            Err(BevelError::TooLarge(p))
        }
    }

    fn vertex(&mut self, v: u32) -> Result<(), BevelError> {
        let s = self.s;
        let p = s.positions[v as usize];
        let mut f = fan(s, &self.topo, v)?;
        let k = f.spokes.len();
        let bevelled = |f: &Fan, i: usize| {
            let key = edge_key(v, f.spokes[i % k]);
            self.edges.contains_key(&key)
        };
        let r0 = (0..k).find(|&i| bevelled(&f, i)).unwrap_or(0);
        f.tris.rotate_left(r0);
        f.spokes.rotate_left(r0);
        let real: Vec<bool> = (0..k)
            .map(|i| s.surfaces[f.tris[(i + k - 1) % k]] != s.surfaces[f.tris[i]])
            .collect();
        let bidx: Vec<usize> = (0..k).filter(|&i| bevelled(&f, i)).collect();
        let m = bidx.len();
        let key_of = |i: usize| edge_key(v, f.spokes[i % k]);
        let mut gap_point: Vec<u32> = Vec::with_capacity(m);
        let mut gap_plain: Vec<bool> = Vec::with_capacity(m);
        for g in 0..m {
            let (lo, hi) = (bidx[g], if g + 1 < m { bidx[g + 1] } else { k });
            let inner: Vec<usize> = (lo + 1..hi).filter(|&i| real[i]).collect();
            let (el, er) = (key_of(lo), key_of(hi));
            let dl = self.edges[&el].after(el, v);
            let dr = self.edges[&er].before(er, v);
            gap_plain.push(inner.is_empty());
            match (m, inner.len()) {
                (2.., 0) => {
                    let b = self.wedge_point(v, &f, lo, hi, dl, dr)?;
                    let id = self.push(b);
                    gap_point.push(id);
                    for &t in &f.tris[lo..hi] {
                        self.repl.insert((t, v), vec![id]);
                    }
                }
                (2.., 1) => {
                    let u = inner[0];
                    let p1 = self.wedge_point(v, &f, lo, u, dl, 0.0)?;
                    let p2 = self.wedge_point(v, &f, u, hi, 0.0, dr)?;
                    let id = self.push((p1 + p2) / 2.0);
                    gap_point.push(id);
                    for &t in &f.tris[lo..hi] {
                        self.repl.insert((t, v), vec![id]);
                    }
                }
                (1, 2) => {
                    // A terminal end: the profile lies on the end face.
                    let (u1, u2) = (inner[0], inner[1]);
                    let a = self.wedge_point(v, &f, lo, u1, dl, 0.0)?;
                    let b = self.wedge_point(v, &f, u2, hi, 0.0, dr)?;
                    let (ia, ib) = (self.push(a), self.push(b));
                    for &t in &f.tris[lo..u1] {
                        self.repl.insert((t, v), vec![ia]);
                    }
                    for &t in &f.tris[u2..hi] {
                        self.repl.insert((t, v), vec![ib]);
                    }
                    let chain = self.chain(ia, ib, p);
                    // Share the chain among the end face's triangles by
                    // the direction of their spokes.
                    let mut split = vec![0usize];
                    for j in u1 + 1..u2 {
                        let d = (s.positions[f.spokes[j] as usize] - p).normalize_or_zero();
                        let from = *split.last().unwrap_or(&0);
                        let mut best = (f64::NEG_INFINITY, from);
                        for (c, &id) in chain.iter().enumerate().skip(from) {
                            let cos = (self.pos[id as usize] - p).normalize_or_zero().dot(d);
                            if cos > best.0 {
                                best = (cos, c);
                            }
                        }
                        split.push(best.1);
                    }
                    split.push(chain.len() - 1);
                    for (w, i) in (u1..u2).enumerate() {
                        let mut seq = chain[split[w]..=split[w + 1]].to_vec();
                        seq.reverse();
                        self.repl.insert((f.tris[i], v), seq);
                    }
                    self.chains.insert((v, f.spokes[bidx[0]]), chain);
                    return Ok(());
                }
                _ => {
                    return Err(BevelError::Unsupported(
                        p,
                        "more than one unbevelled edge between two bevelled ones, or a bevelled edge that ends among smooth faces",
                    ));
                }
            }
        }
        if m == 2 {
            let chain = self.chain(gap_point[0], gap_point[1], p);
            let (e0, e1) = (f.spokes[bidx[0]], f.spokes[bidx[1]]);
            let mut rev = chain.clone();
            rev.reverse();
            self.chains.insert((v, e0), chain);
            self.chains.insert((v, e1), rev);
            if gap_plain[0] && gap_plain[1] {
                self.smooth.push((edge_key(v, e0), edge_key(v, e1)));
            }
            return Ok(());
        }
        // Three or more: chains B_j → B_{j-1} around a corner patch.
        let b: Vec<DVec3> = gap_point.iter().map(|&i| self.pos[i as usize]).collect();
        let o = if m == 3 {
            p + b.iter().fold(DVec3::ZERO, |a, &q| a + (q - p)) / 2.0
        } else {
            DVec3::ZERO
        };
        let mut ends = Vec::with_capacity(m);
        let mut keys = Vec::with_capacity(m);
        for j in 0..m {
            let (ia, ib) = (gap_point[j], gap_point[(j + m - 1) % m]);
            let (a, bb) = (self.pos[ia as usize], self.pos[ib as usize]);
            let c = if m == 3 {
                a + bb - o
            } else {
                let l = (s.positions[f.spokes[bidx[j]] as usize] - p).normalize_or_zero();
                p + l * (((a + bb) / 2.0 - p).dot(l))
            };
            let chain = self.chain(ia, ib, c);
            self.chains.insert((v, f.spokes[bidx[j]]), chain.clone());
            ends.push((a, bb, c, chain));
            keys.push(edge_key(v, f.spokes[bidx[j]]));
        }
        let unit = self.unit();
        let corner = Corner {
            v: p,
            o,
            ends,
            n: self.n,
        };
        let exponent = self.exponent;
        let mut samples = |n: u32| profile::samples(exponent, n);
        let fill = patch::fill(&corner, exponent, &mut samples, &mut self.pos)?;
        for t in fill {
            self.new_tris.push((t, unit));
        }
        self.patches.push((unit, keys));
        Ok(())
    }
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra.max(rb)] = ra.min(rb);
    }
}

/// The plane projection that keeps a triangle facing `n` counter-clockwise.
fn projector(n: DVec3) -> impl Fn(DVec3) -> DVec2 {
    let a = n.abs();
    move |p: DVec3| {
        if a.z >= a.x && a.z >= a.y {
            if n.z >= 0.0 {
                DVec2::new(p.x, p.y)
            } else {
                DVec2::new(p.y, p.x)
            }
        } else if a.x >= a.y {
            if n.x >= 0.0 {
                DVec2::new(p.y, p.z)
            } else {
                DVec2::new(p.z, p.y)
            }
        } else if n.y >= 0.0 {
            DVec2::new(p.z, p.x)
        } else {
            DVec2::new(p.x, p.z)
        }
    }
}

/// Ear-clips a polygon (vertex ids) facing `n`; every triangle must turn
/// counter-clockwise by the exact predicate, or the bevel does not fit.
fn clip(poly: &[u32], pos: &[DVec3], n: DVec3) -> Option<Vec<[u32; 3]>> {
    use core::cmp::Ordering::Greater;
    let proj = projector(n);
    let mut ring: Vec<u32> = poly.to_vec();
    ring.dedup();
    while ring.len() > 1 && ring.first() == ring.last() {
        ring.pop();
    }
    let p2 = |i: u32| proj(pos[i as usize]);
    let mut out = Vec::new();
    while ring.len() > 3 {
        let len = ring.len();
        let ear = (0..len).find(|&i| {
            let (a, b, c) = (ring[(i + len - 1) % len], ring[i], ring[(i + 1) % len]);
            let (pa, pb, pc) = (p2(a), p2(b), p2(c));
            if predicates::orient2d(pa, pb, pc) != Greater {
                return false;
            }
            ring.iter().all(|&q| {
                if q == a || q == b || q == c {
                    return true;
                }
                let pq = p2(q);
                !(predicates::orient2d(pa, pb, pq) != core::cmp::Ordering::Less
                    && predicates::orient2d(pb, pc, pq) != core::cmp::Ordering::Less
                    && predicates::orient2d(pc, pa, pq) != core::cmp::Ordering::Less)
            })
        })?;
        let (a, b, c) = (
            ring[(ear + len - 1) % len],
            ring[ear],
            ring[(ear + 1) % len],
        );
        out.push([a, b, c]);
        ring.remove(ear);
    }
    if ring.len() < 3 {
        return None;
    }
    let t = [ring[0], ring[1], ring[2]];
    if predicates::orient2d(p2(t[0]), p2(t[1]), p2(t[2])) != Greater {
        return None;
    }
    out.push(t);
    Some(out)
}

const CAPS: [&str; 4] = ["top", "bottom", "start", "end"];

fn is_cap(name: &str) -> bool {
    CAPS.contains(&name.rsplit('.').next().unwrap_or(name))
}

pub(crate) fn bevel(s: &Solid, spec: &Spec) -> Result<Solid, BevelError> {
    if !s.measure().closed {
        return Err(BevelError::NotClosed);
    }
    let topo = topology(s);
    let mut edges = BTreeMap::new();
    let mut max_turn = 0.0f64;
    let mut max_off = 0.0f64;
    for &key in &spec.edges {
        let (lo, hi) = key;
        let (Some(&ta), Some(&tb)) = (topo.dir.get(&(lo, hi)), topo.dir.get(&(hi, lo))) else {
            return Err(BevelError::NotClosed);
        };
        let (pa, pb) = (s.positions[lo as usize], s.positions[hi as usize]);
        if s.surfaces[ta] == s.surfaces[tb] {
            return Err(BevelError::NotAnEdge(pa, pb));
        }
        let tags = s.edge_tags.get(&key).cloned().unwrap_or_default();
        let carries = |t: usize| s.face_tags[t].iter().any(|x| tags.contains(x));
        let a_first = carries(ta) || !carries(tb);
        let th = turn(s, ta, tb);
        max_turn = max_turn.max(th);
        let (d1, d2) = match spec.offsets {
            Offsets::Equal(d) => (d, d),
            Offsets::Two(d, e) => (d, e),
            Offsets::Angle(d, angle) => {
                // The faces meet at π − turn; the chamfer leaves the cap at
                // `angle`: law of sines in the cross-section.
                let inner = trig::PI - th;
                (d, d * trig::sin(angle) / trig::sin(inner + angle))
            }
        };
        if !(d1.is_finite() && d2.is_finite() && d1 > 0.0 && d2 > 0.0) {
            return Err(BevelError::TooLarge(pa));
        }
        max_off = max_off.max(d1).max(d2);
        let off = if a_first {
            [d1, d2, d1, d2]
        } else {
            [d2, d1, d2, d1]
        };
        edges.insert(key, EdgeInfo { off });
    }
    let n = if spec.segments > 0 {
        spec.segments
    } else {
        match spec.kind {
            Kind::Chamfer => 1,
            Kind::Fillet => {
                let n = shape_2d::curve::segments(max_off, max_turn);
                n + n % 2
            }
        }
    };
    let mut b = Build {
        s,
        topo,
        edges,
        pos: s.positions.clone(),
        repl: BTreeMap::new(),
        chains: BTreeMap::new(),
        new_tris: Vec::new(),
        units: 0,
        joins: Vec::new(),
        smooth: Vec::new(),
        patches: Vec::new(),
        samples: BTreeMap::new(),
        n,
        exponent: spec.exponent,
        kind: spec.kind,
    };
    let verts: BTreeSet<u32> = b.edges.keys().flat_map(|&(x, y)| [x, y]).collect();
    for &v in &verts {
        b.vertex(v)?;
    }
    // Strips.
    let mut strip_of: BTreeMap<EdgeKey, usize> = BTreeMap::new();
    let keys: Vec<EdgeKey> = b.edges.keys().copied().collect();
    for key in keys {
        let (u, w) = key;
        let cu = b.chains[&(u, w)].clone();
        let mut cw = b.chains[&(w, u)].clone();
        cw.reverse();
        if cu.len() != cw.len() {
            return Err(BevelError::Unsupported(
                s.positions[u as usize],
                "uneven profiles",
            ));
        }
        let unit = b.unit();
        strip_of.insert(key, unit);
        for k in 0..cu.len() - 1 {
            b.new_tris.push(([cw[k], cu[k], cu[k + 1]], unit));
            b.new_tris.push(([cw[k], cu[k + 1], cw[k + 1]], unit));
        }
    }
    for &(e0, e1) in &b.smooth {
        b.joins.push((strip_of[&e0], strip_of[&e1]));
    }
    if b.kind == Kind::Fillet {
        for (unit, ks) in &b.patches {
            for k in ks {
                b.joins.push((*unit, strip_of[k]));
            }
        }
    }
    finish(b)
}

fn finish(b: Build<'_>) -> Result<Solid, BevelError> {
    let s = b.s;
    let kind = b.kind.name();
    let seq = |t: usize, x: u32| -> Vec<u32> { b.repl.get(&(t, x)).cloned().unwrap_or(vec![x]) };
    // Old faces, rebuilt.
    let mut tris: Vec<([u32; 3], Option<usize>, usize)> = Vec::new(); // (tri, unit, old)
    for (i, t) in s.triangles.iter().enumerate() {
        let changed = t.iter().any(|&x| b.repl.contains_key(&(i, x)));
        if !changed {
            tris.push((*t, None, i));
            continue;
        }
        let poly: Vec<u32> = t.iter().flat_map(|&x| seq(i, x)).collect();
        let nrm = tri_normal(s, i);
        let at = t
            .iter()
            .find(|&&x| b.repl.contains_key(&(i, x)))
            .map_or(DVec3::ZERO, |&x| s.positions[x as usize]);
        let pieces = clip(&poly, &b.pos, nrm).ok_or(BevelError::TooLarge(at))?;
        for p in pieces {
            tris.push((p, None, i));
        }
    }
    for &(t, unit) in &b.new_tris {
        tris.push((t, Some(unit), usize::MAX));
    }
    // Surfaces of new units.
    let mut parent: Vec<usize> = (0..b.units).collect();
    for &(x, y) in &b.joins {
        union(&mut parent, x, y);
    }
    let roots: Vec<usize> = (0..b.units).map(|u| find(&mut parent, u)).collect();
    let base = s.surfaces.iter().max().map_or(0, |m| m + 1);
    let normal = |t: [u32; 3]| {
        let [p, q, r] = t.map(|v| b.pos[v as usize]);
        (q - p).cross(r - p).normalize_or_zero()
    };
    // Edge → triangles.
    let mut by_edge: BTreeMap<EdgeKey, Vec<usize>> = BTreeMap::new();
    for (i, (t, _, _)) in tris.iter().enumerate() {
        for k in 0..3 {
            by_edge
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(i);
        }
    }
    let cos10 = trig::cos(trig::to_radians(10.0));
    let cos40 = trig::cos(trig::to_radians(40.0));
    let mut new_names: BTreeMap<EdgeKey, Vec<String>> = BTreeMap::new();
    for (&e, ts) in &by_edge {
        if ts.len() != 2 {
            continue;
        }
        let (x, y) = (&tris[ts[0]], &tris[ts[1]]);
        let (ux, uy) = (x.1.map(|u| roots[u]), y.1.map(|u| roots[u]));
        if ux.is_none() && uy.is_none() || ux == uy {
            continue;
        }
        let cos = normal(x.0).dot(normal(y.0));
        let mut names = Vec::new();
        match (ux, uy) {
            (Some(_), Some(_)) => {
                let limit = if b.kind == Kind::Chamfer {
                    cos40
                } else {
                    cos10
                };
                if cos < limit {
                    names.push(kind.to_string());
                }
            }
            _ => {
                if cos < cos10 {
                    names.push(kind.to_string());
                    let old = if ux.is_none() { x.2 } else { y.2 };
                    for &tag in &s.face_tags[old] {
                        let name = s.names.name(tag);
                        if is_cap(name) {
                            names.push(name.to_string());
                        }
                    }
                }
            }
        }
        if !names.is_empty() {
            new_names.insert(e, names);
        }
    }
    // Old edge names, following their ends.
    for (&(a, c), tags) in &s.edge_tags {
        if b.edges.contains_key(&(a, c)) {
            continue;
        }
        let Some(&t) = b.topo.dir.get(&(a, c)) else {
            continue;
        };
        let ra = *seq(t, a).last().unwrap_or(&a);
        let rc = *seq(t, c).first().unwrap_or(&c);
        let names = new_names.entry(edge_key(ra, rc)).or_default();
        names.extend(tags.iter().map(|&x| s.names.name(x).to_string()));
    }
    // Assemble with only the vertices in use.
    let mut used: Vec<u32> = tris.iter().flat_map(|t| t.0).collect();
    used.sort_unstable();
    used.dedup();
    let mut remap = vec![u32::MAX; b.pos.len()];
    let mut out = Solid::default();
    for &v in &used {
        remap[v as usize] = out.push_vertex(b.pos[v as usize]);
    }
    for (t, unit, old) in &tris {
        let t = t.map(|v| remap[v as usize]);
        match unit {
            None => {
                let names: Vec<&str> = s.face_tags[*old].iter().map(|&x| s.names.name(x)).collect();
                out.push_triangle(t, s.surfaces[*old], &names);
            }
            Some(u) => {
                out.push_triangle(t, base + roots[*u] as u32, &[kind]);
            }
        }
    }
    for ((x, y), names) in new_names {
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        out.tag_edge(remap[x as usize], remap[y as usize], &refs);
    }
    out.canonicalize()
        .map_err(|_| BevelError::NotFinite("result"))?;
    let m = out.measure();
    if !m.closed || m.degenerate_triangles > 0 {
        let at = out.positions.first().copied().unwrap_or(DVec3::ZERO);
        return Err(BevelError::TooLarge(at));
    }
    Ok(out)
}

//! Decimation by edge collapse with quadric error metrics (Garland and
//! Heckbert 1997), and LOD chains. Written here, in f64, deterministic
//! (`docs/research.md` §9 says why not meshoptimizer-rs).
//!
//! - Each vertex carries the area-weighted quadric of its triangles'
//!   planes. Open borders and features (edges between surfaces, between
//!   different face names, named edges) add a heavy quadric of the plane
//!   through the edge square to its face, so they stay in place.
//! - Vertices are interior, on one border or feature line, or corners
//!   (where lines meet or end, on a non-manifold edge, or where two fans
//!   of triangles meet at a point). A line vertex
//!   only slides along its line into the next vertex on it; a corner never
//!   goes. `lock_border` makes every open-border vertex a corner.
//! - A collapse goes to the kept vertex's position, or for two interior
//!   vertices to whichever of the two ends and the midpoint costs least.
//! - A collapse is refused when it would break the link condition (the
//!   two ends share a neighbour that is not across one of the edge's
//!   triangles), so a closed manifold stays closed and manifold; when it
//!   would turn a triangle over (its normal turns by more than 90°) or
//!   leave it with no area; and beyond `max_error`.
//! - Collapses are taken cheapest first; ties go to the smaller edge key,
//!   then the smaller vertex. The order depends only on the input.
//!
//! A skinned solid is refused ([`DecimateError::Skinned`]).
//!
//! Names: every triangle left is an input triangle with corners moved, so
//! it keeps its surface and face names. Edge names follow their vertices
//! and survive on edges that still exist.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};
use std::fmt;

use shape_core::DVec3;
use shape_mesh::{EdgeKey, Solid, edge_key};

use crate::repair::drop_unused;

/// How far to decimate. The zero value asks for nothing and is an error;
/// set `ratio` or `target`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Decimate {
    /// Keep this fraction of the triangles (0 < ratio ≤ 1); 0 is unset.
    pub ratio: f64,
    /// Keep at most this many triangles; 0 is unset. With `ratio` too,
    /// the smaller count wins.
    pub target: usize,
    /// Refuse any collapse that would move the surface more than this, in
    /// mm (the root-mean-square distance its quadric measures); 0 is no
    /// limit.
    pub max_error: f64,
    /// Never move vertices on open borders (holes, the edge of a sheet).
    pub lock_border: bool,
}

/// What a decimation did.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DecimateReport {
    /// Triangles before.
    pub triangles_before: usize,
    /// Triangles after.
    pub triangles: usize,
    /// The count asked for.
    pub target: usize,
    /// The largest error of a collapse taken, mm (root-mean-square
    /// distance to the planes the collapsed vertices stood for).
    pub error: f64,
}

/// Why a decimation could not run.
#[derive(Clone, Debug, PartialEq)]
pub enum DecimateError {
    /// Neither a ratio nor a target was given, or a value is out of range.
    BadDesc(String),
    /// A position is NaN or infinite.
    NotFinite,
    /// The solid is skinned: decimation would break its skin (weights are
    /// interpolated from phase 5b).
    Skinned,
}

impl fmt::Display for DecimateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecimateError::BadDesc(w) => write!(f, "decimate: {w}"),
            DecimateError::NotFinite => f.write_str("decimate: a position is not finite"),
            DecimateError::Skinned => f.write_str(
                "decimate: the solid is skinned; decimating would break its skin, so it is refused",
            ),
        }
    }
}

impl std::error::Error for DecimateError {}

fn target_of(d: &Decimate, n: usize) -> Result<usize, DecimateError> {
    if !(d.ratio >= 0.0 && d.ratio <= 1.0) {
        return Err(DecimateError::BadDesc("ratio must be in 0..=1".into()));
    }
    if !(d.max_error >= 0.0 && d.max_error.is_finite()) {
        return Err(DecimateError::BadDesc(
            "max_error must be finite and >= 0".into(),
        ));
    }
    let by_ratio = (d.ratio > 0.0).then(|| (d.ratio * n as f64).ceil() as usize);
    let by_target = (d.target > 0).then_some(d.target);
    match (by_ratio, by_target) {
        (None, None) => Err(DecimateError::BadDesc(
            "give a ratio or a triangle target".into(),
        )),
        (Some(a), Some(b)) => Ok(a.min(b)),
        (Some(a), None) | (None, Some(a)) => Ok(a),
    }
}

/// A symmetric 4×4 quadric, `[a², ab, ac, ad, b², bc, bd, c², cd, d²]`
/// summed over planes, and the weight (area) behind it.
#[derive(Clone, Copy, Debug, Default)]
struct Quadric {
    q: [f64; 10],
    w: f64,
}

impl Quadric {
    fn plane(n: DVec3, p: DVec3, w: f64) -> Quadric {
        let d = -n.dot(p);
        let (a, b, c) = (n.x, n.y, n.z);
        Quadric {
            q: [
                a * a * w,
                a * b * w,
                a * c * w,
                a * d * w,
                b * b * w,
                b * c * w,
                b * d * w,
                c * c * w,
                c * d * w,
                d * d * w,
            ],
            w,
        }
    }

    fn add(&mut self, o: &Quadric) {
        for k in 0..10 {
            self.q[k] += o.q[k];
        }
        self.w += o.w;
    }

    /// Weighted sum of squared distances of `p` to the planes.
    fn eval(&self, p: DVec3) -> f64 {
        let q = &self.q;
        let (x, y, z) = (p.x, p.y, p.z);
        let v = q[0] * x * x
            + 2.0 * q[1] * x * y
            + 2.0 * q[2] * x * z
            + 2.0 * q[3] * x
            + q[4] * y * y
            + 2.0 * q[5] * y * z
            + 2.0 * q[6] * y
            + q[7] * z * z
            + 2.0 * q[8] * z
            + q[9];
        v.max(0.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Interior,
    Line,
    Corner,
}

/// A queued collapse: cheapest first, then by edge, then by direction.
#[derive(Clone, Copy, Debug)]
struct Entry {
    cost: f64,
    edge: EdgeKey,
    /// The vertex removed.
    from: u32,
    to: u32,
    pos: DVec3,
    stamp: (u32, u32),
}

impl PartialEq for Entry {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Entry {}
impl PartialOrd for Entry {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Entry {
    // Reversed, so the heap pops the cheapest.
    fn cmp(&self, o: &Self) -> Ordering {
        o.cost
            .total_cmp(&self.cost)
            .then(o.edge.cmp(&self.edge))
            .then(o.from.cmp(&self.from))
    }
}

struct Mesh {
    pos: Vec<DVec3>,
    tris: Vec<[u32; 3]>,
    alive: Vec<bool>,
    live: usize,
    /// Per vertex, the live triangles around it.
    around: Vec<Vec<u32>>,
    quadric: Vec<Quadric>,
    kind: Vec<Kind>,
    /// Per vertex, the other ends of its border and feature edges.
    lines: Vec<Vec<u32>>,
    stamp: Vec<u32>,
    /// The vertex each removed one went into.
    into: Vec<u32>,
    /// Only edges shorter than this collapse (0: any).
    max_len: f64,
}

impl Mesh {
    fn neighbours(&self, v: u32) -> Vec<u32> {
        let mut n: Vec<u32> = self.around[v as usize]
            .iter()
            .flat_map(|&t| self.tris[t as usize])
            .filter(|&x| x != v)
            .collect();
        n.sort_unstable();
        n.dedup();
        n
    }

    fn normal(&self, t: [u32; 3], moved: u32, to: DVec3) -> DVec3 {
        let p = t.map(|v| if v == moved { to } else { self.pos[v as usize] });
        (p[1] - p[0]).cross(p[2] - p[0])
    }

    /// The collapse `from → to` at `p`, if it keeps the mesh sound.
    fn valid(&self, from: u32, to: u32, p: DVec3) -> bool {
        // Link condition: the shared neighbours are exactly the third
        // corners of the edge's triangles.
        let b = self.neighbours(to);
        let shared: Vec<u32> = self
            .neighbours(from)
            .into_iter()
            .filter(|x| b.binary_search(x).is_ok())
            .collect();
        let mut opposite: Vec<u32> = self.around[from as usize]
            .iter()
            .map(|&t| self.tris[t as usize])
            .filter(|t| t.contains(&to))
            .flat_map(|t| t.into_iter().filter(|&x| x != from && x != to))
            .collect();
        opposite.sort_unstable();
        opposite.dedup();
        if shared != opposite || opposite.is_empty() {
            return false;
        }
        // Nothing turns over or loses its area.
        for v in [from, to] {
            for &t in &self.around[v as usize] {
                let tri = self.tris[t as usize];
                if tri.contains(&from) && tri.contains(&to) {
                    continue;
                }
                let before = self.normal(tri, u32::MAX, DVec3::ZERO);
                let after = self.normal(tri, v, p);
                if after == DVec3::ZERO || after.dot(before) <= 0.0 {
                    return false;
                }
            }
        }
        true
    }

    /// The best allowed collapse of edge `a b`, if any.
    fn plan(&self, a: u32, b: u32) -> Option<Entry> {
        if self.max_len > 0.0
            && (self.pos[a as usize] - self.pos[b as usize]).length() >= self.max_len
        {
            return None;
        }
        let edge = edge_key(a, b);
        let on_line = self.lines[a as usize].contains(&b);
        let mut best: Option<Entry> = None;
        for (from, to) in [(a, b), (b, a)] {
            let (kf, kt) = (self.kind[from as usize], self.kind[to as usize]);
            let allowed = match kf {
                Kind::Interior => true,
                Kind::Line => on_line && kt != Kind::Interior,
                Kind::Corner => false,
            };
            if !allowed {
                continue;
            }
            let mut q = self.quadric[from as usize];
            q.add(&self.quadric[to as usize]);
            let (pf, pt) = (self.pos[from as usize], self.pos[to as usize]);
            let mut options = vec![pt];
            if kf == Kind::Interior && kt == Kind::Interior {
                options.push((pf + pt) * 0.5);
            }
            for p in options {
                let e = Entry {
                    cost: q.eval(p),
                    edge,
                    from,
                    to,
                    pos: p,
                    stamp: (self.stamp[from as usize], self.stamp[to as usize]),
                };
                if best.is_none_or(|b| e.cmp(&b) == Ordering::Greater) {
                    best = Some(e);
                }
            }
        }
        best
    }

    fn collapse(&mut self, e: &Entry) {
        let (from, to) = (e.from, e.to);
        let around = std::mem::take(&mut self.around[from as usize]);
        for &t in &around {
            let tri = self.tris[t as usize];
            if tri.contains(&to) {
                self.alive[t as usize] = false;
                self.live -= 1;
                for v in tri {
                    if v != from {
                        self.around[v as usize].retain(|&x| x != t);
                    }
                }
            } else {
                for v in &mut self.tris[t as usize] {
                    if *v == from {
                        *v = to;
                    }
                }
                self.around[to as usize].push(t);
            }
        }
        self.around[to as usize].sort_unstable();
        self.pos[to as usize] = e.pos;
        let q = self.quadric[from as usize];
        self.quadric[to as usize].add(&q);
        // Lines through `from` now run through `to`.
        for x in std::mem::take(&mut self.lines[from as usize]) {
            self.lines[x as usize].retain(|&y| y != from);
            if x != to {
                if !self.lines[x as usize].contains(&to) {
                    self.lines[x as usize].push(to);
                }
                if !self.lines[to as usize].contains(&x) {
                    self.lines[to as usize].push(x);
                }
            }
        }
        self.into[from as usize] = to;
        self.stamp[from as usize] += 1;
        self.stamp[to as usize] += 1;
    }

    fn root(&self, mut v: u32) -> u32 {
        while self.into[v as usize] != v {
            v = self.into[v as usize];
        }
        v
    }
}

/// Decimates `s` towards the triangle count `d` asks for. The target may
/// not be reached when every remaining collapse is refused (features,
/// the link condition, the error limit); the report says how many
/// triangles are left. A solid already at or under the target comes back
/// unchanged.
pub fn decimate(s: &Solid, d: &Decimate) -> Result<(Solid, DecimateReport), DecimateError> {
    let n = s.triangles.len();
    let target = target_of(d, n)?;
    decimate_to(s, target, d)
}

/// Collapses every edge shorter than `length` mm that can go without
/// breaking the mesh (the same rules as [`decimate`]: the link condition,
/// no turned-over triangles, features slide only along themselves). For
/// the slivers a boolean leaves, whose vertices f32 export would merge
/// into non-manifold edges. Returns the solid and how many vertices went.
pub fn collapse_short_edges(s: &Solid, length: f64) -> Result<(Solid, usize), DecimateError> {
    if !(length >= 0.0 && length.is_finite()) {
        return Err(DecimateError::BadDesc(
            "an edge length must be finite and >= 0".into(),
        ));
    }
    if s.skin.is_some() {
        return Err(DecimateError::Skinned);
    }
    if length == 0.0 || s.triangles.is_empty() {
        return Ok((s.clone(), 0));
    }
    let before = drop_unused(s).positions.len();
    let (out, _) = collapse(s, 0, &Decimate::default(), length)?;
    let gone = before - out.positions.len();
    Ok((out, gone))
}

fn decimate_to(
    s: &Solid,
    target: usize,
    d: &Decimate,
) -> Result<(Solid, DecimateReport), DecimateError> {
    collapse(s, target, d, 0.0)
}

fn collapse(
    s: &Solid,
    target: usize,
    d: &Decimate,
    max_len: f64,
) -> Result<(Solid, DecimateReport), DecimateError> {
    if s.skin.is_some() {
        return Err(DecimateError::Skinned);
    }
    if s.positions.iter().any(|p| !p.is_finite()) {
        return Err(DecimateError::NotFinite);
    }
    let n = s.triangles.len();
    let mut report = DecimateReport {
        triangles_before: n,
        triangles: n,
        target,
        error: 0.0,
    };
    if n <= target && max_len == 0.0 {
        return Ok((s.clone(), report));
    }
    let nv = s.positions.len();
    let mut m = Mesh {
        pos: s.positions.clone(),
        tris: s.triangles.clone(),
        alive: vec![true; n],
        live: n,
        around: vec![Vec::new(); nv],
        quadric: vec![Quadric::default(); nv],
        kind: vec![Kind::Interior; nv],
        lines: vec![Vec::new(); nv],
        stamp: vec![0; nv],
        into: (0..nv as u32).collect(),
        max_len,
    };
    let mut users: BTreeMap<EdgeKey, Vec<u32>> = BTreeMap::new();
    let mut total_area = 0.0;
    for (i, t) in s.triangles.iter().enumerate() {
        let [a, b, c] = t.map(|v| s.positions[v as usize]);
        let cr = (b - a).cross(c - a);
        let area = cr.length() / 2.0;
        total_area += area;
        let nrm = if area > 0.0 {
            cr / (2.0 * area)
        } else {
            DVec3::ZERO
        };
        let q = Quadric::plane(nrm, a, area);
        for &v in t {
            m.around[v as usize].push(i as u32);
            m.quadric[v as usize].add(&q);
        }
        for k in 0..3 {
            users
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(i as u32);
        }
    }
    let weight = 1000.0 * (total_area / n as f64).max(f64::MIN_POSITIVE);
    // Borders and features: lines, with heavy quadrics.
    let mut pinned = vec![false; nv];
    let mut border = vec![false; nv];
    for (&(a, b), ts) in &users {
        let named = s.edge_tags.contains_key(&(a, b));
        let feature = ts.len() == 2 && {
            let (x, y) = (ts[0] as usize, ts[1] as usize);
            s.surfaces[x] != s.surfaces[y] || s.face_tags[x] != s.face_tags[y]
        };
        let open = ts.len() == 1;
        if ts.len() > 2 {
            pinned[a as usize] = true;
            pinned[b as usize] = true;
        }
        if !(open || feature || named) {
            continue;
        }
        m.lines[a as usize].push(b);
        m.lines[b as usize].push(a);
        if open {
            border[a as usize] = true;
            border[b as usize] = true;
        }
        let (pa, pb) = (s.positions[a as usize], s.positions[b as usize]);
        for &t in ts {
            let [x, y, z] = s.triangles[t as usize].map(|v| s.positions[v as usize]);
            let side = (pb - pa).cross((y - x).cross(z - x));
            let len = side.length();
            if len > 0.0 {
                let q = Quadric::plane(side / len, pa, weight);
                m.quadric[a as usize].add(&q);
                m.quadric[b as usize].add(&q);
            }
        }
    }
    // A vertex whose triangles make more than one fan (two sheets meeting
    // at a point) is pinned: the link condition assumes one.
    for v in 0..nv {
        let tris = &m.around[v];
        if tris.len() < 2 {
            continue;
        }
        // Triangles around v joined across the edges from v they share.
        let mut parent: Vec<usize> = (0..tris.len()).collect();
        fn root(p: &mut [usize], mut i: usize) -> usize {
            while p[i] != i {
                p[i] = p[p[i]];
                i = p[i];
            }
            i
        }
        let mut by_other: Vec<(u32, usize)> = tris
            .iter()
            .enumerate()
            .flat_map(|(k, &t)| {
                s.triangles[t as usize]
                    .into_iter()
                    .filter(move |&o| o as usize != v)
                    .map(move |o| (o, k))
            })
            .collect();
        by_other.sort_unstable();
        for w in by_other.windows(2) {
            if w[0].0 == w[1].0 {
                let (a, b) = (root(&mut parent, w[0].1), root(&mut parent, w[1].1));
                if a != b {
                    parent[a.max(b)] = a.min(b);
                }
            }
        }
        if (0..tris.len()).any(|i| root(&mut parent, i) != 0) {
            pinned[v] = true;
        }
    }
    for v in 0..nv {
        m.kind[v] = match m.lines[v].len() {
            0 => Kind::Interior,
            2 => Kind::Line,
            _ => Kind::Corner,
        };
        if pinned[v] || (d.lock_border && border[v]) {
            m.kind[v] = Kind::Corner;
        }
    }
    let mut heap: BinaryHeap<Entry> = BinaryHeap::new();
    for &(a, b) in users.keys() {
        if let Some(e) = m.plan(a, b) {
            heap.push(e);
        }
    }
    while m.live > target {
        let Some(e) = heap.pop() else { break };
        if (m.stamp[e.from as usize], m.stamp[e.to as usize]) != e.stamp {
            continue;
        }
        let mut q = m.quadric[e.from as usize];
        q.add(&m.quadric[e.to as usize]);
        let err = if q.w > 0.0 {
            (e.cost / q.w).sqrt()
        } else {
            0.0
        };
        if d.max_error > 0.0 && err > d.max_error {
            continue;
        }
        if !m.valid(e.from, e.to, e.pos) {
            continue;
        }
        m.collapse(&e);
        report.error = report.error.max(err);
        let to = e.to;
        for x in m.neighbours(to) {
            if let Some(next) = m.plan(to, x) {
                heap.push(next);
            }
        }
    }
    let mut out = Solid {
        positions: m.pos.clone(),
        names: s.names.clone(),
        ..Default::default()
    };
    for (i, t) in m.tris.iter().enumerate() {
        if m.alive[i] {
            out.triangles.push(*t);
            out.surfaces.push(s.surfaces[i]);
            out.face_tags.push(s.face_tags[i].clone());
        }
    }
    for (&(a, b), tags) in &s.edge_tags {
        let (a, b) = (m.root(a), m.root(b));
        if a != b {
            let e = out.edge_tags.entry(edge_key(a, b)).or_default();
            e.extend(tags);
            e.sort_unstable();
            e.dedup();
        }
    }
    let mut out = drop_unused(&out);
    out.canonicalize().map_err(|_| DecimateError::NotFinite)?;
    report.triangles = out.triangles.len();
    Ok((out, report))
}

/// A chain of levels of detail: level `k` decimates level `k − 1` (level
/// 0 decimates `s`) towards `levels[k]`. A ratio is taken of `s`'s
/// triangle count, so `[1, 0.5, 0.25]` gives the full mesh, a half and a
/// quarter.
pub fn lods(s: &Solid, levels: &[Decimate]) -> Result<Vec<(Solid, DecimateReport)>, DecimateError> {
    let n = s.triangles.len();
    let mut out: Vec<(Solid, DecimateReport)> = Vec::with_capacity(levels.len());
    for d in levels {
        let target = target_of(d, n)?;
        let prev = out.last().map_or(s, |l| &l.0);
        let (next, mut r) = decimate_to(prev, target, d)?;
        if let Some(l) = out.last() {
            r.error = r.error.max(l.1.error);
        }
        out.push((next, r));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shape_gen::solids::{BoxDesc, cuboid, sphere};

    #[test]
    fn halves_a_sphere_closed_and_keeps_a_box_square() {
        let s = sphere(20.0).unwrap();
        let n = s.triangles.len();
        let (half, r) = decimate(
            &s,
            &Decimate {
                ratio: 0.5,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(half.triangles.len() <= n / 2 + 1, "{r:?}");
        assert!(half.measure().closed);
        assert!(r.error > 0.0 && r.error < 2.0, "{r:?}");
        let chain = lods(
            &s,
            &[0.5, 0.25, 0.05].map(|ratio| Decimate {
                ratio,
                ..Default::default()
            }),
        )
        .unwrap();
        let counts: Vec<usize> = chain.iter().map(|l| l.0.triangles.len()).collect();
        assert!(counts[0] > counts[1] && counts[1] > counts[2], "{counts:?}");
        assert!(chain.iter().all(|l| l.0.measure().closed));
        assert!(decimate(&s, &Decimate::default()).is_err());
        // A box's faces are separate surfaces: their edges are features
        // and its corners stay, so it keeps its volume.
        let b = cuboid(&BoxDesc {
            width: 10.0,
            depth: 10.0,
            height: 10.0,
        })
        .unwrap();
        let (low, _) = decimate(
            &b,
            &Decimate {
                target: 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert!((low.measure().volume - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn short_edges_collapse_and_the_box_stays_closed() {
        let b = cuboid(&BoxDesc {
            width: 10.0,
            depth: 10.0,
            height: 10.0,
        })
        .unwrap();
        let (s, gone) = collapse_short_edges(&sliver_box(&b), 1e-3).unwrap();
        assert_eq!(gone, 1);
        let m = s.measure();
        assert!(m.closed && m.triangles == b.triangles.len(), "{m:?}");
    }

    /// The box with its top's first triangle split at a point 1e-5 mm
    /// from one of its corners: three slivers, one short edge.
    fn sliver_box(b: &Solid) -> Solid {
        let mut s = b.clone();
        let top = s.faces(&["top"]).unwrap()[0] as usize;
        let t = s.triangles[top];
        let p = s.positions[t[0] as usize] * (1.0 - 1e-6)
            + (s.positions[t[1] as usize] + s.positions[t[2] as usize]) * 5e-7;
        let v = s.push_vertex(p);
        let (surface, tags) = (s.surfaces[top], s.face_tags[top].clone());
        s.triangles[top] = [t[0], t[1], v];
        for tri in [[t[1], t[2], v], [t[2], t[0], v]] {
            s.triangles.push(tri);
            s.surfaces.push(surface);
            s.face_tags.push(tags.clone());
        }
        s.canonicalize().unwrap();
        s
    }
}

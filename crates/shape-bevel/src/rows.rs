//! Bands wider than the rows of a smooth wall (`docs/research.md` §7.9).
//!
//! A loft, a sweep or a revolve is a wall of rows. Where a bevelled run
//! goes round such a wall (a loft's top rim) and the offset is deeper
//! than the rows next to it, the band (the wall between the run and its
//! offset line) holds whole rows of the wall's vertices. The classic
//! layout keeps every old triangle and moves its corner, so it cannot
//! fit; the cut bands of `wide.rs` end each band on an unbevelled edge,
//! which a closed rim does not have; and the fold layout of §7.8 puts
//! its points beyond the folds' far ends.
//!
//! Tried only after every other attempt failed, so no result they give
//! changes. On each wall (a surface whose triangles are not one plane)
//! along the selected edges:
//!
//! 1. **Gap points.** At each end `v` of a bevelled edge on the wall,
//!    whose gap on the wall is bounded by two bevelled edges with no
//!    real edge between, the gap point is found on the wall itself: the
//!    plane through `v` square to the run (the chord from the run's
//!    previous vertex to its next) is walked down the wall's triangles
//!    for `d / sin(α/2)`, `d` the offset and `α` the wall's angle at `v`
//!    summed over its triangles there (π where the run is straight on the
//!    wall). The point lies on a segment of one triangle, so in its
//!    plane, and is added to the wall there.
//! 2. **The cut.** Between the gap points of each bevelled edge, the
//!    plane through both and square to the edge's wall triangle is walked
//!    across the wall from one to the other. `orient3d` decides each
//!    vertex's side; every edge it crosses is split where it crosses, so
//!    the wall's geometry is unchanged.
//! 3. **The band** is every wall triangle reached from the bevelled
//!    edges without crossing the cut: not rebuilt. The strip gets a
//!    profile at each cut point, at its share of the cut's length, whose
//!    other end lies on the straight offset line of the other face.
//!
//! What is left of the wall keeps its triangles' planes, cut where the
//! line crosses them. A cut that leaves the wall, meets a bevelled vertex
//! or another cut, or a band that reaches the wall's other edges, does
//! not fit: `TooLarge` at the gap's vertex.

use std::collections::{BTreeMap, BTreeSet};

use core::cmp::Ordering;

use shape_core::{DVec3, predicates};
use shape_mesh::{EdgeKey, Solid, edge_key};

use super::wide::{Wide, split};
use super::{Fail, PLANAR_DOT, Spec, TILT_EXACT, Topology, bevel_once, edge_offsets, fan};
use super::{topology, tri_normal};
use crate::BevelError;

/// Steps a walk may take before it is given up.
const LIMIT: usize = 100_000;

/// A point on a walk: a vertex, or a point inside an edge.
#[derive(Clone, Copy, Debug)]
enum At {
    V(u32),
    E(u32, u32, DVec3),
}

impl At {
    fn same(&self, o: &At) -> bool {
        match (self, o) {
            (At::V(a), At::V(b)) => a == b,
            (At::E(a, b, _), At::E(c, d, _)) => edge_key(*a, *b) == edge_key(*c, *d),
            _ => false,
        }
    }
}

fn pos(s: &Solid, at: &At) -> DVec3 {
    match *at {
        At::V(v) => s.positions[v as usize],
        At::E(_, _, p) => p,
    }
}

/// A plane through three points; sides by `orient3d`.
struct Plane {
    p: [DVec3; 3],
    n: DVec3,
}

impl Plane {
    fn new(a: DVec3, b: DVec3, c: DVec3) -> Self {
        Plane {
            p: [a, b, c],
            n: (b - a).cross(c - a),
        }
    }

    fn side(&self, x: DVec3) -> Ordering {
        predicates::orient3d(self.p[0], self.p[1], self.p[2], x)
    }

    /// Where the segment `a – b`, whose ends lie strictly on either side,
    /// crosses the plane.
    fn hit(&self, a: DVec3, b: DVec3) -> DVec3 {
        let (ga, gb) = ((a - self.p[0]).dot(self.n), (b - self.p[0]).dot(self.n));
        let t = ga / (ga - gb);
        let t = if t.is_finite() {
            t.clamp(1e-12, 1.0 - 1e-12)
        } else {
            0.5
        };
        a + (b - a) * t
    }
}

/// Where the plane meets triangle `t`: its corners on the plane and the
/// points inside its edges.
fn crossings(s: &Solid, t: usize, plane: &Plane) -> Vec<At> {
    let tri = s.triangles[t];
    let p = |v: u32| s.positions[v as usize];
    let sg = tri.map(|v| plane.side(p(v)));
    let mut out = Vec::new();
    for k in 0..3 {
        let (a, b) = (tri[k], tri[(k + 1) % 3]);
        let (sa, sb) = (sg[k], sg[(k + 1) % 3]);
        if sa == Ordering::Equal {
            out.push(At::V(a));
        } else if sb != Ordering::Equal && sa != sb {
            out.push(At::E(a, b, plane.hit(p(a), p(b))));
        }
    }
    out
}

/// One step of a walk on the wall `w`: from `cur`, out of the triangle
/// the walk did not come from, the way most along `dir`. `None` where the
/// walk would leave the wall.
fn step(
    s: &Solid,
    topo: &Topology,
    w: u32,
    plane: &Plane,
    cur: &At,
    prev: Option<usize>,
    dir: DVec3,
) -> Option<(At, usize)> {
    let here = pos(s, cur);
    let cands: Vec<usize> = match *cur {
        At::V(c) => topo.at[c as usize]
            .iter()
            .copied()
            .filter(|&t| Some(t) != prev && s.surfaces[t] == w)
            .collect(),
        At::E(a, b, _) => {
            let c: Vec<usize> = [topo.dir.get(&(a, b)), topo.dir.get(&(b, a))]
                .into_iter()
                .flatten()
                .copied()
                .filter(|&t| Some(t) != prev)
                .collect();
            if c.iter().any(|&t| s.surfaces[t] != w) {
                return None;
            }
            c
        }
    };
    let d = dir.normalize_or_zero();
    let mut best: Option<(f64, At, usize)> = None;
    for t in cands {
        for x in crossings(s, t, plane) {
            if x.same(cur) {
                continue;
            }
            let go = (pos(s, &x) - here).normalize_or_zero().dot(d);
            if go.partial_cmp(&0.0) != Some(Ordering::Greater) {
                continue;
            }
            if best.as_ref().is_none_or(|(g, _, _)| go > *g) {
                best = Some((go, x, t));
            }
        }
    }
    best.map(|(_, x, t)| (x, t))
}

/// A new vertex at `p` inside triangle `t`, which becomes three.
fn insert(s: &mut Solid, t: usize, p: DVec3) -> u32 {
    let id = s.push_vertex(p);
    let [a, b, c] = s.triangles[t];
    s.triangles[t] = [a, b, id];
    for tri in [[b, c, id], [c, a, id]] {
        s.triangles.push(tri);
        s.surfaces.push(s.surfaces[t]);
        s.face_tags.push(s.face_tags[t].clone());
    }
    id
}

/// The vertex a walk point becomes: itself, or the edge split there.
fn place(s: &mut Solid, at: &At) -> u32 {
    match *at {
        At::V(v) => v,
        At::E(a, b, p) => split(s, a, b, p),
    }
}

/// Whether the triangles of surface `w` are not all one plane.
fn curved(s: &Solid, w: u32) -> bool {
    let mut first = None;
    for (i, &sf) in s.surfaces.iter().enumerate() {
        if sf != w {
            continue;
        }
        let n = tri_normal(s, i);
        match first {
            None => first = Some(n),
            Some(f) if f.dot(n) <= PLANAR_DOT => return true,
            _ => {}
        }
    }
    false
}

/// A gap on the wall at a bevelled vertex: the vertex, the wall, the
/// spokes of the bevelled edges on either side, the triangles between,
/// and the offset there.
struct Gap {
    v: u32,
    w: u32,
    lo: u32,
    hi: u32,
    tris: Vec<usize>,
    d: f64,
}

/// The wall gaps at `v`: each gap bounded by two bevelled edges with no
/// real edge inside, whose triangles all lie on a curved surface. `None`
/// when a gap on a curved surface has a real edge inside or is the only
/// one (a lone end).
fn gaps(
    s: &Solid,
    topo: &Topology,
    spec_edges: &BTreeMap<EdgeKey, super::EdgeInfo>,
    curved: &BTreeSet<u32>,
    v: u32,
) -> Option<Vec<Gap>> {
    let mut f = fan(s, topo, v).ok()?;
    let k = f.spokes.len();
    let bevelled = |sp: u32| spec_edges.contains_key(&edge_key(v, sp));
    let r0 = (0..k).find(|&i| bevelled(f.spokes[i]))?;
    f.tris.rotate_left(r0);
    f.spokes.rotate_left(r0);
    let bidx: Vec<usize> = (0..k).filter(|&i| bevelled(f.spokes[i])).collect();
    let m = bidx.len();
    let mut out = Vec::new();
    for g in 0..m {
        let (lo, hi) = (bidx[g], if g + 1 < m { bidx[g + 1] } else { k });
        let tris = f.tris[lo..hi].to_vec();
        let w = s.surfaces[tris[0]];
        if !curved.contains(&w) {
            continue;
        }
        if m < 2 || tris.iter().any(|&t| s.surfaces[t] != w) {
            return None;
        }
        let (el, er) = (edge_key(v, f.spokes[lo]), edge_key(v, f.spokes[hi % k]));
        let d = (spec_edges[&el].after(el, v) + spec_edges[&er].before(er, v)) / 2.0;
        out.push(Gap {
            v,
            w,
            lo: f.spokes[lo],
            hi: f.spokes[hi % k],
            tris,
            d,
        });
    }
    Some(out)
}

/// The gap's point on the wall (step 1), added to the wall.
fn gap_point(s: &mut Solid, g: &Gap, ends: &BTreeSet<u32>) -> Option<u32> {
    let topo = topology(s);
    let p = |v: u32| s.positions[v as usize];
    let pv = p(g.v);
    // The wall's angle at v, and the way into the wall.
    let mut angle = 0.0;
    let mut normal = DVec3::ZERO;
    let mut into = DVec3::ZERO;
    for &t in &g.tris {
        let tri = s.triangles[t];
        let r = (0..3).find(|&k| tri[k] == g.v)?;
        let (a, b) = (p(tri[(r + 1) % 3]) - pv, p(tri[(r + 2) % 3]) - pv);
        angle += shape_core::trig::acos(
            a.normalize_or_zero()
                .dot(b.normalize_or_zero())
                .clamp(-1.0, 1.0),
        );
        normal += tri_normal(s, t);
        into += (a + b).normalize_or_zero();
    }
    let half = shape_core::trig::sin(angle / 2.0);
    if half.partial_cmp(&0.1) != Some(Ordering::Greater) {
        return None;
    }
    let length = g.d / half;
    let along = (p(g.hi) - p(g.lo)).normalize_or_zero();
    let up = (normal - along * normal.dot(along)).normalize_or_zero();
    let plane = Plane::new(pv, pv + up, pv + along.cross(up));
    let mut cur = At::V(g.v);
    let mut prev: Option<usize> = None;
    let mut dir = into;
    let mut walked = 0.0;
    for _ in 0..LIMIT {
        let (x, t) = step(s, &topo, g.w, &plane, &cur, prev, dir)?;
        let (a, b) = (pos(s, &cur), pos(s, &x));
        let seg = (b - a).length();
        if walked + seg >= length {
            let frac = (length - walked) / seg;
            let id = if frac >= 1.0 - 1e-9 {
                place(s, &x)
            } else if frac <= 1e-9 {
                place(s, &cur)
            } else if let (At::V(i), At::V(j)) = (cur, x) {
                // The walk runs along an edge.
                split(s, i, j, a + (b - a) * frac)
            } else {
                insert(s, t, a + (b - a) * frac)
            };
            return (!ends.contains(&id)).then_some(id);
        }
        if let At::V(c) = x
            && ends.contains(&c)
        {
            return None;
        }
        walked += seg;
        dir = b - a;
        prev = Some(t);
        cur = x;
    }
    None
}

/// The cut from `from` to `to` across the wall `w` (step 2): its vertices
/// in order, the crossed edges split. `None` where it leaves the wall or
/// meets a bevelled vertex or an earlier cut.
fn cut(
    s: &mut Solid,
    w: u32,
    from: u32,
    to: u32,
    normal: DVec3,
    stop: &BTreeSet<u32>,
    cut_edges: &BTreeSet<EdgeKey>,
) -> Option<Vec<u32>> {
    let topo = topology(s);
    let (a, b) = (s.positions[from as usize], s.positions[to as usize]);
    let plane = Plane::new(a, b, a + normal);
    if plane.n.length_squared() == 0.0 {
        return None;
    }
    let mut cur = At::V(from);
    let mut prev: Option<usize> = None;
    let mut dir = b - a;
    let mut path = Vec::new();
    for _ in 0..LIMIT {
        let (x, t) = step(s, &topo, w, &plane, &cur, prev, dir)?;
        match x {
            At::V(c) if c == to => {
                path.push(x);
                break;
            }
            At::V(c) if stop.contains(&c) => return None,
            At::E(p, q, _) if cut_edges.contains(&edge_key(p, q)) => return None,
            _ => {}
        }
        dir = pos(s, &x) - pos(s, &cur);
        path.push(x);
        prev = Some(t);
        cur = x;
    }
    if !matches!(path.last(), Some(At::V(c)) if *c == to) {
        return None;
    }
    let mut ids = vec![from];
    for x in &path {
        ids.push(place(s, x));
    }
    Some(ids)
}

/// The bevel with its bands cut across the wall's rows. `Err(None)`
/// where this layout does not apply (no curved wall, an edge between two
/// curved walls, a wall gap with a real edge inside or at a lone end) or
/// fails other than by size; `Err(Some(at))` where it applies and does
/// not fit, at the bevelled vertex nearest the failure.
pub(super) fn bevel(s: &Solid, spec: &Spec) -> Result<Solid, Option<DVec3>> {
    let topo = topology(s);
    let (info, n) = edge_offsets(s, &topo, spec).map_err(|_| None)?;
    let spec = Spec {
        segments: n,
        ..spec.clone()
    };
    // The curved walls along the selected edges, and their gaps.
    let mut walls: BTreeSet<u32> = BTreeSet::new();
    let mut sides: Vec<(EdgeKey, u32, bool)> = Vec::new();
    for &key in info.keys() {
        let (lo, hi) = key;
        let (Some(&ta), Some(&tb)) = (topo.dir.get(&(lo, hi)), topo.dir.get(&(hi, lo))) else {
            return Err(None);
        };
        let (wa, wb) = (s.surfaces[ta], s.surfaces[tb]);
        match (curved(s, wa), curved(s, wb)) {
            (true, false) => sides.push((key, wa, true)),
            (false, true) => sides.push((key, wb, false)),
            (false, false) => {}
            // Cuts on both sides of one edge are not built.
            (true, true) => return Err(None),
        }
        walls.insert(wa);
        walls.insert(wb);
    }
    walls.retain(|&w| sides.iter().any(|x| x.1 == w));
    if sides.is_empty() {
        return Err(None);
    }
    let ends: BTreeSet<u32> = info.keys().flat_map(|&(a, b)| [a, b]).collect();
    let mut all_gaps = Vec::new();
    for &v in &ends {
        all_gaps.extend(gaps(s, &topo, &info, &walls, v).ok_or(None)?);
    }
    let at_of = |v: u32| s.positions[v as usize];
    // The bevelled vertex nearest a point, to name where a cut fails.
    let near = |p: DVec3| {
        let mut best = (f64::INFINITY, p);
        for &v in &ends {
            let d = (at_of(v) - p).length_squared();
            if d < best.0 {
                best = (d, at_of(v));
            }
        }
        best.1
    };
    // Step 1.
    let mut cur = s.clone();
    let mut points: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    for g in &all_gaps {
        let id = gap_point(&mut cur, g, &ends).ok_or(Some(at_of(g.v)))?;
        if points.values().any(|&x| x == id) {
            return Err(Some(at_of(g.v)));
        }
        points.insert((g.v, g.w), id);
    }
    // Step 2.
    let mut stop: BTreeSet<u32> = ends.clone();
    stop.extend(points.values().copied());
    let mut cut_edges: BTreeSet<EdgeKey> = BTreeSet::new();
    let mut chains: BTreeMap<EdgeKey, Vec<u32>> = BTreeMap::new();
    for &(key, w, side_a) in &sides {
        let (lo, hi) = key;
        let (Some(&from), Some(&to)) = (points.get(&(lo, w)), points.get(&(hi, w))) else {
            return Err(Some(at_of(lo)));
        };
        let t = {
            let topo = topology(&cur);
            let dir = if side_a { (lo, hi) } else { (hi, lo) };
            *topo.dir.get(&dir).ok_or(None)?
        };
        let normal = tri_normal(&cur, t);
        let mut stop_here = stop.clone();
        stop_here.remove(&to);
        let ids =
            cut(&mut cur, w, from, to, normal, &stop_here, &cut_edges).ok_or(Some(at_of(lo)))?;
        for pair in ids.windows(2) {
            cut_edges.insert(edge_key(pair[0], pair[1]));
        }
        stop.extend(ids.iter().copied());
        chains.insert(key, ids);
    }
    // Step 3: the bands.
    let topo = topology(&cur);
    for e in &cut_edges {
        if !topo.dir.contains_key(e) || !topo.dir.contains_key(&(e.1, e.0)) {
            return Err(Some(near(cur.positions[e.0 as usize])));
        }
    }
    let mut removed: BTreeSet<usize> = BTreeSet::new();
    let mut queue: Vec<usize> = Vec::new();
    let mut fronts: BTreeSet<EdgeKey> = BTreeSet::new();
    for &(key, _, side_a) in &sides {
        let dir = if side_a { key } else { (key.1, key.0) };
        let t = *topo.dir.get(&dir).ok_or(None)?;
        fronts.insert(key);
        if removed.insert(t) {
            queue.push(t);
        }
    }
    let mut qi = 0;
    while qi < queue.len() {
        let t = queue[qi];
        qi += 1;
        let tri = cur.triangles[t];
        for k in 0..3 {
            let (a, b) = (tri[k], tri[(k + 1) % 3]);
            if cut_edges.contains(&edge_key(a, b)) || fronts.contains(&edge_key(a, b)) {
                continue;
            }
            let o = *topo.dir.get(&(b, a)).ok_or(None)?;
            if cur.surfaces[o] != cur.surfaces[t] {
                // The band reached the wall's other edges.
                return Err(Some(near(cur.positions[a as usize])));
            }
            if removed.insert(o) {
                queue.push(o);
            }
        }
    }
    let mut wide = Wide {
        removed,
        ..Default::default()
    };
    for &(key, _, side_a) in &sides {
        let ids = &chains[&key];
        let mut acc = vec![0.0];
        for pair in ids.windows(2) {
            let l = (cur.positions[pair[1] as usize] - cur.positions[pair[0] as usize]).length();
            acc.push(acc[acc.len() - 1] + l);
        }
        let total = acc[acc.len() - 1];
        if total.partial_cmp(&0.0) != Some(Ordering::Greater) {
            return Err(Some(at_of(key.0)));
        }
        let mids: Vec<u32> = ids[1..ids.len() - 1].to_vec();
        let along: Vec<f64> = acc[1..acc.len() - 1].iter().map(|x| x / total).collect();
        wide.stations.insert(key, (side_a, mids));
        wide.along.insert(key, along);
    }
    for (&(v, w), &id) in &points {
        wide.gaps.insert((v, w), id);
    }
    bevel_once(&cur, &spec, Some(&wide), TILT_EXACT).map_err(|e| match e {
        Fail::Error(BevelError::TooLarge(at)) => Some(at),
        // Anything else keeps the earlier attempts' error.
        Fail::Error(_) | Fail::Tilted(..) => None,
    })
}

//! Bevels wider than the segments of their faces (`docs/research.md`
//! §7.6): the V2 notes' "handle overlaps", by insetting the offset line
//! across the face and intersecting it with the face's edges.
//!
//! Tried only where the classic layout, and the straight-run retry, gave
//! `TooLarge`, so every result they produce is unchanged. Three passes;
//! the classic layout is tried after the first two, and on the third's
//! cut bands:
//!
//! 0. **Weld.** Edges shorter than 1e-9 of the largest offset are
//!    collapsed (a boolean leaves points 1e-15 apart, with triangles of
//!    zero area on them that no result may keep).
//! 1. **Swallowed runs.** At a vertex `u` of a bevelled edge `f = u → x`
//!    and another bevelled edge `e`, on each face of `f`: `e`'s offset
//!    line crosses `f` at `(d_e + d_f cos θ) / sin θ` from `u` (θ between
//!    the edges), and `x`'s own point lands where `f`'s offset line meets
//!    that of the run's next edge `g`. When the first is past the second
//!    on a face, `f`'s strip has no length there: the edge event of an
//!    inset. Where `x` is in the middle of a smooth bevelled run (two real
//!    edges, both bevelled, between the same two faces) it is slid along
//!    `g`, onto the old edge, to a tenth of the offset past the reach; or,
//!    when that leaves less than a tenth before `g`'s end, merged into
//!    `u`. Either way the faces change only by `x`'s distance from the
//!    chord that replaces its two edges and from the planes of the
//!    triangles it leaves: at most a tenth of the offset, or it is
//!    refused. Repeated while one is found.
//! 2. **Cut bands.** For each side of a bevelled edge whose band (the
//!    face between the edge and its offset line) holds a vertex of that
//!    face other than the edge's ends, or whose face the classic layout
//!    would tilt off its planes (a twisted row, `docs/research.md` §7.7),
//!    the band is cut out of the face
//!    exactly: the offset line is the plane through the offset points at
//!    both ends, square to the face's first triangle. Which side of it a
//!    vertex lies on is decided by `orient3d`; every edge of the face
//!    from inside the band to outside it is split where it crosses (the
//!    face's geometry is unchanged). Every vertex inside is collapsed: one
//!    on an unbevelled real edge forward along it into the cut there (so
//!    the face across that edge keeps its own triangles' planes but for
//!    the edge's bend at the vertex), one inside the face into the edge's
//!    nearer end. The band's triangles are then not
//!    rebuilt; the gap points at the edge's ends are the cut points on the
//!    unbevelled edges there (no new point), and the strip gets a profile
//!    at each cut point between, its other side split to match. So the
//!    face beyond the offset line keeps its own triangles' planes, cut
//!    where the line crosses them. A swallowed vertex must be inside the
//!    face or on a smooth unbevelled run; a corner is refused.

use std::collections::{BTreeMap, BTreeSet};

use core::cmp::Ordering;

use shape_core::{DVec3, predicates};
use shape_mesh::{EdgeKey, Solid, edge_key};

use super::{
    EdgeInfo, Fail, Spec, TILT_EXACT, TILT_PASS1, Topology, bevel_once, edge_offsets, fan,
    topology, tri_normal,
};
use crate::BevelError;

/// What the classic layout needs to know of the cut bands.
#[derive(Default)]
pub(crate) struct Wide {
    /// Per bevelled edge, the vertices on its offset line's cut.
    pub cuts: BTreeMap<EdgeKey, BTreeSet<u32>>,
    /// Triangles inside a cut band: not rebuilt.
    pub removed: BTreeSet<usize>,
    /// Per bevelled edge: whether the cut is on its face `a` (the one
    /// holding `lo → hi`), and the cut points between its ends, `lo` to
    /// `hi`. Each gets a profile.
    pub stations: BTreeMap<EdgeKey, (bool, Vec<u32>)>,
    /// Whether a swallowed vertex was collapsed (the faces at it moved).
    pub collapsed: bool,
}

/// Whether an outline has fewer than three distinct corners: a band
/// triangle whose corners all landed on the cut.
pub(super) fn collapsed(poly: &[u32]) -> bool {
    poly.iter().copied().collect::<BTreeSet<u32>>().len() < 3
}

/// The bevel, re-meshed where its bands are wider than the faces'
/// segments; `TooLarge` when that does not fit either.
pub(super) fn bevel(s: &Solid, spec: &Spec) -> Result<Solid, BevelError> {
    let at = spec
        .edges
        .first()
        .map_or(DVec3::ZERO, |e| s.positions[e.0 as usize]);
    let topo = topology(s);
    let (info, n) = edge_offsets(s, &topo, spec)?;
    // The segment count of the solid as given, whatever the passes change.
    let spec = Spec {
        segments: n,
        ..spec.clone()
    };
    let largest = info.values().flat_map(|e| e.off).fold(0.0f64, f64::max);
    let mut cur = s.clone();
    let mut spec = spec;
    if let Some((w, edges)) = weld(&cur, &spec.edges, 1e-9 * largest) {
        cur = w;
        spec.edges = edges;
    }
    let mut tilt = TILT_EXACT;
    if let Some((m, edges)) = merge(&cur, &spec) {
        cur = m;
        spec.edges = edges;
        tilt = TILT_PASS1;
    }
    // The classic layout on the passes' solid (the caller's attempt again
    // when they changed nothing): a result, or the bands it would tilt,
    // which are then cut as if they held a vertex.
    let forced = match bevel_once(&cur, &spec, None, tilt) {
        Ok(out) => return Ok(out),
        Err(Fail::Tilted(_, bands)) => bands,
        Err(Fail::Error(_)) => BTreeSet::new(),
    };
    let sp = spec;
    let (solid, wide) = cut(&cur, &sp, &forced).ok_or(BevelError::TooLarge(at))?;
    // A collapse moves the faces at the vertex, as pass 1 does.
    let tilt = if wide.collapsed { TILT_PASS1 } else { tilt };
    bevel_once(&solid, &sp, Some(&wide), tilt).map_err(|_| BevelError::TooLarge(at))
}

/// The offset of `key` on its face `a` (or `b`) at its end `v`.
fn side_offset(info: &BTreeMap<EdgeKey, EdgeInfo>, key: EdgeKey, a: bool, v: u32) -> f64 {
    let off = info[&key].off;
    match (a, v == key.0) {
        (true, true) => off[0],
        (false, true) => off[1],
        (true, false) => off[2],
        (false, false) => off[3],
    }
}

/// The vertices sharing a triangle with `v`.
fn neighbours(s: &Solid, topo: &Topology, v: u32) -> BTreeSet<u32> {
    topo.at[v as usize]
        .iter()
        .flat_map(|&t| s.triangles[t])
        .filter(|&x| x != v)
        .collect()
}

/// `x` collapsed into its neighbour `u`: the two triangles on edge
/// `(u, x)` go, the others take `u` for `x`, and `x`'s edge names move to
/// `u`. `None` where that would pinch the surface (the link condition).
fn collapse(s: &Solid, x: u32, u: u32) -> Option<Solid> {
    let topo = topology(s);
    let both: Vec<usize> = topo.at[x as usize]
        .iter()
        .copied()
        .filter(|&t| s.triangles[t].contains(&u))
        .collect();
    if both.len() != 2 {
        return None;
    }
    let apex: BTreeSet<u32> = both
        .iter()
        .flat_map(|&t| s.triangles[t])
        .filter(|&v| v != u && v != x)
        .collect();
    let common: BTreeSet<u32> = neighbours(s, &topo, u)
        .intersection(&neighbours(s, &topo, x))
        .copied()
        .collect();
    if common != apex {
        return None;
    }
    let mut out = s.clone();
    let mut keep = Vec::new();
    for (i, t) in s.triangles.iter().enumerate() {
        if both.contains(&i) {
            continue;
        }
        let t = t.map(|v| if v == x { u } else { v });
        keep.push((t, s.surfaces[i], s.face_tags[i].clone()));
    }
    out.triangles = keep.iter().map(|k| k.0).collect();
    out.surfaces = keep.iter().map(|k| k.1).collect();
    out.face_tags = keep.into_iter().map(|k| k.2).collect();
    let mut tags = BTreeMap::new();
    for (&(a, b), t) in &s.edge_tags {
        if edge_key(a, b) == edge_key(u, x) {
            continue;
        }
        let m = |v: u32| if v == x { u } else { v };
        let e: &mut Vec<_> = tags.entry(edge_key(m(a), m(b))).or_default();
        e.extend(t.iter().copied());
        e.sort_unstable();
        e.dedup();
    }
    out.edge_tags = tags;
    Some(out)
}

/// The real spokes at `v` (spoke vertices), and the surfaces around it;
/// `None` where its fan does not close.
fn real_spokes(s: &Solid, topo: &Topology, v: u32) -> Option<(Vec<u32>, BTreeSet<u32>)> {
    let f = fan(s, topo, v).ok()?;
    let k = f.tris.len();
    let real = (0..k)
        .filter(|&i| s.surfaces[f.tris[(i + k - 1) % k]] != s.surfaces[f.tris[i]])
        .map(|i| f.spokes[i])
        .collect();
    Some((real, f.tris.iter().map(|&t| s.surfaces[t]).collect()))
}

/// Pass 0: every edge shorter than `tol` collapsed (a boolean leaves
/// points 1e-15 apart, and triangles of zero area on them that no bevel
/// would pass). `None` when there is none.
pub(super) fn weld(s: &Solid, edges: &[EdgeKey], tol: f64) -> Option<(Solid, Vec<EdgeKey>)> {
    let mut cur = s.clone();
    let mut es: BTreeSet<EdgeKey> = edges.iter().copied().collect();
    let mut any = false;
    for _ in 0..s.positions.len() {
        let mut short: BTreeSet<EdgeKey> = BTreeSet::new();
        for t in &cur.triangles {
            for k in 0..3 {
                let (a, b) = (t[k], t[(k + 1) % 3]);
                if (cur.positions[a as usize] - cur.positions[b as usize]).length() <= tol {
                    short.insert(edge_key(a, b));
                }
            }
        }
        let Some((u, x, next)) = short
            .iter()
            .find_map(|&(u, x)| collapse(&cur, x, u).map(|c| (u, x, c)))
        else {
            break;
        };
        cur = next;
        es = es
            .into_iter()
            .filter(|&e| e != edge_key(u, x))
            .map(|(a, b)| {
                let m = |v: u32| if v == x { u } else { v };
                edge_key(m(a), m(b))
            })
            .collect();
        any = true;
    }
    any.then(|| (cur, es.into_iter().collect()))
}

/// Pass 1: run vertices swallowed by a neighbouring bevel, slid past
/// its reach or merged into the corner. `None` when there is none.
fn merge(s: &Solid, spec: &Spec) -> Option<(Solid, Vec<EdgeKey>)> {
    let mut cur = s.clone();
    let mut edges: BTreeSet<EdgeKey> = spec.edges.iter().copied().collect();
    let mut any = false;
    for _ in 0..2 * spec.edges.len() {
        let topo = topology(&cur);
        let sp = Spec {
            edges: edges.iter().copied().collect(),
            ..spec.clone()
        };
        let (info, _) = edge_offsets(&cur, &topo, &sp).ok()?;
        let found = edges.iter().find_map(|&f| {
            [(f.0, f.1), (f.1, f.0)]
                .into_iter()
                .find_map(|(u, x)| swallowed_run(&cur, &topo, &info, &edges, u, x))
        });
        let Some(Swallowed { u, x, y, to }) = found else {
            break;
        };
        match to {
            Some(at) => cur.positions[x as usize] = at,
            None => {
                cur = collapse(&cur, x, u)?;
                edges.remove(&edge_key(u, x));
                edges.remove(&edge_key(x, y));
                edges.insert(edge_key(u, y));
            }
        }
        any = true;
    }
    any.then(|| (cur, edges.into_iter().collect()))
}

/// A run vertex `x` swallowed at `u`: where it goes (`None`: merged into
/// `u`), and the run's next vertex `y`.
struct Swallowed {
    u: u32,
    x: u32,
    y: u32,
    to: Option<DVec3>,
}

/// Whether the bevelled edge `u → x` is swallowed at `u` by another
/// bevelled edge there (pass 1), and where `x` goes: slid along the run
/// toward `y` to a tenth of the offset past the farthest reach, or, when
/// that leaves less than a tenth of the offset before `y`, merged into
/// `u`. Refused when the faces would move by more than a tenth of the
/// offset.
fn swallowed_run(
    s: &Solid,
    topo: &Topology,
    info: &BTreeMap<EdgeKey, EdgeInfo>,
    edges: &BTreeSet<EdgeKey>,
    u: u32,
    x: u32,
) -> Option<Swallowed> {
    let f = edge_key(u, x);
    let (real, surfaces) = real_spokes(s, topo, x)?;
    if real.len() != 2 || surfaces.len() != 2 || !real.contains(&u) {
        return None;
    }
    let y = *real.iter().find(|&&v| v != u)?;
    if !edges.contains(&edge_key(x, y)) {
        return None;
    }
    let p = |v: u32| s.positions[v as usize];
    let len = (p(x) - p(u)).length();
    let fd = (p(x) - p(u)).normalize_or_zero();
    // On each face of f: how far along f another bevel at u reaches, and
    // where x's own point lands (its offset lines with the next edge g's,
    // in the plane of f's triangle there; on f's line where they are
    // parallel). f's strip there runs between the two; with none left, f
    // is swallowed. `need` is how far x must be from u for every face.
    let tri_on = |a: u32, b: u32, surface: u32| {
        [topo.dir.get(&(a, b)), topo.dir.get(&(b, a))]
            .into_iter()
            .flatten()
            .copied()
            .find(|&t| s.surfaces[t] == surface)
    };
    let (g, gd) = (edge_key(x, y), (p(y) - p(x)).normalize_or_zero());
    let mut swallowed = false;
    let mut need = 0.0f64;
    let mut big = 0.0f64;
    for tf in [topo.dir.get(&(u, x)), topo.dir.get(&(x, u))]
        .into_iter()
        .flatten()
    {
        let surface = s.surfaces[*tf];
        let df = side_offset(info, f, topo.dir.get(&f) == Some(tf), u);
        let mut reach = f64::NEG_INFINITY;
        for &e in edges.iter().filter(|&&e| e != f && (e.0 == u || e.1 == u)) {
            let w = if e.0 == u { e.1 } else { e.0 };
            let Some(te) = tri_on(u, w, surface) else {
                continue;
            };
            let de = side_offset(info, e, topo.dir.get(&e) == Some(&te), u);
            let ed = (p(w) - p(u)).normalize_or_zero();
            let (sin, cos) = (ed.cross(fd).length(), ed.dot(fd));
            if sin < 1e-9 {
                continue;
            }
            reach = reach.max((de + df * cos) / sin);
            big = big.max(de);
        }
        if reach == f64::NEG_INFINITY {
            continue;
        }
        let Some(tg) = tri_on(x, y, surface) else {
            continue;
        };
        let dg = side_offset(info, g, topo.dir.get(&g) == Some(&tg), x);
        let n = tri_normal(s, *tf);
        let third = s.triangles[*tf]
            .iter()
            .copied()
            .find(|&v| v != u && v != x)?;
        let mut q = n.cross(fd);
        if q.dot(p(third) - p(u)) < 0.0 {
            q = -q;
        }
        let gthird = s.triangles[tg]
            .iter()
            .copied()
            .find(|&v| v != x && v != y)?;
        // g in f's plane, turned by φ from f toward the face; x's point is
        // where the offset lines meet, `back` along f from x (negative
        // behind it): (df cos φ − dg) / sin φ, in the half-angle form that
        // stays exact as φ → 0. Where g leaves f's plane (a run turning
        // out of it, on a curved face) the point stays on f's line.
        let (gx, gy) = (gd.dot(fd), gd.dot(q));
        let gl = (gx * gx + gy * gy).sqrt();
        let into = p(gthird) - p(x);
        let left = -gy * into.dot(fd) + gx * into.dot(q) > 0.0;
        let (cos, sin) = (gx / gl, gy / gl);
        let back = if gl.is_nan() || gl <= 1e-9 || !left || 1.0 + cos < 1e-9 {
            0.0
        } else if sin.abs() < 1e-6 {
            -df * sin / (1.0 + cos)
        } else {
            (df - dg) / sin - df * sin / (1.0 + cos)
        };
        if reach >= len + back {
            swallowed = true;
        }
        need = need.max(reach - back);
    }
    if !swallowed {
        return None;
    }
    // Slide to |x' − u| = need + big / 10 on x → y, if that is short of y.
    let want = need + big / 10.0;
    let (dx, dy) = (p(x) - p(u), p(y) - p(x));
    let (qa, qb, qc) = (dy.dot(dy), 2.0 * dx.dot(dy), dx.dot(dx) - want * want);
    let disc = qb * qb - 4.0 * qa * qc;
    let t = (-qb + disc.max(0.0).sqrt()) / (2.0 * qa);
    let to =
        (disc >= 0.0 && t > 0.0 && (1.0 - t) * dy.length() >= big / 10.0).then(|| p(x) + dy * t);
    let goes = to.unwrap_or(p(u));
    // How far the faces move: x's distance from the chord that replaces
    // its edges, and from each triangle it leaves behind.
    let chord = (p(y) - goes).normalize_or_zero();
    let mut dev = ((p(x) - goes) - chord * (p(x) - goes).dot(chord)).length();
    for &t in &topo.at[x as usize] {
        let tri = s.triangles[t];
        if to.is_none() && tri.contains(&u) {
            continue;
        }
        let [a, b, c] = tri.map(|v| if v == x { goes } else { p(v) });
        let n = (b - a).cross(c - a).normalize_or_zero();
        dev = dev.max((p(x) - a).dot(n).abs());
    }
    (dev <= 0.1 * big).then_some(Swallowed { u, x, y, to })
}

/// `a – b` split at `m`: both its triangles in two, its names on both
/// halves. Returns the new vertex.
fn split(s: &mut Solid, a: u32, b: u32, m: DVec3) -> u32 {
    let id = s.push_vertex(m);
    let n = s.triangles.len();
    for i in 0..n {
        let t = s.triangles[i];
        for k in 0..3 {
            let (x, y, z) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            if edge_key(x, y) == edge_key(a, b) {
                s.triangles[i] = [x, id, z];
                s.triangles.push([id, y, z]);
                s.surfaces.push(s.surfaces[i]);
                s.face_tags.push(s.face_tags[i].clone());
                break;
            }
        }
    }
    if let Some(tags) = s.edge_tags.remove(&edge_key(a, b)) {
        s.edge_tags.insert(edge_key(a, id), tags.clone());
        s.edge_tags.insert(edge_key(id, b), tags);
    }
    id
}

/// One side's band, after its cut: the edge, its face's surface, whether
/// it is face `a`.
struct Band {
    key: EdgeKey,
    surface: u32,
    side_a: bool,
}

/// Pass 2: every band holding a vertex of its face cut out of the face.
/// `None` when a band cannot be cut (a swallowed corner, a bevelled edge
/// across it, both sides of one edge).
fn cut(s: &Solid, spec: &Spec, forced: &BTreeSet<(EdgeKey, bool)>) -> Option<(Solid, Wide)> {
    let topo0 = topology(s);
    let (info, _) = edge_offsets(s, &topo0, spec).ok()?;
    let bev: BTreeSet<EdgeKey> = spec.edges.iter().copied().collect();
    let ends: BTreeSet<u32> = bev.iter().flat_map(|&(a, b)| [a, b]).collect();
    let mut cur = s.clone();
    let mut wide = Wide::default();
    let mut bands = Vec::new();
    // Cut points on unbevelled real edges, shared by the bands on both
    // sides: the first band's point is the later bands' too (the classic
    // layout gives the faces on either side of such an edge one point).
    let mut real_cuts: BTreeSet<u32> = BTreeSet::new();
    for &key in &spec.edges {
        for side_a in [true, false] {
            let topo = topology(&cur);
            let (lo, hi) = key;
            let dir = if side_a { (lo, hi) } else { (hi, lo) };
            let t = *topo.dir.get(&dir)?;
            let surface = cur.surfaces[t];
            let p = |v: u32| cur.positions[v as usize];
            let (pl, ph) = (p(lo), p(hi));
            let len = (ph - pl).length();
            let along = (ph - pl) / len;
            let n = tri_normal(&cur, t);
            let third = cur.triangles[t]
                .iter()
                .copied()
                .find(|&v| v != lo && v != hi)?;
            let mut q = n.cross(along).normalize_or_zero();
            if q.dot(p(third) - pl) < 0.0 {
                q = -q;
            }
            let (dl, dh) = (
                side_offset(&info, key, side_a, lo),
                side_offset(&info, key, side_a, hi),
            );
            let dmax = dl.max(dh);
            // The offset line's plane, square to the face's first triangle,
            // and the edge's own.
            let (o1, o2) = (pl + q * dl, ph + q * dh);
            let o3 = o1 + n;
            let inside_sign = predicates::orient3d(o1, o2, o3, pl);
            let face_sign = predicates::orient3d(pl, ph, pl + n, p(third));
            if inside_sign == Ordering::Equal || face_sign == Ordering::Equal {
                return None;
            }
            let side = |x: DVec3| predicates::orient3d(o1, o2, o3, x);
            let in_band = |x: DVec3| {
                let lateral = (x - pl).dot(along);
                side(x) == inside_sign
                    && predicates::orient3d(pl, ph, pl + n, x) != face_sign.reverse()
                    && lateral > -dmax
                    && lateral < len + dmax
            };
            // The face's triangles and the vertices inside the band,
            // reached from the edge's ends across the face.
            let on_face: Vec<usize> = (0..cur.triangles.len())
                .filter(|&i| cur.surfaces[i] == surface)
                .collect();
            let mut swallowed: Vec<u32> = Vec::new();
            let mut pinned: BTreeSet<u32> = BTreeSet::new();
            let mut seen: BTreeSet<u32> = BTreeSet::from([lo, hi]);
            let mut queue = vec![lo, hi];
            let mut qi = 0;
            while qi < queue.len() {
                let v = queue[qi];
                qi += 1;
                for &i in &on_face {
                    let tri = cur.triangles[i];
                    if !tri.contains(&v) {
                        continue;
                    }
                    for x in tri {
                        if seen.contains(&x) || !in_band(p(x)) {
                            continue;
                        }
                        // A cut an earlier band left on an unbevelled real
                        // edge is this band's too.
                        if real_cuts.contains(&x) {
                            pinned.insert(x);
                            continue;
                        }
                        seen.insert(x);
                        swallowed.push(x);
                        queue.push(x);
                    }
                }
            }
            if swallowed.is_empty() && pinned.is_empty() && !forced.contains(&(key, side_a)) {
                continue;
            }
            for &x in &swallowed {
                if ends.contains(&x) {
                    return None;
                }
                let (real, surfaces) = real_spokes(&cur, &topo, x)?;
                if !(real.is_empty() || (real.len() == 2 && surfaces.len() == 2)) {
                    return None;
                }
            }
            // Cut every edge of the face from inside the band to outside.
            let inner: BTreeSet<u32> = seen.clone();
            let mut crossing: BTreeSet<EdgeKey> = BTreeSet::new();
            let mut on_cut: BTreeSet<u32> = pinned.clone();
            for &i in &on_face {
                let tri = cur.triangles[i];
                for k in 0..3 {
                    let (a, b) = (tri[k], tri[(k + 1) % 3]);
                    for (a, b) in [(a, b), (b, a)] {
                        if !inner.contains(&a) || inner.contains(&b) || pinned.contains(&b) {
                            continue;
                        }
                        // Only from inside this band's own line.
                        if side(p(a)) != inside_sign {
                            continue;
                        }
                        if real_cuts.contains(&b) {
                            on_cut.insert(b);
                            continue;
                        }
                        match side(p(b)) {
                            Ordering::Equal => {
                                on_cut.insert(b);
                            }
                            o if o == inside_sign => {}
                            _ => {
                                crossing.insert(edge_key(a, b));
                            }
                        }
                    }
                }
            }
            let plane_n = (o2 - o1).cross(o3 - o1);
            let mut at: Vec<(EdgeKey, DVec3)> = Vec::new();
            for &(a, b) in &crossing {
                if bev.contains(&(a, b)) {
                    return None;
                }
                let (ga, gb) = ((p(a) - o1).dot(plane_n), (p(b) - o1).dot(plane_n));
                if ga == gb {
                    return None;
                }
                at.push(((a, b), p(a) + (p(b) - p(a)) * (ga / (ga - gb))));
            }
            for ((a, b), m) in at {
                let topo = topology(&cur);
                let real = match (topo.dir.get(&(a, b)), topo.dir.get(&(b, a))) {
                    (Some(&x), Some(&y)) => cur.surfaces[x] != cur.surfaces[y],
                    _ => false,
                };
                let id = split(&mut cur, a, b, m);
                on_cut.insert(id);
                if real {
                    real_cuts.insert(id);
                }
            }
            // Collapse the swallowed vertices. One on an unbevelled real
            // edge goes forward along it into the cut there (the deepest
            // first), so the face across that edge keeps its triangles'
            // planes; one inside the face goes into the edge's nearer end
            // (nearest first, one row at a time inward).
            let mut left = swallowed.clone();
            while !left.is_empty() {
                let topo = topology(&cur);
                let real_to = |a: u32, b: u32| match (topo.dir.get(&(a, b)), topo.dir.get(&(b, a)))
                {
                    (Some(&x), Some(&y)) => cur.surfaces[x] != cur.surfaces[y],
                    _ => false,
                };
                let pick = left.iter().enumerate().find_map(|(j, &x)| {
                    let nb = neighbours(&cur, &topo, x);
                    if let Some(&c) = nb.iter().find(|&&c| on_cut.contains(&c) && real_to(x, c)) {
                        return Some((j, x, c));
                    }
                    if nb.iter().any(|&c| real_to(x, c)) {
                        return None;
                    }
                    let mut best: Option<(f64, u32)> = None;
                    for e in [lo, hi] {
                        if nb.contains(&e) {
                            let d =
                                (cur.positions[x as usize] - cur.positions[e as usize]).length();
                            if best.is_none_or(|(b, _)| d < b) {
                                best = Some((d, e));
                            }
                        }
                    }
                    best.map(|(_, e)| (j, x, e))
                });
                let (j, x, e) = pick?;
                cur = collapse(&cur, x, e)?;
                left.remove(j);
                wide.collapsed = true;
            }
            wide.cuts.entry(key).or_default().extend(on_cut);
            bands.push(Band {
                key,
                surface,
                side_a,
            });
        }
    }
    if bands.is_empty() {
        return None;
    }
    let topo = topology(&cur);
    let real_to = |e: u32, c: u32| {
        let (Some(&x), Some(&y)) = (topo.dir.get(&(e, c)), topo.dir.get(&(c, e))) else {
            return false;
        };
        cur.surfaces[x] != cur.surfaces[y]
    };
    for band in &bands {
        let (lo, hi) = band.key;
        let cuts = wide.cuts.get(&band.key)?.clone();
        let inside = |v: u32| v == lo || v == hi || cuts.contains(&v);
        for (i, tri) in cur.triangles.iter().enumerate() {
            if cur.surfaces[i] == band.surface && tri.iter().all(|&v| inside(v)) {
                wide.removed.insert(i);
            }
        }
        // The cut's ends: one cut point one real edge from each end.
        let ends_of = |e: u32| cuts.iter().filter(|&&c| real_to(e, c)).count();
        if ends_of(lo) != 1 || ends_of(hi) != 1 {
            return None;
        }
        let (pl, ph) = (cur.positions[lo as usize], cur.positions[hi as usize]);
        let mut mid: Vec<(f64, u32)> = cuts
            .iter()
            .copied()
            .filter(|&c| !real_to(lo, c) && !real_to(hi, c))
            .filter(|&c| {
                cur.triangles
                    .iter()
                    .enumerate()
                    .any(|(i, t)| cur.surfaces[i] == band.surface && t.contains(&c))
            })
            .map(|c| {
                let t = (cur.positions[c as usize] - pl).dot(ph - pl) / (ph - pl).length_squared();
                (t, c)
            })
            .collect();
        mid.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        if mid.iter().any(|&(t, _)| !(t > 0.0 && t < 1.0)) {
            return None;
        }
        if wide.stations.contains_key(&band.key) {
            return None;
        }
        wide.stations.insert(
            band.key,
            (band.side_a, mid.into_iter().map(|m| m.1).collect()),
        );
    }
    Some((cur, wide))
}

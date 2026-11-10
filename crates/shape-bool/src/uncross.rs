//! Boolean outputs that cross themselves by a rounding error
//! (`docs/research.md` §18.5, §23).
//!
//! The arrangement is exact, but its constructed points are rounded to
//! f64. Where two exact points lie a rounding error apart (an edge of one
//! operand passing an ulp off an edge or a vertex of the other), the
//! needles between them can turn through their neighbours once rounded,
//! and the result crosses itself, though it is closed.
//!
//! Only triangles with a constructed corner (a position that is not an
//! operand's vertex) can have moved, so only pairs with one of them are
//! tested ([`crossing_pairs`]). Where there is none, the output is
//! returned as built, byte for byte.
//!
//! Otherwise each short edge at a crossing (its two ends within 2⁻⁴⁰ of
//! the operands' size, the snap distance of `csg::snap_onto`) with a
//! constructed end is collapsed: the constructed end moves onto the other
//! (an operand's vertex stays where it is), the triangles on the edge are
//! dropped, and the collapse is kept only if, by exact predicates, every
//! edge at the merged point is still used once each way, no triangle there
//! is degenerate or doubled, and fewer pairs cross there than before. Edges
//! are taken shortest first; each kept collapse removes a point, so the
//! repair ends. What still crosses is left and counted.

use std::collections::BTreeMap;

use glam::DVec3;

use crate::selfcheck::{crossing, degenerate};

/// Cells a mean triangle across, as `selfcheck::box_pairs` lays them out;
/// a box covering more than this many cells is kept in a list of its own.
const MAX_CELLS: u64 = 1 << 12;

/// Boxes in a uniform grid, for overlap queries.
struct Grid {
    lo: DVec3,
    h: f64,
    side: u64,
    cells: BTreeMap<u64, Vec<u32>>,
    big: Vec<u32>,
}

impl Grid {
    fn new(boxes: &[(DVec3, DVec3)], ids: &[u32]) -> Grid {
        let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
        let mut extent = 0.0;
        for &i in ids {
            let (a, b) = boxes[i as usize];
            lo = lo.min(a);
            hi = hi.max(b);
            extent += (b - a).max_element();
        }
        const SIDE: f64 = (1u64 << 20) as f64;
        let span = (hi - lo).max_element();
        let mut h = if ids.is_empty() {
            1.0
        } else {
            extent / ids.len() as f64 * 2.0
        };
        if h <= 0.0 || !h.is_finite() || span / h > SIDE - 2.0 {
            h = (span / (SIDE - 2.0)).max(f64::MIN_POSITIVE);
        }
        let mut g = Grid {
            lo,
            h,
            side: SIDE as u64,
            cells: BTreeMap::new(),
            big: Vec::new(),
        };
        for &i in ids {
            g.insert(i, boxes[i as usize]);
        }
        g
    }

    fn cell(&self, p: DVec3) -> [u64; 3] {
        let c = (p - self.lo) / self.h;
        // Below `lo` (a box that grew after the grid was laid out) is
        // clamped to the first cell; truncation is the floor above it.
        [c.x, c.y, c.z].map(|x| {
            if x > 0.0 {
                (x as u64).min(self.side - 1)
            } else {
                0
            }
        })
    }

    fn key(c: [u64; 3]) -> u64 {
        (c[0] << 40) | (c[1] << 20) | c[2]
    }

    fn range(&self, b: (DVec3, DVec3)) -> Option<([u64; 3], [u64; 3])> {
        let (ca, cb) = (self.cell(b.0), self.cell(b.1));
        let count = (0..3).map(|k| cb[k] - ca[k] + 1).product::<u64>();
        (count <= MAX_CELLS).then_some((ca, cb))
    }

    fn insert(&mut self, i: u32, b: (DVec3, DVec3)) {
        let Some((ca, cb)) = self.range(b) else {
            self.big.push(i);
            return;
        };
        for x in ca[0]..=cb[0] {
            for y in ca[1]..=cb[1] {
                for z in ca[2]..=cb[2] {
                    self.cells.entry(Self::key([x, y, z])).or_default().push(i);
                }
            }
        }
    }

    /// Ids whose cells the box covers (a superset of the overlapping
    /// boxes), sorted, each once; `all` when the box covers too many.
    fn query(&self, b: (DVec3, DVec3), all: &[u32]) -> Vec<u32> {
        let mut out: Vec<u32> = match self.range(b) {
            None => all.to_vec(),
            Some((ca, cb)) => {
                let mut v = self.big.clone();
                for x in ca[0]..=cb[0] {
                    for y in ca[1]..=cb[1] {
                        for z in ca[2]..=cb[2] {
                            if let Some(c) = self.cells.get(&Self::key([x, y, z])) {
                                v.extend_from_slice(c);
                            }
                        }
                    }
                }
                v
            }
        };
        out.sort_unstable();
        out.dedup();
        out
    }
}

fn overlap(a: (DVec3, DVec3), b: (DVec3, DVec3)) -> bool {
    a.0.cmple(b.1).all() && b.0.cmple(a.1).all()
}

/// A welded closed surface being repaired.
struct Mesh<'a> {
    points: &'a [DVec3],
    tris: Vec<[u32; 3]>,
    alive: Vec<bool>,
    /// Each point's triangles (alive or not; filtered on use).
    star: Vec<Vec<u32>>,
}

impl Mesh<'_> {
    fn pos(&self, t: [u32; 3]) -> [DVec3; 3] {
        t.map(|v| self.points[v as usize])
    }

    fn bbox(&self, t: [u32; 3]) -> (DVec3, DVec3) {
        let p = self.pos(t);
        (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]))
    }

    fn live_star(&self, v: u32) -> Vec<u32> {
        let mut s: Vec<u32> = self.star[v as usize]
            .iter()
            .copied()
            .filter(|&t| self.alive[t as usize] && self.tris[t as usize].contains(&v))
            .collect();
        s.sort_unstable();
        s.dedup();
        s
    }

    /// Crossing pairs `(min, max)` with at least one triangle of `set`
    /// (given as triangles, with ids; an id of `u32::MAX` is a triangle
    /// not in the mesh yet), sorted.
    fn pairs_of(&self, set: &[(u32, [u32; 3])], grid: &Grid, all: &[u32]) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        for &(i, t) in set {
            let pt = self.pos(t);
            if degenerate(pt) {
                continue;
            }
            let bt = self.bbox(t);
            for j in grid.query(bt, all) {
                if j == i || !self.alive[j as usize] {
                    continue;
                }
                // A triangle of the set that is replaced is tested as given.
                if set.iter().any(|&(k, _)| k == j) && j < i {
                    continue;
                }
                let u = set
                    .iter()
                    .find(|&&(k, _)| k == j)
                    .map_or(self.tris[j as usize], |&(_, u)| u);
                let pu = self.pos(u);
                if degenerate(pu) || !overlap(bt, self.bbox(u)) {
                    continue;
                }
                if crossing(t, pt, u, pu) {
                    out.push((i.min(j), i.max(j)));
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// Pairs of `tris` (welded ids into `points`) that cross, with at least one
/// triangle having a corner `constructed`; sorted.
fn crossing_pairs(points: &[DVec3], tris: &[[u32; 3]], constructed: &[bool]) -> Vec<(u32, u32)> {
    let seam: Vec<u32> = (0..tris.len() as u32)
        .filter(|&i| tris[i as usize].iter().any(|&v| constructed[v as usize]))
        .collect();
    if seam.is_empty() {
        return Vec::new();
    }
    let boxes: Vec<(DVec3, DVec3)> = tris
        .iter()
        .map(|t| {
            let p = t.map(|v| points[v as usize]);
            (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]))
        })
        .collect();
    let grid = Grid::new(&boxes, &seam);
    let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
    for &i in &seam {
        lo = lo.min(boxes[i as usize].0);
        hi = hi.max(boxes[i as usize].1);
    }
    let mut out = Vec::new();
    for (j, t) in tris.iter().enumerate() {
        let b = boxes[j];
        if !overlap(b, (lo, hi)) {
            continue;
        }
        let pt = t.map(|v| points[v as usize]);
        if degenerate(pt) {
            continue;
        }
        let j = j as u32;
        let is_seam = seam.binary_search(&j).is_ok();
        for i in grid.query(b, &seam) {
            // Two seam triangles are tested once, the lower first.
            if i == j || (is_seam && i < j) || !overlap(b, boxes[i as usize]) {
                continue;
            }
            let u = tris[i as usize];
            let pu = u.map(|v| points[v as usize]);
            if degenerate(pu) {
                continue;
            }
            if crossing(*t, pt, u, pu) {
                out.push((i.min(j), i.max(j)));
            }
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// What [`uncross`](crate::uncross_output) did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Uncrossed {
    /// Crossing pairs found in the output as built (pairs with a triangle
    /// that has a constructed corner).
    pub before: usize,
    /// Crossing pairs left.
    pub after: usize,
    /// Points merged into a neighbour a rounding error away.
    pub collapsed: usize,
    /// Needles split across their long edge.
    pub split: usize,
}

/// Repairs a closed output `(points, tris)` (its points welded by exact
/// position) that crosses itself by a rounding error; `rank` gives each
/// point 0 for the target's vertex, 1 for the tool's and 2 for a
/// constructed point, `tol` the distance within which points are merged
/// and a needle split. Returns, when it repaired anything, the triangles
/// kept with their corners and the index of the input triangle each lies
/// in, in order.
#[allow(clippy::type_complexity)]
pub(crate) fn uncross(
    wpoints: &[DVec3],
    wtris: &[[u32; 3]],
    rank: &[u8],
    tol: f64,
) -> (Uncrossed, Option<Vec<(usize, [u32; 3])>>) {
    let constructed: Vec<bool> = rank.iter().map(|&r| r == 2).collect();
    let first = crossing_pairs(wpoints, wtris, &constructed);
    let mut report = Uncrossed {
        before: first.len(),
        after: first.len(),
        ..Uncrossed::default()
    };
    if first.is_empty() {
        return (report, None);
    }
    let mut m = Mesh {
        points: wpoints,
        tris: wtris.to_vec(),
        alive: vec![true; wtris.len()],
        star: vec![Vec::new(); wpoints.len()],
    };
    let mut src: Vec<usize> = (0..wtris.len()).collect();
    for (i, t) in wtris.iter().enumerate() {
        for &v in t {
            m.star[v as usize].push(i as u32);
        }
    }
    let all: Vec<u32> = (0..wtris.len() as u32).collect();
    let boxes: Vec<(DVec3, DVec3)> = wtris.iter().map(|&t| m.bbox(t)).collect();
    let mut grid = Grid::new(&boxes, &all);
    let tol2 = tol * tol;
    let mut pairs = first;
    let mut removed = vec![false; wpoints.len()];
    for _round in 0..16 {
        let mut any = false;
        // Short edges at the crossing pairs' corners, shortest first.
        let mut cands: Vec<(f64, u32, u32)> = Vec::new();
        for &(i, j) in &pairs {
            for t in [i, j] {
                for v in m.tris[t as usize] {
                    for s in m.live_star(v) {
                        for u in m.tris[s as usize] {
                            if u == v || (rank[u as usize] < 2 && rank[v as usize] < 2) {
                                continue;
                            }
                            let d2 = (wpoints[u as usize] - wpoints[v as usize]).length_squared();
                            if d2 <= tol2 {
                                cands.push((d2, u.min(v), u.max(v)));
                            }
                        }
                    }
                }
            }
        }
        shape_core::sort::sort_by(&mut cands, |a, b| {
            a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
        });
        cands.dedup();
        for &(_, u, v) in &cands {
            if removed[u as usize] || removed[v as usize] {
                continue;
            }
            let (s, r) = if (rank[u as usize], u) <= (rank[v as usize], v) {
                (u, v)
            } else {
                (v, u)
            };
            if collapse(&mut m, &mut grid, &all, s, r) {
                removed[r as usize] = true;
                report.collapsed += 1;
                any = true;
            }
        }
        // Needles at the pairs still crossing: a corner within `tol` of
        // the opposite edge, between its ends.
        let live = live_pairs(&m, &constructed);
        let mut needles: Vec<(f64, u32, usize)> = Vec::new();
        for &(i, j) in &live {
            for t in [i, j] {
                let mut near = vec![t];
                for v in m.tris[t as usize] {
                    near.extend(m.live_star(v));
                }
                for n in near {
                    if let Some((h, k)) = needle(&m, m.tris[n as usize], tol) {
                        needles.push((h, n, k));
                    }
                }
            }
        }
        shape_core::sort::sort_by(&mut needles, |a, b| {
            a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2))
        });
        needles.dedup();
        for &(_, n, k) in &needles {
            if m.alive[n as usize] && split(&mut m, &mut src, &mut grid, &all, n, k) {
                report.split += 1;
                any = true;
            }
        }
        pairs = live_pairs(&m, &constructed);
        if pairs.is_empty() || !any {
            break;
        }
    }
    report.after = pairs.len();
    if report.after >= report.before {
        return (
            Uncrossed {
                after: report.before,
                collapsed: 0,
                split: 0,
                ..report
            },
            None,
        );
    }
    let kept = (0..m.tris.len())
        .filter(|&i| m.alive[i])
        .map(|i| (src[i], m.tris[i]))
        .collect();
    (report, Some(kept))
}

/// The crossing pairs of the live triangles, by slot.
fn live_pairs(m: &Mesh, constructed: &[bool]) -> Vec<(u32, u32)> {
    let idx: Vec<u32> = (0..m.tris.len() as u32)
        .filter(|&i| m.alive[i as usize])
        .collect();
    let live: Vec<[u32; 3]> = idx.iter().map(|&i| m.tris[i as usize]).collect();
    crossing_pairs(m.points, &live, constructed)
        .into_iter()
        .map(|(i, j)| (idx[i as usize], idx[j as usize]))
        .collect()
}

/// Merges `r` into `s` if they share an edge and that leaves a valid
/// surface with fewer crossing pairs around them.
fn collapse(m: &mut Mesh, grid: &mut Grid, all: &[u32], s: u32, r: u32) -> bool {
    let star_s = m.live_star(s);
    let star_r = m.live_star(r);
    if !star_r.iter().any(|t| m.tris[*t as usize].contains(&s)) {
        return false;
    }
    let mut old: Vec<(u32, [u32; 3])> = star_s
        .iter()
        .chain(&star_r)
        .map(|&t| (t, m.tris[t as usize]))
        .collect();
    old.sort_unstable();
    old.dedup();
    // The triangles on the edge go; the rest of r's move onto s.
    let mut new: Vec<(u32, [u32; 3])> = Vec::new();
    let mut gone: Vec<u32> = Vec::new();
    for &(t, tt) in &old {
        if tt.contains(&s) && tt.contains(&r) {
            gone.push(t);
        } else {
            new.push((t, tt.map(|x| if x == r { s } else { x })));
        }
    }
    if !valid_star(m, s, &new) {
        return false;
    }
    let before = m.pairs_of(&old, grid, all).len();
    for &t in &gone {
        m.alive[t as usize] = false;
    }
    if m.pairs_of(&new, grid, all).len() >= before {
        for &t in &gone {
            m.alive[t as usize] = true;
        }
        return false;
    }
    commit(m, grid, &new);
    true
}

/// Writes changed triangles, and lists them under their corners and in
/// the grid.
fn commit(m: &mut Mesh, grid: &mut Grid, new: &[(u32, [u32; 3])]) {
    for &(t, tt) in new {
        if m.tris[t as usize] != tt || !m.alive[t as usize] {
            m.tris[t as usize] = tt;
            m.alive[t as usize] = true;
            for v in tt {
                m.star[v as usize].push(t);
            }
            let b = m.bbox(tt);
            grid.insert(t, b);
        }
    }
}

/// The triangle as a needle: its corner `k` lies within `tol` of the line
/// through the other two, strictly between them (a candidate only; what
/// is done with it is decided exactly). With its height.
fn needle(m: &Mesh, t: [u32; 3], tol: f64) -> Option<(f64, usize)> {
    let p = m.pos(t);
    let mut best: Option<(f64, usize)> = None;
    for k in 0..3 {
        let (a, b, c) = (p[(k + 1) % 3], p[(k + 2) % 3], p[k]);
        let ab = b - a;
        let l2 = ab.length_squared();
        if l2 <= 0.0 || (c - a).dot(ab) <= 0.0 || (c - b).dot(-ab) <= 0.0 {
            continue;
        }
        let h2 = (c - a).cross(ab).length_squared() / l2;
        if h2 <= tol * tol && best.is_none_or(|(bh, _)| h2 < bh) {
            best = Some((h2, k));
        }
    }
    best
}

/// Splits the triangle across a needle's long edge at the needle's corner
/// `k`, dropping the needle, if that leaves a valid surface with fewer
/// crossing pairs there. The two halves lie in the neighbour's input
/// triangle.
fn split(m: &mut Mesh, src: &mut [usize], grid: &mut Grid, all: &[u32], n: u32, k: usize) -> bool {
    let t = m.tris[n as usize];
    let (c, a, b) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
    // The needle runs a -> b; its neighbour across uses b -> a.
    let Some(o) = m.live_star(a).into_iter().find(|&x| {
        let u = m.tris[x as usize];
        x != n && (0..3).any(|e| u[e] == b && u[(e + 1) % 3] == a)
    }) else {
        return false;
    };
    let u = m.tris[o as usize];
    let e = (0..3)
        .find(|&e| u[e] == b && u[(e + 1) % 3] == a)
        .unwrap_or(0);
    let d = u[(e + 2) % 3];
    if d == c {
        return false;
    }
    // (b, a, d) at c: (b, c, d) and (c, a, d).
    let new = [(o, [b, c, d]), (n, [c, a, d])];
    // c -> d must be new: used once each way after.
    let star_c = m.live_star(c);
    if star_c.iter().any(|&x| {
        let w = m.tris[x as usize];
        w.contains(&d)
    }) {
        return false;
    }
    if new.iter().any(|&(_, w)| degenerate(m.pos(w))) {
        return false;
    }
    let old = [
        (n.min(o), m.tris[n.min(o) as usize]),
        (n.max(o), m.tris[n.max(o) as usize]),
    ];
    let before = m.pairs_of(&old, grid, all).len();
    let mut new_sorted = new;
    new_sorted.sort_unstable();
    if m.pairs_of(&new_sorted, grid, all).len() >= before {
        return false;
    }
    src[n as usize] = src[o as usize];
    commit(m, grid, &new);
    true
}

/// After a collapse onto `s`, its triangles `star` (the others unchanged):
/// none degenerate or doubled, and every edge at `s` used once each way.
fn valid_star(m: &Mesh, s: u32, star: &[(u32, [u32; 3])]) -> bool {
    let mut sets: Vec<[u32; 3]> = Vec::new();
    let mut uses: BTreeMap<u32, (u32, u32)> = BTreeMap::new();
    for &(_, t) in star {
        if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] || degenerate(m.pos(t)) {
            return false;
        }
        let mut k = t;
        k.sort_unstable();
        if sets.contains(&k) {
            return false;
        }
        sets.push(k);
        for e in 0..3 {
            let (a, b) = (t[e], t[(e + 1) % 3]);
            if a == s {
                uses.entry(b).or_default().0 += 1;
            } else if b == s {
                uses.entry(a).or_default().1 += 1;
            }
        }
    }
    uses.values().all(|&(f, b)| f == 1 && b == 1)
}

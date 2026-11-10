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
//! - A collapse is refused when it would make the surface pass through
//!   itself: each triangle it rewrites is tested, exactly
//!   (`shape_bool::selfcheck::crossing`), against the triangles near it,
//!   found in a spatial hash over the triangles' boxes ([`Grid`]), and
//!   against the other rewritten ones. On an input that already crosses
//!   itself, a collapse that keeps a crossing is taken only when it adds
//!   no crossing partner and no pair: the count never rises.
//! - Collapses are taken cheapest first; ties go to the smaller edge key,
//!   then the smaller vertex. The order depends only on the input; a
//!   refused collapse is only skipped, so where none would fold the
//!   result is the one without the test.
//! - When the target is not reached, the report says why: each edge left
//!   is counted under the first rule that refuses its cheapest collapse
//!   ([`Blocked`]).
//!
//! Skins are carried: each collapse gives the kept vertex the blend of
//! the two ends' weights by where it lands (`Weights::blend`: its own
//! weights at its own position, half and half at the midpoint). Bone
//! borders are kept by a penalty, not a ban: removing a vertex costs, on
//! top of its quadric error, the error of moving the surface by its
//! largest weight difference to any neighbour (on any bone) times 0.5 %
//! of the bounding-box diagonal. A joint so keeps the rows of vertices it
//! bends with while the triangle target is still met (a ban cannot meet
//! it). The reported `error` leaves the penalty out.
//! `Decimate::cross_bone_borders` drops the penalty; the cost of keeping
//! borders, in surface error at the same triangle count, is measured in
//! `docs/research.md` §11.5.
//!
//! Names: every triangle left is an input triangle with corners moved, so
//! it keeps its surface and face names. Edge names follow their vertices
//! and survive on edges that still exist.
//!
//! UVs (`Solid::uvs`, per triangle corner) are carried. A UV seam (an
//! edge whose two triangles give either end different UVs) is a feature
//! line like a surface border, so a seam vertex only slides along its
//! seam and the seam stays where it was. A collapse gives each corner of
//! the removed vertex the UV the kept vertex has on the same side of any
//! seam (matched through the edge's own triangles); a collapse to the
//! midpoint gives both ends' corners the mean of their UVs. A collapse
//! whose corners cannot be matched (more than one UV side on one end, no
//! seam to say which) or that would turn a UV triangle over or flatten it
//! (`orient2d`, exact) is refused, as a turned-over face is.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};
use std::fmt;

use shape_bool::selfcheck::{crossing, degenerate};
use shape_core::predicates::orient2d;
use shape_core::{DVec2, DVec3};
use shape_mesh::{EdgeKey, Skin, Solid, Weights, edge_key};

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
    /// On a skinned solid, collapse across bone-weight borders as freely as
    /// anywhere (off by default: removing a vertex whose weights differ
    /// from a neighbour's costs extra, so joints keep their rows).
    pub cross_bone_borders: bool,
    /// On a solid with UVs: UV islands that meet along an edge and agree
    /// there up to a rigid move in UV space (a rotation and a translation,
    /// not a mirror), within this distance in UV units, are one chart for
    /// the seam rule ([`merge_charts`]): their shared seam is not a
    /// feature line, and a corner across it gets its UV through the move.
    /// The texture is not rewritten. 0: off, every seam stays.
    pub merge_charts: f64,
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
    /// UV islands (triangles joined across edges whose two sides give
    /// both ends the same UVs), and the charts they make with
    /// `merge_charts` (the same count without it). 0 without UVs.
    pub uv_islands: usize,
    /// See `uv_islands`.
    pub uv_charts: usize,
    /// Collapses not taken because the surface would have passed through
    /// itself (each attempt counts; an edge can be tried again after its
    /// neighbourhood changes).
    pub folds_refused: usize,
    /// When `triangles` is above `target`: the edges left, each counted
    /// under the rule that refuses its cheapest collapse. All zero when
    /// the target was reached.
    pub blocked: Blocked,
}

/// Why the edges left after a decimation that missed its target cannot
/// collapse: each under the first rule that refuses its cheapest
/// collapse, in this order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Blocked {
    /// Neither end may move: corners, or a border or feature line (a
    /// surface or name border, a named edge, a UV seam) the edge does not
    /// run along; or longer than the short-edge limit.
    pub fixed: usize,
    /// Beyond `max_error`.
    pub error: usize,
    /// The ends share a neighbour off the edge's triangles: collapsing
    /// would pinch the surface.
    pub link: usize,
    /// A triangle would turn over or lose its area.
    pub turn: usize,
    /// The UVs cannot follow: a corner's side of a UV seam is unclear, or
    /// a UV triangle would turn over or flatten.
    pub uv: usize,
    /// The surface would pass through itself.
    pub fold: usize,
    /// Collapsible now, but not tried again: its last try was refused,
    /// and what refused it changed in a collapse that re-planned only the
    /// edges at the vertex it kept.
    pub not_retried: usize,
}

/// Why a decimation could not run.
#[derive(Clone, Debug, PartialEq)]
pub enum DecimateError {
    /// Neither a ratio nor a target was given, or a value is out of range.
    BadDesc(String),
    /// A position is NaN or infinite.
    NotFinite,
    /// No longer returned: skins are carried (phase 5b part 2). Kept so
    /// that code matching on it still compiles.
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
    if !(d.merge_charts >= 0.0 && d.merge_charts.is_finite()) {
        return Err(DecimateError::BadDesc(
            "merge_charts must be finite and >= 0".into(),
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

/// The largest difference between two vertices' weights on any bone.
fn weight_step(a: &Weights, b: &Weights) -> f64 {
    let mut d: f64 = 0.0;
    for (bone, x) in a.pairs() {
        let y = b.pairs().find(|p| p.0 == bone).map_or(0.0, |p| p.1);
        d = d.max((x - y).abs());
    }
    for (bone, y) in b.pairs() {
        if !a.pairs().any(|p| p.0 == bone) {
            d = d.max(y);
        }
    }
    d
}

/// The bend scale, as a fraction of the solid's bounding-box diagonal: a
/// collapse whose removed vertex differs from a neighbour's weights by `Δ`
/// on some bone costs as if it moved the surface by `Δ ×` this. The
/// smallest of the scales swept on the acceptance meshes (0.005 to 0.1,
/// `docs/research.md` §11.5): all keep a bending joint exact, and larger
/// ones cost more surface error elsewhere.
const BEND_FRACTION: f64 = 0.005;

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
    /// The part of `cost` that is the skin's (weights changed).
    penalty: f64,
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
    /// Per vertex, its bone weights, on a skinned solid.
    weights: Option<Vec<Weights>>,
    /// Collapses that change weights cost extra.
    keep_borders: bool,
    /// The bend scale, mm.
    bend: f64,
    /// Per triangle, its corners' UVs, on a solid with UVs.
    uvs: Option<Vec<[DVec2; 3]>>,
    /// Per triangle, its chart and the move from its UVs to the chart's
    /// frame, when charts are merged; and the tolerance.
    charts: Option<(Vec<u32>, Vec<Rigid>, f64)>,
}

/// A corner's new UV: triangle, corner, UV.
type UvMove = (u32, usize, DVec2);

/// Why a collapse is refused, besides a fold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refusal {
    Link,
    Turn,
    Uv,
}

/// A rigid move of the UV plane: `p ↦ R p + t`, `R` the rotation with
/// cosine `c` and sine `s`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rigid {
    /// The rotation's cosine.
    pub c: f64,
    /// The rotation's sine.
    pub s: f64,
    /// The translation.
    pub t: DVec2,
}

impl Rigid {
    /// The identity.
    pub const IDENTITY: Rigid = Rigid {
        c: 1.0,
        s: 0.0,
        t: DVec2::ZERO,
    };

    fn rotate(&self, p: DVec2) -> DVec2 {
        DVec2::new(self.c * p.x - self.s * p.y, self.s * p.x + self.c * p.y)
    }

    /// The image of `p`.
    pub fn apply(&self, p: DVec2) -> DVec2 {
        self.rotate(p) + self.t
    }

    /// The point whose image is `q`.
    pub fn unapply(&self, q: DVec2) -> DVec2 {
        let d = q - self.t;
        DVec2::new(self.c * d.x + self.s * d.y, -self.s * d.x + self.c * d.y)
    }

    /// `self ∘ o`: `o` first.
    fn after(&self, o: &Rigid) -> Rigid {
        Rigid {
            c: self.c * o.c - self.s * o.s,
            s: self.s * o.c + self.c * o.s,
            t: self.rotate(o.t) + self.t,
        }
    }

    fn inverse(&self) -> Rigid {
        let r = Rigid {
            c: self.c,
            s: -self.s,
            t: DVec2::ZERO,
        };
        Rigid {
            t: -r.rotate(self.t),
            ..r
        }
    }

    /// The move taking `a0 → a1` and `b0 → b1`, if the two segments have
    /// the same length within `tol` (and are not points).
    fn between(a0: DVec2, b0: DVec2, a1: DVec2, b1: DVec2, tol: f64) -> Option<Rigid> {
        let (u, v) = (b0 - a0, b1 - a1);
        let (lu, lv) = (u.length(), v.length());
        if !(lu > 0.0 && lv > 0.0) || (lu - lv).abs() > tol {
            return None;
        }
        let r = Rigid {
            c: u.dot(v) / (lu * lv),
            s: u.perp_dot(v) / (lu * lv),
            t: DVec2::ZERO,
        };
        Some(Rigid {
            t: a1 - r.rotate(a0),
            ..r
        })
    }

    fn near(&self, o: &Rigid, tol: f64) -> bool {
        (self.c - o.c).abs() <= tol && (self.s - o.s).abs() <= tol && (self.t - o.t).length() <= tol
    }
}

/// The UV charts of `s` for the seam rule (`Decimate::merge_charts`):
/// per triangle, its chart and the rigid move from its UVs to its chart's
/// frame; and the number of islands. Islands are triangles joined across
/// edges whose two sides give both ends the same UVs. Two islands join
/// when every seam edge between them gives one rigid move from one's UVs
/// to the other's (a rotation and a translation, within `tol`: the edge's
/// two lengths agree, and the moves agree), and the move puts the two
/// triangles at each such edge on opposite sides of it (no mirror, no
/// overlap there). Pairs of islands are taken in order, each chart framed
/// by its lowest island; the result depends only on the input.
pub fn merge_charts(s: &Solid, tol: f64) -> Option<(Vec<u32>, Vec<Rigid>, usize)> {
    let uvs = s.uvs.as_ref()?;
    let n = s.triangles.len();
    let at = |t: usize, v: u32| {
        let k = s.triangles[t].iter().position(|&w| w == v).unwrap_or(0);
        uvs[t][k]
    };
    let mut users: BTreeMap<EdgeKey, Vec<u32>> = BTreeMap::new();
    for (i, t) in s.triangles.iter().enumerate() {
        for k in 0..3 {
            users
                .entry(edge_key(t[k], t[(k + 1) % 3]))
                .or_default()
                .push(i as u32);
        }
    }
    fn root(p: &mut [u32], mut i: u32) -> u32 {
        while p[i as usize] != i {
            p[i as usize] = p[p[i as usize] as usize];
            i = p[i as usize];
        }
        i
    }
    let mut parent: Vec<u32> = (0..n as u32).collect();
    let mut seams: Vec<(u32, u32, usize, usize)> = Vec::new();
    for (&(a, b), ts) in &users {
        if ts.len() != 2 {
            continue;
        }
        let (x, y) = (ts[0] as usize, ts[1] as usize);
        if at(x, a) == at(y, a) && at(x, b) == at(y, b) {
            let (rx, ry) = (root(&mut parent, x as u32), root(&mut parent, y as u32));
            if rx != ry {
                parent[rx.max(ry) as usize] = rx.min(ry);
            }
        } else {
            seams.push((a, b, x, y));
        }
    }
    let island: Vec<u32> = (0..n as u32).map(|t| root(&mut parent, t)).collect();
    let islands = (0..n).filter(|&t| island[t] == t as u32).count();
    // Per pair of islands (lower, higher): the move from the lower's UVs
    // to the higher's that each seam between them gives, if any.
    let mut between: BTreeMap<(u32, u32), Vec<Option<Rigid>>> = BTreeMap::new();
    for &(a, b, x, y) in &seams {
        let (ix, iy) = (island[x], island[y]);
        if ix == iy {
            continue;
        }
        let (x, y) = if ix < iy { (x, y) } else { (y, x) };
        let m = Rigid::between(at(x, a), at(x, b), at(y, a), at(y, b), tol).filter(|m| {
            // The two triangles on opposite sides of the edge after the
            // move.
            let third = |t: usize| {
                let tri = s.triangles[t];
                let v = tri[(0..3).find(|&k| tri[k] != a && tri[k] != b).unwrap_or(0)];
                at(t, v)
            };
            let (pa, pb) = (at(y, a), at(y, b));
            let sx = orient2d(pa, pb, m.apply(third(x)));
            let sy = orient2d(pa, pb, third(y));
            sx.is_ne() && sy.is_ne() && sx != sy
        });
        between.entry((ix.min(iy), ix.max(iy))).or_default().push(m);
    }
    // Charts: islands joined, each with its move into the chart's frame.
    let mut chart: Vec<u32> = (0..n as u32).collect();
    let mut frame: Vec<Rigid> = vec![Rigid::IDENTITY; n];
    let mut members: BTreeMap<u32, Vec<u32>> = (0..n as u32)
        .filter(|&t| island[t as usize] == t)
        .map(|t| (t, vec![t]))
        .collect();
    for (&(i, j), moves) in &between {
        let Some(Some(m)) = moves.first() else {
            continue;
        };
        if !moves.iter().all(|x| x.is_some_and(|x| x.near(m, tol))) {
            continue;
        }
        let (ci, cj) = (chart[i as usize], chart[j as usize]);
        if ci == cj {
            continue;
        }
        // Reframe the smaller chart into the larger's frame (ties: j's
        // into i's).
        let (keep, gone, to_keep) = if members[&cj].len() <= members[&ci].len() {
            // j-raw → chart i: frame[i] ∘ m⁻¹; then chart j → j-raw.
            let f = frame[i as usize]
                .after(&m.inverse())
                .after(&frame[j as usize].inverse());
            (ci, cj, f)
        } else {
            let f = frame[j as usize]
                .after(m)
                .after(&frame[i as usize].inverse());
            (cj, ci, f)
        };
        let moved = members.remove(&gone).unwrap_or_default();
        for &k in &moved {
            frame[k as usize] = to_keep.after(&frame[k as usize]);
            chart[k as usize] = keep;
        }
        members.entry(keep).or_default().extend(moved);
    }
    let tri_chart = (0..n).map(|t| chart[island[t] as usize]).collect();
    let tri_frame = (0..n).map(|t| frame[island[t] as usize]).collect();
    Some((tri_chart, tri_frame, islands))
}

/// A triangle's box.
fn bounds(t: [DVec3; 3]) -> (DVec3, DVec3) {
    (t[0].min(t[1]).min(t[2]), t[0].max(t[1]).max(t[2]))
}

/// A spatial hash over the live triangles' boxes: each triangle is listed
/// in the buckets of the cells its box covers (cells about twice a mean
/// triangle across); a triangle covering more than [`Grid::MAX_CELLS`]
/// cells is listed apart and returned by every query. Rebuilt with larger
/// cells each time the triangle count halves, so a cell keeps a few
/// triangles. Queries return candidates sorted, so nothing depends on
/// the order triangles came and went.
struct Grid {
    lo: DVec3,
    h: f64,
    buckets: Vec<Vec<u32>>,
    big: Vec<u32>,
    /// Live triangles when built.
    built: usize,
}

impl Grid {
    /// Cells a triangle may cover and still be listed in them.
    const MAX_CELLS: i64 = 512;
    /// Cells a query may visit before it scans every bucket instead.
    const MAX_QUERY: i64 = 8192;

    fn new(m: &Mesh) -> Grid {
        let live: Vec<u32> = (0..m.tris.len() as u32)
            .filter(|&t| m.alive[t as usize])
            .collect();
        let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
        let mut extent = 0.0;
        for &t in &live {
            let (a, b) = bounds(m.tris[t as usize].map(|v| m.pos[v as usize]));
            lo = lo.min(a);
            hi = hi.max(b);
            extent += (b - a).max_element();
        }
        let span = (hi - lo).max_element();
        let mut h = 2.0 * extent / live.len().max(1) as f64;
        // At most about 2^20 cells a side.
        if !(h > 0.0 && h.is_finite()) || span / h > (1u64 << 20) as f64 {
            h = (span / (1u64 << 20) as f64).max(f64::MIN_POSITIVE);
        }
        let mut g = Grid {
            lo: if lo.is_finite() { lo } else { DVec3::ZERO },
            h,
            buckets: vec![Vec::new(); (2 * live.len()).max(16).next_power_of_two()],
            big: Vec::new(),
            built: live.len(),
        };
        for t in live {
            g.insert(m, t);
        }
        g
    }

    fn cells(&self, lo: DVec3, hi: DVec3, max: i64) -> Option<([i64; 3], [i64; 3])> {
        let c = |p: DVec3| {
            let q = ((p - self.lo) / self.h).floor();
            [q.x as i64, q.y as i64, q.z as i64]
        };
        let (a, b) = (c(lo), c(hi));
        let n = (0..3).fold(1i64, |n, k| n.saturating_mul(b[k] - a[k] + 1));
        (n <= max).then_some((a, b))
    }

    fn bucket(&self, c: [i64; 3]) -> usize {
        let h = (c[0] as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (c[1] as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ (c[2] as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
        ((h ^ (h >> 29)) as usize) & (self.buckets.len() - 1)
    }

    fn each_bucket(&self, lo: DVec3, hi: DVec3, max: i64, mut f: impl FnMut(usize)) -> bool {
        let Some((a, b)) = self.cells(lo, hi, max) else {
            return false;
        };
        for x in a[0]..=b[0] {
            for y in a[1]..=b[1] {
                for z in a[2]..=b[2] {
                    f(self.bucket([x, y, z]));
                }
            }
        }
        true
    }

    fn insert(&mut self, m: &Mesh, t: u32) {
        let (lo, hi) = bounds(m.tris[t as usize].map(|v| m.pos[v as usize]));
        let mut at = Vec::new();
        if self.each_bucket(lo, hi, Self::MAX_CELLS, |b| at.push(b)) {
            for b in at {
                self.buckets[b].push(t);
            }
        } else {
            self.big.push(t);
        }
    }

    /// Takes `t` out, with the box it was inserted with.
    fn remove(&mut self, m: &Mesh, t: u32) {
        let (lo, hi) = bounds(m.tris[t as usize].map(|v| m.pos[v as usize]));
        let mut at = Vec::new();
        if self.each_bucket(lo, hi, Self::MAX_CELLS, |b| at.push(b)) {
            for b in at {
                self.buckets[b].retain(|&x| x != t);
            }
        } else {
            self.big.retain(|&x| x != t);
        }
    }

    /// The triangles listed near the box, sorted, each once.
    fn near(&self, lo: DVec3, hi: DVec3, out: &mut Vec<u32>) {
        let mut at = Vec::new();
        if self.each_bucket(lo, hi, Self::MAX_QUERY, |b| at.push(b)) {
            for b in at {
                out.extend(&self.buckets[b]);
            }
            out.extend(&self.big);
        } else {
            // A box over many cells: every triangle.
            for b in &self.buckets {
                out.extend(b);
            }
            out.extend(&self.big);
        }
        out.sort_unstable();
        out.dedup();
    }
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
        self.refusal(from, to, p).is_none()
    }

    /// Why the collapse `from → to` at `p` would break the mesh (not
    /// counting folds), if it would.
    fn refusal(&self, from: u32, to: u32, p: DVec3) -> Option<Refusal> {
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
            return Some(Refusal::Link);
        }
        if self.uv_moves(from, to, p).is_none() {
            return Some(Refusal::Uv);
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
                    return Some(Refusal::Turn);
                }
            }
        }
        None
    }

    /// The triangle's corners and positions after the collapse
    /// `from → to` at `p` (unchanged when it has neither).
    fn moved(&self, t: u32, from: u32, to: u32, p: DVec3) -> ([u32; 3], [DVec3; 3]) {
        let ids = self.tris[t as usize].map(|v| if v == from { to } else { v });
        let pos = ids.map(|v| if v == to { p } else { self.pos[v as usize] });
        (ids, pos)
    }

    /// The live triangles around `from` or `to`, sorted.
    fn fan(&self, from: u32, to: u32) -> Vec<u32> {
        let mut f: Vec<u32> = self.around[from as usize]
            .iter()
            .chain(&self.around[to as usize])
            .copied()
            .collect();
        f.sort_unstable();
        f.dedup();
        f
    }

    /// Crossing pairs of `tris` (ids and positions) with the live
    /// triangles near them outside `fan`, and among themselves: the pairs,
    /// as (index in `tris`, partner or `u32::MAX` for one of `tris`).
    /// Stops at the first when `first`.
    fn crossings(
        &self,
        grid: &Grid,
        tris: &[([u32; 3], [DVec3; 3])],
        fan: &[u32],
        first: bool,
    ) -> Vec<(usize, u32)> {
        let mut out = Vec::new();
        let boxes: Vec<(DVec3, DVec3)> = tris.iter().map(|t| bounds(t.1)).collect();
        let (lo, hi) = boxes
            .iter()
            .fold((DVec3::INFINITY, DVec3::NEG_INFINITY), |(l, h), b| {
                (l.min(b.0), h.max(b.1))
            });
        // One query for the whole fan; each candidate's box against each
        // triangle's.
        let mut near: Vec<(u32, [u32; 3], [DVec3; 3], (DVec3, DVec3))> = Vec::new();
        let mut ids: Vec<u32> = Vec::new();
        grid.near(lo, hi, &mut ids);
        for c in ids {
            if !self.alive[c as usize] || fan.binary_search(&c).is_ok() {
                continue;
            }
            let ib = self.tris[c as usize];
            let b = ib.map(|v| self.pos[v as usize]);
            let bb = bounds(b);
            if lo.cmple(bb.1).all() && bb.0.cmple(hi).all() && !degenerate(b) {
                near.push((c, ib, b, bb));
            }
        }
        for (i, &(ia, a)) in tris.iter().enumerate() {
            let (lo, hi) = boxes[i];
            for &(c, ib, b, (blo, bhi)) in &near {
                if lo.cmple(bhi).all() && blo.cmple(hi).all() && crossing(ia, a, ib, b) {
                    out.push((i, c));
                    if first {
                        return out;
                    }
                }
            }
            for &(ib, b) in &tris[i + 1..] {
                let (blo, bhi) = bounds(b);
                if lo.cmple(bhi).all() && blo.cmple(hi).all() && crossing(ia, a, ib, b) {
                    out.push((i, u32::MAX));
                    if first {
                        return out;
                    }
                }
            }
        }
        out
    }

    /// The collapse `from → to` at `p` would make the surface pass through
    /// itself: a rewritten triangle would be flat (exactly), or cross a
    /// triangle it did not cross before, or the fan would cross more
    /// pairs than it does now.
    fn folds(&self, grid: &Grid, from: u32, to: u32, p: DVec3) -> bool {
        let fan = self.fan(from, to);
        let after: Vec<([u32; 3], [DVec3; 3])> = fan
            .iter()
            .filter(|&&t| {
                let tri = self.tris[t as usize];
                !(tri.contains(&from) && tri.contains(&to))
            })
            .map(|&t| self.moved(t, from, to, p))
            .collect();
        if after.iter().any(|t| degenerate(t.1)) {
            return true;
        }
        if self.crossings(grid, &after, &fan, true).is_empty() {
            return false;
        }
        // The fan crosses something after: allowed only if it already did,
        // with the same partners and no more pairs.
        let before: Vec<([u32; 3], [DVec3; 3])> = fan
            .iter()
            .map(|&t| {
                let ids = self.tris[t as usize];
                (ids, ids.map(|v| self.pos[v as usize]))
            })
            .filter(|t| !degenerate(t.1))
            .collect();
        let was = self.crossings(grid, &before, &fan, false);
        let now = self.crossings(grid, &after, &fan, false);
        if now.len() > was.len() {
            return true;
        }
        let mut partners: Vec<u32> = was.iter().map(|x| x.1).collect();
        partners.sort_unstable();
        now.iter()
            .any(|x| x.1 != u32::MAX && partners.binary_search(&x.1).is_err())
    }

    /// The corners' new UVs for the collapse `from → to` at `p`: empty on
    /// a solid without UVs; `None` if they cannot be matched across a
    /// seam, or a UV triangle would turn over or flatten.
    fn uv_moves(&self, from: u32, to: u32, p: DVec3) -> Option<Vec<UvMove>> {
        let Some(uvs) = &self.uvs else {
            return Some(Vec::new());
        };
        let corner = |t: u32, v: u32| self.tris[t as usize].iter().position(|&x| x == v);
        // The edge's own triangles pair each side's UVs at the two ends.
        let mut pairs: Vec<(DVec2, DVec2)> = Vec::new();
        for &t in &self.around[from as usize] {
            if let (Some(kf), Some(kt)) = (corner(t, from), corner(t, to)) {
                let u = uvs[t as usize];
                pairs.push((u[kf], u[kt]));
            }
        }
        let midpoint = p != self.pos[to as usize];
        let mut moves: Vec<UvMove> = Vec::new();
        // One end's UV mapped to the other's, if exactly one pairing has
        // it; `Err` if two disagree.
        let other = |u: DVec2, from_end: bool| -> Result<Option<DVec2>, ()> {
            let mut found: Option<DVec2> = None;
            for &(a, b) in &pairs {
                let (mine, theirs) = if from_end { (a, b) } else { (b, a) };
                if mine == u {
                    if found.is_some_and(|f| f != theirs) {
                        return Err(());
                    }
                    found = Some(theirs);
                }
            }
            Ok(found)
        };
        // A corner across a merged seam: through the charts' frames, from
        // an edge triangle of the same chart whose corner at `v` is the
        // same point of the chart (within the tolerance). The corner's
        // new UV in the chart's frame: the other end's (or the midpoint),
        // moved back into the corner's own UVs.
        let across = |t: u32, u: DVec2, v: u32, w: u32| -> Option<DVec2> {
            let (chart, frame, tol) = self.charts.as_ref()?;
            let e = self.around[from as usize]
                .iter()
                .copied()
                .find(|&e| corner(e, to).is_some() && chart[e as usize] == chart[t as usize])?;
            let (fe, ft) = (frame[e as usize], frame[t as usize]);
            let uv = uvs[e as usize];
            let (mine, theirs) = (uv[corner(e, v)?], uv[corner(e, w)?]);
            if (fe.apply(mine) - ft.apply(u)).length() > *tol {
                return None;
            }
            let goal = if midpoint {
                (fe.apply(mine) + fe.apply(theirs)) * 0.5
            } else {
                fe.apply(theirs)
            };
            Some(ft.unapply(goal))
        };
        for (v, w, from_end) in [(from, to, true), (to, from, false)] {
            if !from_end && !midpoint {
                break;
            }
            for &t in &self.around[v as usize] {
                if corner(t, w).is_some() {
                    continue;
                }
                let k = corner(t, v)?;
                let u = uvs[t as usize][k];
                let new = match other(u, from_end).ok()? {
                    Some(o) => match (midpoint, from_end) {
                        (false, _) => o,
                        (true, true) => (u + o) * 0.5,
                        (true, false) => (o + u) * 0.5,
                    },
                    None => across(t, u, v, w)?,
                };
                moves.push((t, k, new));
            }
        }
        for &(t, k, new) in &moves {
            let before = uvs[t as usize];
            let mut after = before;
            after[k] = new;
            let (b, a) = (
                orient2d(before[0], before[1], before[2]),
                orient2d(after[0], after[1], after[2]),
            );
            if b.is_ne() && a != b {
                return None;
            }
        }
        Some(moves)
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
            // On a skinned solid: what changing the weights costs, as if the
            // surface moved by the weight change times the bend scale.
            let penalty = match (&self.weights, self.keep_borders) {
                (Some(w), true) => {
                    // Removing `from` changes how weights blend over its
                    // ring: the largest difference from it to a neighbour.
                    let wf = &w[from as usize];
                    let step = self
                        .neighbours(from)
                        .iter()
                        .map(|&n| weight_step(wf, &w[n as usize]))
                        .fold(0.0, f64::max);
                    let d = step * self.bend;
                    q.w * d * d
                }
                _ => 0.0,
            };
            for p in options {
                let e = Entry {
                    cost: q.eval(p) + penalty,
                    penalty,
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
        let moves = self.uv_moves(from, to, e.pos).unwrap_or_default();
        if let Some(uvs) = &mut self.uvs {
            for (t, k, new) in moves {
                uvs[t as usize][k] = new;
            }
        }
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
        if let Some(w) = &mut self.weights {
            // The kept vertex's weights by where it lands between the two:
            // its own at its own position, half and half at the midpoint.
            let (wt, wf) = (w[to as usize], w[from as usize]);
            if e.pos != self.pos[to as usize] {
                let mut items = [(wt, 0.5), (wf, 0.5)];
                items.sort_by(|a, b| a.0.cmp_total(&b.0));
                w[to as usize] = Weights::blend(&items);
            }
        }
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

    /// The collapse's error, mm, without the skin's penalty.
    fn error_of(&self, e: &Entry) -> f64 {
        let mut q = self.quadric[e.from as usize];
        q.add(&self.quadric[e.to as usize]);
        if q.w > 0.0 {
            ((e.cost - e.penalty).max(0.0) / q.w).sqrt()
        } else {
            0.0
        }
    }

    /// `a` and `b` are both still there and joined by an edge.
    fn joined(&self, a: u32, b: u32) -> bool {
        self.into[a as usize] == a
            && self.into[b as usize] == b
            && self.around[a as usize]
                .iter()
                .any(|&t| self.tris[t as usize].contains(&b))
    }

    /// Every live edge under the first rule that refuses its cheapest
    /// collapse.
    fn blocked(&self, grid: &Grid, d: &Decimate) -> Blocked {
        let mut edges: Vec<EdgeKey> = Vec::new();
        for (t, tri) in self.tris.iter().enumerate() {
            if self.alive[t] {
                for k in 0..3 {
                    edges.push(edge_key(tri[k], tri[(k + 1) % 3]));
                }
            }
        }
        edges.sort_unstable();
        edges.dedup();
        let mut b = Blocked::default();
        for (x, y) in edges {
            let Some(e) = self.plan(x, y) else {
                b.fixed += 1;
                continue;
            };
            if d.max_error > 0.0 && self.error_of(&e) > d.max_error {
                b.error += 1;
                continue;
            }
            match self.refusal(e.from, e.to, e.pos) {
                Some(Refusal::Link) => b.link += 1,
                Some(Refusal::Turn) => b.turn += 1,
                Some(Refusal::Uv) => b.uv += 1,
                None if self.folds(grid, e.from, e.to, e.pos) => b.fold += 1,
                None => b.not_retried += 1,
            }
        }
        b
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
    if s.positions.iter().any(|p| !p.is_finite()) {
        return Err(DecimateError::NotFinite);
    }
    let n = s.triangles.len();
    let mut report = DecimateReport {
        triangles_before: n,
        triangles: n,
        target,
        error: 0.0,
        folds_refused: 0,
        blocked: Blocked::default(),
        uv_islands: 0,
        uv_charts: 0,
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
        weights: s.skin.as_ref().map(|k| {
            let mut w = k.weights.clone();
            w.resize(nv, Weights::default());
            w
        }),
        keep_borders: !d.cross_bone_borders,
        bend: {
            let (lo, hi) = s.positions.iter().fold(
                (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
                |(l, h), p| (l.min(*p), h.max(*p)),
            );
            if s.positions.is_empty() {
                0.0
            } else {
                (hi - lo).length() * BEND_FRACTION
            }
        },
        uvs: s.uvs.clone(),
        charts: None,
    };
    if s.uvs.is_some() {
        let tol = d.merge_charts;
        if let Some((chart, frame, islands)) = merge_charts(s, tol) {
            report.uv_islands = islands;
            let mut roots: Vec<u32> = chart.clone();
            roots.sort_unstable();
            roots.dedup();
            report.uv_charts = if tol > 0.0 { roots.len() } else { islands };
            if tol > 0.0 {
                m.charts = Some((chart, frame, tol));
            }
        }
    }
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
            s.surfaces[x] != s.surfaces[y] || s.face_tags[x] != s.face_tags[y] || {
                // A UV seam: either end has different UVs on the two sides,
                // unless the two sides are one merged chart and agree in
                // its frame.
                s.uvs.as_ref().is_some_and(|u| {
                    let at = |t: usize, v: u32| {
                        let k = s.triangles[t].iter().position(|&w| w == v).unwrap_or(0);
                        u[t][k]
                    };
                    let differ = at(x, a) != at(y, a) || at(x, b) != at(y, b);
                    differ
                        && !m.charts.as_ref().is_some_and(|(chart, frame, tol)| {
                            chart[x] == chart[y]
                                && [a, b].iter().all(|&v| {
                                    (frame[x].apply(at(x, v)) - frame[y].apply(at(y, v))).length()
                                        <= *tol
                                })
                        })
                })
            }
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
    let mut grid = Grid::new(&m);
    // Edges refused for folding: whether they fold depends on triangles
    // that are not their neighbours, so they are tried again once the
    // queue runs dry, until a round takes none.
    let mut folded: Vec<EdgeKey> = Vec::new();
    let mut first = true;
    loop {
        let mut taken = 0usize;
        while m.live > target {
            let Some(e) = heap.pop() else { break };
            if (m.stamp[e.from as usize], m.stamp[e.to as usize]) != e.stamp {
                continue;
            }
            let err = m.error_of(&e);
            if d.max_error > 0.0 && err > d.max_error {
                continue;
            }
            if !m.valid(e.from, e.to, e.pos) {
                continue;
            }
            if m.folds(&grid, e.from, e.to, e.pos) {
                report.folds_refused += 1;
                folded.push(e.edge);
                continue;
            }
            let fan = m.fan(e.from, e.to);
            for &t in &fan {
                grid.remove(&m, t);
            }
            m.collapse(&e);
            for &t in &fan {
                if m.alive[t as usize] {
                    grid.insert(&m, t);
                }
            }
            if 2 * m.live < grid.built {
                grid = Grid::new(&m);
            }
            taken += 1;
            report.error = report.error.max(err);
            let to = e.to;
            for x in m.neighbours(to) {
                if let Some(next) = m.plan(to, x) {
                    heap.push(next);
                }
            }
        }
        if m.live <= target || folded.is_empty() || (!first && taken == 0) {
            break;
        }
        first = false;
        folded.sort_unstable();
        folded.dedup();
        for (a, b) in std::mem::take(&mut folded) {
            if m.joined(a, b)
                && let Some(e) = m.plan(a, b)
            {
                heap.push(e);
            }
        }
        if heap.is_empty() {
            break;
        }
    }
    if m.live > target {
        report.blocked = m.blocked(&grid, d);
    }
    let mut out = Solid {
        positions: m.pos.clone(),
        names: s.names.clone(),
        uvs: s.uvs.as_ref().map(|_| Vec::new()),
        skin: s.skin.as_ref().map(|k| Skin {
            bones: k.bones.clone(),
            weights: m.weights.clone().unwrap_or_default(),
        }),
        ..Default::default()
    };
    for (i, t) in m.tris.iter().enumerate() {
        if m.alive[i] {
            out.triangles.push(*t);
            out.surfaces.push(s.surfaces[i]);
            out.face_tags.push(s.face_tags[i].clone());
            if let (Some(to), Some(from)) = (&mut out.uvs, &m.uvs) {
                to.push(from[i]);
            }
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

    /// Crossing pairs, as `measure` counts them.
    fn crossings(s: &Solid) -> usize {
        let (p, t) = crate::measure::weld_exact(s);
        crate::intersect::self_intersections(&p, &t).len()
    }

    /// A hollow ball, its wall 1 % of the radius: decimated hard, the
    /// outer sphere's chords would cut through the inner one.
    fn hollow_ball() -> Solid {
        let mut shell = sphere(10.0).unwrap();
        let mut inner = sphere(9.9).unwrap();
        for i in 0..inner.triangles.len() {
            inner.reverse_triangle(i);
        }
        shell.append(&inner);
        shell.canonicalize().unwrap();
        shell
    }

    #[test]
    fn a_thin_shell_does_not_fold_through_itself() {
        let shell = hollow_ball();
        assert_eq!(crossings(&shell), 0);
        let (low, r) = decimate(
            &shell,
            &Decimate {
                target: 8,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(crossings(&low), 0, "{r:?}");
        assert!(low.measure().closed);
        assert!(r.folds_refused > 0, "{r:?}");
        // The target is not reachable without folding: the report says
        // how far it got and why.
        assert!(r.triangles > r.target && r.blocked.fold > 0, "{r:?}");
        assert_eq!(r.triangles, low.triangles.len());
    }

    #[test]
    fn a_shell_crossing_itself_crosses_no_more_after() {
        // The hollow ball with one outer vertex pushed through the inner
        // wall: a few crossings to start with.
        let mut shell = hollow_ball();
        let v = (0..shell.positions.len())
            .max_by(|&a, &b| shell.positions[a].x.total_cmp(&shell.positions[b].x))
            .unwrap();
        shell.positions[v] *= 0.97;
        let before = crossings(&shell);
        assert!(before > 0);
        let (low, r) = decimate(
            &shell,
            &Decimate {
                target: 100,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(crossings(&low) <= before, "{before} {r:?}");
    }

    #[test]
    fn islands_a_rigid_move_apart_are_one_chart() {
        // A flat 6 × 6 grid; its right half's UVs are its left half's
        // turned a quarter and moved: two islands, one rigid move apart
        // along the middle column.
        let mut g = Solid {
            uvs: Some(Vec::new()),
            ..Default::default()
        };
        for y in 0..=6 {
            for x in 0..=6 {
                g.push_vertex(DVec3::new(f64::from(x), f64::from(y), 0.0));
            }
        }
        let uv = |x: u32, y: u32, right: bool| {
            let (x, y) = (f64::from(x) * 0.125, f64::from(y) * 0.125);
            if right {
                DVec2::new(2.0 - y, x)
            } else {
                DVec2::new(x, y)
            }
        };
        for y in 0..6u32 {
            for x in 0..6u32 {
                let right = x >= 3;
                let v = |x: u32, y: u32| y * 7 + x;
                for t in [
                    [(x, y), (x + 1, y), (x + 1, y + 1)],
                    [(x, y), (x + 1, y + 1), (x, y + 1)],
                ] {
                    g.push_triangle_uv(
                        t.map(|(x, y)| v(x, y)),
                        0,
                        &["grid"],
                        t.map(|(x, y)| uv(x, y, right)),
                    );
                }
            }
        }
        g.canonicalize().unwrap();
        let run = |merge_charts: f64| {
            decimate(
                &g,
                &Decimate {
                    target: 2,
                    max_error: 1e-6,
                    merge_charts,
                    ..Default::default()
                },
            )
            .unwrap()
        };
        let (apart, a) = run(0.0);
        let (one, b) = run(1e-9);
        assert_eq!((a.uv_islands, a.uv_charts), (2, 2));
        assert_eq!((b.uv_islands, b.uv_charts), (2, 1));
        assert!(one.triangles.len() < apart.triangles.len(), "{a:?} {b:?}");
        // Every corner still lies on the texture as the grid put it: its
        // UV is the left half's mapping of its position, or the right
        // half's.
        let uvs = one.uvs.as_ref().unwrap();
        for (t, tri) in one.triangles.iter().enumerate() {
            for k in 0..3 {
                let p = one.positions[tri[k] as usize] * 0.125;
                let u = uvs[t][k];
                let left = (u - DVec2::new(p.x, p.y)).length();
                let right = (u - DVec2::new(2.0 - p.y, p.x)).length();
                assert!(left.min(right) < 1e-12, "{t} {k} {u:?} {p:?}");
            }
        }
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

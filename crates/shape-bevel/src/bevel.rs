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

use crate::patch::{self, Corner};
use crate::profile;
use crate::{BevelError, Miter};

#[path = "check.rs"]
mod check;
#[path = "cones.rs"]
mod cones;
#[path = "general.rs"]
mod general;
#[path = "runs.rs"]
mod runs;
#[path = "tips.rs"]
mod tips;
#[path = "wide.rs"]
mod wide;

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
#[derive(Clone)]
pub(crate) struct Spec {
    pub edges: Vec<EdgeKey>,
    pub offsets: Offsets,
    /// 0: from the radius (`shape_2d::curve::segments`).
    pub segments: u32,
    /// The superellipse exponent (2 a circle, 1 straight).
    pub exponent: f64,
    pub kind: Kind,
    pub miters: Miters,
    /// Even out each bevel side's width where its ends slide along
    /// unbevelled edges.
    pub loop_slide: bool,
    /// The checked input's triangles (`check.rs`); `None` until checked.
    pub checked: Option<std::sync::Arc<Vec<[DVec3; 3]>>>,
}

/// Miter kinds and their spread (0: the gap's offset).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Miters {
    pub inner: Miter,
    pub outer: Miter,
    pub spread: f64,
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
        // Each directed edge has one triangle and its twin another (the
        // solid is closed), so the walk is a cycle and comes back to
        // `start`; this guards that invariant.
        if f.tris.len() > here.len() {
            return Err(BevelError::NotClosed);
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
    miters: Miters,
    /// The cut bands of a wide bevel (`wide.rs`); `None` for every other.
    wide: Option<&'a wide::Wide>,
    /// How far a replaced corner may leave its triangle's plane, as a
    /// fraction of the largest offset.
    tilt: f64,
    /// Whether a fold (two triangles of one surface not in one plane)
    /// counts as a real edge (`docs/research.md` §7.8).
    folds: bool,
    /// The checked input's triangles, for the final check (`check.rs`).
    checked: Option<std::sync::Arc<Vec<[DVec3; 3]>>>,
}

impl Build<'_> {
    /// Whether spoke `i` of the fan is a real edge: the surface changes
    /// across it, or, with `folds`, its two triangles are not one plane.
    fn real(&self, f: &Fan, i: usize) -> bool {
        let k = f.tris.len();
        let (x, y) = (f.tris[(i + k - 1) % k], f.tris[i]);
        self.s.surfaces[x] != self.s.surfaces[y]
            || self.folds && tri_normal(self.s, x).dot(tri_normal(self.s, y)) <= PLANAR_DOT
    }

    /// Where a wide bevel's band was cut: the vertex the gap point is, when
    /// the point slides from `v` along spoke `i` and that spoke ends on the
    /// cut of the bevelled edge `key` (`docs/research.md` §7.6).
    fn snap(&self, f: &Fan, i: usize, key: EdgeKey) -> Option<u32> {
        let w = self.wide?;
        let end = f.spokes[i % f.spokes.len()];
        w.cuts.get(&key).filter(|c| c.contains(&end)).map(|_| end)
    }

    /// A wide bevel's profiles between the end chains of `key` (`cu` at
    /// its low end, `cw` at its high end, both from face `a` to face `b`):
    /// one at each cut point between, its other end on the straight side
    /// between the end chains' points there. Also the points to put on
    /// that side's face triangle, after the corner they follow.
    #[allow(clippy::type_complexity)]
    fn stations(
        &mut self,
        key: EdgeKey,
        st: &(bool, Vec<u32>),
        cu: &[u32],
        cw: &[u32],
    ) -> Result<(Vec<Vec<u32>>, ((usize, u32), Vec<u32>)), BevelError> {
        let (lo, hi) = key;
        let (pl, ph) = (self.s.positions[lo as usize], self.s.positions[hi as usize]);
        let &(side_a, ref mids) = st;
        let (u_other, w_other) = if side_a {
            (cu[cu.len() - 1], cw[cw.len() - 1])
        } else {
            (cu[0], cw[0])
        };
        let (bu, bw) = (self.pos[u_other as usize], self.pos[w_other as usize]);
        let mut profiles = Vec::new();
        let mut others = Vec::new();
        let mut last = 0.0;
        for &c in mids {
            let t = (self.pos[c as usize] - pl).dot(ph - pl) / (ph - pl).length_squared();
            if !(t > last && t < 1.0) {
                return Err(BevelError::TooLarge(pl));
            }
            last = t;
            let other = self.push(bu + (bw - bu) * t);
            let corner = pl + (ph - pl) * t;
            profiles.push(if side_a {
                self.chain(c, other, corner)
            } else {
                self.chain(other, c, corner)
            });
            others.push(other);
        }
        // The other side's face triangle holds the edge the other way
        // round; its corner before the points is `hi` for face `b`.
        let at = if side_a {
            others.reverse();
            (self.topo.dir[&(hi, lo)], hi)
        } else {
            (self.topo.dir[&(lo, hi)], lo)
        };
        Ok((profiles, (at, others)))
    }

    /// The point slid along unbevelled spoke `i`: the cut vertex there, or
    /// a new one at `p`.
    fn slid(&mut self, f: &Fan, i: usize, key: EdgeKey, p: DVec3) -> u32 {
        match self.snap(f, i, key) {
            Some(id) => id,
            None => self.push(p),
        }
    }

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
                    // A corner of zero angle: the offset lines are
                    // parallel on either side of one ray and meet nowhere,
                    // the limit of a corner too sharp for its offsets.
                    return Err(BevelError::TooLarge(p));
                }
                p + e2 * ((dl + dr) / 2.0)
            } else {
                p + l * ((dr + rx * dl) / ry) + e2 * dl
            }
        } else {
            let sin = |a: DVec3, b: DVec3| a.cross(b).length();
            let in_line = sin(l, r) < COLLINEAR_SIN;
            if in_line && l.dot(r) > 0.0 {
                // Zero angle, as on a plane.
                return Err(BevelError::TooLarge(p));
            }
            if (dl == 0.0 || dr == 0.0) && !in_line {
                // Slide along the unbevelled edge.
                let (along, d) = if dl == 0.0 { (l, dr) } else { (r, dl) };
                p + along * (d / sin(l, r))
            } else {
                // Slide along the inner edge most across the two edges,
                // until the point's distances from them sum to the two
                // offsets.
                // Also where an unbevelled edge is in line with the
                // bevelled one, whose offset line it never meets: the
                // point then steps straight across by half the offset, as
                // collinear edges on a plane do (`docs/research.md` §7.5).
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
        if self.needs_general(v)? {
            return self.general(v);
        }
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
        let real: Vec<bool> = (0..k).map(|i| self.real(&f, i)).collect();
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
                    let id = match (self.snap(&f, u, el), self.snap(&f, u, er)) {
                        (Some(a), _) | (None, Some(a)) => a,
                        (None, None) => self.push((p1 + p2) / 2.0),
                    };
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
                    let ia = self.slid(&f, u1, el, a);
                    let ib = self.slid(&f, u2, er, b);
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
            octant: true,
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
///
/// The first ear found is cut each time. Where that leaves a triangle of
/// zero area in floating point (three points of a straight chamfer
/// profile, counter-clockwise only by the exact predicate), the polygon
/// is clipped again cutting the best-shaped ear each time
/// (`docs/research.md` §7.5); results without such a triangle are the
/// first pass's.
fn clip(poly: &[u32], pos: &[DVec3], n: DVec3) -> Option<Vec<[u32; 3]>> {
    let first = clip_with(poly, pos, n, false)?;
    let flat = |t: &[u32; 3]| flat(t, pos);
    if !first.iter().any(flat) {
        return Some(first);
    }
    clip_with(poly, pos, n, true).filter(|ts| !ts.iter().any(flat))
}

/// Zero area as [`Solid::measure`] will find it: the canonical solid
/// starts each triangle at its first vertex in position order, and the
/// rounding of the area depends on the corner it starts from. Two equal
/// positions (welded into one) count as zero area too.
fn flat(t: &[u32; 3], pos: &[DVec3]) -> bool {
    use shape_core::order::cmp_dvec3;
    let p = t.map(|v| pos[v as usize] + DVec3::ZERO);
    let mut r = 0;
    for k in 1..3 {
        if cmp_dvec3(p[k], p[r]).is_lt() {
            r = k;
        }
    }
    let (a, b, c) = (p[r], p[(r + 1) % 3], p[(r + 2) % 3]);
    cmp_dvec3(a, b).is_eq()
        || cmp_dvec3(b, c).is_eq()
        || cmp_dvec3(c, a).is_eq()
        || (b - a).cross(c - a).length() == 0.0
}

fn clip_with(poly: &[u32], pos: &[DVec3], n: DVec3, best: bool) -> Option<Vec<[u32; 3]>> {
    use core::cmp::Ordering::Greater;
    let proj = projector(n);
    let mut ring: Vec<u32> = poly.to_vec();
    ring.dedup();
    while ring.len() > 1 && ring.first() == ring.last() {
        ring.pop();
    }
    let p2 = |i: u32| proj(pos[i as usize]);
    let flat = |t: &[u32; 3]| flat(t, pos);
    let mut out = Vec::new();
    while ring.len() > 3 {
        let len = ring.len();
        let is_ear = |i: usize| {
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
        };
        let ear = if best {
            // Never a flat ear while another fits; first an ear beside a
            // flat vertex, which then gets a new neighbour (otherwise the
            // flat vertex and its neighbours end as the last triangle);
            // then the most nearly equilateral (twice its area over the
            // sum of its squared sides); then the first by index.
            let at = |i: usize| [ring[(i + len - 1) % len], ring[i], ring[(i + 1) % len]];
            let shape = |i: usize| {
                let [a, b, c] = at(i).map(|v| pos[v as usize]);
                let sides =
                    (b - a).length_squared() + (c - b).length_squared() + (a - c).length_squared();
                (b - a).cross(c - a).length() / sides
            };
            let mut found: Option<((bool, bool, f64), usize)> = None;
            for i in (0..len).filter(|&i| is_ear(i)) {
                let beside = flat(&at((i + len - 1) % len)) || flat(&at((i + 1) % len));
                let q = (!flat(&at(i)), beside, shape(i));
                let better = found.is_none_or(|(f, _)| {
                    (q.0, q.1)
                        .cmp(&(f.0, f.1))
                        .then(q.2.total_cmp(&f.2))
                        .is_gt()
                });
                if better {
                    found = Some((q, i));
                }
            }
            found.map(|(_, i)| i)
        } else {
            (0..len).find(|&i| is_ear(i))
        }?;
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

/// The triangles of `t`'s face that lie in its plane, reached across
/// shared edges, in index order.
fn planar_region(s: &Solid, topo: &Topology, t: usize) -> Vec<usize> {
    let n = tri_normal(s, t);
    let mut seen = BTreeSet::from([t]);
    let mut stack = vec![t];
    while let Some(x) = stack.pop() {
        let tri = s.triangles[x];
        for k in 0..3 {
            let Some(&y) = topo.dir.get(&(tri[(k + 1) % 3], tri[k])) else {
                continue;
            };
            if !seen.contains(&y)
                && s.surfaces[y] == s.surfaces[t]
                && tri_normal(s, y).dot(n) > PLANAR_DOT
            {
                seen.insert(y);
                stack.push(y);
            }
        }
    }
    seen.into_iter().collect()
}

/// The outline loops of a set of polygons sharing edges, or `None` when
/// a vertex is passed twice.
fn outline(polys: &[Vec<u32>]) -> Option<Vec<Vec<u32>>> {
    let mut edges: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    for poly in polys {
        let mut ring = poly.clone();
        ring.dedup();
        while ring.len() > 1 && ring.first() == ring.last() {
            ring.pop();
        }
        for k in 0..ring.len() {
            *edges
                .entry((ring[k], ring[(k + 1) % ring.len()]))
                .or_default() += 1;
        }
    }
    let mut next: BTreeMap<u32, u32> = BTreeMap::new();
    for (&(a, c), &count) in &edges {
        let back = edges.get(&(c, a)).copied().unwrap_or(0);
        if count > back && (count - back > 1 || next.insert(a, c).is_some()) {
            return None;
        }
    }
    let mut loops = Vec::new();
    let mut seen = BTreeSet::new();
    for &start in next.keys() {
        if seen.contains(&start) {
            continue;
        }
        let mut ring = vec![start];
        seen.insert(start);
        let mut cur = next[&start];
        while cur != start {
            if !seen.insert(cur) {
                return None;
            }
            ring.push(cur);
            cur = *next.get(&cur)?;
        }
        loops.push(ring);
    }
    Some(loops)
}

/// One ring from an outer loop and its holes, each hole joined to the
/// outer ring by a bridge to the nearest vertex it sees (exact
/// predicates), as in ear-cut hole elimination.
fn bridge(loops: Vec<Vec<u32>>, pos: &[DVec3], n: DVec3) -> Option<Vec<u32>> {
    use core::cmp::Ordering::{Greater, Less};
    let proj = projector(n);
    let p2 = |i: u32| proj(pos[i as usize]);
    let area = |r: &[u32]| {
        (0..r.len())
            .map(|k| p2(r[k]).perp_dot(p2(r[(k + 1) % r.len()])))
            .sum::<f64>()
    };
    let (outer, mut holes): (Vec<Vec<u32>>, Vec<Vec<u32>>) =
        loops.into_iter().partition(|r| area(r) > 0.0);
    let [mut ring] = <[Vec<u32>; 1]>::try_from(outer).ok()?;
    // Rightmost hole first.
    let right = |r: &[u32]| {
        let mut best = 0;
        for k in 1..r.len() {
            let (a, b) = (p2(r[k]), p2(r[best]));
            if a.x > b.x || (a.x == b.x && a.y < b.y) {
                best = k;
            }
        }
        best
    };
    holes.sort_by(|a, b| {
        let (pa, pb) = (p2(a[right(a)]), p2(b[right(b)]));
        pb.x.total_cmp(&pa.x).then(pa.y.total_cmp(&pb.y))
    });
    for (hi, hole) in holes.iter().enumerate() {
        let h = right(hole);
        let ph = p2(hole[h]);
        let mut cands: Vec<(f64, usize)> = ring
            .iter()
            .enumerate()
            .map(|(k, &o)| ((p2(o) - ph).length_squared(), k))
            .collect();
        cands.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let blocked = |po: shape_core::DVec2, r: &[u32]| {
            (0..r.len()).any(|k| {
                let (pa, pb) = (p2(r[k]), p2(r[(k + 1) % r.len()]));
                if pa == ph || pb == ph || pa == po || pb == po {
                    return false;
                }
                let o1 = predicates::orient2d(ph, po, pa);
                let o2 = predicates::orient2d(ph, po, pb);
                let o3 = predicates::orient2d(pa, pb, ph);
                let o4 = predicates::orient2d(pa, pb, po);
                ((o1 == Less && o2 == Greater) || (o1 == Greater && o2 == Less))
                    && ((o3 == Less && o4 == Greater) || (o3 == Greater && o4 == Less))
            })
        };
        // The bridge must leave the hole into the region, which lies to
        // the left of every outline edge, the holes' included.
        let len = hole.len();
        let (hp, hn) = (p2(hole[(h + len - 1) % len]), p2(hole[(h + 1) % len]));
        let k = cands.iter().map(|&(_, k)| k).find(|&k| {
            let po = p2(ring[k]);
            if po == ph {
                return false;
            }
            let (l1, l2) = (
                predicates::orient2d(hp, ph, po) == Greater,
                predicates::orient2d(ph, hn, po) == Greater,
            );
            let into = if predicates::orient2d(hp, ph, hn) == Greater {
                l1 && l2
            } else {
                l1 || l2
            };
            into && !blocked(po, &ring) && holes.iter().skip(hi).all(|r| !blocked(po, r))
        })?;
        let mut merged = ring[..=k].to_vec();
        merged.extend((0..=len).map(|j| hole[(h + j) % len]));
        merged.extend_from_slice(&ring[k..]);
        ring = merged;
    }
    Some(ring)
}

const CAPS: [&str; 4] = ["top", "bottom", "start", "end"];

fn is_cap(name: &str) -> bool {
    CAPS.contains(&name.rsplit('.').next().unwrap_or(name))
}

pub(crate) fn bevel(s: &Solid, spec: &Spec) -> Result<Solid, BevelError> {
    if let Some((solid, edges)) = input(s, spec)? {
        return bevel_input(
            &solid,
            &Spec {
                edges,
                checked: Some(std::sync::Arc::new(check::triangles(&solid))),
                ..spec.clone()
            },
        );
    }
    bevel_input(
        s,
        &Spec {
            checked: Some(std::sync::Arc::new(check::triangles(s))),
            ..spec.clone()
        },
    )
}

/// The input checked (`docs/research.md` §7.7): closed, every edge between
/// two faces, and not crossing itself. A solid that crosses itself only
/// where a boolean left points closer than 1e-9 of the offset (a sliver
/// lying across its neighbour) is welded there first, as a wide bevel's
/// pass 0 does; the welded solid comes back. Otherwise the result could
/// not be told apart from a bevel that crossed itself, so it is refused.
fn input(s: &Solid, spec: &Spec) -> Result<Option<(Solid, Vec<EdgeKey>)>, BevelError> {
    if !s.measure().closed {
        let mut count: BTreeMap<(u32, u32), usize> = BTreeMap::new();
        for t in &s.triangles {
            for k in 0..3 {
                *count.entry((t[k], t[(k + 1) % 3])).or_default() += 1;
            }
        }
        for (&(a, c), &n) in &count {
            let back = count.get(&(c, a)).copied().unwrap_or(0);
            if n > 1 || back > 1 {
                return Err(BevelError::NotManifold(
                    s.positions[a as usize],
                    s.positions[c as usize],
                ));
            }
        }
        return Err(BevelError::NotClosed);
    }
    let crossings = shape_bool::selfcheck::self_crossings(&s.positions, &s.triangles);
    let Some(&(i, _)) = crossings.first() else {
        return Ok(None);
    };
    let at = s.positions[s.triangles[i as usize][0] as usize];
    let topo = topology(s);
    let (info, _) = edge_offsets(s, &topo, spec)?;
    let largest = info.values().flat_map(|e| e.off).fold(0.0f64, f64::max);
    let (w, edges) =
        wide::weld(s, &spec.edges, 1e-9 * largest).ok_or(BevelError::SelfCrossing(at))?;
    if let Some(&(i, _)) = shape_bool::selfcheck::self_crossings(&w.positions, &w.triangles).first()
    {
        return Err(BevelError::SelfCrossing(
            w.positions[w.triangles[i as usize][0] as usize],
        ));
    }
    Ok(Some((w, edges)))
}

fn bevel_input(s: &Solid, spec: &Spec) -> Result<Solid, BevelError> {
    // A vertex where two cones of faces touch is split into one per cone.
    if let Some((solid, edges)) = cones::split(s, &spec.edges) {
        return bevel_runs(
            &solid,
            &Spec {
                edges,
                ..spec.clone()
            },
        );
    }
    bevel_runs(s, spec)
}

/// Why one layout attempt failed: an error, or a result that would tilt
/// old faces off their planes (`docs/research.md` §7.7), with the vertex
/// and the bands (bevelled edge, its face `a`) whose faces it tilts.
pub(crate) enum Fail {
    Error(BevelError),
    Tilted(DVec3, BTreeSet<(EdgeKey, bool)>),
}

impl From<BevelError> for Fail {
    fn from(e: BevelError) -> Self {
        Fail::Error(e)
    }
}

/// How far a rebuilt triangle's corner may leave the triangle's plane, as
/// a fraction of the largest offset: rounding only, for the faces as
/// given (the classic layout keeps every corner it replaces in its
/// triangle's plane, to 1.5e-11 in the scan) ...
pub(crate) const TILT_EXACT: f64 = 1e-9;
/// ... and a tenth where a wide bevel's pass 1 or 2 already moved a
/// vertex, by up to that (`wide.rs`).
pub(crate) const TILT_PASS1: f64 = 0.1;

const TILTED: &str = "a bevel along a face whose triangles at this vertex are not in one plane (a twisted or converging face), where the bevel would tilt them";

fn bevel_runs(s: &Solid, spec: &Spec) -> Result<Solid, BevelError> {
    let (at, tilted) = match bevel_once(s, spec, None, TILT_EXACT) {
        Ok(out) => return Ok(out),
        Err(Fail::Error(BevelError::TooLarge(at))) => (at, false),
        Err(Fail::Tilted(at, _)) => (at, true),
        Err(Fail::Error(e)) => return Err(e),
    };
    {
        {
            // Straight runs may be cut into pieces shorter than the bevel.
            let dissolved = runs::dissolve(s, &spec.edges);
            if let Some((solid, edges)) = &dissolved {
                let spec = Spec {
                    edges: edges.clone(),
                    ..spec.clone()
                };
                if let Ok(out) = bevel_once(solid, &spec, None, TILT_EXACT) {
                    return Ok(out);
                }
            }
            // A bevel wider than the faces' own segments.
            let first = wide::bevel(s, spec);
            if first.is_ok() {
                return first;
            }
            if let Some((solid, edges)) = &dissolved {
                let spec = Spec {
                    edges: edges.clone(),
                    ..spec.clone()
                };
                if let Ok(out) = wide::bevel(solid, &spec) {
                    return Ok(out);
                }
            }
            if tilted {
                // Cut every corner on its folds, in each triangle's plane.
                match layout(s, spec, None, TILT_EXACT, true) {
                    Ok(out) => Ok(out),
                    Err(Fail::Error(e @ BevelError::LonePiece(..))) => Err(e),
                    Err(_) => {
                        Err(tips::lone_piece(s, spec)
                            .unwrap_or(BevelError::Unsupported(at, TILTED)))
                    }
                }
            } else {
                Err(tips::lone_piece(s, spec).unwrap_or(BevelError::TooLarge(at)))
            }
        }
    }
}

/// Per selected edge, its offsets; and the segment count.
fn edge_offsets(
    s: &Solid,
    topo: &Topology,
    spec: &Spec,
) -> Result<(BTreeMap<EdgeKey, EdgeInfo>, u32), BevelError> {
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
    Ok((edges, n))
}

/// One layout attempt; `tilt` as for [`TILT_EXACT`].
pub(crate) fn bevel_once(
    s: &Solid,
    spec: &Spec,
    wide: Option<&wide::Wide>,
    tilt: f64,
) -> Result<Solid, Fail> {
    layout(s, spec, wide, tilt, false)
}

/// One layout attempt; with `folds`, every fold of a face counts as a
/// real edge, so a corner on a face whose triangles at it are not one
/// plane is cut in each triangle's own plane (`docs/research.md` §7.8).
fn layout(
    s: &Solid,
    spec: &Spec,
    wide: Option<&wide::Wide>,
    tilt: f64,
    folds: bool,
) -> Result<Solid, Fail> {
    if !s.measure().closed {
        return Err(BevelError::NotClosed.into());
    }
    let topo = topology(s);
    let (edges, n) = edge_offsets(s, &topo, spec)?;
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
        miters: spec.miters,
        wide,
        tilt,
        folds,
        checked: spec.checked.clone(),
    };
    let verts: BTreeSet<u32> = b.edges.keys().flat_map(|&(x, y)| [x, y]).collect();
    if spec.loop_slide {
        b.slide(&verts)?;
    }
    for &v in &verts {
        b.vertex(v)?;
    }
    // Strips.
    let mut strip_of: BTreeMap<EdgeKey, usize> = BTreeMap::new();
    let mut inserts: BTreeMap<(usize, u32), Vec<u32>> = BTreeMap::new();
    let keys: Vec<EdgeKey> = b.edges.keys().copied().collect();
    for key in keys {
        let (u, w) = key;
        let cu = b.chains[&(u, w)].clone();
        let mut cw = b.chains[&(w, u)].clone();
        cw.reverse();
        if cu.len() != cw.len() {
            return Err(BevelError::Unsupported(s.positions[u as usize], "uneven profiles").into());
        }
        let stations = wide.and_then(|w| w.stations.get(&key));
        let point = |c: &[u32]| c.iter().all(|&x| x == c[0]);
        let mut profiles = vec![cu.clone()];
        if stations.is_none() && point(&cu) && point(&cw) {
            // Both ends taper to a tip (`general.rs`): the width grows
            // inside the edge (`tips.rs`, `docs/research.md` §7.8).
            let (mid, ends) = b.taper(key)?;
            profiles.extend(mid);
            for (at, pts) in ends {
                inserts.insert(at, pts);
            }
        } else if let Some(st) = stations {
            let (mid, insert) = b.stations(key, st, &cu, &cw)?;
            profiles.extend(mid);
            inserts.insert(insert.0, insert.1);
        }
        profiles.push(cw);
        let unit = b.unit();
        strip_of.insert(key, unit);
        for pair in profiles.windows(2) {
            let (cu, cw) = (&pair[0], &pair[1]);
            for k in 0..cu.len() - 1 {
                // A tip's profile is one point: skip the folded triangles.
                for t in [[cw[k], cu[k], cu[k + 1]], [cw[k], cu[k + 1], cw[k + 1]]] {
                    if t[0] != t[1] && t[1] != t[2] && t[2] != t[0] {
                        b.new_tris.push((t, unit));
                    }
                }
            }
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
    finish(b, &inserts)
}

fn finish(b: Build<'_>, inserts: &BTreeMap<(usize, u32), Vec<u32>>) -> Result<Solid, Fail> {
    let s = b.s;
    let kind = b.kind.name();
    let seq = |t: usize, x: u32| -> Vec<u32> { b.repl.get(&(t, x)).cloned().unwrap_or(vec![x]) };
    // A rebuilt triangle's outline: each corner's replacement, then the
    // points a wide bevel's strip put on the side after it.
    let poly = |t: usize| -> Vec<u32> {
        s.triangles[t]
            .iter()
            .flat_map(|&x| {
                let mut v = seq(t, x);
                v.extend(inserts.get(&(t, x)).into_iter().flatten());
                v
            })
            .collect()
    };
    let removed = |t: usize| b.wide.is_some_and(|w| w.removed.contains(&t));
    // The classic model keeps each old triangle and moves its corner at a
    // bevelled vertex. Where the faces there are not one plane (a twisted
    // quad's long triangle, a cone's fan at its apex), the new corner
    // leaves the triangle's plane and tilts the whole triangle, far from
    // the bevel: the result would be wrong without crossing itself
    // (`docs/research.md` §7.7). Such bands are cut instead (`wide.rs`).
    let largest = b.edges.values().flat_map(|e| e.off).fold(0.0f64, f64::max);
    let mut tilted: BTreeSet<(EdgeKey, bool)> = BTreeSet::new();
    let mut tilted_at = None;
    for (&(t, v), pts) in &b.repl {
        if removed(t) {
            continue;
        }
        let n = tri_normal(s, t);
        let p0 = s.positions[v as usize];
        // A vertex of the solid taken as the corner (a wide bevel's cut
        // point) is exact geometry; pass 2 lets a second band take the
        // first band's cut at a shared corner (`wide.rs`).
        let new = s.positions.len() as u32;
        if pts
            .iter()
            .all(|&q| q < new || (b.pos[q as usize] - p0).dot(n).abs() <= b.tilt * largest)
        {
            continue;
        }
        tilted_at.get_or_insert(p0);
        for &(lo, hi) in b.edges.keys().filter(|k| k.0 == v || k.1 == v) {
            for side_a in [true, false] {
                let dir = if side_a { (lo, hi) } else { (hi, lo) };
                if b.topo.dir.get(&dir).map(|&x| s.surfaces[x]) == Some(s.surfaces[t]) {
                    tilted.insert(((lo, hi), side_a));
                }
            }
        }
    }
    if let Some(at) = tilted_at {
        return Err(Fail::Tilted(at, tilted));
    }
    // Old faces, rebuilt triangle by triangle; where a triangle does not
    // fit (a sliver whose corners all moved), its whole planar face is
    // rebuilt from its outline instead.
    let n_old = s.triangles.len();
    let mut pieces: Vec<Option<Vec<[u32; 3]>>> = vec![None; n_old];
    let mut failed = Vec::new();
    for (i, t) in s.triangles.iter().enumerate() {
        if removed(i) {
            pieces[i] = Some(Vec::new());
            continue;
        }
        if !t
            .iter()
            .any(|&x| b.repl.contains_key(&(i, x)) || inserts.contains_key(&(i, x)))
        {
            continue;
        }
        let poly = poly(i);
        if b.wide.is_some() && wide::collapsed(&poly) {
            // A band triangle whose corners all landed on the cut.
            pieces[i] = Some(Vec::new());
            continue;
        }
        match clip(&poly, &b.pos, tri_normal(s, i)) {
            Some(p) => pieces[i] = Some(p),
            None => failed.push(i),
        }
    }
    let mut absorbed = vec![false; n_old];
    let mut regions: BTreeMap<usize, Vec<[u32; 3]>> = BTreeMap::new();
    for &i in &failed {
        if absorbed[i] {
            continue;
        }
        let t = s.triangles[i];
        let at = t
            .iter()
            .find(|&&x| b.repl.contains_key(&(i, x)))
            .map_or(DVec3::ZERO, |&x| s.positions[x as usize]);
        let mut region = planar_region(s, &b.topo, i);
        region.retain(|&r| !removed(r));
        let polys: Vec<Vec<u32>> = region.iter().map(|&r| poly(r)).collect();
        let nrm = tri_normal(s, i);
        let tris = outline(&polys)
            .and_then(|loops| bridge(loops, &b.pos, nrm))
            .and_then(|ring| clip(&ring, &b.pos, nrm))
            .ok_or(BevelError::TooLarge(at))?;
        for &r in &region {
            absorbed[r] = true;
        }
        regions.insert(region[0], tris);
    }
    let mut tris: Vec<([u32; 3], Option<usize>, usize)> = Vec::new(); // (tri, unit, old)
    for (i, t) in s.triangles.iter().enumerate() {
        if absorbed[i] {
            for &p in regions.get(&i).into_iter().flatten() {
                tris.push((p, None, i));
            }
            continue;
        }
        match &pieces[i] {
            None => tris.push((*t, None, i)),
            Some(ps) => {
                for &p in ps {
                    tris.push((p, None, i));
                }
            }
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
        return Err(BevelError::TooLarge(at).into());
    }
    // The backstop (`docs/research.md` §7.7): the input does not cross
    // itself, so two triangles of the result that cross are the bevel's
    // own, and it did not fit. The error names the bevelled vertex nearest
    // to where they cross.
    let first = match &b.checked {
        Some(input) => check::first_crossing(&out, input),
        None => shape_bool::selfcheck::self_crossings(&out.positions, &out.triangles)
            .first()
            .copied(),
    };
    if let Some((i, j)) = first {
        let c = [i, j]
            .iter()
            .flat_map(|&t| out.triangles[t as usize])
            .fold(DVec3::ZERO, |a, v| a + out.positions[v as usize])
            / 6.0;
        let mut at = (f64::INFINITY, c);
        for v in b.edges.keys().flat_map(|&(x, y)| [x, y]) {
            let p = s.positions[v as usize];
            let d = (p - c).length_squared();
            if d < at.0 {
                at = (d, p);
            }
        }
        return Err(BevelError::TooLarge(at.1).into());
    }
    Ok(out)
}

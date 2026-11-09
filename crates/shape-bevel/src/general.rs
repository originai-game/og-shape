//! Vertices the classic layout of `bevel.rs` does not cover
//! (`docs/research.md` §7.4): a gap with two or more unbevelled edges,
//! a bevelled edge that ends where its own two faces meet again, and arc
//! and patch miters.
//!
//! Each gap between bevelled edges gets a run of boundary points
//! `Q_0 … Q_r` (counter-clockwise, `Q_0` next to the bevelled edge before
//! the gap) instead of one. Consecutive points are joined by a **face
//! chain**: a profile of `n` segments lying on the gap's faces, around a
//! corner of its own. The triangles of the gap take the run, each the
//! piece in front of it. The corner hole is then bounded by the edge
//! chains and the face chains, `m' = m + Σr` sides, and is filled by the
//! corner patch of `patch.rs` like any vertex of `m'` bevelled edges.
//!
//! - **Two or more unbevelled edges** in a gap (Blender's Full Vertex
//!   Bevel): one point slides along each unbevelled edge; the first and
//!   last where the offset lines of the bevelled edges meet them, the
//!   others by distances evenly between. Each face between two
//!   unbevelled edges takes a chain around the vertex, so it is cut along
//!   a profile, as the end face of a terminal edge is.
//! - **A lone bevelled edge whose two faces meet again at its end**
//!   (one unbevelled edge, typically continuing it): the bevel tapers to
//!   a tip at the vertex itself. This is the design's choice (b) with a
//!   slide of zero; Blender slides the tip along the unbevelled edge by
//!   the offset, but that tilts every face triangle at the vertex, and on
//!   a faceted wall (a cylinder's) shaves a wedge off its whole height.
//!   The strip's last profile is a single point, so its last row is
//!   triangles.
//! - **Miters** on a planar gap with no unbevelled edge, at a vertex of
//!   two or more bevelled edges. The gap is **outer** when its angle at
//!   the vertex, summed over its faces, is more than 180°, **inner** when
//!   less. An **arc** miter splits the gap's point `P` in two, slid by the
//!   spread along each bevelled edge, joined by an arc around `P`, so the
//!   face's corner is rounded and the hole gains a side. A **patch**
//!   miter (outer gaps only) adds a third point between them, slid from
//!   `P` toward the vertex, joined by two arcs whose corners lie on the
//!   bevels' edge lines: the outline stays tangent to them and to itself,
//!   and the hole gains two sides. Unbevelled edges cannot be in such a
//!   gap; the face's diagonals take the arc points nearest them.

use shape_core::{DVec3, trig};
use shape_mesh::edge_key;

use super::{Build, Fan, PLANAR_DOT, fan, tri_normal};
use crate::patch::{self, Corner};
use crate::{BevelError, Miter};

/// The fan around `v`, turned so spoke 0 is bevelled; which spokes are
/// real; the bevelled spokes.
pub(super) struct Layout {
    pub f: Fan,
    pub real: Vec<bool>,
    pub bidx: Vec<usize>,
}

/// A run of boundary points over the triangles `lo..hi` of the fan,
/// counter-clockwise.
struct Span {
    lo: usize,
    hi: usize,
    pts: Vec<u32>,
}

/// One side of the corner hole: a chain from `a` to `b` (`b` is the
/// previous side's `a`). Edge sides name their spoke.
struct Side {
    a: u32,
    b: u32,
    spoke: Option<u32>,
    /// The face chain's points, `a → b`; empty for an edge side.
    chain: Vec<u32>,
    c: DVec3,
}

impl Build<'_> {
    pub(super) fn layout(&self, v: u32) -> Result<Layout, BevelError> {
        let s = self.s;
        let mut f = fan(s, &self.topo, v)?;
        let k = f.spokes.len();
        let bevelled = |f: &Fan, i: usize| self.edges.contains_key(&edge_key(v, f.spokes[i % k]));
        let r0 = (0..k).find(|&i| bevelled(&f, i)).unwrap_or(0);
        f.tris.rotate_left(r0);
        f.spokes.rotate_left(r0);
        let real: Vec<bool> = (0..k)
            .map(|i| s.surfaces[f.tris[(i + k - 1) % k]] != s.surfaces[f.tris[i]])
            .collect();
        let bidx: Vec<usize> = (0..k).filter(|&i| bevelled(&f, i)).collect();
        Ok(Layout { f, real, bidx })
    }

    /// The miter a gap `lo..hi` with no unbevelled edge takes, by its
    /// angle; sharp when its faces are not one plane.
    fn miter(&self, v: u32, f: &Fan, lo: usize, hi: usize) -> Miter {
        let (inner, outer) = (self.miters.inner, self.miters.outer);
        if inner == Miter::Sharp && outer == Miter::Sharp {
            return Miter::Sharp;
        }
        let s = self.s;
        let k = f.spokes.len();
        let p = s.positions[v as usize];
        let normals: Vec<DVec3> = (lo..hi).map(|i| tri_normal(s, f.tris[i])).collect();
        if !normals.iter().all(|n| n.dot(normals[0]) > PLANAR_DOT) {
            return Miter::Sharp;
        }
        let dir = |i: usize| (s.positions[f.spokes[i % k] as usize] - p).normalize_or_zero();
        let angle: f64 = (lo..hi)
            .map(|i| trig::acos(dir(i).dot(dir(i + 1)).clamp(-1.0, 1.0)))
            .sum();
        if angle > trig::PI + 1e-6 {
            outer
        } else if angle < trig::PI - 1e-6 {
            inner
        } else {
            Miter::Sharp
        }
    }

    /// Whether the classic layout cannot bevel `v`.
    pub(super) fn needs_general(&self, v: u32) -> Result<bool, BevelError> {
        let Layout { f, real, bidx } = self.layout(v)?;
        let (k, m) = (f.spokes.len(), bidx.len());
        for g in 0..m {
            let (lo, hi) = (bidx[g], if g + 1 < m { bidx[g + 1] } else { k });
            let inner = (lo + 1..hi).filter(|&i| real[i]).count();
            let general = match (m, inner) {
                (1, 2) => false,
                (1, _) => true,
                (_, 0) => self.miter(v, &f, lo, hi) != Miter::Sharp,
                (_, 1) => false,
                _ => true,
            };
            if general {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Gives each triangle of the span its piece of the run, split by the
    /// directions of the spokes inside the span.
    fn spread(&mut self, v: u32, f: &Fan, span: &Span) {
        let p = self.s.positions[v as usize];
        if span.pts == [v] {
            return;
        }
        if span.pts.len() == 1 {
            for &t in &f.tris[span.lo..span.hi] {
                self.repl.insert((t, v), vec![span.pts[0]]);
            }
            return;
        }
        let k = f.spokes.len();
        let mut split = vec![0usize];
        for j in span.lo + 1..span.hi {
            let d = (self.s.positions[f.spokes[j % k] as usize] - p).normalize_or_zero();
            let from = *split.last().unwrap_or(&0);
            let mut best = (f64::NEG_INFINITY, from);
            for (c, &id) in span.pts.iter().enumerate().skip(from) {
                let cos = (self.pos[id as usize] - p).normalize_or_zero().dot(d);
                if cos > best.0 {
                    best = (cos, c);
                }
            }
            split.push(best.1);
        }
        split.push(span.pts.len() - 1);
        for (w, i) in (span.lo..span.hi).enumerate() {
            let mut seq = span.pts[split[w]..=split[w + 1]].to_vec();
            seq.reverse();
            self.repl.insert((f.tris[i], v), seq);
        }
    }

    /// A face chain from `b` to `a` around `c`, as a counter-clockwise run
    /// (the chain `a → b` reversed), without its first point when `skip`.
    fn run(&mut self, a: u32, b: u32, c: DVec3, sides: &mut Vec<Side>) -> Vec<u32> {
        let chain = self.chain(a, b, c);
        let mut r = chain.clone();
        r.reverse();
        sides.push(Side {
            a,
            b,
            spoke: None,
            chain,
            c,
        });
        r
    }

    pub(super) fn general(&mut self, v: u32) -> Result<(), BevelError> {
        let s = self.s;
        let p = s.positions[v as usize];
        let Layout { f, real, bidx } = self.layout(v)?;
        let k = f.spokes.len();
        let m = bidx.len();
        let dir = |i: usize| (s.positions[f.spokes[i % k] as usize] - p).normalize_or_zero();
        let key_of = |i: usize| edge_key(v, f.spokes[i % k]);
        // Per gap: its run's ends and its face-chain sides, in order.
        let mut gaps: Vec<(u32, u32, Vec<Side>)> = Vec::with_capacity(m);
        for g in 0..m {
            let (lo, hi) = (bidx[g], if g + 1 < m { bidx[g + 1] } else { k });
            let inner: Vec<usize> = (lo + 1..hi).filter(|&i| real[i]).collect();
            let (el, er) = (key_of(lo), key_of(hi));
            let dl = self.edges[&el].after(el, v);
            let dr = self.edges[&er].before(er, v);
            let mut sides = Vec::new();
            let mut spans = Vec::new();
            match inner.len() {
                0 if m >= 2 => {
                    let b = self.wedge_point(v, &f, lo, hi, dl, dr)?;
                    let spread = if self.miters.spread > 0.0 {
                        self.miters.spread
                    } else {
                        (dl + dr) / 2.0
                    };
                    let miter = self.miter(v, &f, lo, hi);
                    let pts = match miter {
                        Miter::Sharp => vec![self.push(b)],
                        Miter::Arc => {
                            let a0 = self.push(b + dir(lo) * spread);
                            let a1 = self.push(b + dir(hi) * spread);
                            self.run(a1, a0, b, &mut sides)
                        }
                        Miter::Patch => {
                            // The middle point slides toward the vertex;
                            // each arc's corner lies on the bevel's edge
                            // line, where the line through the middle
                            // point square to its slide meets it, so the
                            // outline turns smoothly through all three.
                            let w = (p - b).normalize_or_zero();
                            let (k0, k1) = (dir(lo).dot(w), dir(hi).dot(w));
                            if !(k0 > 0.0 && k1 > 0.0) {
                                return Err(BevelError::Unsupported(
                                    p,
                                    "a patch miter on a corner that is not reflex",
                                ));
                            }
                            let mu = spread / 2.0 * k0.min(k1);
                            let (c0, c1) = (b + dir(lo) * (mu / k0), b + dir(hi) * (mu / k1));
                            let a0 = self.push(b + dir(lo) * spread);
                            let mid = self.push(b + w * mu);
                            let a1 = self.push(b + dir(hi) * spread);
                            let mut r = self.run(mid, a0, c0, &mut sides);
                            let r2 = self.run(a1, mid, c1, &mut sides);
                            r.extend_from_slice(&r2[1..]);
                            r
                        }
                    };
                    spans.push(Span { lo, hi, pts });
                }
                1 if m >= 2 => {
                    let u = inner[0];
                    let p1 = self.wedge_point(v, &f, lo, u, dl, 0.0)?;
                    let p2 = self.wedge_point(v, &f, u, hi, 0.0, dr)?;
                    let id = self.push((p1 + p2) / 2.0);
                    spans.push(Span {
                        lo,
                        hi,
                        pts: vec![id],
                    });
                }
                1 => {
                    // A lone edge whose faces meet again: the bevel
                    // tapers to a tip at the vertex, and the faces past
                    // it keep their corners.
                    spans.push(Span {
                        lo,
                        hi,
                        pts: vec![v],
                    });
                }
                0 => {
                    return Err(BevelError::Unsupported(
                        p,
                        "a bevelled edge that is the only real edge at its vertex",
                    ));
                }
                c => {
                    let (u1, uk) = (inner[0], inner[c - 1]);
                    let x1 = self.wedge_point(v, &f, lo, u1, dl, 0.0)?;
                    let xk = self.wedge_point(v, &f, uk, hi, 0.0, dr)?;
                    let (t1, tk) = ((x1 - p).length(), (xk - p).length());
                    let mut ids = vec![self.push(x1)];
                    for (w, &u) in inner.iter().enumerate().take(c - 1).skip(1) {
                        let t = t1 + (tk - t1) * (w as f64 / (c - 1) as f64);
                        ids.push(self.push(p + dir(u) * t));
                    }
                    ids.push(self.push(xk));
                    spans.push(Span {
                        lo,
                        hi: u1,
                        pts: vec![ids[0]],
                    });
                    for w in 0..c - 1 {
                        let pts = self.run(ids[w + 1], ids[w], p, &mut sides);
                        spans.push(Span {
                            lo: inner[w],
                            hi: inner[w + 1],
                            pts,
                        });
                    }
                    spans.push(Span {
                        lo: uk,
                        hi,
                        pts: vec![ids[c - 1]],
                    });
                }
            }
            let first = spans[0].pts[0];
            let last = *spans[spans.len() - 1].pts.last().unwrap_or(&first);
            for span in &spans {
                self.spread(v, &f, span);
            }
            gaps.push((first, last, sides));
        }
        // The sides of the hole, counter-clockwise: each bevelled edge,
        // then its gap's face chains.
        let mut sides: Vec<Side> = Vec::new();
        for j in 0..m {
            let (a, _, _) = gaps[j];
            let (_, b, _) = gaps[(j + m - 1) % m];
            sides.push(Side {
                a,
                b,
                spoke: Some(f.spokes[bidx[j]]),
                chain: Vec::new(),
                c: p,
            });
            sides.append(&mut gaps[j].2);
        }
        let mm = sides.len();
        if mm == 1 {
            // The tip: the strip's last profile is one point.
            let side = &sides[0];
            let spoke = side.spoke.unwrap_or(0);
            self.chains
                .insert((v, spoke), vec![side.a; self.n as usize + 1]);
            return Ok(());
        }
        if mm == 2 {
            // Not reached: two sides are the classic layouts.
            return Err(BevelError::Unsupported(p, "this vertex"));
        }
        // The octant only where every side is a bevelled edge: a face
        // chain keeps its own corner, which the octant's centre does not
        // fit, so such holes take the ADJ pattern.
        let octant = mm == 3 && sides.iter().all(|sd| sd.spoke.is_some());
        let o = if octant {
            p + sides
                .iter()
                .fold(DVec3::ZERO, |acc, sd| acc + (self.pos[sd.a as usize] - p))
                / 2.0
        } else {
            DVec3::ZERO
        };
        let mut ends = Vec::with_capacity(mm);
        let mut keys = Vec::new();
        for sd in &sides {
            let (a, b) = (self.pos[sd.a as usize], self.pos[sd.b as usize]);
            let (c, chain) = match sd.spoke {
                None => (sd.c, sd.chain.clone()),
                Some(w) => {
                    let c = if octant {
                        a + b - o
                    } else {
                        let l = (s.positions[w as usize] - p).normalize_or_zero();
                        p + l * (((a + b) / 2.0 - p).dot(l))
                    };
                    let chain = self.chain(sd.a, sd.b, c);
                    self.chains.insert((v, w), chain.clone());
                    keys.push(edge_key(v, w));
                    (c, chain)
                }
            };
            ends.push((a, b, c, chain));
        }
        let unit = self.unit();
        let corner = Corner {
            v: p,
            o,
            ends,
            n: self.n,
            octant,
        };
        let exponent = self.exponent;
        let mut samples = |n: u32| crate::profile::samples(exponent, n);
        let fill = patch::fill(&corner, exponent, &mut samples, &mut self.pos)?;
        for t in fill {
            self.new_tris.push((t, unit));
        }
        self.patches.push((unit, keys));
        Ok(())
    }
}

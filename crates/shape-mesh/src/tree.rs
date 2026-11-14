//! A bounding-box tree over a solid's triangles, for segment crossings
//! (exact sides by `orient3d`) and nearest points. Median splits along
//! the longest axis of the centroids, ties by index, so the tree and
//! every answer are the same on every host.

use core::cmp::Ordering;

use shape_core::DVec3;
use shape_core::predicates::orient3d;

use crate::Solid;

const LEAF: usize = 4;

struct Node {
    min: DVec3,
    max: DVec3,
    /// Triangles `order[lo..hi]` (a leaf), or children `kids`, `kids + 1`.
    lo: u32,
    hi: u32,
    kids: u32,
}

/// A bounding-box hierarchy over a solid's triangles: median splits along
/// the longest axis of the centroids (ties by index).
pub struct Tree<'a> {
    s: &'a Solid,
    order: Vec<u32>,
    nodes: Vec<Node>,
}

/// Triangle `i`'s corners.
pub fn tri(s: &Solid, i: u32) -> [DVec3; 3] {
    s.triangles[i as usize].map(|v| s.positions[v as usize])
}

impl<'a> Tree<'a> {
    /// The tree over every triangle of `s`.
    pub fn new(s: &'a Solid) -> Tree<'a> {
        let n = s.triangles.len();
        let cen: Vec<DVec3> = (0..n as u32)
            .map(|i| {
                let [a, b, c] = tri(s, i);
                (a + b + c) / 3.0
            })
            .collect();
        let mut order: Vec<u32> = (0..n as u32).collect();
        let mut nodes = vec![Node {
            min: DVec3::ZERO,
            max: DVec3::ZERO,
            lo: 0,
            hi: n as u32,
            kids: 0,
        }];
        let mut stack = vec![0usize];
        while let Some(k) = stack.pop() {
            let (lo, hi) = (nodes[k].lo as usize, nodes[k].hi as usize);
            let (mut mn, mut mx) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
            for &i in &order[lo..hi] {
                for p in tri(s, i) {
                    mn = mn.min(p);
                    mx = mx.max(p);
                }
            }
            nodes[k].min = mn;
            nodes[k].max = mx;
            if hi - lo <= LEAF {
                continue;
            }
            let (mut cmn, mut cmx) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
            for &i in &order[lo..hi] {
                cmn = cmn.min(cen[i as usize]);
                cmx = cmx.max(cen[i as usize]);
            }
            let e = cmx - cmn;
            let axis = if e.x >= e.y && e.x >= e.z {
                0
            } else if e.y >= e.z {
                1
            } else {
                2
            };
            shape_core::sort::sort_by(&mut order[lo..hi], |&a, &b| {
                cen[a as usize][axis]
                    .total_cmp(&cen[b as usize][axis])
                    .then(a.cmp(&b))
            });
            let mid = (lo + (hi - lo) / 2) as u32;
            let kids = nodes.len() as u32;
            nodes[k].kids = kids;
            for (a, b) in [(lo as u32, mid), (mid, hi as u32)] {
                nodes.push(Node {
                    min: DVec3::ZERO,
                    max: DVec3::ZERO,
                    lo: a,
                    hi: b,
                    kids: 0,
                });
            }
            stack.push(kids as usize + 1);
            stack.push(kids as usize);
        }
        Tree { s, order, nodes }
    }

    /// Every triangle the segment `a → b` crosses or touches (exact), with
    /// the crossing's parameter along it (0 at `a`, 1 at `b`).
    pub fn crossings(&self, a: DVec3, b: DVec3, out: &mut Vec<(u32, f64)>) {
        out.clear();
        let d = b - a;
        // The boxes only prune; they are widened so that rounding in the
        // slab test never drops a crossing the exact test would find.
        let pad = |n: &Node| (n.max - n.min).max_element() * 1e-9 + 1e-12;
        let mut stack = vec![0u32];
        while let Some(k) = stack.pop() {
            let n = &self.nodes[k as usize];
            let e = pad(n);
            if !segment_box(a, d, n.min - DVec3::splat(e), n.max + DVec3::splat(e)) {
                continue;
            }
            if n.kids == 0 {
                for &i in &self.order[n.lo as usize..n.hi as usize] {
                    if let Some(t) = crossing(a, b, tri(self.s, i)) {
                        out.push((i, t));
                    }
                }
            } else {
                stack.push(n.kids + 1);
                stack.push(n.kids);
            }
        }
    }

    /// The nearest point of the solid to `p` within `limit` (by squared
    /// distance, ties by triangle): its triangle, the point and the
    /// squared distance.
    pub fn nearest(&self, p: DVec3, limit: f64) -> Option<(u32, DVec3, f64)> {
        let mut best: Option<(u32, DVec3, f64)> = None;
        let mut bound = limit * limit;
        let mut stack = vec![0u32];
        while let Some(k) = stack.pop() {
            let n = &self.nodes[k as usize];
            if box_dist2(p, n.min, n.max) > bound {
                continue;
            }
            if n.kids == 0 {
                for &i in &self.order[n.lo as usize..n.hi as usize] {
                    let q = closest_on_triangle(p, tri(self.s, i));
                    let d2 = (q - p).length_squared();
                    let better = match best {
                        None => d2 <= bound,
                        Some((bi, _, bd)) => d2 < bd || (d2 == bd && i < bi),
                    };
                    if better {
                        best = Some((i, q, d2));
                        bound = d2;
                    }
                }
            } else {
                let (l, r) = (n.kids, n.kids + 1);
                let dl = box_dist2(p, self.nodes[l as usize].min, self.nodes[l as usize].max);
                let dr = box_dist2(p, self.nodes[r as usize].min, self.nodes[r as usize].max);
                // The nearer child is visited first.
                if dl <= dr {
                    stack.push(r);
                    stack.push(l);
                } else {
                    stack.push(l);
                    stack.push(r);
                }
            }
        }
        best
    }
}

fn box_dist2(p: DVec3, min: DVec3, max: DVec3) -> f64 {
    let q = p.clamp(min, max);
    (q - p).length_squared()
}

/// Whether the segment `a + t d`, t in [0, 1], meets the box (slabs).
fn segment_box(a: DVec3, d: DVec3, min: DVec3, max: DVec3) -> bool {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for k in 0..3 {
        if d[k] == 0.0 {
            if a[k] < min[k] || a[k] > max[k] {
                return false;
            }
            continue;
        }
        let (mut u, mut v) = ((min[k] - a[k]) / d[k], (max[k] - a[k]) / d[k]);
        if u > v {
            core::mem::swap(&mut u, &mut v);
        }
        t0 = t0.max(u);
        t1 = t1.min(v);
        if t0 > t1 {
            return false;
        }
    }
    true
}

/// The segment `a → b` crosses or touches triangle `t` (exact sides by
/// `orient3d`): its parameter, or `None`. A segment in the triangle's
/// plane crosses nothing.
fn crossing(a: DVec3, b: DVec3, t: [DVec3; 3]) -> Option<f64> {
    let sa = orient3d(t[0], t[1], t[2], a);
    let sb = orient3d(t[0], t[1], t[2], b);
    if sa == sb {
        // Both on one side, or both in the plane.
        return None;
    }
    // The line a–b passes the triangle's edges all on one side.
    let e = [
        orient3d(a, b, t[0], t[1]),
        orient3d(a, b, t[1], t[2]),
        orient3d(a, b, t[2], t[0]),
    ];
    let pos = e.contains(&Ordering::Greater);
    let neg = e.contains(&Ordering::Less);
    if pos && neg {
        return None;
    }
    let n = (t[1] - t[0]).cross(t[2] - t[0]);
    let (da, db) = (n.dot(a - t[0]), n.dot(b - t[0]));
    let tt = if da == db { 0.5 } else { da / (da - db) };
    Some(tt.clamp(0.0, 1.0))
}

/// The point of triangle `t` nearest `p` (Ericson's regions).
pub fn closest_on_triangle(p: DVec3, t: [DVec3; 3]) -> DVec3 {
    let [a, b, c] = t;
    let (ab, ac, ap) = (b - a, c - a, p - a);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return a + ab * v;
    }
    let cp = p - c;
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return a + ac * w;
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return b + (c - b) * w;
    }
    let denom = va + vb + vc;
    if denom == 0.0 {
        return a;
    }
    let (v, w) = (vb / denom, vc / denom);
    a + ab * v + ac * w
}

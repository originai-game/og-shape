//! Winding numbers of a triangle mesh round many points, fast and with the
//! direct sum's decisions (`docs/research.md` §18).
//!
//! [`winding`] sums every triangle's solid angle (Van Oosterom and
//! Strackee). [`Tree`] sums the same terms over a hierarchy of triangle
//! clusters. A cluster's solid angle is `∫ K(x) · n dA` with the kernel
//! `K(x) = (x − p) / |x − p|³`; far from the point it counts by the
//! kernel's Taylor expansion about the cluster's centre `c` to first
//! order: `K(c) · N + Σ_ij ∂_j K_i(c) M_ij`, where `N` is the cluster's
//! vector area and `M_ij = Σ v_i (g − c)_j` over its triangles (vector area
//! `v`, centroid `g`; exact, as the integrand is linear). The remainder is
//! at most `½ sup |∂²K| ∫ |x − c|² dA ≤ 3 S / ρ⁴`, where `S = ∫ |x − c|² dA`
//! and `ρ` is the point's clearance from the cluster's ball, which holds
//! every segment from `c` into the cluster. The sup is proved: with
//! `y = x − p`, `u = y/|y|`, a unit `h` and `c = u·h`,
//! `∂²K[h, h] = (u (9c² − 3) − 6c (h − c u)) / |y|⁴`, whose length squared
//! times `|y|⁸` is `45c⁴ − 18c² + 9 ≤ 36` for `c²` in [0, 1], so
//! `|∂²K[h, h]| ≤ 6 |h|² / |y|⁴`, with equality along `u`
//! (`docs/research.md` §23). Near triangles are summed term by
//! term, with the direct sum's arithmetic. [`Tree::side`] answers which
//! side of each threshold the direct sum lies: by the tree's value where
//! that value clears every threshold by more than its bound and the
//! rounding of both sums, coarse clusters first, finer ones next, and by
//! the direct sum where none does. Every decision is the direct sum's, so
//! output does not change.

use glam::DVec3;
use shape_gen::trig;

/// One triangle's solid angle seen from `p`, doubled as `atan2` gives it
/// (Van Oosterom and Strackee): the direct sum's term.
fn term(points: &[DVec3], t: [u32; 3], p: DVec3) -> f64 {
    let a = points[t[0] as usize] - p;
    let b = points[t[1] as usize] - p;
    let c = points[t[2] as usize] - p;
    let (la, lb, lc) = (a.length(), b.length(), c.length());
    let det = a.dot(b.cross(c));
    let den = la * lb * lc + a.dot(b) * lc + a.dot(c) * lb + b.dot(c) * la;
    2.0 * trig::atan2(det, den)
}

/// The winding number of a closed mesh round `p`: 1 inside, 0 outside;
/// triangle `skip`, if any, left out. The direct sum, in triangle order.
pub(crate) fn winding(points: &[DVec3], tris: &[[u32; 3]], skip: Option<usize>, p: DVec3) -> f64 {
    let mut sum = 0.0;
    for (i, &t) in tris.iter().enumerate() {
        if Some(i) == skip {
            continue;
        }
        sum += term(points, t, p);
    }
    sum / (4.0 * trig::PI)
}

/// Triangles per leaf.
const LEAF: usize = 8;
/// The largest bound, in radians, a cluster may add by its expansion: a
/// coarse estimate first, finer ones where it leaves a threshold in doubt.
const THETAS: [f64; 3] = [1.0 / 64.0, 1.0 / 512.0, 1.0 / 4096.0];
/// Below this many triangles the tree is not worth building.
pub(crate) const MIN_TRIS: usize = 256;

struct Node {
    /// Triangles `order[lo..hi]`.
    lo: u32,
    hi: u32,
    /// Children, or none for a leaf.
    kids: Option<(u32, u32)>,
    centre: DVec3,
    /// Every corner lies within it of `centre` (rounded up).
    radius: f64,
    /// `∫ |x − centre|² dA` over the triangles (rounded up).
    spread: f64,
    /// Sum of the triangles' vector areas.
    vector: DVec3,
    /// Sum over the triangles of their vector area times their centroid's
    /// offset from `centre`: `m[i][j] = Σ v_i (g − c)_j`.
    moment: [DVec3; 3],
}

/// A hierarchy of a mesh's triangles for [`Tree::side`].
pub(crate) struct Tree<'a> {
    points: &'a [DVec3],
    tris: &'a [[u32; 3]],
    order: Vec<u32>,
    /// Each triangle's place in `order`.
    place: Vec<u32>,
    nodes: Vec<Node>,
}

impl<'a> Tree<'a> {
    /// Clusters by median splits along the longest axis of their
    /// triangles' centroids (ties by index).
    pub(crate) fn new(points: &'a [DVec3], tris: &'a [[u32; 3]]) -> Tree<'a> {
        let centroid: Vec<DVec3> = tris
            .iter()
            .map(|t| (points[t[0] as usize] + points[t[1] as usize] + points[t[2] as usize]) / 3.0)
            .collect();
        let mut order: Vec<u32> = (0..tris.len() as u32).collect();
        let mut nodes: Vec<Node> = Vec::new();
        let mut tree_stack: Vec<(u32, u32, usize)> = Vec::new();
        nodes.push(Self::node(points, tris, &order, 0, tris.len() as u32));
        tree_stack.push((0, tris.len() as u32, 0));
        while let Some((lo, hi, at)) = tree_stack.pop() {
            if (hi - lo) as usize <= LEAF {
                continue;
            }
            let slice = &mut order[lo as usize..hi as usize];
            let (mut cmin, mut cmax) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
            for &i in slice.iter() {
                cmin = cmin.min(centroid[i as usize]);
                cmax = cmax.max(centroid[i as usize]);
            }
            let ext = cmax - cmin;
            let axis = if ext.x >= ext.y && ext.x >= ext.z {
                0
            } else if ext.y >= ext.z {
                1
            } else {
                2
            };
            shape_core::sort::sort_by(slice, |&i, &j| {
                centroid[i as usize][axis]
                    .total_cmp(&centroid[j as usize][axis])
                    .then(i.cmp(&j))
            });
            let mid = lo + (hi - lo) / 2;
            let (l, r) = (nodes.len() as u32, nodes.len() as u32 + 1);
            nodes.push(Self::node(points, tris, &order, lo, mid));
            nodes.push(Self::node(points, tris, &order, mid, hi));
            nodes[at].kids = Some((l, r));
            tree_stack.push((lo, mid, l as usize));
            tree_stack.push((mid, hi, r as usize));
        }
        let mut place = vec![0; tris.len()];
        for (k, &i) in order.iter().enumerate() {
            place[i as usize] = k as u32;
        }
        Tree {
            points,
            tris,
            order,
            place,
            nodes,
        }
    }

    fn node(points: &[DVec3], tris: &[[u32; 3]], order: &[u32], lo: u32, hi: u32) -> Node {
        let (mut min, mut max) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
        let mut vector = DVec3::ZERO;
        for &i in &order[lo as usize..hi as usize] {
            let [a, b, c] = tris[i as usize].map(|v| points[v as usize]);
            for p in [a, b, c] {
                min = min.min(p);
                max = max.max(p);
            }
            vector += (b - a).cross(c - a) * 0.5;
        }
        let centre = (min + max) * 0.5;
        let mut radius: f64 = 0.0;
        let mut moment = [DVec3::ZERO; 3];
        let mut spread = 0.0;
        for &i in &order[lo as usize..hi as usize] {
            let [a, b, c] = tris[i as usize].map(|v| points[v as usize]);
            for p in [a, b, c] {
                radius = radius.max((p - centre).length());
            }
            let n = (b - a).cross(c - a) * 0.5;
            let g = (a + b + c) / 3.0;
            let own =
                (a - g).length_squared() + (b - g).length_squared() + (c - g).length_squared();
            let g = g - centre;
            spread += n.length() * (g.length_squared() + own / 12.0);
            moment[0] += g * n.x;
            moment[1] += g * n.y;
            moment[2] += g * n.z;
        }
        Node {
            lo,
            hi,
            kids: None,
            centre,
            // Rounded up past the rounding of the lengths above.
            radius: radius * (1.0 + 1e-12) + f64::MIN_POSITIVE,
            spread: spread * (1.0 + 1e-12),
            vector,
            moment,
        }
    }

    /// The winding number round `p` (triangle `skip` left out), as the tree
    /// sums it, with a bound on how far the direct sum ([`winding`]) can
    /// lie from it.
    fn estimate(&self, p: DVec3, skip: Option<usize>, theta: f64) -> (f64, f64) {
        let eps = f64::EPSILON;
        let (mut sum, mut err, mut abs, mut far) = (0.0, 0.0, 0.0, 0.0);
        let inside = |n: &Node| skip.is_some_and(|s| (n.lo..n.hi).contains(&self.place[s]));
        let mut stack = vec![0u32];
        while let Some(k) = stack.pop() {
            let n = &self.nodes[k as usize];
            let d = (n.centre - p).length();
            let rho = d - n.radius;
            if rho > n.radius && !inside(n) {
                let r2 = rho * rho;
                // ½ · 6/ρ⁴ · S: the proved remainder (module doc).
                let e = 3.0 * n.spread / (r2 * r2);
                if e <= theta {
                    // K(c) · N + Σ_ij ∂_j K_i(c) m_ij, with ∂K = (I − 3ŷŷᵀ)/|y|³
                    // at y = c − p.
                    let y = n.centre - p;
                    let d3 = d * d * d;
                    let u = y / d;
                    let m = n.moment;
                    let trace = m[0].x + m[1].y + m[2].z;
                    let mu = DVec3::new(m[0].dot(u), m[1].dot(u), m[2].dot(u));
                    let t0 = n.vector.dot(y) / d3;
                    let t1 = (trace - 3.0 * u.dot(mu)) / d3;
                    let t = t0 + t1;
                    sum += t;
                    // The remainder, and the rounding of the two terms.
                    err += e + 1e-12 * (t0.abs() + t1.abs());
                    abs += t.abs() + e;
                    far += (n.hi - n.lo) as f64;
                    continue;
                }
            }
            match n.kids {
                Some((l, r)) => {
                    stack.push(r);
                    stack.push(l);
                }
                None => {
                    for &i in &self.order[n.lo as usize..n.hi as usize] {
                        if Some(i as usize) == skip {
                            continue;
                        }
                        let t = term(self.points, self.tris[i as usize], p);
                        sum += t;
                        abs += t.abs();
                    }
                }
            }
        }
        // The first-order bounds; each far triangle's term, well
        // conditioned at that distance, to a few ulps of 2π; and the
        // rounding of either sum, in any order (n·ε of the terms' sizes).
        let count = self.tris.len() as f64;
        let bound = err + far * 16.0 * eps * trig::PI + 4.0 * count * eps * (abs + 1.0);
        (
            sum / (4.0 * trig::PI),
            bound / (4.0 * trig::PI) * 1.5 + 1e-12,
        )
    }

    /// The winding number round `p`, triangle `skip` left out, for
    /// comparing with each of `thresholds`: the tree's value where it
    /// clears every threshold by more than its bound, the direct sum
    /// ([`winding`]) otherwise. Either lies on the same side of each
    /// threshold as the direct sum.
    pub(crate) fn side(&self, p: DVec3, skip: Option<usize>, thresholds: &[f64]) -> f64 {
        for theta in THETAS {
            let (w, bound) = self.estimate(p, skip, theta);
            if thresholds.iter().all(|&t| (w - t).abs() > bound) {
                return w;
            }
        }
        winding(self.points, self.tris, skip, p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sphere whose vertices are moved by up to `amount` of its radius,
    /// by a fixed sequence: noise this large folds it through itself.
    fn noisy_ball(amount: f64) -> (Vec<DVec3>, Vec<[u32; 3]>) {
        let s = shape_gen::solids::sphere(10.0).unwrap();
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 11) as f64 / (1u64 << 53) as f64 - 0.5
        };
        let points = s
            .positions
            .iter()
            .map(|&p| p + DVec3::new(next(), next(), next()) * (20.0 * amount))
            .collect();
        (points, s.triangles)
    }

    #[test]
    fn the_tree_bounds_the_direct_sum_and_keeps_its_decisions() {
        let (points, tris) = noisy_ball(0.03);
        let tree = Tree::new(&points, &tris);
        let mut far_off = 0;
        let mut check = |p: DVec3, skip: Option<usize>, thresholds: &[f64]| {
            let direct = winding(&points, &tris, skip, p);
            for theta in THETAS {
                let (w, bound) = tree.estimate(p, skip, theta);
                assert!((w - direct).abs() <= bound, "{w} {direct} {bound}");
                if (w - direct).abs() > 1e-9 {
                    far_off += 1;
                }
            }
            let w = tree.side(p, skip, thresholds);
            for &t in thresholds {
                assert_eq!(w > t, direct > t, "{w} {direct} {t}");
            }
        };
        // On the surface, at each triangle's centroid, without it.
        for (i, t) in tris.iter().enumerate() {
            let g = (points[t[0] as usize] + points[t[1] as usize] + points[t[2] as usize]) / 3.0;
            check(g, Some(i), &[1.0, 0.0]);
        }
        // Off it, on a grid through the ball and around it.
        for i in 0..9 {
            for j in 0..9 {
                for k in 0..9 {
                    let p = DVec3::new(i as f64, j as f64, k as f64) * 3.0 - DVec3::splat(12.1);
                    check(p, None, &[0.5]);
                }
            }
        }
        // The expansion was used, not only the leaves.
        assert!(far_off > 0);
    }
}

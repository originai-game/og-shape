//! The backstop of `docs/research.md` §7.7, cheaper (§7.8): which pairs of
//! a result's triangles cross, testing only pairs with a triangle the
//! bevel made or moved.
//!
//! Why that is enough. The input passed the exact check: no two of its
//! triangles cross. A result triangle whose three corners are, in order,
//! those of an input triangle is **kept**. Two kept triangles are tested
//! exactly as two input triangles were: the result is canonical, so its
//! vertex ids are its positions, and in the input two triangles meeting
//! at a position without sharing that vertex would have touched and been
//! refused, so there too sharing a vertex is sharing a position. So no
//! pair of kept triangles crosses, and the crossing pairs (and the first
//! of them, which names the error) are those of the full check.

use shape_core::DVec3;
use shape_core::order::cmp_dvec3;
use shape_mesh::Solid;

use shape_bool::selfcheck::{crossing, degenerate};

/// A triangle's corners turned to start at the least position.
fn key(t: [DVec3; 3]) -> [DVec3; 3] {
    let mut r = 0;
    for k in 1..3 {
        if cmp_dvec3(t[k], t[r]).is_lt() {
            r = k;
        }
    }
    [t[r], t[(r + 1) % 3], t[(r + 2) % 3]]
}

fn cmp_key(a: &[DVec3; 3], b: &[DVec3; 3]) -> core::cmp::Ordering {
    (0..3)
        .map(|k| cmp_dvec3(a[k], b[k]))
        .find(|o| o.is_ne())
        .unwrap_or(core::cmp::Ordering::Equal)
}

/// The input's triangles, sorted, for [`first_crossing`].
pub(crate) fn triangles(s: &Solid) -> Vec<[DVec3; 3]> {
    let mut v: Vec<[DVec3; 3]> = s
        .triangles
        .iter()
        .map(|t| key(t.map(|i| s.positions[i as usize])))
        .collect();
    v.sort_by(cmp_key);
    v
}

/// A node of the tree over the result's triangle boxes: its box, whether
/// every triangle under it is kept, and its children or its triangles.
struct Node {
    lo: DVec3,
    hi: DVec3,
    kept: bool,
    kids: Option<(usize, usize)>,
    tris: Vec<u32>,
}

struct Tree<'a> {
    nodes: Vec<Node>,
    boxes: &'a [(DVec3, DVec3)],
    kept: &'a [bool],
}

const LEAF: usize = 8;

impl Tree<'_> {
    /// A node over `ids`, split at the median of the boxes' centres along
    /// their widest axis (ties by index).
    fn build(&mut self, mut ids: Vec<u32>) -> usize {
        let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
        let (mut clo, mut chi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
        for &i in &ids {
            let (a, b) = self.boxes[i as usize];
            lo = lo.min(a);
            hi = hi.max(b);
            clo = clo.min(a + b);
            chi = chi.max(a + b);
        }
        let kept = ids.iter().all(|&i| self.kept[i as usize]);
        let at = self.nodes.len();
        self.nodes.push(Node {
            lo,
            hi,
            kept,
            kids: None,
            tris: Vec::new(),
        });
        if ids.len() <= LEAF {
            self.nodes[at].tris = ids;
            return at;
        }
        let w = chi - clo;
        let axis = if w.x >= w.y && w.x >= w.z {
            0
        } else if w.y >= w.z {
            1
        } else {
            2
        };
        let c = |i: u32| {
            let (a, b) = self.boxes[i as usize];
            (a + b)[axis]
        };
        ids.sort_by(|&x, &y| c(x).total_cmp(&c(y)).then(x.cmp(&y)));
        let right = ids.split_off(ids.len() / 2);
        let l = self.build(ids);
        let r = self.build(right);
        self.nodes[at].kids = Some((l, r));
        at
    }

    fn meet(&self, a: usize, b: usize) -> bool {
        let (x, y) = (&self.nodes[a], &self.nodes[b]);
        x.lo.cmple(y.hi).all() && y.lo.cmple(x.hi).all()
    }

    /// Every pair of triangles under `a` and `b` (under `a` alone when
    /// equal) whose boxes overlap, not both kept.
    fn pairs(&self, a: usize, b: usize, out: &mut Vec<(u32, u32)>) {
        if (self.nodes[a].kept && self.nodes[b].kept) || !self.meet(a, b) {
            return;
        }
        match (self.nodes[a].kids, self.nodes[b].kids) {
            (None, None) => {
                let (ta, tb) = (&self.nodes[a].tris, &self.nodes[b].tris);
                for (x, &i) in ta.iter().enumerate() {
                    let from = if a == b { x + 1 } else { 0 };
                    for &j in &tb[from..] {
                        let ((a0, a1), (b0, b1)) = (self.boxes[i as usize], self.boxes[j as usize]);
                        if !(self.kept[i as usize] && self.kept[j as usize])
                            && a0.cmple(b1).all()
                            && b0.cmple(a1).all()
                        {
                            out.push((i.min(j), i.max(j)));
                        }
                    }
                }
            }
            (Some((l, r)), _) if a == b => {
                self.pairs(l, l, out);
                self.pairs(r, r, out);
                self.pairs(l, r, out);
            }
            (Some((l, r)), None) => {
                self.pairs(l, b, out);
                self.pairs(r, b, out);
            }
            (None, Some((l, r))) => {
                self.pairs(a, l, out);
                self.pairs(a, r, out);
            }
            (Some((l, r)), Some((m, k))) => {
                // Split the larger.
                let size = |n: &Node| (n.hi - n.lo).max_element();
                if size(&self.nodes[a]) >= size(&self.nodes[b]) {
                    self.pairs(l, b, out);
                    self.pairs(r, b, out);
                } else {
                    self.pairs(a, m, out);
                    self.pairs(a, k, out);
                }
            }
        }
    }
}

/// The first pair `(i, j)`, `i < j`, of the canonical result's triangles
/// that cross, as `shape_bool::selfcheck::self_crossings` would list
/// first; `input` the checked input's [`triangles`].
///
/// The broad phase is a tree of boxes split at the median along the
/// widest axis, walked in pairs of nodes; a pair of subtrees holding only
/// kept triangles is skipped whole. (A uniform grid, as `box_pairs` uses,
/// is slow on a bevel's long thin strip triangles.) Every pair of
/// non-degenerate triangles whose boxes overlap, not both kept, is found
/// once and goes to the exact test in index order.
pub(crate) fn first_crossing(out: &Solid, input: &[[DVec3; 3]]) -> Option<(u32, u32)> {
    let corners = |i: usize| out.triangles[i].map(|v| out.positions[v as usize]);
    let n = out.triangles.len();
    let live: Vec<u32> = (0..n as u32)
        .filter(|&i| !degenerate(corners(i as usize)))
        .collect();
    let kept: Vec<bool> = (0..n)
        .map(|i| {
            input
                .binary_search_by(|x| cmp_key(x, &key(corners(i))))
                .is_ok()
        })
        .collect();
    if live.iter().all(|&i| kept[i as usize]) {
        return None;
    }
    let boxes: Vec<(DVec3, DVec3)> = (0..n)
        .map(|i| {
            let p = corners(i);
            (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]))
        })
        .collect();
    let mut tree = Tree {
        nodes: Vec::new(),
        boxes: &boxes,
        kept: &kept,
    };
    let root = tree.build(live);
    let mut pairs = Vec::new();
    tree.pairs(root, root, &mut pairs);
    pairs.sort_unstable();
    pairs.into_iter().find(|&(i, j)| {
        let (ti, tj) = (out.triangles[i as usize], out.triangles[j as usize]);
        crossing(ti, corners(i as usize), tj, corners(j as usize))
    })
}

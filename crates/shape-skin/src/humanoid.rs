//! A humanoid's joints found from its mesh alone.
//!
//! The mesh stands up +Z and faces −Y (as `place` leaves it), so its left
//! is +X, in a T or A pose (arms away from the body, legs apart). The
//! method, this project's own:
//! - The solid is voxelized (a winding count along vertical rays, so
//!   overlapping closed pieces fill as one), and its largest connected
//!   lump kept.
//! - Five tips: the top of the head, each hand (the farthest voxel
//!   sideways above a third of the height) and each toe (the front-most
//!   low voxel on each side).
//! - Distances through the voxels (a chamfer metric in integers). From
//!   the head's top, the connected pieces of each band of distance form
//!   a tree that branches where the body does: a limb leaves the body
//!   where its branch, and a sibling's, each hold at least 1.5 % of the
//!   voxels (so fingers are walked past).
//! - From each limb's tip, the pieces of the bands that follow on from
//!   one another are cross-sections of the limb; their centres are its
//!   centre line.
//! - The shoulder is where the arm's line passes the chest's side under
//!   the arm (the first layer down whose side stays put, below a
//!   shoulder pad or a sleeve); the wrist and the elbow at 24.5 % and
//!   62 % of the arm from its tip. The ankle is a radius past where the
//!   foot's line turns upward; the hip a radius above where the leg
//!   leaves the body, or, where that is lower than 64 % of the
//!   shoulders' height (a skirt, armour on the hips), there; the knee
//!   halfway up. The neck's base is 3.5 % of the height above the
//!   shoulders, the skull's base a third of the way on to the top; the
//!   spine runs from the hips to the neck, centred front to back in the
//!   torso's sections. The proportions are Mixamo's, measured on
//!   three.js's Xbot and Soldier (`docs/research.md` §79).
//!
//! The joints are named as Mixamo names them (no prefix), so clips made
//! for that skeleton map by name. [`FitReport`] measures where they fell;
//! nothing is judged.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use shape_core::DVec3;
use shape_mesh::Solid;
use shape_mesh::tree::Tree;

use crate::heat::{Joint, Skeleton};
use crate::rig::RigError;

/// Voxels along the height.
const LAYERS: f64 = 160.0;
/// Chamfer steps (×10): a face, an edge and a corner neighbour.
const STEPS: [u32; 4] = [0, 10, 14, 17];
/// A band's width, in chamfer units (two voxels).
const BAND: u32 = 20;
/// The least share of the body's voxels a limb holds: smaller branches
/// (fingers, a ponytail) are walked past.
const LIMB: f64 = 0.015;

/// One traced limb.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Limb {
    /// `head`, `left_arm`, `right_arm`, `left_leg` or `right_leg`.
    pub name: String,
    /// The tip it was traced from.
    pub tip: DVec3,
    /// From the tip to where it meets the body, along its centre line, mm.
    pub length: f64,
    /// The radius of its last section before the body (from the section's
    /// area), mm.
    pub radius: f64,
}

/// What [`humanoid`] measured.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FitReport {
    /// The voxel's edge, mm.
    pub voxel: f64,
    /// The mesh's height, mm.
    pub height: f64,
    /// Voxels inside the mesh, and those in the lump kept.
    pub voxels: usize,
    /// See `voxels`.
    pub kept: usize,
    /// The limbs, head first, then arms and legs, left first.
    pub limbs: Vec<Limb>,
    /// Joints that fell outside the voxels kept (outside the mesh, or in
    /// a lump that was not kept).
    pub outside: Vec<String>,
    /// Each arm's angle below the horizontal, shoulder to wrist, radians:
    /// about 0 for a T pose, more for an A pose. Left, then right.
    pub arm_drop: [f64; 2],
}

struct Grid {
    min: DVec3,
    h: f64,
    n: [usize; 3],
    occ: Vec<bool>,
}

impl Grid {
    fn index(&self, i: usize, j: usize, k: usize) -> usize {
        (k * self.n[1] + j) * self.n[0] + i
    }

    fn cell(&self, v: usize) -> [usize; 3] {
        let i = v % self.n[0];
        let j = (v / self.n[0]) % self.n[1];
        [i, j, v / (self.n[0] * self.n[1])]
    }

    fn center(&self, v: usize) -> DVec3 {
        let [i, j, k] = self.cell(v);
        self.min + DVec3::new(i as f64 + 0.5, j as f64 + 0.5, k as f64 + 0.5) * self.h
    }

    fn at(&self, p: DVec3) -> Option<usize> {
        let q = (p - self.min) / self.h;
        if q.x < 0.0 || q.y < 0.0 || q.z < 0.0 {
            return None;
        }
        let c = [q.x as usize, q.y as usize, q.z as usize];
        (c[0] < self.n[0] && c[1] < self.n[1] && c[2] < self.n[2])
            .then(|| self.index(c[0], c[1], c[2]))
    }

    /// The 26 neighbours of `v` with their chamfer steps.
    fn around(&self, v: usize, out: &mut Vec<(usize, u32)>) {
        out.clear();
        let [i, j, k] = self.cell(v);
        for dk in -1i64..=1 {
            for dj in -1i64..=1 {
                for di in -1i64..=1 {
                    let order = (di != 0) as usize + (dj != 0) as usize + (dk != 0) as usize;
                    if order == 0 {
                        continue;
                    }
                    let (a, b, c) = (i as i64 + di, j as i64 + dj, k as i64 + dk);
                    if a < 0
                        || b < 0
                        || c < 0
                        || a as usize >= self.n[0]
                        || b as usize >= self.n[1]
                        || c as usize >= self.n[2]
                    {
                        continue;
                    }
                    out.push((self.index(a as usize, b as usize, c as usize), STEPS[order]));
                }
            }
        }
    }
}

/// The voxels inside `s`: per column, a vertical ray's crossings counted
/// by the triangles' facing (in +1, out −1); inside where the count is
/// above 0.
fn voxelize(s: &Solid, min: DVec3, max: DVec3, h: f64) -> Grid {
    let n = [
        ((max.x - min.x) / h) as usize + 3,
        ((max.y - min.y) / h) as usize + 3,
        ((max.z - min.z) / h) as usize + 3,
    ];
    let min = min - DVec3::splat(h);
    let mut grid = Grid {
        min,
        h,
        n,
        occ: vec![false; n[0] * n[1] * n[2]],
    };
    let tree = Tree::new(s);
    let mut hits = Vec::new();
    let (z0, z1) = (min.z - h, min.z + (n[2] as f64 + 1.0) * h);
    for j in 0..n[1] {
        for i in 0..n[0] {
            // Off the voxel centres by a little, so a ray meets the edges
            // of an axis-aligned model only by accident.
            let x = min.x + (i as f64 + 0.500_123) * h;
            let y = min.y + (j as f64 + 0.500_271) * h;
            let (a, b) = (DVec3::new(x, y, z0), DVec3::new(x, y, z1));
            tree.crossings(a, b, &mut hits);
            let mut list: Vec<(f64, i32)> = hits
                .iter()
                .filter_map(|&(t, at)| {
                    let [p, q, r] = shape_mesh::tree::tri(s, t);
                    let nz = (q - p).cross(r - p).z;
                    (nz != 0.0).then_some((z0 + at * (z1 - z0), if nz < 0.0 { 1 } else { -1 }))
                })
                .collect();
            list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            // A ray through an edge meets both its triangles at one point.
            list.dedup();
            let mut w = 0;
            let mut next = 0;
            for k in 0..n[2] {
                let zc = min.z + (k as f64 + 0.5) * h;
                while next < list.len() && list[next].0 < zc {
                    w += list[next].1;
                    next += 1;
                }
                if w > 0 {
                    let v = grid.index(i, j, k);
                    grid.occ[v] = true;
                }
            }
        }
    }
    grid
}

/// Keeps the largest 26-connected lump (ties to the lowest voxel).
fn keep_largest(g: &mut Grid) -> usize {
    let mut label = vec![u32::MAX; g.occ.len()];
    let mut best = (0usize, u32::MAX);
    let mut next = 0u32;
    let mut stack = Vec::new();
    let mut around = Vec::new();
    for v in 0..g.occ.len() {
        if !g.occ[v] || label[v] != u32::MAX {
            continue;
        }
        let mut count = 0;
        label[v] = next;
        stack.push(v);
        while let Some(u) = stack.pop() {
            count += 1;
            g.around(u, &mut around);
            for &(w, _) in &around {
                if g.occ[w] && label[w] == u32::MAX {
                    label[w] = next;
                    stack.push(w);
                }
            }
        }
        if count > best.0 {
            best = (count, next);
        }
        next += 1;
    }
    for v in 0..g.occ.len() {
        g.occ[v] = g.occ[v] && label[v] == best.1;
    }
    best.0
}

/// Chamfer distances through the voxels from `from`; `u32::MAX` where
/// none reaches.
fn distances(g: &Grid, from: usize) -> Vec<u32> {
    let mut d = vec![u32::MAX; g.occ.len()];
    let mut heap = BinaryHeap::new();
    d[from] = 0;
    heap.push(Reverse((0u32, from)));
    let mut around = Vec::new();
    while let Some(Reverse((dv, v))) = heap.pop() {
        if dv > d[v] {
            continue;
        }
        g.around(v, &mut around);
        for &(w, step) in &around {
            if g.occ[w] && dv + step < d[w] {
                d[w] = dv + step;
                heap.push(Reverse((dv + step, w)));
            }
        }
    }
    d
}

/// One connected piece of one band of distance from the head's top: the
/// bands' pieces joined to the pieces of the band before that they
/// touch form a tree that branches where the body does (a Reeb graph).
struct Node {
    voxels: Vec<usize>,
    center: DVec3,
    /// The piece of the band before that this one hangs on (the largest
    /// of those it touches), if any.
    parent: Option<usize>,
    /// The pieces that hang on this one.
    children: Vec<usize>,
    /// Voxels in this piece and every piece below it.
    below: usize,
}

/// The pieces of the bands of `d` and each voxel's piece
/// (`usize::MAX` for voxels `d` does not reach).
fn pieces(g: &Grid, d: &[u32]) -> (Vec<Node>, Vec<usize>) {
    let bands = d
        .iter()
        .filter(|&&x| x != u32::MAX)
        .max()
        .copied()
        .unwrap_or(0)
        / BAND
        + 1;
    let mut by_band: Vec<Vec<usize>> = vec![Vec::new(); bands as usize];
    for (v, &x) in d.iter().enumerate() {
        if x != u32::MAX {
            by_band[(x / BAND) as usize].push(v);
        }
    }
    let mut label = vec![usize::MAX; d.len()];
    let mut nodes: Vec<Node> = Vec::new();
    let mut around = Vec::new();
    for (b, members) in by_band.iter().enumerate() {
        for &seed in members {
            if label[seed] != usize::MAX {
                continue;
            }
            let id = nodes.len();
            let mut voxels = vec![seed];
            label[seed] = id;
            let mut k = 0;
            let mut touched: Vec<usize> = Vec::new();
            while k < voxels.len() {
                let u = voxels[k];
                k += 1;
                g.around(u, &mut around);
                for &(w, _) in &around {
                    if d[w] == u32::MAX {
                        continue;
                    }
                    let bw = (d[w] / BAND) as usize;
                    if bw == b && label[w] == usize::MAX {
                        label[w] = id;
                        voxels.push(w);
                    } else if bw + 1 == b {
                        touched.push(label[w]);
                    }
                }
            }
            voxels.sort_unstable();
            touched.sort_unstable();
            touched.dedup();
            let parent = touched.iter().copied().max_by(|&a, &c| {
                nodes[a]
                    .voxels
                    .len()
                    .cmp(&nodes[c].voxels.len())
                    .then(c.cmp(&a))
            });
            if let Some(p) = parent {
                nodes[p].children.push(id);
            }
            let mut c = DVec3::ZERO;
            for &v in &voxels {
                c += g.center(v);
            }
            let center = c / voxels.len() as f64;
            nodes.push(Node {
                below: voxels.len(),
                voxels,
                center,
                parent,
                children: Vec::new(),
            });
        }
    }
    for n in (0..nodes.len()).rev() {
        if let Some(p) = nodes[n].parent {
            nodes[p].below += nodes[n].below;
        }
    }
    (nodes, label)
}

/// Walking from a tip's piece toward the head, the first piece whose
/// branch, and a sibling branch, each hold at least `least` voxels: where
/// the limb leaves the body (smaller branches, such as fingers, are
/// walked past).
fn branch(nodes: &[Node], from: usize, least: usize, name: &str) -> Result<usize, RigError> {
    let mut n = from;
    while let Some(p) = nodes[n].parent {
        let sibling = nodes[p]
            .children
            .iter()
            .any(|&c| c != n && nodes[c].below >= least);
        if nodes[n].below >= least && sibling {
            return Ok(n);
        }
        n = p;
    }
    Err(RigError::Humanoid(format!(
        "the {name} never branches off the body: is the mesh in a T or A pose, its limbs apart?"
    )))
}

/// A limb's centre line: per band of distance from its tip, the centre
/// and the voxel count of the piece that follows on from the band before.
struct Trace {
    centers: Vec<DVec3>,
    counts: Vec<usize>,
    band_mm: f64,
    h: f64,
}

/// Traces the limb from `tip` for `length` mm and a little more.
fn trace(g: &Grid, d: &[u32], tip: usize, length: f64, name: &str) -> Result<Trace, RigError> {
    let band_mm = BAND as f64 / 10.0 * g.h;
    let bands = (length / band_mm) as usize + 3;
    let mut by_band: Vec<Vec<usize>> = vec![Vec::new(); bands];
    for (v, &x) in d.iter().enumerate() {
        if x != u32::MAX && ((x / BAND) as usize) < bands {
            by_band[(x / BAND) as usize].push(v);
        }
    }
    let mut chosen = vec![false; g.occ.len()];
    let mut centers = Vec::new();
    let mut counts = Vec::new();
    let mut around = Vec::new();
    for (b, members) in by_band.iter().enumerate() {
        let mut picked: Vec<usize> = if b == 0 {
            vec![tip]
        } else {
            members
                .iter()
                .copied()
                .filter(|&v| {
                    g.around(v, &mut around);
                    around
                        .iter()
                        .any(|&(w, _)| chosen[w] && d[w] / BAND == b as u32 - 1)
                })
                .collect()
        };
        for &v in &picked {
            chosen[v] = true;
        }
        let mut stack = picked.clone();
        while let Some(u) = stack.pop() {
            g.around(u, &mut around);
            for &(w, _) in &around {
                if !chosen[w] && d[w] != u32::MAX && d[w] / BAND == b as u32 {
                    chosen[w] = true;
                    picked.push(w);
                    stack.push(w);
                }
            }
        }
        if picked.is_empty() {
            return Err(RigError::Humanoid(format!(
                "the {name}'s centre line ends {:.0} mm from its tip",
                b as f64 * band_mm
            )));
        }
        let mut c = DVec3::ZERO;
        for &v in &picked {
            c += g.center(v);
        }
        centers.push(c / picked.len() as f64);
        counts.push(picked.len());
    }
    Ok(Trace {
        centers,
        counts,
        band_mm,
        h: g.h,
    })
}

impl Trace {
    fn band(&self, s: f64) -> f64 {
        (s / self.band_mm - 0.5).clamp(0.0, (self.centers.len() - 1) as f64)
    }

    /// The centre line at `s` mm from the tip (between band centres).
    fn at(&self, s: f64) -> DVec3 {
        let f = self.band(s);
        let k = (f as usize).min(self.centers.len() - 2);
        self.centers[k] + (self.centers[k + 1] - self.centers[k]) * (f - k as f64)
    }

    /// The radius of the limb's section at `s` mm from the tip, from the
    /// band's volume.
    fn radius(&self, s: f64) -> f64 {
        let k = self.band(s).round() as usize;
        let area = self.counts[k] as f64 * self.h * self.h * self.h / self.band_mm;
        (area / core::f64::consts::PI).sqrt()
    }
}

/// The voxels of the layer at height `z` joined (sideways, by edges and
/// corners) to the one nearest the line `x` of that layer; none when the
/// layer is empty.
fn layer_piece(g: &Grid, z: f64, x: f64) -> Vec<usize> {
    let k = (((z - g.min.z) / g.h).max(0.0) as usize).min(g.n[2] - 1);
    let mut seed = None;
    for j in 0..g.n[1] {
        for i in 0..g.n[0] {
            let v = g.index(i, j, k);
            if g.occ[v] {
                let dx = (g.center(v).x - x).abs();
                if seed.is_none_or(|(d, _)| dx < d) {
                    seed = Some((dx, v));
                }
            }
        }
    }
    let Some((_, seed)) = seed else {
        return Vec::new();
    };
    let mut seen = vec![false; g.n[0] * g.n[1]];
    let plane = |v: usize| v % (g.n[0] * g.n[1]);
    seen[plane(seed)] = true;
    let mut out = vec![seed];
    let mut next = 0;
    while next < out.len() {
        let [i, j, _] = g.cell(out[next]);
        next += 1;
        for (di, dj) in [
            (-1i64, -1i64),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ] {
            let (a, b) = (i as i64 + di, j as i64 + dj);
            if a < 0 || b < 0 || a as usize >= g.n[0] || b as usize >= g.n[1] {
                continue;
            }
            let w = g.index(a as usize, b as usize, k);
            if g.occ[w] && !seen[plane(w)] {
                seen[plane(w)] = true;
                out.push(w);
            }
        }
    }
    out.sort_unstable();
    out
}

/// The mean distance from a tip, in mm, of a piece's voxels.
fn reach(d: &[u32], node: &Node, h: f64) -> f64 {
    let sum: f64 = node.voxels.iter().map(|&v| d[v] as f64).sum();
    sum / node.voxels.len() as f64 / 10.0 * h
}

/// The skeleton of a humanoid standing up +Z, facing −Y, in a T or A
/// pose, found from the mesh (see the module docs), and what was measured.
pub fn humanoid(s: &Solid) -> Result<(Skeleton, FitReport), RigError> {
    if s.triangles.is_empty() {
        return Err(RigError::Humanoid("the mesh has no triangles".into()));
    }
    if s.positions.iter().any(|p| !p.is_finite()) {
        return Err(RigError::NotFinite("a position".into()));
    }
    let m = s.measure();
    let (lo, hi) = (m.bounds_min, m.bounds_max);
    let height = hi.z - lo.z;
    if height <= 0.0 {
        return Err(RigError::Humanoid("the mesh has no height".into()));
    }
    let h = height / LAYERS;
    let mut g = voxelize(s, lo, hi, h);
    let voxels = g.occ.iter().filter(|x| **x).count();
    if voxels == 0 {
        return Err(RigError::Humanoid(
            "no voxel is inside the mesh: is it closed, its faces outward?".into(),
        ));
    }
    let kept = keep_largest(&mut g);
    let inside: Vec<usize> = (0..g.occ.len()).filter(|&v| g.occ[v]).collect();
    let mut cx = 0.0;
    for &v in &inside {
        cx += g.center(v).x;
    }
    cx /= inside.len() as f64;
    let ground = g.center(inside[0]).z;
    let top = inside
        .iter()
        .map(|&v| g.center(v).z)
        .fold(f64::MIN, f64::max);
    let tall = top - ground;
    let pick = |filter: &dyn Fn(DVec3) -> bool, key: &dyn Fn(DVec3) -> f64| {
        inside
            .iter()
            .copied()
            .filter(|&v| filter(g.center(v)))
            .min_by(|&a, &b| {
                key(g.center(a))
                    .total_cmp(&key(g.center(b)))
                    .then(a.cmp(&b))
            })
    };
    let missing = |what: &str| RigError::Humanoid(format!("no voxel for the {what}"));
    let head_tip =
        pick(&|_| true, &|p| -p.z + (p.x - cx).abs() * 1e-6).ok_or_else(|| missing("head"))?;
    let arm_z = ground + tall / 3.0;
    let hand_l =
        pick(&|p| p.z > arm_z && p.x > cx, &|p| -p.x).ok_or_else(|| missing("left hand"))?;
    let hand_r =
        pick(&|p| p.z > arm_z && p.x < cx, &|p| p.x).ok_or_else(|| missing("right hand"))?;
    let foot_z = ground + tall * 0.15;
    let toe_l = pick(&|p| p.z < foot_z && p.x > cx, &|p| p.y + p.z * 1e-3)
        .ok_or_else(|| missing("left foot"))?;
    let toe_r = pick(&|p| p.z < foot_z && p.x < cx, &|p| p.y + p.z * 1e-3)
        .ok_or_else(|| missing("right foot"))?;

    // Where each limb leaves the body: the branches of the bands from the
    // head's top.
    let from_head = distances(&g, head_tip);
    let (nodes, label) = pieces(&g, &from_head);
    let least = (kept as f64 * LIMB) as usize;
    let mut roots = Vec::new();
    for (tip, name) in [
        (hand_l, "left arm"),
        (hand_r, "right arm"),
        (toe_l, "left leg"),
        (toe_r, "right leg"),
    ] {
        if label[tip] == usize::MAX {
            return Err(missing(name));
        }
        roots.push(branch(&nodes, label[tip], least, name)?);
    }
    // The head: down from its top while one branch goes on.
    let mut chain = vec![label[head_tip]];
    loop {
        let n = *chain.last().unwrap_or(&0);
        let big: Vec<usize> = nodes[n]
            .children
            .iter()
            .copied()
            .filter(|&c| nodes[c].below >= least)
            .collect();
        match big.as_slice() {
            [one] => chain.push(*one),
            _ => break,
        }
    }
    let head_top = nodes[chain[0]].center;
    let mut limbs = Vec::new();
    // Arms: the shoulder where the arm's centre line passes the side of
    // the chest under the arm, the wrist and the elbow at a
    // human's proportions of the arm's length from there.
    let below_arms = roots[..2]
        .iter()
        .map(|&r| nodes[r].center.z)
        .fold(f64::INFINITY, f64::min);
    let mut arm_joints = Vec::new();
    for (k, (tip, name)) in [(hand_l, "left_arm"), (hand_r, "right_arm")]
        .into_iter()
        .enumerate()
    {
        let d = distances(&g, tip);
        let len = reach(&d, &nodes[roots[k]], h);
        let t = trace(&g, &d, tip, len * 1.5, name)?;
        let radius = t.radius(len);
        let side = if k == 0 { 1.0 } else { -1.0 };
        let edge = |z: f64| {
            layer_piece(&g, z, cx)
                .iter()
                .map(|&v| g.center(v).x * side)
                .fold(f64::NEG_INFINITY, f64::max)
        };
        // Down from where the arm leaves the chest, the first layer whose
        // side stays put (within a voxel over two layers): the armpit,
        // below a shoulder pad or a sleeve.
        let target = (0..(4.0 * radius / h) as usize)
            .map(|i| below_arms - radius - (i + 1) as f64 * h)
            .find(|&z| {
                let e = edge(z);
                e < nodes[roots[k]].center.x * side && (e - edge(z - 2.0 * h)).abs() <= h
            })
            .map(edge);
        // Inward from the tip, the first band whose centre is past it.
        let s = (0..t.centers.len())
            .find(|&b| target.is_some_and(|x| t.centers[b].x * side <= x))
            .map(|b| (b as f64 + 0.5) * t.band_mm)
            .unwrap_or(len + radius);
        let shoulder = t.at(s);
        arm_joints.push([shoulder, t.at(s * 0.62), t.at(s * 0.245), t.centers[0]]);
        limbs.push(Limb {
            name: name.into(),
            tip: g.center(tip),
            length: s,
            radius,
        });
    }
    let shoulder_z = (arm_joints[0][0].z + arm_joints[1][0].z) / 2.0;
    // The neck's base a little above the shoulders (Mixamo's proportion,
    // 3.5 % of the height), but not above the neck's narrowest band; the
    // skull's base a third of the way from there to the top.
    let lower = chain.len() / 2;
    let narrow = (lower..chain.len())
        .min_by(|&a, &b| {
            nodes[chain[a]]
                .voxels
                .len()
                .cmp(&nodes[chain[b]].voxels.len())
                .then(a.cmp(&b))
        })
        .unwrap_or(chain.len() - 1);
    let on_head = |z: f64| {
        chain
            .iter()
            .map(|&n| nodes[n].center)
            .min_by(|a, b| (a.z - z).abs().total_cmp(&(b.z - z).abs()))
            .map(|c| DVec3::new(c.x, c.y, z))
            .unwrap_or(head_top)
    };
    let neck_z = (shoulder_z + 0.035 * tall).min(nodes[chain[narrow]].center.z);
    let neck_base = on_head(neck_z);
    let skull = on_head(neck_z + (head_top.z - neck_z) * 0.35);
    limbs.insert(
        0,
        Limb {
            name: "head".into(),
            tip: g.center(head_tip),
            length: head_top.z - neck_z,
            radius: 0.0,
        },
    );
    // Legs: the ankle where the foot's line turns upward; the hip a
    // radius above where the leg leaves the body, or, where that is lower
    // than 64 % of the shoulders' height (a skirt, armour on the hips),
    // there; the knee halfway up between them.
    let ratio_z = ground + 0.64 * (shoulder_z - ground);
    let mut leg_joints = Vec::new();
    for (k, (tip, name)) in [(toe_l, "left_leg"), (toe_r, "right_leg")]
        .into_iter()
        .enumerate()
    {
        let d = distances(&g, tip);
        let len = reach(&d, &nodes[roots[2 + k]], h);
        let t = trace(&g, &d, tip, len, name)?;
        let radius = t.radius(len);
        let steps = (tall * 0.05 / t.band_mm).ceil().max(1.0) as usize;
        let last = t.band(len) as usize;
        let ankle = (0..last.saturating_sub(steps))
            .find(|&b| {
                let d = t.centers[b + steps] - t.centers[b];
                d.z > 0.7 * d.length()
            })
            .ok_or_else(|| {
                RigError::Humanoid(format!("the {name}'s centre line never turns upward"))
            })?;
        // The ankle's centre is a radius up from where the line turns.
        let turn = (ankle as f64 + 0.5) * t.band_mm;
        let s_ankle = turn + t.radius(turn);
        let foot = t.at(s_ankle);
        let end = t.at(len);
        let hip = DVec3::new(end.x, end.y, (end.z + radius).max(ratio_z));
        let knee_z = (foot.z + hip.z) / 2.0;
        let knee = (0..t.centers.len())
            .find(|&b| t.centers[b].z >= knee_z)
            .map(|b| t.centers[b])
            .unwrap_or(end);
        leg_joints.push([
            hip,
            DVec3::new(knee.x, knee.y, knee_z),
            foot,
            t.at(turn * 0.55),
            t.centers[0],
        ]);
        limbs.push(Limb {
            name: name.into(),
            tip: g.center(tip),
            length: foot.z - ground + (hip - foot).length(),
            radius,
        });
    }
    // The spine from between the hips to the neck's base, centred in the
    // torso's sections front to back.
    // Mixamo's hips sit 3.5 % of the height above the hip joints.
    let hips = (leg_joints[0][0] + leg_joints[1][0]) / 2.0 + DVec3::Z * 0.035 * tall;
    let torso_y = |z: f64, x: f64| -> f64 {
        let piece = layer_piece(&g, z, x);
        if piece.is_empty() {
            return hips.y;
        }
        piece.iter().map(|&v| g.center(v).y).sum::<f64>() / piece.len() as f64
    };
    let spine_at = |f: f64| {
        let p = hips + (neck_base - hips) * f;
        DVec3::new(p.x, torso_y(p.z, p.x), p.z)
    };
    let spine = [spine_at(0.2), spine_at(0.44), spine_at(0.72)];
    let clavicle = |arm: DVec3| {
        let f = ((arm.z - spine[2].z) / (neck_base.z - spine[2].z)).clamp(0.0, 1.0);
        let base = spine[2] + (neck_base - spine[2]) * f;
        base + (arm - base) * 0.3
    };

    let mut joints: Vec<Joint> = Vec::new();
    let mut add = |name: &str, parent: &str, at: DVec3| {
        joints.push(Joint {
            name: name.into(),
            parent: parent.into(),
            at,
        });
    };
    add("Hips", "", hips);
    add("Spine", "Hips", spine[0]);
    add("Spine1", "Spine", spine[1]);
    add("Spine2", "Spine1", spine[2]);
    add("Neck", "Spine2", neck_base);
    add("Head", "Neck", skull);
    add("HeadTop_End", "Head", head_top);
    for (side, a) in ["Left", "Right"].iter().zip(&arm_joints) {
        add(&format!("{side}Shoulder"), "Spine2", clavicle(a[0]));
        add(&format!("{side}Arm"), &format!("{side}Shoulder"), a[0]);
        add(&format!("{side}ForeArm"), &format!("{side}Arm"), a[1]);
        add(&format!("{side}Hand"), &format!("{side}ForeArm"), a[2]);
        add(&format!("{side}Hand_End"), &format!("{side}Hand"), a[3]);
    }
    for (side, l) in ["Left", "Right"].iter().zip(&leg_joints) {
        add(&format!("{side}UpLeg"), "Hips", l[0]);
        add(&format!("{side}Leg"), &format!("{side}UpLeg"), l[1]);
        add(&format!("{side}Foot"), &format!("{side}Leg"), l[2]);
        add(&format!("{side}ToeBase"), &format!("{side}Foot"), l[3]);
        add(&format!("{side}Toe_End"), &format!("{side}ToeBase"), l[4]);
    }
    let outside = joints
        .iter()
        .filter(|j| !g.at(j.at).is_some_and(|v| g.occ[v]))
        .map(|j| j.name.clone())
        .collect();
    let drop = |a: &[DVec3; 4]| {
        let d = a[2] - a[0];
        let flat = (d.x * d.x + d.y * d.y).sqrt();
        shape_core::trig::atan2(-d.z, flat)
    };
    let report = FitReport {
        voxel: h,
        height,
        voxels,
        kept,
        limbs,
        outside,
        arm_drop: [drop(&arm_joints[0]), drop(&arm_joints[1])],
    };
    Ok((Skeleton { joints }, report))
}

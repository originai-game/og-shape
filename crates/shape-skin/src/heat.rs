//! Weights for a skeleton given as joints: bone heat (Baran and Popović,
//! "Automatic Rigging and Animation of 3D Characters", SIGGRAPH 2007,
//! §4), written here from the paper.
//!
//! A bone is the segments from its joint to each of its children; a leaf
//! joint only marks where its parent's bone ends and gets no weight (a
//! skeleton of one joint is a point bone). Each vertex is heated by the
//! nearest bone it sees (the segment to the bone's nearest point crosses
//! no triangle) with heat `1 / d²`; a vertex that sees none is not heated.
//! Bone `i`'s weights solve the steady state of heat on the surface,
//! `(L + A·H) wᵢ = A·H·pᵢ`: `L` the cotangent stiffness, `A` the
//! vertices' areas, `H` the heat, `pᵢ` 1 where bone `i` is the heating
//! bone. The weights of a vertex sum to 1 before the four largest are
//! kept. A piece of the mesh that sees no bone at all (an eye behind its
//! lid) is heated by its nearest bones regardless of sight.
//!
//! The systems are solved by conjugate gradients with a diagonal
//! preconditioner, in a fixed order: the same mesh and joints give the
//! same bits on every host.

use std::collections::BTreeMap;

use shape_core::{DVec3, Frame};
use shape_mesh::tree::Tree;
use shape_mesh::{Bone, Skin, Solid, Weights};

use crate::rig::RigError;

/// One joint of a [`Skeleton`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Joint {
    /// Unique in the skeleton: the glTF joint node's name.
    pub name: String,
    /// The joint it hangs on, declared before it; empty for a root.
    pub parent: String,
    /// Where the joint is, in the solid's coordinates. Its bone turns
    /// about this point; its axes at rest are the solid's.
    pub at: DVec3,
}

/// A skeleton as joints at points, parents before children.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Skeleton {
    /// The joints.
    pub joints: Vec<Joint>,
}

/// What [`bind`] measured. Counts, no verdicts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BindReport {
    /// Vertices weighted.
    pub vertices: usize,
    /// Joints that carry weight (those with children, or a lone joint).
    pub weighted_bones: usize,
    /// Vertices that see no bone, so take their weights from the heat
    /// around them.
    pub unseen: usize,
    /// Pieces of the mesh (by shared edges).
    pub pieces: usize,
    /// Pieces that see no bone, heated by their nearest bones instead.
    pub pieces_unseen: usize,
    /// Per joint (in the skeleton's order), the vertices whose largest
    /// weight is that joint's.
    pub vertices_by_bone: Vec<usize>,
    /// The most conjugate-gradient steps a bone's system took.
    pub iterations: usize,
    /// The largest relative residual left in a bone's system.
    pub residual: f64,
}

/// Relative residual at which a bone's system counts as solved.
const TOLERANCE: f64 = 1e-9;

/// Weights below this, after solving, are dropped before the four
/// largest are kept and normalized.
const SMALLEST: f64 = 1e-3;

fn check(sk: &Skeleton) -> Result<Vec<Option<u32>>, RigError> {
    if sk.joints.is_empty() {
        return Err(RigError::NoBones);
    }
    let mut index: BTreeMap<&str, u32> = BTreeMap::new();
    let mut parents = Vec::with_capacity(sk.joints.len());
    for j in &sk.joints {
        if !j.at.is_finite() {
            return Err(RigError::NotFinite(format!("joint {:?}", j.name)));
        }
        if index.contains_key(j.name.as_str()) {
            return Err(RigError::DuplicateBone(j.name.clone()));
        }
        let parent = if j.parent.is_empty() {
            None
        } else {
            Some(
                *index
                    .get(j.parent.as_str())
                    .ok_or_else(|| RigError::UnknownParent {
                        bone: j.name.clone(),
                        parent: j.parent.clone(),
                    })?,
            )
        };
        index.insert(j.name.as_str(), parents.len() as u32);
        parents.push(parent);
    }
    Ok(parents)
}

/// The point of segment `a → b` nearest `p`.
fn nearest_on_segment(p: DVec3, a: DVec3, b: DVec3) -> DVec3 {
    let d = b - a;
    let dd = d.dot(d);
    if dd == 0.0 {
        return a;
    }
    let t = ((p - a).dot(d) / dd).clamp(0.0, 1.0);
    a + d * t
}

/// The cotangent stiffness (rows of `(column, value)` by column, the
/// diagonal included) and each vertex's area (a third of its triangles').
fn stiffness(s: &Solid) -> (Vec<Vec<(u32, f64)>>, Vec<f64>) {
    let n = s.positions.len();
    let mut rows: Vec<BTreeMap<u32, f64>> = vec![BTreeMap::new(); n];
    let mut area = vec![0.0; n];
    for t in &s.triangles {
        let p = t.map(|v| s.positions[v as usize]);
        let cross = (p[1] - p[0]).cross(p[2] - p[0]).length();
        let longest = (0..3)
            .map(|k| (p[(k + 1) % 3] - p[k]).length_squared())
            .fold(0.0, f64::max);
        // A sliver adds nothing: its cotangents would only spoil the
        // system's conditioning, and dropping a whole triangle keeps `L`
        // positive semi-definite.
        if cross.is_nan() || cross <= 1e-10 * longest {
            continue;
        }
        for k in 0..3 {
            area[t[k] as usize] += cross / 6.0;
            // The angle at corner k faces the edge (k + 1, k + 2).
            let (i, j) = (t[(k + 1) % 3], t[(k + 2) % 3]);
            let (u, v) = (p[(k + 1) % 3] - p[k], p[(k + 2) % 3] - p[k]);
            let w = u.dot(v) / cross / 2.0;
            *rows[i as usize].entry(j).or_insert(0.0) -= w;
            *rows[j as usize].entry(i).or_insert(0.0) -= w;
            *rows[i as usize].entry(i).or_insert(0.0) += w;
            *rows[j as usize].entry(j).or_insert(0.0) += w;
        }
    }
    (
        rows.into_iter().map(|r| r.into_iter().collect()).collect(),
        area,
    )
}

/// Solves `(L + diag(h)) x = b` by preconditioned conjugate gradients:
/// the steps taken and the relative residual left.
fn solve(rows: &[Vec<(u32, f64)>], h: &[f64], b: &[f64], x: &mut [f64]) -> (usize, f64) {
    let n = b.len();
    let apply = |v: &[f64], out: &mut [f64]| {
        for (i, row) in rows.iter().enumerate() {
            let mut sum = h[i] * v[i];
            for &(j, a) in row {
                sum += a * v[j as usize];
            }
            out[i] = sum;
        }
    };
    let diag: Vec<f64> = rows
        .iter()
        .enumerate()
        .map(|(i, row)| {
            let d = h[i]
                + row
                    .iter()
                    .find(|(j, _)| *j as usize == i)
                    .map_or(0.0, |(_, a)| *a);
            if d > 0.0 { 1.0 / d } else { 1.0 }
        })
        .collect();
    let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let norm_b = dot(b, b).sqrt();
    if norm_b == 0.0 {
        x.iter_mut().for_each(|v| *v = 0.0);
        return (0, 0.0);
    }
    let mut ax = vec![0.0; n];
    apply(x, &mut ax);
    let mut r: Vec<f64> = b.iter().zip(&ax).map(|(b, a)| b - a).collect();
    let mut z: Vec<f64> = r.iter().zip(&diag).map(|(r, d)| r * d).collect();
    let mut p = z.clone();
    let mut rz = dot(&r, &z);
    let mut q = vec![0.0; n];
    let limit = 20 * n.max(50);
    for step in 0..limit {
        let res = dot(&r, &r).sqrt() / norm_b;
        if res <= TOLERANCE {
            return (step, res);
        }
        apply(&p, &mut q);
        let pq = dot(&p, &q);
        if pq.is_nan() || pq <= 0.0 {
            return (step, res);
        }
        let alpha = rz / pq;
        for i in 0..n {
            x[i] += alpha * p[i];
            r[i] -= alpha * q[i];
        }
        for i in 0..n {
            z[i] = r[i] * diag[i];
        }
        let rz_next = dot(&r, &z);
        let beta = rz_next / rz;
        rz = rz_next;
        for i in 0..n {
            p[i] = z[i] + beta * p[i];
        }
    }
    (limit, dot(&r, &r).sqrt() / norm_b)
}

/// Each vertex's piece (by shared edges), numbered from 0 in vertex order,
/// and the number of pieces.
fn pieces(s: &Solid) -> (Vec<u32>, usize) {
    let n = s.positions.len();
    let mut up: Vec<u32> = (0..n as u32).collect();
    fn find(up: &mut [u32], mut v: u32) -> u32 {
        while up[v as usize] != v {
            up[v as usize] = up[up[v as usize] as usize];
            v = up[v as usize];
        }
        v
    }
    for t in &s.triangles {
        for k in 0..3 {
            let (a, b) = (find(&mut up, t[k]), find(&mut up, t[(k + 1) % 3]));
            if a != b {
                up[a.max(b) as usize] = a.min(b);
            }
        }
    }
    let mut id: BTreeMap<u32, u32> = BTreeMap::new();
    let mut out = Vec::with_capacity(n);
    for v in 0..n as u32 {
        let r = find(&mut up, v);
        let next = id.len() as u32;
        out.push(*id.entry(r).or_insert(next));
    }
    (out, id.len())
}

/// The solid with a skin for the skeleton, weighted by bone heat (see the
/// module docs), and what was measured. The bones' rest frames sit at the
/// joints with the solid's axes.
pub fn bind(s: &Solid, sk: &Skeleton) -> Result<(Solid, BindReport), RigError> {
    if s.skin.is_some() {
        return Err(RigError::AlreadySkinned);
    }
    if s.positions.iter().any(|p| !p.is_finite()) {
        return Err(RigError::NotFinite("a position".into()));
    }
    let parents = check(sk)?;
    let nj = sk.joints.len();
    // Each weighted bone's segments.
    let mut segments: Vec<Vec<(DVec3, DVec3)>> = vec![Vec::new(); nj];
    for (j, p) in parents.iter().enumerate() {
        if let Some(p) = p {
            segments[*p as usize].push((sk.joints[*p as usize].at, sk.joints[j].at));
        }
    }
    if segments.iter().all(Vec::is_empty) {
        for (j, p) in parents.iter().enumerate() {
            if p.is_none() {
                segments[j].push((sk.joints[j].at, sk.joints[j].at));
            }
        }
    }
    let weighted: Vec<u32> = (0..nj as u32)
        .filter(|&j| !segments[j as usize].is_empty())
        .collect();
    let n = s.positions.len();
    let scale = {
        let m = s.measure();
        (m.bounds_max - m.bounds_min).length().max(1e-9)
    };
    let floor = scale * 1e-6;
    let tree = Tree::new(s);
    let mut incident: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (i, t) in s.triangles.iter().enumerate() {
        for &v in t {
            incident[v as usize].push(i as u32);
        }
    }
    // Per vertex, its bones by distance, then the nearest one it sees.
    let mut heat = vec![0.0; n];
    let mut heater: Vec<Option<u32>> = vec![None; n];
    let mut nearest: Vec<(u32, f64)> = Vec::with_capacity(n);
    let mut hits = Vec::new();
    let mut unseen = 0;
    for v in 0..n {
        let p = s.positions[v];
        let mut by: Vec<(f64, u32, DVec3)> = weighted
            .iter()
            .map(|&b| {
                let (d, q) = segments[b as usize]
                    .iter()
                    .map(|&(a, c)| {
                        let q = nearest_on_segment(p, a, c);
                        ((q - p).length(), q)
                    })
                    .fold((f64::INFINITY, p), |x, y| if y.0 < x.0 { y } else { x });
                (d, b, q)
            })
            .collect();
        by.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        nearest.push((by[0].1, by[0].0.max(floor)));
        if incident[v].is_empty() {
            continue;
        }
        // Only bones up to twice as far as the nearest: one farther off
        // would only heat the vertex wrongly.
        let reach = by[0].0 * 2.0 + floor;
        let seen = by.iter().take_while(|x| x.0 <= reach).find(|(_, _, q)| {
            tree.crossings(p, *q, &mut hits);
            !hits
                .iter()
                .any(|(t, _)| incident[v].binary_search(t).is_err())
        });
        match seen {
            Some(&(d, b, _)) => {
                let d = d.max(floor);
                heat[v] = 1.0 / (d * d);
                heater[v] = Some(b);
            }
            None => unseen += 1,
        }
    }
    // A piece that sees no bone is heated by its nearest ones.
    let (piece, count) = pieces(s);
    let mut lit = vec![false; count];
    for v in 0..n {
        if heater[v].is_some() {
            lit[piece[v] as usize] = true;
        }
    }
    let pieces_unseen = lit.iter().filter(|l| !**l).count();
    for v in 0..n {
        if !lit[piece[v] as usize] {
            let (b, d) = nearest[v];
            heat[v] = 1.0 / (d * d);
            heater[v] = Some(b);
        }
    }
    let (rows, area) = stiffness(s);
    let h: Vec<f64> = (0..n).map(|v| area[v] * heat[v]).collect();
    let mut raw: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n];
    let mut report = BindReport {
        vertices: n,
        weighted_bones: weighted.len(),
        unseen,
        pieces: count,
        pieces_unseen,
        vertices_by_bone: vec![0; nj],
        ..Default::default()
    };
    let mut x = vec![0.0; n];
    for &b in &weighted {
        let rhs: Vec<f64> = (0..n)
            .map(|v| if heater[v] == Some(b) { h[v] } else { 0.0 })
            .collect();
        x.iter_mut().for_each(|x| *x = 0.0);
        let (steps, res) = solve(&rows, &h, &rhs, &mut x);
        report.iterations = report.iterations.max(steps);
        report.residual = report.residual.max(res);
        for v in 0..n {
            if x[v] >= SMALLEST {
                raw[v].push((b, x[v].min(1.0)));
            }
        }
    }
    let weights: Vec<Weights> = raw
        .into_iter()
        .enumerate()
        .map(|(v, r)| {
            let w = Weights::from_raw(r);
            // A vertex no heat reached (one with no triangle) is its
            // nearest bone's.
            if w.weights[0] == 0.0 {
                Weights::single(nearest[v].0)
            } else {
                w
            }
        })
        .collect();
    for w in &weights {
        report.vertices_by_bone[w.bones[0] as usize] += 1;
    }
    let bones = sk
        .joints
        .iter()
        .zip(&parents)
        .map(|(j, p)| {
            Bone::new(
                &j.name,
                *p,
                Frame {
                    origin: j.at,
                    ..Frame::IDENTITY
                },
            )
        })
        .collect();
    let mut out = s.clone();
    out.skin = Some(Skin { bones, weights });
    out.canonicalize()
        .map_err(|_| RigError::NotFinite("a position".into()))?;
    Ok((out, report))
}

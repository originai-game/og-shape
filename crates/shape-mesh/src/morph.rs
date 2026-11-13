//! Morph targets (blend shapes) on a [`crate::Solid`] (`docs/design.md`
//! §4, `docs/research.md` §43): named sets of per-vertex offsets, each
//! with a weight, that move the rest shape before a skin poses it.
//!
//! Entry: [`Morphs`]. A solid's `morphs` follow its vertices as its
//! skin's weights do: a new vertex has no offset until it is given one,
//! and vertices welded into one take the mean of their offsets.
//! [`carried`] gives a solid made from others (cut, joined, bevelled)
//! their targets.

use std::collections::BTreeMap;

use shape_core::DVec3;
use shape_core::order::cmp_dvec3;

use crate::{Skin, Solid, Weights};

/// One target: where every vertex goes at weight 1, as an offset from
/// its rest position.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Morph {
    /// The target's name (unique in its solid).
    pub name: String,
    /// The weight the shape rests at (glTF's `weights`).
    pub weight: f64,
    /// Per vertex of the solid, its offset at weight 1, in the solid's
    /// units.
    pub deltas: Vec<DVec3>,
}

/// A solid's morph targets, in the file's order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Morphs {
    /// The targets.
    pub targets: Vec<Morph>,
}

/// Why a solid's morph targets are not valid.
#[derive(Clone, Debug, PartialEq)]
pub enum MorphError {
    /// Two targets share a name.
    DuplicateTarget(String),
    /// A target's offsets do not match the vertices one to one.
    VertexCount {
        /// The target.
        target: String,
        /// Its offsets.
        deltas: usize,
        /// The solid's vertices.
        vertices: usize,
    },
    /// A target has an offset or a weight that is not finite.
    NotFinite(String),
}

impl core::fmt::Display for MorphError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MorphError::DuplicateTarget(n) => write!(f, "two morph targets are named {n:?}"),
            MorphError::VertexCount {
                target,
                deltas,
                vertices,
            } => write!(
                f,
                "morph target {target:?} has {deltas} offsets for {vertices} vertices"
            ),
            MorphError::NotFinite(n) => {
                write!(
                    f,
                    "morph target {n:?} has an offset or weight that is not finite"
                )
            }
        }
    }
}

impl Morphs {
    /// Checks the targets against a solid of `vertices` vertices.
    pub fn check(&self, vertices: usize) -> Result<(), MorphError> {
        for (i, t) in self.targets.iter().enumerate() {
            if self.targets[..i].iter().any(|o| o.name == t.name) {
                return Err(MorphError::DuplicateTarget(t.name.clone()));
            }
            if t.deltas.len() != vertices {
                return Err(MorphError::VertexCount {
                    target: t.name.clone(),
                    deltas: t.deltas.len(),
                    vertices,
                });
            }
            if !t.weight.is_finite() || t.deltas.iter().any(|d| !d.is_finite()) {
                return Err(MorphError::NotFinite(t.name.clone()));
            }
        }
        Ok(())
    }

    /// Every target given `vertices` offsets: new ones zero.
    pub fn resize(&mut self, vertices: usize) {
        for t in &mut self.targets {
            t.deltas.resize(vertices, DVec3::ZERO);
        }
    }

    /// The targets' rest weights.
    pub fn weights(&self) -> Vec<f64> {
        self.targets.iter().map(|t| t.weight).collect()
    }

    /// Vertex `to` given vertex `from`'s offsets (a copy of it).
    pub fn copy(&mut self, from: usize, to: usize) {
        for t in &mut self.targets {
            if let Some(d) = t.deltas.get(from).copied()
                && let Some(slot) = t.deltas.get_mut(to)
            {
                *slot = d;
            }
        }
    }

    /// Vertex `to` given the weighted sum of other vertices' offsets
    /// (`items`: vertex and share).
    pub fn blend(&mut self, to: usize, items: &[(usize, f64)]) {
        for t in &mut self.targets {
            let mut sum = DVec3::ZERO;
            for &(v, share) in items {
                sum += t.deltas.get(v).copied().unwrap_or(DVec3::ZERO) * share;
            }
            if let Some(slot) = t.deltas.get_mut(to) {
                *slot = sum;
            }
        }
    }

    /// The targets on new vertex indices (`remap[old]`, or `u32::MAX`
    /// for a vertex that is gone): vertices merged into one take the
    /// mean of their offsets, sorted first so their order does not
    /// matter; a new vertex no old one maps to has none.
    pub fn remapped(&self, remap: &[u32], vertices: usize) -> Morphs {
        let mut groups: Vec<Vec<u32>> = vec![Vec::new(); vertices];
        for (old, &new) in remap.iter().enumerate() {
            if let Some(g) = groups.get_mut(new as usize) {
                g.push(old as u32);
            }
        }
        Morphs {
            targets: self
                .targets
                .iter()
                .map(|t| Morph {
                    name: t.name.clone(),
                    weight: t.weight,
                    deltas: groups
                        .iter()
                        .map(|g| {
                            let mut copies: Vec<DVec3> = g
                                .iter()
                                .map(|&o| t.deltas.get(o as usize).copied().unwrap_or(DVec3::ZERO))
                                .collect();
                            weld(&mut copies)
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    /// How many groups of vertices `remap` merges whose offsets differ
    /// in some target: a place the mean changes the shape (lips that
    /// touch at rest and part in a target).
    pub fn torn(&self, remap: &[u32], vertices: usize) -> usize {
        let mut first: Vec<u32> = vec![u32::MAX; vertices];
        let mut torn = vec![false; vertices];
        for (old, &new) in remap.iter().enumerate() {
            let Some(slot) = first.get_mut(new as usize) else {
                continue;
            };
            if *slot == u32::MAX {
                *slot = old as u32;
            } else if self.targets.iter().any(|t| {
                t.deltas
                    .get(old)
                    .is_some_and(|d| cmp_dvec3(*d, t.deltas[*slot as usize]).is_ne())
            }) {
                torn[new as usize] = true;
            }
        }
        torn.iter().filter(|t| **t).count()
    }

    /// `positions` moved by the targets at `weights` (one a target; a
    /// missing one is the target's rest weight).
    pub fn applied(&self, positions: &[DVec3], weights: &[f64]) -> Vec<DVec3> {
        let mut out = positions.to_vec();
        for (k, t) in self.targets.iter().enumerate() {
            let w = weights.get(k).copied().unwrap_or(t.weight);
            if w == 0.0 {
                continue;
            }
            for (p, d) in out.iter_mut().zip(&t.deltas) {
                *p += *d * w;
            }
        }
        out
    }
}

/// The offset of vertices welded into one: their mean, summed in sorted
/// order.
fn weld(copies: &mut [DVec3]) -> DVec3 {
    let Some(first) = copies.first().copied() else {
        return DVec3::ZERO;
    };
    if copies.iter().all(|c| cmp_dvec3(*c, first).is_eq()) {
        return first;
    }
    copies.sort_by(|a, b| cmp_dvec3(*a, *b));
    let mut sum = DVec3::ZERO;
    for c in copies.iter() {
        sum += *c;
    }
    sum / copies.len() as f64
}

/// The closest point of triangle `t` to `p`, as weights of its corners.
fn closest(p: DVec3, t: [DVec3; 3]) -> [f64; 3] {
    let (ab, ac, ap) = (t[1] - t[0], t[2] - t[0], p - t[0]);
    let (d1, d2) = (ab.dot(ap), ac.dot(ap));
    if d1 <= 0.0 && d2 <= 0.0 {
        return [1.0, 0.0, 0.0];
    }
    let bp = p - t[1];
    let (d3, d4) = (ab.dot(bp), ac.dot(bp));
    if d3 >= 0.0 && d4 <= d3 {
        return [0.0, 1.0, 0.0];
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = if d1 - d3 != 0.0 { d1 / (d1 - d3) } else { 0.0 };
        return [1.0 - v, v, 0.0];
    }
    let cp = p - t[2];
    let (d5, d6) = (ab.dot(cp), ac.dot(cp));
    if d6 >= 0.0 && d5 <= d6 {
        return [0.0, 0.0, 1.0];
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = if d2 - d6 != 0.0 { d2 / (d2 - d6) } else { 0.0 };
        return [1.0 - w, 0.0, w];
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 - d3 >= 0.0 && d5 - d6 >= 0.0 {
        let sum = (d4 - d3) + (d5 - d6);
        let w = if sum != 0.0 { (d4 - d3) / sum } else { 0.0 };
        return [0.0, 1.0 - w, w];
    }
    let sum = va + vb + vc;
    if sum == 0.0 {
        return [1.0, 0.0, 0.0];
    }
    [va / sum, vb / sum, vc / sum]
}

/// The morph targets of `sources` carried onto `onto`, a solid made
/// from them (cut, joined, bevelled): each of its vertices takes, for
/// each target, the offset of the nearest point of the sources'
/// surfaces, blended from that triangle's corners. A vertex that was a
/// source's own keeps its offsets exactly; one a cut made on a face
/// moves as the face does there; one on a source without a target does
/// not move for it. Targets are those of every source, by name, in the
/// order met. `None` when no source has any.
pub fn carried(sources: &[&Solid], onto: &Solid) -> Option<Morphs> {
    let mut names: Vec<(String, f64)> = Vec::new();
    for s in sources {
        for t in s.morphs.iter().flat_map(|m| &m.targets) {
            if !names.iter().any(|(n, _)| *n == t.name) {
                names.push((t.name.clone(), t.weight));
            }
        }
    }
    if names.is_empty() {
        return None;
    }
    // Each source's targets, by the result's order (a missing one: none).
    let slots: Vec<Vec<Option<usize>>> = sources
        .iter()
        .map(|s| {
            names
                .iter()
                .map(|(n, _)| {
                    s.morphs
                        .as_ref()
                        .and_then(|m| m.targets.iter().position(|t| t.name == *n))
                })
                .collect()
        })
        .collect();
    let mut targets: Vec<Morph> = names
        .into_iter()
        .map(|(name, weight)| Morph {
            name,
            weight,
            deltas: vec![DVec3::ZERO; onto.positions.len()],
        })
        .collect();
    for (v, near) in nearest(sources, onto).into_iter().enumerate() {
        let Some((si, ti, w)) = near else { continue };
        let s = sources[si as usize];
        let corners = s.triangles[ti as usize];
        for (k, target) in targets.iter_mut().enumerate() {
            let Some(slot) = slots[si as usize][k] else {
                continue;
            };
            let Some(deltas) = s.morphs.as_ref().map(|m| &m.targets[slot].deltas) else {
                continue;
            };
            target.deltas[v] = deltas[corners[0] as usize] * w[0]
                + deltas[corners[1] as usize] * w[1]
                + deltas[corners[2] as usize] * w[2];
        }
    }
    Some(Morphs { targets })
}

/// Two sources are skinned, by skeletons that differ: one solid has one
/// skin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkinsDiffer;

/// The skin of `sources` carried onto `onto`, a solid made from them:
/// each of its vertices takes the weights of the nearest point of the
/// skinned sources' surfaces, blended from that triangle's corners
/// (the four largest kept, summing to 1). A vertex that was a skinned
/// source's own keeps its weights; one a cut made, or one of a source
/// with no skin, moves with the skinned surface nearest it. `None` when
/// no source is skinned; refused when two are, by different skeletons.
pub fn carried_skin(sources: &[&Solid], onto: &Solid) -> Result<Option<Skin>, SkinsDiffer> {
    let skinned: Vec<&Solid> = sources
        .iter()
        .copied()
        .filter(|s| s.skin.is_some())
        .collect();
    let Some(bones) = skinned
        .first()
        .and_then(|s| s.skin.as_ref())
        .map(|k| &k.bones)
    else {
        return Ok(None);
    };
    if skinned
        .iter()
        .any(|s| s.skin.as_ref().is_some_and(|k| k.bones != *bones))
    {
        return Err(SkinsDiffer);
    }
    let weights = nearest(&skinned, onto)
        .into_iter()
        .map(|near| {
            let Some((si, ti, w)) = near else {
                return Weights::single(0);
            };
            let s = skinned[si as usize];
            let of = &s.skin.as_ref().expect("skinned above").weights;
            let corners = s.triangles[ti as usize];
            // At a corner: the corner's own weights, to the bit.
            if let Some(k) = w.iter().position(|&x| x == 1.0) {
                return of[corners[k] as usize];
            }
            Weights::blend(&[
                (of[corners[0] as usize], w[0]),
                (of[corners[1] as usize], w[1]),
                (of[corners[2] as usize], w[2]),
            ])
        })
        .collect();
    Ok(Some(Skin {
        bones: bones.clone(),
        weights,
    }))
}

/// Whether `sources` can be made into one solid: refused when two are
/// skinned by different skeletons. An operation asks before its work,
/// and [`carry`] then cannot fail.
pub fn one_skin(sources: &[&Solid]) -> Result<(), SkinsDiffer> {
    let mut bones = sources
        .iter()
        .filter_map(|s| s.skin.as_ref())
        .map(|k| &k.bones);
    match bones.next() {
        Some(first) if bones.any(|b| b != first) => Err(SkinsDiffer),
        _ => Ok(()),
    }
}

/// `s` at rest and bare: without its morph targets and its skin, as an
/// operation that carries them afterwards ([`carry`]) works on it.
pub fn bare(s: &Solid) -> Solid {
    Solid {
        morphs: None,
        skin: None,
        ..s.clone()
    }
}

/// Whether `s` has morph targets or a skin for [`carry`] to carry.
pub fn rides(s: &Solid) -> bool {
    s.morphs.is_some() || s.skin.is_some()
}

/// Gives `onto`, made from `sources` at rest ([`bare`]), their morph
/// targets ([`carried`]) and their skin ([`carried_skin`]).
pub fn carry(sources: &[&Solid], onto: &mut Solid) -> Result<(), SkinsDiffer> {
    onto.morphs = carried(sources, onto);
    onto.skin = carried_skin(sources, onto)?;
    Ok(())
}

/// [`carry`] for an operation on one solid that moves with it (a
/// bevel, a subdivision): each morph target is what `redo` (the
/// operation again) makes of the solid at that target, less `onto`,
/// where that is the same mesh ([`matched`]: the same triangles
/// between the same vertices, in whatever order each keeps them); a
/// target whose mesh comes out otherwise stays as [`carried`] left it.
pub fn carry_redone(
    source: &Solid,
    onto: &mut Solid,
    redo: &dyn Fn(&Solid) -> Option<Solid>,
) -> Result<(), SkinsDiffer> {
    carry(&[source], onto)?;
    let (Some(from), Some(mut to)) = (&source.morphs, onto.morphs.take()) else {
        return Ok(());
    };
    let rest = bare(source);
    for (t, target) in from.targets.iter().zip(&mut to.targets) {
        let mut moved = rest.clone();
        for (p, delta) in moved.positions.iter_mut().zip(&t.deltas) {
            *p += *delta;
        }
        let Some(at) = redo(&moved) else { continue };
        let guess: Vec<DVec3> = onto
            .positions
            .iter()
            .zip(&target.deltas)
            .map(|(p, d)| *p + *d)
            .collect();
        if let Some(to_at) = matched(onto, &at, &guess) {
            for (v, delta) in target.deltas.iter_mut().enumerate() {
                *delta = at.positions[to_at[v] as usize] - onto.positions[v];
            }
        }
    }
    onto.morphs = Some(to);
    Ok(())
}

/// For each vertex of `a`, the vertex of `b` that is the same one, when
/// the two are one mesh: the same triangles between the same vertices,
/// whatever order each solid keeps them in. `guess` says about where
/// each of `a`'s vertices is in `b`; it only chooses where to start
/// (and, on a mesh alike from several sides, which side), and the rest
/// follows from triangle to triangle across edges. `None` when they
/// are not one mesh, or an edge has more than two triangles.
fn matched(a: &Solid, b: &Solid, guess: &[DVec3]) -> Option<Vec<u32>> {
    if a.positions.len() != b.positions.len() || a.triangles.len() != b.triangles.len() {
        return None;
    }
    // Each directed edge's triangle, by its third vertex.
    let across = |s: &Solid| -> Option<BTreeMap<(u32, u32), (u32, u32)>> {
        let mut m = BTreeMap::new();
        for (i, t) in s.triangles.iter().enumerate() {
            for k in 0..3 {
                let e = (t[k], t[(k + 1) % 3]);
                if m.insert(e, (t[(k + 2) % 3], i as u32)).is_some() {
                    return None;
                }
            }
        }
        Some(m)
    };
    let (ea, eb) = (across(a)?, across(b)?);
    let nearest = |p: DVec3| {
        let mut best = (f64::INFINITY, 0u32);
        for (i, q) in b.positions.iter().enumerate() {
            let d = (*q - p).length_squared();
            if d < best.0 {
                best = (d, i as u32);
            }
        }
        best.1
    };
    let mut to = vec![u32::MAX; a.positions.len()];
    let mut from = vec![u32::MAX; b.positions.len()];
    let mut done = vec![false; a.triangles.len()];
    let mut used = vec![false; b.triangles.len()];
    for (i, t) in a.triangles.iter().enumerate() {
        if done[i] {
            continue;
        }
        // A piece not yet reached: its first triangle, where the guess
        // puts it.
        let [x, y, z] = t.map(|v| nearest(guess[v as usize]));
        let &(third, j) = eb.get(&(x, y))?;
        if third != z {
            return None;
        }
        let mut queue = vec![(i as u32, j, [(t[0], x), (t[1], y), (t[2], z)])];
        while let Some((i, j, pairs)) = queue.pop() {
            if done[i as usize] || used[j as usize] {
                if done[i as usize] && used[j as usize] {
                    continue;
                }
                return None;
            }
            done[i as usize] = true;
            used[j as usize] = true;
            for (v, w) in pairs {
                if to[v as usize] == u32::MAX && from[w as usize] == u32::MAX {
                    to[v as usize] = w;
                    from[w as usize] = v;
                } else if to[v as usize] != w || from[w as usize] != v {
                    return None;
                }
            }
            for k in 0..3 {
                let ((v0, w0), (v1, w1)) = (pairs[k], pairs[(k + 1) % 3]);
                let (Some(&(v2, ni)), Some(&(w2, nj))) = (ea.get(&(v1, v0)), eb.get(&(w1, w0)))
                else {
                    if ea.contains_key(&(v1, v0)) || eb.contains_key(&(w1, w0)) {
                        return None;
                    }
                    continue;
                };
                if !done[ni as usize] {
                    queue.push((ni, nj, [(v1, w1), (v0, w0), (v2, w2)]));
                }
            }
        }
    }
    to.iter().all(|&w| w != u32::MAX).then_some(to)
}

/// For each vertex of `onto`, the nearest point of the sources'
/// surfaces: the source, its triangle, and the point as weights of the
/// triangle's corners. Of points equally near, the first source's
/// first triangle. `None` for every vertex when no source has a
/// triangle.
fn nearest(sources: &[&Solid], onto: &Solid) -> Vec<Option<(u32, u32, [f64; 3])>> {
    // A grid of the sources' triangles.
    let mut edge_sum = 0.0;
    let mut count = 0usize;
    for s in sources {
        for t in &s.triangles {
            let [a, b, c] = t.map(|v| s.positions[v as usize]);
            edge_sum += (b - a).length() + (c - b).length() + (a - c).length();
            count += 3;
        }
    }
    let cell = if count > 0 && edge_sum > 0.0 {
        edge_sum / count as f64 * 2.0
    } else {
        1.0
    };
    let key = |p: DVec3| {
        (
            (p.x / cell).floor() as i64,
            (p.y / cell).floor() as i64,
            (p.z / cell).floor() as i64,
        )
    };
    let mut grid: BTreeMap<(i64, i64, i64), Vec<(u32, u32)>> = BTreeMap::new();
    let (mut low, mut high) = (
        (i64::MAX, i64::MAX, i64::MAX),
        (i64::MIN, i64::MIN, i64::MIN),
    );
    for (si, s) in sources.iter().enumerate() {
        for (ti, t) in s.triangles.iter().enumerate() {
            let [a, b, c] = t.map(|v| s.positions[v as usize]);
            let (lo, hi) = (key(a.min(b).min(c)), key(a.max(b).max(c)));
            for x in lo.0..=hi.0 {
                for y in lo.1..=hi.1 {
                    for z in lo.2..=hi.2 {
                        grid.entry((x, y, z))
                            .or_default()
                            .push((si as u32, ti as u32));
                    }
                }
            }
            low = (low.0.min(lo.0), low.1.min(lo.1), low.2.min(lo.2));
            high = (high.0.max(hi.0), high.1.max(hi.1), high.2.max(hi.2));
        }
    }
    let reach = if count > 0 {
        (high.0 - low.0).max(high.1 - low.1).max(high.2 - low.2) + 2
    } else {
        0
    };
    onto.positions
        .iter()
        .map(|&p| {
            let at = key(p);
            // The nearest triangle: rings of cells outward until none
            // further out can be nearer.
            let mut best: Option<(f64, u32, u32, [f64; 3])> = None;
            let start = (at.0.clamp(low.0, high.0) - at.0)
                .abs()
                .max((at.1.clamp(low.1, high.1) - at.1).abs())
                .max((at.2.clamp(low.2, high.2) - at.2).abs());
            let mut ring = start;
            while count > 0 && ring <= start + reach {
                if best.is_none() && ring > start + 2 {
                    // Far from every surface: all the triangles in
                    // turn cost less than the empty cells between.
                    for (si, s) in sources.iter().enumerate() {
                        for (ti, t) in s.triangles.iter().enumerate() {
                            let t = t.map(|i| s.positions[i as usize]);
                            let w = closest(p, t);
                            let q = t[0] * w[0] + t[1] * w[1] + t[2] * w[2];
                            let d = (q - p).length_squared();
                            if best.is_none_or(|(b, ..)| d < b) {
                                best = Some((d, si as u32, ti as u32, w));
                            }
                        }
                    }
                    break;
                }
                if let Some((d, ..)) = best
                    && d <= ((ring - 1).max(0) as f64 * cell) * ((ring - 1).max(0) as f64 * cell)
                {
                    break;
                }
                for dx in -ring..=ring {
                    for dy in -ring..=ring {
                        for dz in -ring..=ring {
                            if dx.abs().max(dy.abs()).max(dz.abs()) != ring {
                                continue;
                            }
                            let cell_key = (at.0 + dx, at.1 + dy, at.2 + dz);
                            for &(si, ti) in grid.get(&cell_key).into_iter().flatten() {
                                let s = sources[si as usize];
                                let t = s.triangles[ti as usize].map(|i| s.positions[i as usize]);
                                let w = closest(p, t);
                                let q = t[0] * w[0] + t[1] * w[1] + t[2] * w[2];
                                let d = (q - p).length_squared();
                                let nearer = match best {
                                    None => true,
                                    Some((b, bs, bt, _)) => {
                                        d < b || (d == b && (si, ti) < (bs, bt))
                                    }
                                };
                                if nearer {
                                    best = Some((d, si, ti, w));
                                }
                            }
                        }
                    }
                }
                ring += 1;
            }
            best.map(|(_, si, ti, w)| (si, ti, w))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Solid;

    /// A square of two triangles, built as a file has it: the diagonal's
    /// two vertices twice. Target `lift` raises one corner; `tear` moves
    /// the two copies of a diagonal vertex apart.
    fn square() -> Solid {
        let mut s = Solid::default();
        let p = [DVec3::ZERO, DVec3::X, DVec3::new(1.0, 1.0, 0.0), DVec3::Y];
        for k in [0, 1, 2, 0, 2, 3] {
            s.push_vertex(p[k]);
        }
        s.push_triangle([0, 1, 2], 0, &["a"]);
        s.push_triangle([3, 4, 5], 0, &["a"]);
        let mut lift = vec![DVec3::ZERO; 6];
        lift[1] = DVec3::Z;
        let mut tear = vec![DVec3::ZERO; 6];
        tear[2] = DVec3::Z;
        tear[4] = -DVec3::Z;
        s.morphs = Some(Morphs {
            targets: vec![
                Morph {
                    name: "lift".into(),
                    weight: 0.5,
                    deltas: lift,
                },
                Morph {
                    name: "tear".into(),
                    weight: 0.0,
                    deltas: tear,
                },
            ],
        });
        s
    }

    #[test]
    fn targets_follow_their_vertices_through_a_weld_a_map_and_a_join() {
        let mut s = square();
        let plain = {
            let mut p = square();
            p.morphs = None;
            p.canonicalize().unwrap();
            p
        };
        // Welding: the copies that agree keep their offset; the two that
        // part take the mean, and `torn` counts the place.
        let order = shape_core::order::sorted_order(&s.positions);
        let mut remap = vec![0u32; 6];
        let mut groups = 0u32;
        for (k, &v) in order.iter().enumerate() {
            if k > 0 && s.positions[order[k - 1] as usize] != s.positions[v as usize] {
                groups += 1;
            }
            remap[v as usize] = groups;
        }
        assert_eq!(s.morphs.as_ref().unwrap().torn(&remap, 4), 1);
        s.canonicalize().unwrap();
        assert_eq!(s.positions, plain.positions);
        let m = s.morphs.clone().unwrap();
        m.check(4).unwrap();
        let at = |p: DVec3| s.positions.iter().position(|q| *q == p).unwrap();
        assert_eq!(m.targets[0].deltas[at(DVec3::X)], DVec3::Z);
        assert_eq!(
            m.targets[1].deltas[at(DVec3::new(1.0, 1.0, 0.0))],
            DVec3::ZERO
        );
        // The hash is the plain solid's only without targets.
        let hash = |s: &Solid| {
            let mut h = shape_core::OutputHash::new();
            s.hash_into(&mut h).unwrap();
            h.finish_hex()
        };
        assert_ne!(hash(&s), hash(&plain));
        // At its rest weights the lifted corner stands half way up; at
        // weights given, where they put it.
        assert_eq!(
            s.morphed(&[]).positions[at(DVec3::X)],
            DVec3::new(1.0, 0.0, 0.5)
        );
        let up = s.morphed(&[2.0, 0.0]);
        assert_eq!(up.positions[at(DVec3::X)], DVec3::new(1.0, 0.0, 2.0));
        assert!(up.morphs.is_none());
        // A map scales and turns an offset and does not shift it.
        let mut big = s.clone();
        big.map_positions(|p| p * 3.0 + DVec3::new(7.0, 0.0, 0.0), false);
        let d = &big.morphs.as_ref().unwrap().targets[0].deltas;
        assert_eq!(d[at(DVec3::X)], DVec3::new(0.0, 0.0, 3.0));
        // Joined with a solid that has another target, each side's
        // targets move its own vertices only.
        let mut other = plain.clone();
        other.map_positions(|p| p + DVec3::new(5.0, 0.0, 0.0), false);
        other.morphs = Some(Morphs {
            targets: vec![Morph {
                name: "sink".into(),
                weight: 0.0,
                deltas: vec![-DVec3::Z; 4],
            }],
        });
        let mut both = s.clone();
        both.append(&other);
        let m = both.morphs.as_ref().unwrap();
        m.check(8).unwrap();
        let names: Vec<&str> = m.targets.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["lift", "tear", "sink"]);
        assert!(m.targets[2].deltas[..4].iter().all(|d| *d == DVec3::ZERO));
        assert!(m.targets[0].deltas[4..].iter().all(|d| *d == DVec3::ZERO));
    }
}

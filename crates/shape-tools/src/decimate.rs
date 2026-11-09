//! Decimation by edge collapse, and LOD chains.
//!
//! The collapses are meshoptimizer's simplifier, through
//! `meshoptimizer-rs` (a pure-Rust port of meshoptimizer 1.3, MIT;
//! `docs/research.md` §9). It only removes vertices: every vertex left is
//! one of the input's, so positions keep their f64 bits.
//!
//! Names: each vertex is split per (surface, face names) before
//! simplifying, so the simplifier treats a change of surface or of face
//! names as a seam: it collapses along it but never across it. Every
//! output triangle takes the surface and face names of its corners. Edge
//! names survive on edges whose two ends survive and that still exist.

use std::collections::BTreeMap;
use std::fmt;

use meshoptimizer_rs::{Positions, SimplifyOptions, SimplifySettings, Workspace};
use shape_core::DVec3;
use shape_mesh::{Solid, Tag};

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
    /// Stop before any collapse would move the surface more than this, in
    /// mm; 0 is no limit.
    pub max_error: f64,
    /// Never move vertices on open borders (holes, the edge of a sheet).
    pub lock_border: bool,
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
    /// How far the surface moved, in mm (the simplifier's own estimate).
    pub error: f64,
}

/// Why a decimation could not run.
#[derive(Clone, Debug, PartialEq)]
pub enum DecimateError {
    /// Neither a ratio nor a target was given, or a value is out of range.
    BadDesc(String),
    /// The simplifier refused the mesh (non-finite position, overflow).
    Simplifier(String),
}

impl fmt::Display for DecimateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecimateError::BadDesc(w) => write!(f, "decimate: {w}"),
            DecimateError::Simplifier(w) => write!(f, "the simplifier refused the mesh: {w}"),
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

/// Decimates `s` towards the triangle count `d` asks for. The target may
/// not be reached when the error limit or the seams between surfaces stop
/// it; the report says how many triangles are left. A solid already at
/// or under the target comes back unchanged.
pub fn decimate(s: &Solid, d: &Decimate) -> Result<(Solid, DecimateReport), DecimateError> {
    let n = s.triangles.len();
    let target = target_of(d, n)?;
    decimate_to(s, target, d)
}

fn decimate_to(
    s: &Solid,
    target: usize,
    d: &Decimate,
) -> Result<(Solid, DecimateReport), DecimateError> {
    let n = s.triangles.len();
    let mut report = DecimateReport {
        triangles_before: n,
        triangles: n,
        target,
        error: 0.0,
    };
    if n <= target {
        return Ok((s.clone(), report));
    }
    // Split vertices per (vertex, surface, face names).
    let mut groups: BTreeMap<(u32, &[Tag]), u32> = BTreeMap::new();
    let mut ids: BTreeMap<(u32, u32), u32> = BTreeMap::new();
    let mut split_of: Vec<(u32, u32)> = Vec::new();
    let mut indices: Vec<u32> = Vec::with_capacity(n * 3);
    for (i, t) in s.triangles.iter().enumerate() {
        let next = groups.len() as u32;
        let g = *groups
            .entry((s.surfaces[i], s.face_tags[i].as_slice()))
            .or_insert(next);
        for &v in t {
            let next = split_of.len() as u32;
            let id = *ids.entry((v, g)).or_insert(next);
            if id == next {
                split_of.push((v, i as u32));
            }
            indices.push(id);
        }
    }
    // f32 positions about the box's centre, so the rounding is small.
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for p in &s.positions {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let centre = (lo + hi) / 2.0;
    let pos: Vec<[f32; 3]> = split_of
        .iter()
        .map(|&(v, _)| {
            let p = s.positions[v as usize] - centre;
            [p.x as f32, p.y as f32, p.z as f32]
        })
        .collect();
    let mut options = SimplifyOptions::ERROR_ABSOLUTE;
    if d.lock_border {
        options |= SimplifyOptions::LOCK_BORDER;
    }
    let settings = SimplifySettings {
        target_index_count: target * 3,
        target_error: if d.max_error > 0.0 {
            d.max_error as f32
        } else {
            f32::MAX
        },
        options,
    };
    let mut ws = Workspace::default();
    let out = meshoptimizer_rs::simplify(&indices, Positions::from_packed(&pos), settings, &mut ws)
        .map_err(|e| DecimateError::Simplifier(format!("{e:?}")))?;
    let mut result = Solid {
        positions: s.positions.clone(),
        names: s.names.clone(),
        edge_tags: s.edge_tags.clone(),
        ..Default::default()
    };
    for t in out.indices.chunks_exact(3) {
        // A triangle takes its first corner's surface and names.
        let (_, from) = split_of[t[0] as usize];
        result
            .triangles
            .push([t[0], t[1], t[2]].map(|k| split_of[k as usize].0));
        result.surfaces.push(s.surfaces[from as usize]);
        result.face_tags.push(s.face_tags[from as usize].clone());
    }
    let mut result = drop_unused(&result);
    result
        .canonicalize()
        .map_err(|e| DecimateError::Simplifier(e.to_string()))?;
    report.triangles = result.triangles.len();
    report.error = f64::from(out.error);
    Ok((result, report))
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
    use shape_gen::solids::sphere;

    #[test]
    fn halves_a_sphere_and_keeps_its_names() {
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
        assert!(r.error > 0.0 && r.error < 2.0);
        // Every vertex left is one of the input's.
        for p in &half.positions {
            assert!(s.positions.contains(p));
        }
        let chain = lods(
            &s,
            &[0.5, 0.25, 0.125].map(|ratio| Decimate {
                ratio,
                ..Default::default()
            }),
        )
        .unwrap();
        let counts: Vec<usize> = chain.iter().map(|l| l.0.triangles.len()).collect();
        assert!(counts[0] > counts[1] && counts[1] > counts[2], "{counts:?}");
        assert!(decimate(&s, &Decimate::default()).is_err());
    }
}

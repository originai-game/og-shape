//! A section swept along a path (img2threejs's tapered sweep, which
//! three.js has no class for): a closed 2D outline carried along the
//! path's Frenet frames (x along the normal, y along the binormal), scaled
//! and twisted by values keyed over the path's length, its ends capped.
//! Horns, tusks, handles, blades, tapered pipes and twisted columns.
//!
//! Rings are evenly spaced along the path's length (u along it, as
//! three.js `TubeGeometry`); v runs round the section, flipped to run down
//! the image. The sides shade smooth round the section and along the path.

use crate::extrude::ExtrudePath;
use crate::frames::frenet_frames;
use crate::shape::{area, triangulate_shape};
use crate::{Mesh, finish, normalize, trig};
use glam::{DVec2, DVec3};

/// A value at a fraction `t` of the path's length; between keys it is
/// linear, before the first and past the last it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub t: f64,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sweep {
    /// The section's outline (at least three points), either winding.
    pub section: Vec<DVec2>,
    /// Rings along the path, less one.
    pub steps: u32,
    /// The section's scale over t (none: 1).
    pub scale: Vec<Key>,
    /// The section's turn round the path over t, in radians (none: 0).
    pub twist: Vec<Key>,
    /// Close the two ends of an open path.
    pub caps: bool,
}

impl Default for Sweep {
    fn default() -> Self {
        Sweep {
            section: Vec::new(),
            steps: 64,
            scale: Vec::new(),
            twist: Vec::new(),
            caps: true,
        }
    }
}

/// The keys' value at `t`.
pub fn key_at(keys: &[Key], t: f64, default: f64) -> f64 {
    let Some(first) = keys.first() else {
        return default;
    };
    if t <= first.t {
        return first.value;
    }
    for w in keys.windows(2) {
        let (a, b) = (w[0], w[1]);
        if t <= b.t {
            let span = b.t - a.t;
            return if span > 0.0 {
                a.value + (b.value - a.value) * ((t - a.t) / span)
            } else {
                b.value
            };
        }
    }
    keys[keys.len() - 1].value
}

/// Refused for a section of fewer than three distinct points or of no
/// area, or a key that is not finite.
pub fn sweep(s: &Sweep, path: &ExtrudePath) -> Result<Mesh, String> {
    let mut section = s.section.clone();
    if section.len() > 2 && section[section.len() - 1] == section[0] {
        section.pop();
    }
    if section.len() < 3 {
        return Err(format!(
            "a section needs at least three points, got {}",
            section.len()
        ));
    }
    if section.iter().any(|p| !p.is_finite()) {
        return Err("the section holds a point that is not finite".into());
    }
    let a = area(&section);
    if a == 0.0 {
        return Err("the section encloses no area".into());
    }
    if a < 0.0 {
        section.reverse();
    }
    let sorted = |keys: &[Key], what: &str| -> Result<Vec<Key>, String> {
        if keys
            .iter()
            .any(|k| !(k.t.is_finite() && k.value.is_finite()))
        {
            return Err(format!("{what} holds a key that is not finite"));
        }
        let mut k = keys.to_vec();
        shape_core::sort::sort_by(&mut k, |a, b| a.t.total_cmp(&b.t));
        Ok(k)
    };
    let scale = sorted(&s.scale, "scale")?;
    let twist = sorted(&s.twist, "twist")?;
    let steps = s.steps.max(1) as usize;
    let n = section.len();
    let samples: Vec<(DVec3, DVec3)> = (0..=steps)
        .map(|j| (path.sample)(j as f64 / steps as f64))
        .collect();
    let tangents: Vec<DVec3> = samples.iter().map(|x| x.1).collect();
    let frames = frenet_frames(&tangents, path.closed);

    // The rings' points: (steps + 1) × n.
    let mut ring: Vec<DVec3> = Vec::with_capacity((steps + 1) * n);
    for j in 0..=steps {
        let t = j as f64 / steps as f64;
        let k = key_at(&scale, t, 1.0);
        let (sn, cs) = trig::sincos(key_at(&twist, t, 0.0));
        let f = &frames[j];
        for q in &section {
            let x = (q.x * cs - q.y * sn) * k;
            let y = (q.x * sn + q.y * cs) * k;
            ring.push(samples[j].0 + f.normal * x + f.binormal * y);
        }
    }
    if path.closed {
        for i in 0..n {
            ring[steps * n + i] = ring[i];
        }
    }
    // Smooth normals over the sides' logical vertices (the seam shared).
    let id = |j: usize, i: usize| j * n + i % n;
    let mut sums = vec![DVec3::ZERO; ring.len()];
    for j in 0..steps {
        for i in 0..n {
            let [a, b, c, d] = [id(j, i), id(j, i + 1), id(j + 1, i + 1), id(j + 1, i)];
            for [x, y, z] in [[a, b, c], [a, c, d]] {
                let face = (ring[z] - ring[y]).cross(ring[x] - ring[y]);
                for v in [x, y, z] {
                    sums[v] += face;
                }
            }
        }
    }
    if path.closed {
        for i in 0..n {
            let total = sums[i] + sums[steps * n + i];
            sums[i] = total;
            sums[steps * n + i] = total;
        }
    }
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    let row = n + 1;
    for j in 0..=steps {
        for i in 0..=n {
            positions.push(ring[id(j, i)]);
            normals.push(normalize(sums[id(j, i)]));
            uvs.push([j as f64 / steps as f64, 1.0 - i as f64 / n as f64]);
        }
    }
    for j in 0..steps {
        for i in 0..n {
            let a = (j * row + i) as u32;
            let (b, c, d) = (a + 1, a + 1 + row as u32, a + row as u32);
            indices.extend([a, b, c, a, c, d]);
        }
    }
    if s.caps && !path.closed {
        let mut outline = section.clone();
        let faces = triangulate_shape(&mut outline, &mut []);
        for (j, facing) in [(0usize, -1.0), (steps, 1.0)] {
            let base = positions.len() as u32;
            let nrm = frames[j].tangent * facing;
            for (i, q) in section.iter().enumerate() {
                positions.push(ring[j * n + i]);
                normals.push(normalize(nrm));
                uvs.push([q.x, 1.0 - q.y]);
            }
            for f in &faces {
                let [a, b, c] = f.map(|i| base + i as u32);
                indices.extend(if facing > 0.0 { [a, b, c] } else { [c, b, a] });
            }
        }
    }
    Ok(finish(&positions, &normals, &uvs, indices))
}

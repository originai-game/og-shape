//! three.js `LatheGeometry(points, segments, phiStart, phiLength)`: a
//! profile in the xy plane (x the distance from the axis) turned round
//! +Y. three.js's vertex order, indices and smooth normals (the profile's
//! normals averaged at each point, turned with it); uvs u round, v along
//! the profile (flipped to run down the image). Sines and cosines come
//! from [`crate::trig`], and a whole turn's last column takes the first
//! column's, so the seam closes exactly.

use crate::{Mesh, finish, normalize, trig};
use glam::{DVec2, DVec3};

#[derive(Clone, Debug, PartialEq)]
pub struct Lathe {
    pub points: Vec<DVec2>,
    pub segments: u32,
    pub phi_start: f64,
    pub phi_length: f64,
}

impl Default for Lathe {
    fn default() -> Self {
        Lathe {
            points: vec![
                DVec2::new(0.0, -0.5),
                DVec2::new(0.5, 0.0),
                DVec2::new(0.0, 0.5),
            ],
            segments: 12,
            phi_start: 0.0,
            phi_length: trig::TAU,
        }
    }
}

pub fn lathe(l: &Lathe) -> Mesh {
    let points = &l.points;
    let n = points.len();
    let segments = l.segments.max(1) as usize;
    let phi_length = l.phi_length.clamp(0.0, trig::TAU);
    let whole = phi_length == trig::TAU;
    let mut init_normals = Vec::with_capacity(n);
    let mut prev = DVec3::ZERO;
    for j in 0..n {
        if j + 1 == n {
            init_normals.push(normalize(prev));
            continue;
        }
        let d = points[j + 1] - points[j];
        let normal = DVec3::new(d.y, -d.x, d.y * 0.0);
        if j == 0 {
            prev = normal;
            init_normals.push(normalize(normal));
        } else {
            init_normals.push(normalize(normal + prev));
            prev = normal;
        }
    }
    let inverse = 1.0 / segments as f64;
    let mut positions = Vec::with_capacity((segments + 1) * n);
    let mut normals = Vec::with_capacity((segments + 1) * n);
    let mut uvs = Vec::with_capacity((segments + 1) * n);
    for i in 0..=segments {
        let k = if whole && i == segments { 0 } else { i };
        let (sin, cos) = trig::sincos(l.phi_start + k as f64 * inverse * phi_length);
        for j in 0..n {
            let p = points[j];
            positions.push(DVec3::new(p.x * sin, p.y, p.x * cos));
            uvs.push([
                i as f64 / segments as f64,
                1.0 - j as f64 / (n.max(2) - 1) as f64,
            ]);
            let nn = init_normals[j];
            normals.push(DVec3::new(nn.x * sin, nn.y, nn.x * cos));
        }
    }
    let mut indices = Vec::with_capacity(segments * n.saturating_sub(1) * 6);
    for i in 0..segments {
        for j in 0..n.saturating_sub(1) {
            let base = (j + i * n) as u32;
            let (a, b, c, d) = (base, base + n as u32, base + n as u32 + 1, base + 1);
            indices.extend([a, b, d, c, d, b]);
        }
    }
    finish(&positions, &normals, &uvs, indices)
}

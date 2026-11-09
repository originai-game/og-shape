//! three.js `Curve.computeFrenetFrames`: parallel-transport frames along a
//! curve's tangents, rotated by the angle between successive tangents,
//! and spread evenly round a closed curve so its ends meet. The rotations
//! take their cosine from the dot product and their sine from the square
//! root of one less its square, so no transcendental function decides a
//! frame; the closed curve's correction reads [`crate::trig`].

use crate::{normalize, trig};
use glam::DVec3;

/// A frame: the tangent and the normal and binormal across it, as three.js
/// returns them (`binormal = tangent × normal`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub tangent: DVec3,
    pub normal: DVec3,
    pub binormal: DVec3,
}

/// `v` turned round the unit axis `k` by the angle whose cosine and sine
/// are `c` and `s` (Rodrigues).
fn rotate(v: DVec3, k: DVec3, c: f64, s: f64) -> DVec3 {
    v * c + k.cross(v) * s + k * (k.dot(v) * (1.0 - c))
}

/// Frames at each tangent (three.js samples `segments + 1` of them with
/// `getTangentAt(i / segments)`).
pub fn frenet_frames(tangents: &[DVec3], closed: bool) -> Vec<Frame> {
    let n = tangents.len();
    if n == 0 {
        return Vec::new();
    }
    let t0 = tangents[0];
    let mut min = f64::MAX;
    let mut axis = DVec3::Z;
    let (tx, ty, tz) = (t0.x.abs(), t0.y.abs(), t0.z.abs());
    if tx <= min {
        min = tx;
        axis = DVec3::X;
    }
    if ty <= min {
        min = ty;
        axis = DVec3::Y;
    }
    if tz <= min {
        axis = DVec3::Z;
    }
    let vec = normalize(t0.cross(axis));
    let mut normals = vec![t0.cross(vec)];
    let mut binormals = vec![t0.cross(normals[0])];
    for i in 1..n {
        let mut normal = normals[i - 1];
        let v = tangents[i - 1].cross(tangents[i]);
        if v.length() > f64::EPSILON {
            let k = normalize(v);
            let c = tangents[i - 1].dot(tangents[i]).clamp(-1.0, 1.0);
            let s = (1.0 - c * c).max(0.0).sqrt();
            normal = rotate(normal, k, c, s);
        }
        binormals.push(tangents[i].cross(normal));
        normals.push(normal);
    }
    if closed && n > 1 {
        let segments = (n - 1) as f64;
        let c = normals[0].dot(normals[n - 1]).clamp(-1.0, 1.0);
        let mut theta = trig::atan2((1.0 - c * c).max(0.0).sqrt(), c) / segments;
        if t0.dot(normals[0].cross(normals[n - 1])) > 0.0 {
            theta = -theta;
        }
        for i in 1..n {
            let (s, c) = trig::sincos(theta * i as f64);
            normals[i] = rotate(normals[i], tangents[i], c, s);
            binormals[i] = tangents[i].cross(normals[i]);
        }
    }
    (0..n)
        .map(|i| Frame {
            tangent: tangents[i],
            normal: normals[i],
            binormal: binormals[i],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_straight_line_keeps_one_frame_and_frames_stay_orthonormal() {
        let f = frenet_frames(&[DVec3::Z; 4], false);
        assert!(f.iter().all(|x| x.normal == f[0].normal));
        let ts: Vec<DVec3> = (0..=16)
            .map(|i| {
                let (s, c) = trig::sincos(i as f64 / 16.0 * trig::TAU);
                DVec3::new(-s, 0.3, c).normalize()
            })
            .collect();
        for fr in frenet_frames(&ts, true) {
            assert!((fr.normal.length() - 1.0).abs() < 1e-9);
            assert!(fr.normal.dot(fr.tangent).abs() < 1e-9);
            assert!(fr.binormal.dot(fr.normal).abs() < 1e-9);
        }
    }
}

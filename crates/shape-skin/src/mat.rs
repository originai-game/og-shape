//! 4×4 matrices, column-major (glTF's order), `+ - * /` only.

use shape_core::{DVec3, trig};
use shape_mesh::skin::Mat4;

/// `a · b`: `b` first.
pub fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut m = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[4 * k + r] * b[4 * c + k];
            }
            m[4 * c + r] = s;
        }
    }
    m
}

/// The point `p` moved by `m`.
pub fn apply(m: &Mat4, p: DVec3) -> DVec3 {
    DVec3::new(
        m[0] * p.x + m[4] * p.y + m[8] * p.z + m[12],
        m[1] * p.x + m[5] * p.y + m[9] * p.z + m[13],
        m[2] * p.x + m[6] * p.y + m[10] * p.z + m[14],
    )
}

/// The direction `v` turned by `m` (no translation).
pub fn apply_vector(m: &Mat4, v: DVec3) -> DVec3 {
    DVec3::new(
        m[0] * v.x + m[4] * v.y + m[8] * v.z,
        m[1] * v.x + m[5] * v.y + m[9] * v.z,
        m[2] * v.x + m[6] * v.y + m[10] * v.z,
    )
}

/// A turn of `angle` radians about the unit axis `k` through the origin
/// (Rodrigues).
pub fn turn(k: DVec3, angle: f64) -> Mat4 {
    let (sn, cs) = trig::sin_cos(angle);
    let r = |v: DVec3| v * cs + k.cross(v) * sn + k * (k.dot(v) * (1.0 - cs));
    let (x, y, z) = (r(DVec3::X), r(DVec3::Y), r(DVec3::Z));
    [
        x.x, x.y, x.z, 0.0, //
        y.x, y.y, y.z, 0.0, //
        z.x, z.y, z.z, 0.0, //
        0.0, 0.0, 0.0, 1.0,
    ]
}

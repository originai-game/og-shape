//! A right-handed orthonormal frame: an origin and three axes.

use glam::DVec3;

/// A placement in space: an origin and right-handed orthonormal axes.
///
/// The zero value is not a valid frame; start from [`Frame::IDENTITY`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    /// Where the frame's local origin sits.
    pub origin: DVec3,
    /// Local +X.
    pub x: DVec3,
    /// Local +Y.
    pub y: DVec3,
    /// Local +Z.
    pub z: DVec3,
}

impl Default for Frame {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Frame {
    /// The world frame.
    pub const IDENTITY: Frame = Frame {
        origin: DVec3::ZERO,
        x: DVec3::X,
        y: DVec3::Y,
        z: DVec3::Z,
    };

    /// A frame at `origin` whose +Z is `normal` and whose +X is `x_hint`
    /// made orthogonal to it. Returns `None` if `normal` is zero or
    /// `x_hint` is parallel to it.
    pub fn from_normal(origin: DVec3, normal: DVec3, x_hint: DVec3) -> Option<Frame> {
        let z = normal.try_normalize()?;
        let x = (x_hint - z * x_hint.dot(z)).try_normalize()?;
        let y = z.cross(x);
        Some(Frame { origin, x, y, z })
    }

    /// A local point in world coordinates.
    pub fn point_to_world(&self, p: DVec3) -> DVec3 {
        self.origin + self.vector_to_world(p)
    }

    /// A local direction in world coordinates.
    pub fn vector_to_world(&self, v: DVec3) -> DVec3 {
        self.x * v.x + self.y * v.y + self.z * v.z
    }

    /// A world point in local coordinates.
    pub fn point_to_local(&self, p: DVec3) -> DVec3 {
        self.vector_to_local(p - self.origin)
    }

    /// A world direction in local coordinates.
    pub fn vector_to_local(&self, v: DVec3) -> DVec3 {
        DVec3::new(v.dot(self.x), v.dot(self.y), v.dot(self.z))
    }

    /// The frame `local`, given in this frame's coordinates, in world
    /// coordinates.
    pub fn compose(&self, local: &Frame) -> Frame {
        Frame {
            origin: self.point_to_world(local.origin),
            x: self.vector_to_world(local.x),
            y: self.vector_to_world(local.y),
            z: self.vector_to_world(local.z),
        }
    }

    // ── placement (`docs/design.md` §2, the Placement row) ─────────────────

    /// A frame at `origin` looking along `normal`, with the axes a face
    /// gets (CadQuery's convention, as sand3d): on a face whose normal is
    /// ±Z, +X is world +X; on any other, X = Z × normal, so a wall facing
    /// −Y has X to the right and Y up. Components within 1e-6 of zero are
    /// cleaned off the normal first, so a face read as `(0, 1e-9, 1)` still
    /// gets world +X. `None` for a zero or non-finite normal.
    pub fn facing(origin: DVec3, normal: DVec3) -> Option<Frame> {
        let z = clean(normal)?;
        let x = DVec3::Z.cross(z);
        let x = if x.length() > 1e-6 {
            x.normalize()
        } else {
            DVec3::X
        };
        Some(Frame {
            origin,
            x,
            y: z.cross(x),
            z,
        })
    }

    /// A frame at `origin` looking along `normal` with +Y toward `up`
    /// (made orthogonal to the normal). `None` if either is zero or they
    /// are parallel.
    pub fn with_up(origin: DVec3, normal: DVec3, up: DVec3) -> Option<Frame> {
        let z = clean(normal)?;
        let y = clean(up)?;
        let y = (y - z * y.dot(z)).try_normalize()?;
        Some(Frame {
            origin,
            x: y.cross(z),
            y,
            z,
        })
    }

    /// The same frame moved `d` along its own +Z (its normal).
    pub fn offset(&self, d: f64) -> Frame {
        Frame {
            origin: self.origin + self.z * d,
            ..*self
        }
    }

    /// The same frame moved along its own axes (sand3d's `move`; three.js's
    /// `translateX`, `translateY`, `translateZ` at once).
    pub fn translate(&self, x: f64, y: f64, z: f64) -> Frame {
        Frame {
            origin: self.point_to_world(DVec3::new(x, y, z)),
            ..*self
        }
    }

    /// The same frame turned about its own X, then its own Y, then its own
    /// Z, by `rx`, `ry`, `rz` radians (three.js's Euler order `XYZ`;
    /// Blender's XYZ Euler). Sines and cosines within 1e-12 of 0 or ±1 are
    /// snapped onto them, so quarter turns are exact.
    pub fn turn(&self, rx: f64, ry: f64, rz: f64) -> Frame {
        let r = euler_xyz(rx, ry, rz);
        Frame {
            origin: self.origin,
            x: self.vector_to_world(r[0]),
            y: self.vector_to_world(r[1]),
            z: self.vector_to_world(r[2]),
        }
    }

    /// The inverse placement: world coordinates in this frame's.
    /// `self.compose(&self.inverse())` is the identity, to rounding.
    pub fn inverse(&self) -> Frame {
        let t = |v: DVec3| DVec3::new(self.x.dot(v), self.y.dot(v), self.z.dot(v));
        Frame {
            origin: -t(self.origin),
            x: t(DVec3::X),
            y: t(DVec3::Y),
            z: t(DVec3::Z),
        }
    }

    /// Whether the axes are orthonormal and right-handed to `tolerance`
    /// (a placement, not a mirror or a scale).
    pub fn is_rigid(&self, tolerance: f64) -> bool {
        let finite = [self.origin, self.x, self.y, self.z]
            .iter()
            .all(|v| v.is_finite());
        finite
            && (self.x.length() - 1.0).abs() <= tolerance
            && (self.y.length() - 1.0).abs() <= tolerance
            && (self.z.length() - 1.0).abs() <= tolerance
            && self.x.dot(self.y).abs() <= tolerance
            && self.y.dot(self.z).abs() <= tolerance
            && self.z.dot(self.x).abs() <= tolerance
            && (self.x.cross(self.y) - self.z).length() <= tolerance
    }
}

/// A unit vector with components within 1e-6 of zero cleaned off (mesh
/// normals carry rounding dust on axis-aligned faces).
fn clean(v: DVec3) -> Option<DVec3> {
    if !v.is_finite() {
        return None;
    }
    let v = v.try_normalize()?;
    let c = |x: f64| if x.abs() < 1e-6 { 0.0 } else { x };
    DVec3::new(c(v.x), c(v.y), c(v.z)).try_normalize()
}

/// sin and cos of `a`, snapped onto 0 and ±1 within 1e-12.
fn sin_cos_snapped(a: f64) -> (f64, f64) {
    let snap = |v: f64| {
        let r = v.round();
        if (v - r).abs() < 1e-12 { r + 0.0 } else { v }
    };
    let (s, c) = crate::trig::sin_cos(a);
    (snap(s), snap(c))
}

/// The columns of `Rz · Ry · Rx`: the images of X, Y and Z under a turn
/// about X, then about (the fixed) Y, then Z, which is the same as turning
/// about the moving X, Y, Z in that order when applied to a frame's axes.
fn euler_xyz(rx: f64, ry: f64, rz: f64) -> [DVec3; 3] {
    let (sx, cx) = sin_cos_snapped(rx);
    let (sy, cy) = sin_cos_snapped(ry);
    let (sz, cz) = sin_cos_snapped(rz);
    let r = |v: DVec3| {
        let v = DVec3::new(v.x, cx * v.y - sx * v.z, sx * v.y + cx * v.z);
        let v = DVec3::new(cy * v.x + sy * v.z, v.y, -sy * v.x + cy * v.z);
        DVec3::new(cz * v.x - sz * v.y, sz * v.x + cz * v.y, v.z)
    };
    [r(DVec3::X), r(DVec3::Y), r(DVec3::Z)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_normal_is_right_handed_and_round_trips() {
        let f = Frame::from_normal(
            DVec3::new(1.0, 2.0, 3.0),
            DVec3::new(0.0, 0.0, 2.0),
            DVec3::new(1.0, 1.0, 5.0),
        )
        .unwrap();
        assert!((f.x.cross(f.y) - f.z).length() < 1e-15);
        let p = DVec3::new(-4.0, 0.5, 7.0);
        assert!((f.point_to_local(f.point_to_world(p)) - p).length() < 1e-12);
    }

    #[test]
    fn facing_follows_cadquery_axes() {
        let top = Frame::facing(DVec3::ZERO, DVec3::new(0.0, 1e-9, 2.0)).unwrap();
        assert_eq!((top.x, top.y, top.z), (DVec3::X, DVec3::Y, DVec3::Z));
        // A wall facing −Y: X to the right (+X), Y up.
        let wall = Frame::facing(DVec3::ZERO, DVec3::NEG_Y).unwrap();
        assert_eq!((wall.x, wall.y), (DVec3::X, DVec3::Z));
        // Facing +X: X is +Y, Y is up.
        let front = Frame::facing(DVec3::ZERO, DVec3::X).unwrap();
        assert_eq!((front.x, front.y), (DVec3::Y, DVec3::Z));
        assert!(Frame::facing(DVec3::ZERO, DVec3::ZERO).is_none());
    }

    #[test]
    fn translate_offset_and_turn_use_own_axes() {
        let f = Frame::facing(DVec3::new(300.0, 0.0, 0.0), DVec3::X).unwrap();
        // move(0, 400): along the frame's Y, which is world up.
        assert_eq!(
            f.translate(0.0, 400.0, 0.0).origin,
            DVec3::new(300.0, 0.0, 400.0)
        );
        assert_eq!(f.offset(-10.0).origin, DVec3::new(290.0, 0.0, 0.0));
        // A quarter turn about own Z is exact.
        let t = f.turn(0.0, 0.0, crate::trig::PI / 2.0);
        assert_eq!((t.x, t.y, t.z), (DVec3::Z, DVec3::NEG_Y, DVec3::X));
        assert!(t.is_rigid(1e-12));
        let g = f.turn(0.3, -1.1, 2.0).translate(1.0, 2.0, 3.0);
        let id = g.compose(&g.inverse());
        assert!((id.origin).length() < 1e-12 && (id.x - DVec3::X).length() < 1e-12);
    }

    #[test]
    fn from_normal_rejects_degenerate_input() {
        assert!(Frame::from_normal(DVec3::ZERO, DVec3::ZERO, DVec3::X).is_none());
        assert!(Frame::from_normal(DVec3::ZERO, DVec3::Z, DVec3::Z * 3.0).is_none());
    }
}

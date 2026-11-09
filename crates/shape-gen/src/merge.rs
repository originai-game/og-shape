//! three.js `BufferGeometryUtils.mergeGeometries` with each part placed
//! first (its `position`, `quaternion` and `scale`, as an Object3D), and
//! mirrored through the origin's planes when asked: a symmetric prop is
//! one half and its mirror, an `array` is the same part at several
//! placements. Normals follow the placement (its inverse transpose), and
//! a mirrored part's triangles turn round so they still face out.

use crate::{Mesh, Placement, place};
use glam::{DMat4, DVec3};

/// Which world axes a part is mirrored along (its coordinate negated),
/// after its placement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mirror {
    pub x: bool,
    pub y: bool,
    pub z: bool,
}

/// One part: a mesh, where it goes, and its mirror.
#[derive(Clone, Debug, PartialEq)]
pub struct Part<'a> {
    pub mesh: &'a Mesh,
    pub placement: Placement,
    pub mirror: Mirror,
}

/// The parts in one mesh, in order. Vertex colours are kept when any part
/// has them (the others white). Refused for a singular placement.
pub fn merge(parts: &[Part]) -> Result<Mesh, String> {
    let mut out = Mesh::default();
    let colors = parts.iter().any(|p| !p.mesh.colors.is_empty());
    for (k, part) in parts.iter().enumerate() {
        let sign = |b: bool| if b { -1.0 } else { 1.0 };
        let mirror = DMat4::from_scale(DVec3::new(
            sign(part.mirror.x),
            sign(part.mirror.y),
            sign(part.mirror.z),
        ));
        let placed = place(part.mesh, mirror * part.placement.matrix())
            .map_err(|e| format!("part {k}: {e}"))?;
        let base = out.positions.len() as u32;
        out.positions
            .extend(placed.positions.iter().map(|p| p.as_vec3().to_array()));
        out.normals
            .extend(placed.normals.iter().map(|n| n.as_vec3().to_array()));
        out.uvs
            .extend(placed.uvs.iter().map(|u| [u[0] as f32, u[1] as f32]));
        if colors {
            out.colors
                .extend(placed.colors.iter().map(|c| c.map(|x| x as f32)));
        }
        out.indices.extend(placed.indices.iter().map(|i| i + base));
    }
    Ok(out)
}

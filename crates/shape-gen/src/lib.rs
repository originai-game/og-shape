#![allow(missing_docs)]
//! Copied from origin-engine's og-geom (`third_party/og-geom/SOURCE`).
//!
//! Procedural geometry for Origin Engine, with no renderer dependency
//! (`docs/design.md` §2.6): three.js's generators with three.js's
//! parameter names, defaults, vertex orders and counts ([`shape_geometry`],
//! [`extrude`], [`lathe`], [`polyhedron`] and its four solids), the one
//! three.js lacks ([`sweep`]: a section along a path, scaled and twisted
//! over it), and [`merge`]. CSG lives in `shape-bool`.
//!
//! Everything is deterministic on every host: the generators use IEEE
//! `+ - * /` and `sqrt` only (angles through [`trig`]), and CSG decides
//! every side of a plane with exact predicates. The runtime turns a
//! [`Mesh`] into a renderer geometry (og-runtime's `geometry.rs`), the cook
//! into a model (`type model`).
//!
//! The named generators (sand3d's, `docs/design.md` §2) return a
//! [`shape_mesh::Solid`]: [`solids`], [`paths`], [`loft`], [`body`],
//! [`wire`] and [`parts`]; [`path3`] flattens a path given as pieces.
//!
//! uvs follow og-render's convention: v runs down the image (glTF's), so
//! where three.js writes `v` this crate writes `1 - v`.

pub mod body;
pub mod earcut;
pub mod extrude;
pub mod frames;
pub mod lathe;
pub mod loft;
pub mod merge;
pub mod parts;
pub mod path2;
pub mod path3;
pub mod paths;
pub mod polyhedron;
pub mod shape;
pub mod solids;
pub mod spline;
pub mod sweep;
pub mod trig;
pub mod wire;

pub use extrude::{Extrude, extrude};
pub use frames::{Frame, frenet_frames};
pub use lathe::{Lathe, lathe};
pub use merge::{Mirror, Part, merge};
pub use path2::{Path2, Shape};
pub use polyhedron::{Polyhedron, dodecahedron, icosahedron, octahedron, polyhedron, tetrahedron};
pub use shape::{ShapePoints, shape_geometry};
pub use sweep::{Key, Sweep, sweep};

use glam::{DMat3, DMat4, DQuat, DVec3};

/// A triangle mesh: one entry per vertex in each attribute (`colors` empty
/// when every vertex is white), triangles three `indices` each,
/// counter-clockwise seen from the front.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Every attribute as its bit pattern, for byte-for-byte comparisons.
    pub fn bits(&self) -> Vec<u32> {
        let mut out = Vec::new();
        out.extend(self.positions.iter().flatten().map(|x| x.to_bits()));
        out.extend(self.normals.iter().flatten().map(|x| x.to_bits()));
        out.extend(self.uvs.iter().flatten().map(|x| x.to_bits()));
        out.extend(self.colors.iter().flatten().map(|x| x.to_bits()));
        out.extend(&self.indices);
        out
    }

    /// The enclosed volume (the sum of the signed tetrahedra from the
    /// origin): positive for a closed mesh facing out.
    pub fn volume(&self) -> f64 {
        self.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let p = |i: u32| DVec3::from(self.positions[i as usize].map(f64::from));
                p(t[0]).dot(p(t[1]).cross(p(t[2]))) / 6.0
            })
            .sum()
    }
}

/// Builds a mesh from f64 positions, normals and uvs (rounded once to f32).
#[doc(hidden)]
pub fn finish(positions: &[DVec3], normals: &[DVec3], uvs: &[[f64; 2]], indices: Vec<u32>) -> Mesh {
    Mesh {
        positions: positions.iter().map(|p| p.as_vec3().to_array()).collect(),
        normals: normals.iter().map(|n| n.as_vec3().to_array()).collect(),
        uvs: uvs.iter().map(|u| [u[0] as f32, u[1] as f32]).collect(),
        colors: Vec::new(),
        indices,
    }
}

/// three.js `computeVertexNormals`: each vertex the normalised sum of the
/// unnormalised face normals of the triangles that use it (an unindexed
/// mesh, every vertex its own, is flat).
#[doc(hidden)]
pub fn vertex_normals(positions: &[DVec3], indices: &[u32]) -> Vec<DVec3> {
    let mut sums = vec![DVec3::ZERO; positions.len()];
    for t in indices.as_chunks::<3>().0.iter() {
        let [a, b, c] = [t[0], t[1], t[2]].map(|i| i as usize);
        let (pa, pb, pc) = (positions[a], positions[b], positions[c]);
        let face = (pc - pb).cross(pa - pb);
        for i in [a, b, c] {
            sums[i] += face;
        }
    }
    sums.into_iter().map(normalize).collect()
}

/// three.js `Vector3.normalize`: divided by its length, the zero vector
/// stays zero.
#[doc(hidden)]
pub fn normalize(v: DVec3) -> DVec3 {
    let l = v.length();
    if l == 0.0 { v } else { v / l }
}

/// Where a part goes (three.js's `position`, `quaternion`, `scale`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub position: DVec3,
    pub quaternion: DQuat,
    pub scale: DVec3,
}

impl Default for Placement {
    fn default() -> Self {
        Placement {
            position: DVec3::ZERO,
            quaternion: DQuat::IDENTITY,
            scale: DVec3::ONE,
        }
    }
}

impl Placement {
    /// three.js `Matrix4.compose`: scale, then rotate, then translate.
    pub fn matrix(&self) -> DMat4 {
        DMat4::from_scale_rotation_translation(self.scale, self.quaternion, self.position)
    }
}

/// A mesh's positions, normals and uvs carried through an affine map: the
/// normals by its inverse transpose, the triangles' winding reversed where
/// it mirrors (a negative determinant), so they still face out.
#[doc(hidden)]
pub struct Placed {
    pub positions: Vec<DVec3>,
    pub normals: Vec<DVec3>,
    pub uvs: Vec<[f64; 2]>,
    pub colors: Vec<[f64; 4]>,
    pub indices: Vec<u32>,
}

#[doc(hidden)]
pub fn place(mesh: &Mesh, m: DMat4) -> Result<Placed, String> {
    let linear = DMat3::from_mat4(m);
    let det = linear.determinant();
    if !(det.is_finite() && det != 0.0) {
        return Err("its placement is singular (a scale component is 0)".into());
    }
    let normal_m = linear.inverse().transpose();
    let n = mesh.positions.len();
    let positions = mesh
        .positions
        .iter()
        .map(|p| m.transform_point3(DVec3::from(p.map(f64::from))))
        .collect();
    let normals = (0..n)
        .map(|i| {
            let v = mesh
                .normals
                .get(i)
                .map_or(DVec3::Y, |v| DVec3::from(v.map(f64::from)));
            normalize(normal_m * v)
        })
        .collect();
    let uvs = (0..n)
        .map(|i| mesh.uvs.get(i).map_or([0.0; 2], |u| u.map(f64::from)))
        .collect();
    let colors = (0..n)
        .map(|i| mesh.colors.get(i).map_or([1.0; 4], |c| c.map(f64::from)))
        .collect();
    let mut indices = mesh.indices.clone();
    if det < 0.0 {
        for t in indices.as_chunks_mut::<3>().0.iter_mut() {
            t.swap(1, 2);
        }
    }
    Ok(Placed {
        positions,
        normals,
        uvs,
        colors,
        indices,
    })
}

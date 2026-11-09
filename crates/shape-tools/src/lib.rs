//! Tools on finished solids: export and the CPU turntable preview
//! (`docs/design.md` §7).
//!
//! Entry: [`Scene`], the parts to write or draw, each a named
//! [`shape_mesh::Solid`] with a [`Material`] and an optional [`Joint`].
//!
//! - [`glb`]: glTF 2.0 binary, written by hand: metres, Y up, normals smooth
//!   within a surface and split at real edges, metallic-roughness
//!   materials, joints in node extras as `s3d_joint`
//!   (`docs/research.md` §1.5). A skinned solid ([`shape_mesh::Skin`])
//!   is written as a glTF skin: bone nodes under its part's node, inverse
//!   bind matrices, JOINTS_0 and WEIGHTS_0 (`docs/research.md` §11).
//! - [`obj`] and [`stl`]: meshes only, metres; OBJ Y up, STL Z up.
//! - [`preview`]: a turntable of views from a deterministic rasterizer,
//!   as RGBA [`Image`]s; [`png`] encodes one for a person to look at, and
//!   [`sheet`] lays several side by side.
//!
//! Every output is a pure function of the scene: the same bytes on every
//! host. Programs are in millimetres with Z up; the files say which units
//! and axes they use.
//!
//! Mesh tools for generated meshes (`docs/research.md` §9), the chain
//! `import → repair → decimate → unwrap → measure → preview → export`:
//!
//! - [`import()`]: glTF, GLB, OBJ and FBX 7 into [`ImportedPart`]s, each a
//!   solid with its [`Affine`] transform, per-corner UVs and its textures'
//!   sizes ([`ImageInfo`]).
//! - [`repair()`]: [`Repair`] runs [`weld`], [`remove_degenerate`],
//!   [`remove_floaters`], [`orient`], [`close_holes`] and a self-union of
//!   interpenetrating shells; [`apply_transform`] and [`join`] place and
//!   merge parts.
//! - [`decimate()`] and [`lods`]: quadric error metrics; closed stays
//!   closed.
//! - [`unwrap()`]: charts, planar projection, packing; [`Uvs`] per
//!   corner.
//! - [`measure()`]: [`MeshMeasures`] and their JSON, no thresholds.
//! - [`glb_plain`]: one node, one mesh, positions, normals and UVs.

mod decimate;
mod export;
mod fbx;
mod image;
mod import;
mod intersect;
mod json;
mod jsonread;
mod measure;
mod meshglb;
mod png;
mod polygon;
mod preview;
mod repair;
mod skin;
mod surface;
mod unwrap;

pub use decimate::{Decimate, DecimateError, DecimateReport, collapse_short_edges, decimate, lods};
pub use export::{ExportError, Joint, JointKind, Material, Mimic, Part, Scene, glb, obj, stl};
pub use fbx::import_fbx;
pub use image::ImageInfo;
pub use import::{
    Affine, Import, ImportError, Imported, ImportedPart, Resolve, SkinScale, UNEVEN, import,
    import_gltf, import_obj, no_files,
};
pub use measure::{Measure, MeshMeasures, Piece, TextureMeasures, UvMeasures, measure};
pub use meshglb::{PlainGlb, PlainGlbError, glb_plain};
pub use png::png;
pub use preview::{Image, Preview, preview, sheet};
pub use repair::{
    Floaters, Repair, RepairError, RepairReport, Weld, apply_transform, close_holes, join, orient,
    remove_degenerate, remove_floaters, repair, weld,
};
pub use unwrap::{Unwrap, UnwrapError, Uvs, unwrap};

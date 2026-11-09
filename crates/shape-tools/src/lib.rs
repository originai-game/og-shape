//! Tools on finished solids: export and the CPU turntable preview
//! (`docs/design.md` §7).
//!
//! Entry: [`Scene`], the parts to write or draw, each a named
//! [`shape_mesh::Solid`] with a [`Material`] and an optional [`Joint`].
//!
//! - [`glb`]: glTF 2.0 binary, written by hand: metres, Y up, normals smooth
//!   within a surface and split at real edges, metallic-roughness
//!   materials, joints in node extras as `s3d_joint`
//!   (`docs/research.md` §1.5).
//! - [`obj`] and [`stl`]: meshes only, metres; OBJ Y up, STL Z up.
//! - [`preview`]: a turntable of views from a deterministic rasterizer,
//!   as RGBA [`Image`]s; [`png`] encodes one for a person to look at, and
//!   [`sheet`] lays several side by side.
//!
//! Every output is a pure function of the scene: the same bytes on every
//! host. Programs are in millimetres with Z up; the files say which units
//! and axes they use.

mod decimate;
mod export;
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
mod surface;
mod unwrap;

pub use decimate::{Decimate, DecimateError, DecimateReport, decimate, lods};
pub use export::{ExportError, Joint, JointKind, Material, Mimic, Part, Scene, glb, obj, stl};
pub use image::ImageInfo;
pub use import::{
    Affine, Import, ImportError, Imported, ImportedPart, Resolve, import, import_gltf, import_obj,
    no_files,
};
pub use measure::{Measure, MeshMeasures, Piece, TextureMeasures, UvMeasures, measure};
pub use meshglb::{PlainGlb, PlainGlbError, glb_plain};
pub use png::png;
pub use preview::{Image, Preview, preview, sheet};
pub use repair::{
    Floaters, Repair, RepairError, RepairReport, apply_transform, close_holes, join, orient,
    remove_degenerate, remove_floaters, repair, weld,
};
pub use unwrap::{Unwrap, UnwrapError, Uvs, unwrap};

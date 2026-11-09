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

mod export;
mod json;
mod png;
mod preview;
mod surface;

pub use export::{ExportError, Joint, JointKind, Material, Mimic, Part, Scene, glb, obj, stl};
pub use png::png;
pub use preview::{Image, Preview, preview, sheet};

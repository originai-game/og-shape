//! Models: named parts, groups and assemblies, their placement, the joints
//! between them, their materials and pose (`docs/design.md` §2 and §4,
//! `docs/plan.md` phase 4).
//!
//! Entry: [`Model`]. A program adds parts (solids from `shape-gen`),
//! places them on frames it reads off other parts ([`Model::face`],
//! [`Model::bbox`], [`Model::local`], and the distributors in [`place`]),
//! edits them (booleans, holes, pockets, fillets…), groups them and
//! declares joints, then exports or previews the posed model through
//! `shape-tools` ([`Model::build`]). Frames are read at rest: a read of
//! a part a posed joint moves is refused ([`ModelError::Posed`]);
//! [`Model::measure_posed`] and [`Model::seating`] measure the pose and
//! how each joint holds.
//!
//! - [`place`]: frames on boxes, faces and edges, and grids, rings,
//!   paths and snaps of frames.
//! - [`loft`]: `shape-gen`'s loft, refused when its surface passes
//!   through itself (a curve overshooting between sections).
//! - [`program`]: the same API as JSON operations, applied one at a time;
//!   a binding records the operations that change the model as a program
//!   that replays to the same bytes.
//!
//! Units: the model's [`Unit`] (millimetres by default) in, metres on
//! export. Angles are radians.

mod checked;
mod error;
mod model;
pub mod place;
pub mod program;

pub use checked::loft;
pub use error::ModelError;
pub use model::{BoolRecord, Built, Combine, Id, JointDesc, Model, PartMeasures, Seating, Unit};
pub use place::Path;
pub use shape_bool::{BoolOp, BoolReport};
pub use shape_core::Frame;
pub use shape_tools::{Interfering, JointKind, Material, Mimic, Placement, Preview};

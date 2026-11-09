//! `shape measure <program.json>`: measures of every part and of the
//! whole model as JSON (`docs/design.md` §7), with no thresholds.

use crate::args::{Args, Failure, failed, load};

pub fn run(a: Args) -> Result<String, Failure> {
    a.only(&[])?;
    let model = load(&a.program()?)?;
    shape_model::program::measures_json(&model).map_err(failed)
}

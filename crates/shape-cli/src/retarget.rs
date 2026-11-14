//! `shape retarget <rigged mesh> --from <animated file> [--clip NAME]
//! [--map SOURCE=TARGET,…] [-o out.glb]`: clips made for another
//! skeleton moved onto a skinned mesh (`shape_skin::retarget`), such as
//! one `shape rig` wrote. Bones match by name (Mixamo's prefix taken off)
//! and by the pairs given; both files stand up +Z, facing −Y, as read.
//!
//! The report gives, per clip, the pairs matched, the rig's bones left
//! to follow their parents, the source's bones unused, and how the root's
//! motion was scaled. The output keeps the mesh's own clips, a clip of
//! the same name replaced.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use shape_skin::{Retarget, retarget as move_clip};
use shape_tools::Import;

use crate::args::{Args, IMPORT_OPTIONS};
use crate::report::object;
use crate::source::{Input, Local, load_mesh};
use crate::{Failure, Files, failed};

/// What to retarget. The zero value has no input.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RetargetDesc {
    /// The skinned mesh the clips go onto.
    pub input: Input,
    /// How to read it.
    pub import: Import,
    /// The file the clips come from, with its skeleton.
    pub from: PathBuf,
    /// The clips to take, by name; empty takes them all.
    pub clips: Vec<String>,
    /// Extra bone pairs.
    pub retarget: Retarget,
    /// Where to write the mesh with its clips, if anywhere.
    pub out: Option<PathBuf>,
}

/// `SOURCE=TARGET,…` read into bone pairs.
pub fn pairs(v: &str) -> Result<Vec<(String, String)>, Failure> {
    v.split(',')
        .filter(|p| !p.trim().is_empty())
        .map(|p| {
            p.split_once('=')
                .map(|(s, t)| (s.trim().to_string(), t.trim().to_string()))
                .ok_or_else(|| Failure::Usage(format!("--map: {p:?} is not SOURCE=TARGET")))
        })
        .collect()
}

/// The retarget report: `clips`, `output`.
pub fn retarget(files: &dyn Files, d: &RetargetDesc) -> Result<String, Failure> {
    let mut mesh = load_mesh(files, &d.input, &d.import)?;
    let mut clips = mesh.clips()?;
    let target = mesh.take_joined()?;
    let Some(target_skin) = target.skin.clone() else {
        return Err(Failure::Failed(
            "the mesh has no skin: rig it first (shape rig)".into(),
        ));
    };
    let mut source = load_mesh(files, &Input::Path(d.from.clone()), &Import::default())?;
    let source_clips = source.clips()?;
    let source_solid = source.take_joined()?;
    let Some(source_skin) = source_solid.skin else {
        return Err(Failure::Failed(format!(
            "{}: no skin, so no skeleton for its clips",
            d.from.display()
        )));
    };
    let names: Vec<&str> = source_clips.iter().map(|c| c.name.as_str()).collect();
    for want in &d.clips {
        if !names.contains(&want.as_str()) {
            return Err(Failure::Usage(format!(
                "{}: no clip {want:?}; clips: {}",
                d.from.display(),
                names.join(", ")
            )));
        }
    }
    let mut report = Vec::new();
    for c in &source_clips {
        if !d.clips.is_empty() && !d.clips.contains(&c.name) {
            continue;
        }
        let (moved, r) = move_clip(&source_skin, c, &target_skin, &d.retarget).map_err(failed)?;
        report.push(json!({
            "name": c.name,
            "keys": c.times.len(),
            "duration": c.duration(),
            "matched": r.matched.iter().map(|(s, t)| json!([s, t])).collect::<Vec<_>>(),
            "unmatched": r.unmatched,
            "unused": r.unused,
            "scale": r.scale,
        }));
        clips.retain(|k| k.name != moved.name);
        clips.push(moved);
    }
    if report.is_empty() {
        return Err(Failure::Failed(format!(
            "{}: no clips to retarget",
            d.from.display()
        )));
    }
    let (material, _, _) = mesh.material();
    let output = crate::export::write_solid_clips(
        files,
        &mesh.name,
        &target,
        Some(&material),
        &clips,
        d.out.as_deref(),
    )?;
    Ok(object(&[
        ("clips", Value::Array(report).to_string()),
        ("output", output),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    let mut known = vec!["-o", "--out", "--from", "--clip", "--map"];
    known.extend(IMPORT_OPTIONS);
    a.only(&known)?;
    let from = a
        .value(&["--from"])?
        .ok_or_else(|| Failure::Usage("--from: the file the clips come from".into()))?;
    let clips = a
        .value(&["--clip"])?
        .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();
    let map = a
        .value(&["--map"])?
        .map(pairs)
        .transpose()?
        .unwrap_or_default();
    retarget(
        &Local,
        &RetargetDesc {
            input: a.input()?,
            import: a.import()?,
            from: Path::new(from).to_path_buf(),
            clips,
            retarget: Retarget { map },
            out: a.out_opt()?,
        },
    )
}

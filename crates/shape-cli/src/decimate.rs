//! `shape decimate <mesh> (--ratio R | --target N | --lod-ratios A,B,…) [--lods N] [-o out]`:
//! the file's parts joined and decimated (`shape_tools::decimate`), as
//! JSON: per level, the decimation's report and the measures after. With
//! `--lods N`, N levels, each decimated from the one before
//! (`shape_tools::lods`): level k keeps `ratio`^k of the triangles, or,
//! with a target, level 1 keeps `target` and each next level half the
//! one before. They are written as `<out stem>.lod1.<ext>` … With `-o`,
//! writes each level (a plain GLB, OBJ or STL).

use std::path::{Path, PathBuf};

use shape_tools::{Decimate, Import, Measure, lods};

use crate::args::{Args, IMPORT_OPTIONS};
use crate::report::{self, array, object};
use crate::source::{Input, Local, extension, load_mesh};
use crate::{Failure, Files, failed};

/// What to decimate. The zero value has no input.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecimateDesc {
    /// The mesh file.
    pub input: Input,
    /// How to read the mesh file: its unit (OBJ) and axes.
    pub import: Import,
    /// The decimation of each level (a ratio or a triangle target).
    pub decimate: Decimate,
    /// Levels of detail, each from the one before; 0 is one level
    /// written to `out` itself. Level k keeps `ratio`^k of the
    /// triangles; with a `target`, level 1 keeps it and each next level
    /// half the level before.
    pub lods: u32,
    /// The levels' shares of the input's triangles, one a level, each
    /// level decimated from the one before (`[0.5, 0.2, 0.05]`); empty
    /// for `lods`' rule. Each over 0 and at most 1, falling. Not with
    /// `lods`, a ratio or a target.
    pub lod_ratios: Vec<f64>,
    /// Where to write the result, if anywhere.
    pub out: Option<PathBuf>,
}

/// `<stem>.lod<k>.<ext>` beside `out`.
pub fn lod_path(out: &Path, k: u32) -> PathBuf {
    let stem = out.file_stem().and_then(|s| s.to_str()).unwrap_or("mesh");
    out.with_file_name(format!("{stem}.lod{k}.{}", extension(out)))
}

/// The levels of [`DecimateDesc::lods`]: `ratio`^k, or the target
/// halved at each level after the first. (Repeating one desc gave every
/// level the same count, since `shape_tools::lods` takes a ratio of the
/// input.)
fn lod_levels(d: &Decimate, n: u32) -> Vec<Decimate> {
    let mut levels = Vec::new();
    let (mut ratio, mut target) = (d.ratio, d.target);
    for _ in 0..n {
        levels.push(Decimate {
            ratio,
            target,
            ..*d
        });
        ratio *= d.ratio;
        target = target.div_ceil(2);
    }
    levels
}

/// The decimation report: `triangles_in` and `levels`, each with
/// `decimate`, `measures` and `output`.
pub fn decimate(files: &dyn Files, d: &DecimateDesc) -> Result<String, Failure> {
    let mesh = load_mesh(files, &d.input, &d.import)?;
    let joined = mesh.joined()?;
    let clips = mesh.clips()?;
    let levels = if !d.lod_ratios.is_empty() {
        let falling = d.lod_ratios.windows(2).all(|w| w[1] < w[0]);
        let shares = d.lod_ratios.iter().all(|&r| r > 0.0 && r <= 1.0);
        if d.lods > 0 || d.decimate.ratio != 0.0 || d.decimate.target != 0 || !falling || !shares {
            return Err(Failure::Usage(
                "lod ratios: shares of the input over 0 and at most 1, falling, and no \
                 --lods, --ratio or --target with them"
                    .into(),
            ));
        }
        let each: Vec<Decimate> = d
            .lod_ratios
            .iter()
            .map(|&ratio| Decimate {
                ratio,
                ..d.decimate
            })
            .collect();
        lods(&joined, &each).map_err(failed)?
    } else if d.lods == 0 {
        vec![shape_tools::decimate(&joined, &d.decimate).map_err(failed)?]
    } else {
        lods(&joined, &lod_levels(&d.decimate, d.lods)).map_err(failed)?
    };
    let (material, defaults, dropped) = mesh.material();
    let mut list = Vec::new();
    for (k, (s, r)) in levels.iter().enumerate() {
        let several = d.lods > 0 || !d.lod_ratios.is_empty();
        let out = match (&d.out, several) {
            (None, _) => None,
            (Some(o), false) => Some(o.clone()),
            (Some(o), true) => Some(lod_path(o, k as u32 + 1)),
        };
        let m = shape_tools::measure(s, &Measure::default());
        list.push(object(&[
            ("decimate", report::decimate(r)),
            ("measures", m.to_json()),
            (
                "output",
                crate::export::write_solid_clips(
                    files,
                    &mesh.name,
                    s,
                    Some(&material),
                    &clips,
                    out.as_deref(),
                )?,
            ),
        ]));
    }
    Ok(object(&[
        ("triangles_in", joined.triangles.len().to_string()),
        ("material", report::material(&material, &defaults, &dropped)),
        ("levels", array(&list)),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    a.only(&[
        "-o",
        "--out",
        "--ratio",
        "--target",
        "--max-error",
        "--lock-border",
        "--merge-charts",
        "--cross-color-borders",
        "--lods",
        "--lod-ratios",
        IMPORT_OPTIONS[0],
        IMPORT_OPTIONS[1],
        IMPORT_OPTIONS[2],
        IMPORT_OPTIONS[3],
    ])?;
    let dec = Decimate {
        ratio: a.number("--ratio")?.unwrap_or(0.0),
        target: a.number("--target")?.unwrap_or(0),
        max_error: a.number("--max-error")?.unwrap_or(0.0),
        lock_border: a.switch("--lock-border"),
        merge_charts: a.number("--merge-charts")?.unwrap_or(0.0),
        cross_color_borders: a.switch("--cross-color-borders"),
        ..Default::default()
    };
    // `--lod-ratios 0.5,0.2,0.05`: one share a level.
    let lod_ratios: Vec<f64> = match a.value(&["--lod-ratios"])? {
        None => Vec::new(),
        Some(list) => list
            .split(',')
            .map(|x| x.trim().parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| Failure::Usage("--lod-ratios: numbers with commas between".into()))?,
    };
    if dec.ratio == 0.0 && dec.target == 0 && lod_ratios.is_empty() {
        return Err(Failure::Usage(
            "give --ratio R, --target N or --lod-ratios A,B,…".into(),
        ));
    }
    decimate(
        &Local,
        &DecimateDesc {
            input: a.input()?,
            import: a.import()?,
            decimate: dec,
            lods: a.number("--lods")?.unwrap_or(0),
            lod_ratios,
            out: a.out_opt()?,
        },
    )
}

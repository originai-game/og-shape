//! `shape process <mesh> --budget N [-o out.glb]`: the mesh tools'
//! chain on one file (`guides/meshtools.md`): import → join → repair →
//! decimate to the triangle budget → unwrap → measure → export, as one
//! JSON report of every step. The output is a plain GLB with the UVs
//! (one node, one mesh), or OBJ or STL without them. Repair and unwrap
//! take their zero values unless set, as in `shape repair` and
//! `shape unwrap`.
//!
//! A mesh that carries UVs from its file keeps them, and its material with
//! every map (when its parts share one material), through repair and
//! decimation: the unwrap step is then skipped (`"unwrap": null`) unless
//! `--unwrap` asks for fresh UVs, which drops the old UVs before
//! decimation (their seams no longer hold it back) and every map (each
//! is listed in the material's `dropped`). Vertex colours and the
//! material's factors are kept either way. `--texture-max PX` shrinks
//! each map kept whose longest side is over `PX` texels
//! (`shape_tools::downsize`) and reports each map's size before and
//! after (`textures`).

use std::path::PathBuf;

use shape_tools::{
    Decimate, Downsize, ImageInfo, Import, Material, Measure, Repair, Unwrap, downsize,
};

use crate::args::{Args, IMPORT_OPTIONS};
use crate::export::write_solid;
use crate::report::{self, object};
use crate::source::{Input, Local, load_mesh};
use crate::{Failure, Files, failed};

/// The chain. The zero value has no input and no budget (no
/// decimation).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProcessDesc {
    /// The mesh file.
    pub input: Input,
    /// How to read an OBJ file.
    pub import: Import,
    /// The repair.
    pub repair: Repair,
    /// Decimate to at most this many triangles; 0 skips decimation, and
    /// so does a mesh already within it.
    pub budget: usize,
    /// The unwrap.
    pub unwrap: Unwrap,
    /// Unwrap even a mesh that carries UVs (its texture is then not
    /// written: it was painted for the old UVs).
    pub fresh_uvs: bool,
    /// Count self-intersecting triangle pairs in the final measures.
    pub intersections: bool,
    /// Shrink every map kept whose longest side is over this many texels
    /// (box filter, written as PNG); 0 keeps them as they are.
    pub texture_max: u32,
    /// Where to write the result, if anywhere.
    pub out: Option<PathBuf>,
}

/// The chain's steps, in the order [`process_steps`] calls them.
pub const STEPS: [&str; 7] = [
    "import", "repair", "decimate", "unwrap", "textures", "measure", "export",
];

/// Takes every map off a material whose UVs were replaced, and says so.
pub fn drop_maps(m: &mut Material) -> Vec<String> {
    m.maps_mut()
        .into_iter()
        .filter_map(|(kind, slot)| {
            slot.take().map(|t| {
                format!(
                    "{kind} {:?}: painted for the file's UVs, which the unwrap replaced",
                    t.name
                )
            })
        })
        .collect()
}

/// Each map's size, as reported.
fn map_size(i: &ImageInfo) -> String {
    object(&[
        ("width", i.width.map_or("null".into(), |w| w.to_string())),
        ("height", i.height.map_or("null".into(), |h| h.to_string())),
        ("file_bytes", i.file_bytes.to_string()),
        (
            "decoded_bytes_with_mips",
            i.decoded_bytes().map_or("null".into(), |b| b.1.to_string()),
        ),
    ])
}

/// Shrinks the material's maps to `max` texels (0: none); per map, its
/// kind, name, size before and after (null when kept as it was), and why
/// it could not be read.
fn texture_budget(m: &mut Material, max: u32) -> String {
    if max == 0 {
        return "null".into();
    }
    let mut out = Vec::new();
    for (kind, slot) in m.maps_mut() {
        let Some(t) = slot else { continue };
        let before = ImageInfo::from_bytes(&t.name, &t.bytes);
        let d = Downsize {
            max,
            color: Material::is_color_map(kind),
        };
        let (after, error) = match downsize(t, &d) {
            Ok(Some(small)) => {
                let info = ImageInfo::from_bytes(&small.name, &small.bytes);
                *t = small;
                (map_size(&info), "null".to_string())
            }
            Ok(None) => ("null".to_string(), "null".to_string()),
            Err(e) => ("null".to_string(), report::string(&e.to_string())),
        };
        out.push(object(&[
            ("kind", report::string(kind)),
            ("name", report::string(&before.name)),
            ("before", map_size(&before)),
            ("after", after),
            ("error", error),
        ]));
    }
    report::array(&out)
}

/// The chain's report: `import`, `repair`, `decimate` (null when
/// skipped), `unwrap`, `material` (what is written, with what was
/// dropped), `textures` (null without a limit), `measures` and
/// `output`.
pub fn process(files: &dyn Files, d: &ProcessDesc) -> Result<String, Failure> {
    process_steps(files, d, &mut |_| Ok(()))
}

/// As [`process`], calling `step(k)` before step `k` of [`STEPS`] (a
/// front end reports progress there). An error from `step` stops the
/// chain before that step and is returned; a step itself is never
/// interrupted.
pub fn process_steps(
    files: &dyn Files,
    d: &ProcessDesc,
    step: &mut dyn FnMut(usize) -> Result<(), Failure>,
) -> Result<String, Failure> {
    step(0)?;
    let mesh = load_mesh(files, &d.input, &d.import)?;
    let imported = report::imported(&mesh.format, &mesh.imported);
    let joined = mesh.joined()?;
    step(1)?;
    let (mut fixed, r) = shape_tools::repair(&joined, &d.repair).map_err(failed)?;
    if d.fresh_uvs {
        // The file's UVs are replaced after decimation, so their seams
        // must not hold the decimation back.
        fixed.uvs = None;
    }
    step(2)?;
    let (low, dec) = if d.budget > 0 && fixed.triangles.len() > d.budget {
        let (low, dr) = shape_tools::decimate(
            &fixed,
            &Decimate {
                target: d.budget,
                ..Default::default()
            },
        )
        .map_err(failed)?;
        (low, report::decimate(&dr))
    } else {
        (fixed, "null".to_string())
    };
    step(3)?;
    let mut low = low;
    let (mut material, defaults, mut dropped) = mesh.material();
    let unwrapped = if low.uvs.is_none() || d.fresh_uvs {
        let uvs = shape_tools::unwrap(&mut low, &d.unwrap).map_err(failed)?;
        dropped.extend(drop_maps(&mut material));
        report::unwrap(&uvs)
    } else {
        "null".to_string()
    };
    step(4)?;
    let textures = texture_budget(&mut material, d.texture_max);
    step(5)?;
    let images: Vec<ImageInfo> = material
        .maps()
        .into_iter()
        .map(|(_, t)| ImageInfo::from_bytes(&t.name, &t.bytes))
        .collect();
    let m = shape_tools::measure(
        &low,
        &Measure {
            images: &images,
            uv_resolution: d.unwrap.resolution,
            intersections: d.intersections,
            materials: core::slice::from_ref(&material),
        },
    );
    step(6)?;
    let output = write_solid(files, &mesh.name, &low, Some(&material), d.out.as_deref())?;
    Ok(object(&[
        ("import", imported),
        ("repair", report::repair(&r)),
        ("decimate", dec),
        ("unwrap", unwrapped),
        ("material", report::material(&material, &defaults, &dropped)),
        ("textures", textures),
        ("measures", m.to_json()),
        ("output", output),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    let mut known = vec![
        "-o",
        "--out",
        "--budget",
        "--intersections",
        "--unwrap",
        "--texture-max",
    ];
    known.extend(IMPORT_OPTIONS);
    known.extend(crate::repair::OPTIONS);
    known.extend(crate::unwrap::OPTIONS);
    a.only(&known)?;
    let mut repair = crate::repair::options(a)?;
    repair.check_intersections = false;
    process(
        &Local,
        &ProcessDesc {
            input: a.input()?,
            import: a.import()?,
            repair,
            budget: a.number("--budget")?.unwrap_or(0),
            unwrap: crate::unwrap::options(a)?,
            fresh_uvs: a.switch("--unwrap"),
            intersections: a.switch("--intersections"),
            texture_max: a.number("--texture-max")?.unwrap_or(0),
            out: a.out_opt()?,
        },
    )
}

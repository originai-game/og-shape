//! `shape process <mesh> (--budget N | --ratio R) [-o out.glb]`: the mesh tools'
//! chain on one file (`guides/meshtools.md`): import → join → repair →
//! decimate to the triangle budget (or `--ratio R` of what the repair
//! leaves) → place → unwrap → textures → measure →
//! export,
//! as one JSON report of every step. The place step (turn, size, pivot:
//! `shape place`) runs only when asked, and only then has a `place` key
//! in the report; it comes after the steps that drop or move vertices,
//! so the output stands on the base it reports. The repair's and the
//! decimation's lengths are the file's. The output is a plain GLB with the UVs
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
//!
//! `--bake PX` keeps the look through a fresh unwrap: the repaired mesh
//! (with its UVs, maps and vertex colours) is the source; a copy without
//! UVs or colours is decimated to the budget, its charts packed on a
//! PX × PX grid (`shape_tools::unwrap_atlas`; `--padding` texels apart,
//! 4 if not given), and the source's base colour (times its vertex
//! colours), metallic-roughness, emissive and occlusion maps and its
//! shading normals are baked into new maps in that layout
//! (`shape_tools::bake`; `--bake-distance` in the file's units, mm for a
//! glTF, 2 % of the diagonal if not given). The GLB carries the baked
//! maps, the material's factors and `TANGENT`. The report gains `bake`
//! (what was baked, texels without a source, the distances, the normal
//! map's angles), and `unwrap` is the atlas's report (its measures:
//! coverage, overlap, texel density). Without `--bake` nothing changes.

use std::path::PathBuf;

use shape_tools::{
    Decimate, Downsize, ImageInfo, Import, Material, Measure, Place, Repair, Unwrap, downsize,
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
    /// How to read the mesh file: its unit (OBJ) and axes.
    pub import: Import,
    /// The placement after decimation; the zero value skips it. Its `up`
    /// and `front` are kernel axes; the file's go in `import`.
    pub place: Place,
    /// The repair.
    pub repair: Repair,
    /// Decimate to at most this many triangles; 0 skips decimation, and
    /// so does a mesh already within it.
    pub budget: usize,
    /// Decimate to this share of the triangles the repair leaves, over 0
    /// and at most 1; 0 is off. Not with `budget`.
    pub ratio: f64,
    /// `Decimate::merge_charts`: UV islands that agree across a seam up
    /// to a rigid move within this distance are one chart for the seam
    /// rule; 0 is off.
    pub merge_charts: f64,
    /// `Decimate::cross_color_borders`: collapse across hard vertex-colour
    /// borders as anywhere.
    pub cross_color_borders: bool,
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
    /// Bake the source's look into maps of this many texels across, on
    /// fresh UVs (implies `fresh_uvs`); 0 does not bake.
    pub bake: u32,
    /// How far from the low surface a source point may be when baking, in
    /// the mesh's units (mm); 0 is 2 % of the diagonal.
    pub bake_distance: f64,
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
    let placing = d.place != Place::default();
    let (import, place) = if placing {
        let format = match &d.input {
            Input::Path(p) => crate::source::extension(p),
            _ => String::new(),
        };
        crate::place::plain_and_place(&format, &d.import, &d.place)?
    } else {
        (d.import, d.place)
    };
    let mesh = load_mesh(files, &d.input, &import)?;
    let imported = report::imported(&mesh.format, &mesh.imported);
    let joined = mesh.joined()?;
    step(1)?;
    let (mut fixed, r) = shape_tools::repair_owned(joined, &d.repair).map_err(failed)?;
    // The bake's source: the repaired mesh with its UVs and colours.
    let source = (d.bake > 0).then(|| fixed.clone());
    if source.is_some() {
        // Both are baked into the maps; neither holds the decimation back.
        fixed.colors = None;
    }
    if d.fresh_uvs || source.is_some() {
        // The file's UVs are replaced after decimation, so their seams
        // must not hold the decimation back.
        fixed.uvs = None;
    }
    step(2)?;
    let budget = if d.ratio == 0.0 {
        d.budget
    } else if d.budget > 0 || !(d.ratio > 0.0 && d.ratio <= 1.0) {
        return Err(Failure::Usage(
            "give --budget N or --ratio R (over 0, at most 1), not both".into(),
        ));
    } else {
        // Of what the repair leaves: the share asked, one triangle at least.
        ((fixed.triangles.len() as f64 * d.ratio).round() as usize).max(1)
    };
    let (low, dec) = if budget > 0 && fixed.triangles.len() > budget {
        let (low, dr) = shape_tools::decimate(
            &fixed,
            &Decimate {
                target: budget,
                merge_charts: d.merge_charts,
                cross_color_borders: d.cross_color_borders,
                ..Default::default()
            },
        )
        .map_err(failed)?;
        (low, report::decimate(&dr))
    } else {
        (fixed, "null".to_string())
    };
    step(3)?;
    let (mut material, defaults, mut dropped) = mesh.material();
    // Baked before placing: the bake's distances are the file's.
    let mut low = low;
    let baked = match &source {
        Some(src) => {
            let (atlas, baked) = bake_onto(&mesh, src, &mut low, d)?;
            dropped.extend(drop_maps(&mut material));
            material = baked.material;
            Some((atlas, baked.report))
        }
        None => None,
    };
    // Placed last of the steps that move vertices, so the box it stands
    // on is the output's.
    let (mut low, placed) = if placing {
        let (mut out, r) =
            shape_tools::place(std::slice::from_ref(&low), &place).map_err(failed)?;
        (out.pop().unwrap_or_default(), Some(r.to_json()))
    } else {
        (low, None)
    };
    let unwrapped = if let Some((atlas, _)) = &baked {
        atlas.to_json()
    } else if low.uvs.is_none() || d.fresh_uvs {
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
    let output = if baked.is_some() {
        crate::export::write_solid_tangents(files, &mesh.name, &low, &material, d.out.as_deref())?
    } else {
        write_solid(files, &mesh.name, &low, Some(&material), d.out.as_deref())?
    };
    let mut fields = vec![
        ("import", imported),
        ("repair", report::repair(&r)),
        ("decimate", dec),
    ];
    if let Some(p) = placed {
        fields.push(("place", p));
    }
    fields.push(("unwrap", unwrapped));
    if let Some((_, b)) = &baked {
        fields.push(("bake", b.to_json()));
    }
    fields.extend([
        ("material", report::material(&material, &defaults, &dropped)),
        ("textures", textures),
        ("measures", m.to_json()),
        ("output", output),
    ]);
    Ok(object(&fields))
}

/// Packs `low`'s fresh charts at `d.bake` texels and bakes `src` into
/// them, with the file's materials (per part, by the part's face name,
/// when they differ).
fn bake_onto(
    mesh: &crate::source::Mesh,
    src: &shape_mesh::Solid,
    low: &mut shape_mesh::Solid,
    d: &ProcessDesc,
) -> Result<(shape_tools::AtlasReport, shape_tools::Baked), Failure> {
    let padding = if d.unwrap.padding > 0.0 {
        d.unwrap.padding.ceil() as u32
    } else {
        4
    };
    let atlas = shape_tools::unwrap_atlas(
        low,
        &d.unwrap,
        &shape_tools::Atlas {
            size: d.bake,
            padding,
            no_turns: false,
        },
    )
    .map_err(failed)?;
    let parts = &mesh.imported.parts;
    let materials: Vec<Material> = parts.iter().map(|p| p.material.clone()).collect();
    let bare = |m: &Material| Material {
        name: String::new(),
        ..m.clone()
    };
    let same = materials.iter().all(|m| bare(m) == bare(&materials[0]));
    let material_of: Vec<u32> = if same {
        Vec::new()
    } else {
        src.face_tags
            .iter()
            .map(|tags| {
                tags.iter()
                    .find_map(|&t| {
                        let name = src.names.name(t);
                        parts.iter().position(|p| p.name == name)
                    })
                    .unwrap_or(0) as u32
            })
            .collect()
    };
    let (materials, material_of) = if same {
        (materials[..materials.len().min(1)].to_vec(), material_of)
    } else {
        (materials, material_of)
    };
    let baked = shape_tools::bake(
        &shape_tools::BakeSource {
            solid: src,
            materials: &materials,
            material_of: &material_of,
        },
        low,
        &shape_tools::Bake {
            size: d.bake,
            distance: d.bake_distance,
            ..Default::default()
        },
    )
    .map_err(failed)?;
    if baked.report.colors_baked {
        low.colors = None;
    }
    Ok((atlas, baked))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    let mut known = vec![
        "-o",
        "--out",
        "--budget",
        "--ratio",
        "--merge-charts",
        "--cross-color-borders",
        "--intersections",
        "--unwrap",
        "--texture-max",
        "--bake",
        "--bake-distance",
    ];
    known.extend(IMPORT_OPTIONS);
    known.extend(crate::repair::OPTIONS);
    known.extend(crate::unwrap::OPTIONS);
    known.extend(crate::place::OPTIONS);
    a.only(&known)?;
    let mut repair = crate::repair::options(a)?;
    repair.check_intersections = false;
    process(
        &Local,
        &ProcessDesc {
            input: a.input()?,
            import: a.import()?,
            place: crate::place::options(a)?,
            repair,
            budget: a.number("--budget")?.unwrap_or(0),
            ratio: a.number("--ratio")?.unwrap_or(0.0),
            merge_charts: a.number("--merge-charts")?.unwrap_or(0.0),
            cross_color_borders: a.switch("--cross-color-borders"),
            unwrap: crate::unwrap::options(a)?,
            fresh_uvs: a.switch("--unwrap"),
            intersections: a.switch("--intersections"),
            texture_max: a.number("--texture-max")?.unwrap_or(0),
            bake: a.number("--bake")?.unwrap_or(0),
            bake_distance: a.number("--bake-distance")?.unwrap_or(0.0),
            out: a.out_opt()?,
        },
    )
}

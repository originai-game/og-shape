//! `shape preview <program.json | mesh> -o <out.png>`: a turntable of the
//! posed model (or of a mesh file's parts, placed) from the CPU
//! rasterizer, its views side by side in one PNG; with `--each`, one PNG
//! a view with its camera; with `--uv`, the UV layout of each material
//! instead. A mesh file's parts are welded by exact position first, so
//! the seams a file splits its vertices at are not drawn as edges, and
//! each keeps its base-colour map.
//!
//! The report (stdout, JSON) lists every PNG written with what it shows:
//! the cameras (`shape_tools::ViewCamera`, in the scene's frame and in
//! the GLB's), or a UV view's mapping from pixels to UVs; and the maps
//! not drawn, with why.

use std::path::{Path, PathBuf};

use shape_tools::{Part, Preview, Scene, UvView, ViewCamera, uv_view, views};

use crate::args::{Args, IMPORT_OPTIONS};
use crate::export::written;
use crate::report::{array, object, string};
use crate::source::{Local, Source, load_uvs};
use crate::{Failure, Files, failed};

/// The views and how to lay them out. The zero value is the kernel's
/// default turntable (`shape_tools::Preview`) in one row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreviewDesc {
    /// The views.
    pub preview: Preview,
    /// Views a row; 0 puts them all in one row.
    pub columns: u32,
    /// One image a view, each with its camera, instead of one sheet.
    pub each: bool,
    /// The UV layout of each material instead of views.
    pub uv: Option<UvView>,
    /// A skinned mesh file's clip to pose it by, by name; `""` with a
    /// `time` or `frames` is the file's first. `None` draws the bind pose.
    pub clip: Option<String>,
    /// The time in the clip, seconds from its first key (held past its
    /// ends).
    pub time: f64,
    /// This many frames of the clip, evenly from its start to its end
    /// (the end left out, so a loop does not repeat a frame), one image
    /// each, the camera still; 0 draws one pose at `time`.
    pub frames: u32,
}

/// The most frames of one clip drawn at once.
pub const MAX_FRAMES: u32 = 720;

/// What [`render`] made.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rendered {
    /// PNG bytes, each with a JSON object (text) saying what it shows.
    pub images: Vec<(Vec<u8>, String)>,
    /// `[{"part", "why"}, …]`: maps and vertex colours not drawn.
    pub undrawn: String,
}

fn triple(v: [f64; 3]) -> String {
    array(&v.map(num))
}

/// A number as JSON (`null` if not finite, which a camera never is).
fn num(v: f64) -> String {
    serde_json::Value::from(v).to_string()
}

/// A camera as JSON: in the scene's frame (kernel units, +Z up), and in
/// the GLB's (metres, +Y up: kernel x, z, −y over 1000).
pub fn camera(c: &ViewCamera) -> String {
    let p = |v: shape_core::DVec3| triple([v.x, v.y, v.z]);
    let gltf = |v: shape_core::DVec3, k: f64| triple([v.x * k, v.z * k, -v.y * k]);
    object(&[
        ("position", p(c.position)),
        ("target", p(c.target)),
        ("up", p(c.up)),
        ("fov", c.fov.map_or("null".into(), num)),
        ("ortho_height", num(c.ortho_height)),
        ("width", c.width.to_string()),
        ("height", c.height.to_string()),
        (
            "gltf",
            object(&[
                ("position", gltf(c.position, 0.001)),
                ("target", gltf(c.target, 0.001)),
                ("up", gltf(c.up, 1.0)),
                ("ortho_height", num(c.ortho_height * 0.001)),
            ]),
        ),
    ])
}

/// What the source shows: a program's built scene, or a mesh's parts
/// placed, welded and each with its map.
fn with_scene<T>(
    source: &Source,
    pose: Option<(&str, f64)>,
    run: impl FnOnce(&Scene) -> Result<T, Failure>,
) -> Result<T, Failure> {
    match source {
        Source::Program(m) => {
            if pose.is_some() {
                return Err(Failure::Usage(
                    "--clip, --time and --frames pose a skinned mesh file; a program is posed \
                     by its own pose and clip ops"
                        .into(),
                ));
            }
            let built = m.build().map_err(failed)?;
            run(&built.scene())
        }
        Source::Mesh(mesh) => {
            // A file splits vertices at UV seams and hard normals; welded
            // by exact position, a seam is no open edge (drawn as a line)
            // and no crease (shaded flat).
            let mut placed = mesh.placed()?;
            if let Some((clip, time)) = pose {
                // Each skinned part by the clip of that name among its own.
                let mut any = false;
                for ((_, s), p) in placed.iter_mut().zip(&mesh.imported.parts) {
                    let found = match clip {
                        "" => p.clips.first(),
                        name => p.clips.iter().find(|c| c.name == name),
                    };
                    if let Some(m) = found.and_then(|c| c.at(time)) {
                        *s = shape_skin::pose_by(s, &m).map_err(failed)?;
                        any = true;
                    }
                }
                if !any {
                    return Err(Failure::Failed(format!(
                        "no clip {clip:?}; the file's clips: {:?}",
                        clip_names(mesh)
                    )));
                }
            }
            for (_, s) in &mut placed {
                s.canonicalize().map_err(failed)?;
            }
            let scene = Scene {
                name: mesh.name.clone(),
                parts: placed
                    .iter()
                    .zip(&mesh.imported.parts)
                    .map(|((n, s), p)| Part {
                        material: p.material.clone(),
                        ..Part::new(n, s)
                    })
                    .collect(),
            };
            run(&scene)
        }
    }
}

/// The names of a mesh file's clips, each once, in file order.
fn clip_names(mesh: &crate::source::Mesh) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for c in mesh.imported.parts.iter().flat_map(|p| &p.clips) {
        if !names.contains(&c.name) {
            names.push(c.name.clone());
        }
    }
    names
}

/// The turntable, its views, or the UV layouts, as PNG bytes; with
/// `frames`, a clip's frames, each as [`render`] draws one pose.
pub fn render(source: &Source, d: &PreviewDesc) -> Result<Rendered, Failure> {
    if d.frames == 0 {
        return render_pose(source, d, d.clip.as_deref().map(|c| (c, d.time)));
    }
    if d.frames > MAX_FRAMES {
        return Err(Failure::Usage(format!(
            "--frames {}: at most {MAX_FRAMES}",
            d.frames
        )));
    }
    let Source::Mesh(mesh) = source else {
        return Err(Failure::Usage(
            "--frames draws a skinned mesh file's clip".into(),
        ));
    };
    let name = d.clip.as_deref().unwrap_or("");
    let clip = mesh
        .imported
        .parts
        .iter()
        .flat_map(|p| &p.clips)
        .find(|c| name.is_empty() || c.name == name)
        .ok_or_else(|| {
            Failure::Failed(format!(
                "no clip {name:?}; the file's clips: {:?}",
                clip_names(mesh)
            ))
        })?;
    let (name, duration) = (clip.name.clone(), clip.duration());
    let times: Vec<f64> = (0..d.frames)
        .map(|k| duration * (f64::from(k) / f64::from(d.frames)))
        .collect();
    // One box for all the frames: the camera stands still.
    let mut frame: Option<(shape_core::DVec3, shape_core::DVec3)> = None;
    for &t in &times {
        with_scene(source, Some((&name, t)), |scene| {
            for p in scene.parts.iter().flat_map(|p| &p.solid.positions) {
                frame = Some(match frame {
                    None => (*p, *p),
                    Some((lo, hi)) => (lo.min(*p), hi.max(*p)),
                });
            }
            Ok(())
        })?;
    }
    let mut one = d.clone();
    one.preview.frame = frame;
    if one.preview.views == 0 {
        one.preview.views = 1;
    }
    // A frame is one image: its views side by side.
    one.each = false;
    let mut out = Rendered::default();
    for (k, &t) in times.iter().enumerate() {
        let r = render_pose(source, &one, Some((&name, t)))?;
        for (png, about) in r.images {
            let about = object(&[
                ("clip", string(&name)),
                ("frame", k.to_string()),
                ("time", num(t)),
                ("shows", about),
            ]);
            out.images.push((png, about));
        }
        out.undrawn = r.undrawn;
    }
    Ok(out)
}

/// [`render`] of one pose: the bind pose, or a clip at a time.
fn render_pose(
    source: &Source,
    d: &PreviewDesc,
    pose: Option<(&str, f64)>,
) -> Result<Rendered, Failure> {
    with_scene(source, pose, |scene| {
        if let Some(uv) = &d.uv {
            let layouts = uv_view(scene, uv).map_err(failed)?;
            if layouts.is_empty() {
                return Err(Failure::Failed(
                    "no part has UVs: nothing to lay out (a program's parts get them with \
                     `uvs`, a mesh with `unwrap`)"
                        .into(),
                ));
            }
            let mut undrawn = Vec::new();
            let images = layouts
                .iter()
                .map(|l| {
                    if let Some(e) = &l.map_error {
                        undrawn.push(object(&[("part", string(&l.parts[0])), ("why", string(e))]));
                    }
                    let about = object(&[
                        ("material", string(&l.material)),
                        (
                            "parts",
                            array(&l.parts.iter().map(|p| string(p)).collect::<Vec<_>>()),
                        ),
                        ("size", l.image.width.to_string()),
                        ("uv_min", array(&l.uv_min.map(num))),
                        ("tiles", l.tiles.to_string()),
                        ("under", string(if l.textured { "map" } else { "checker" })),
                    ]);
                    (shape_tools::png(&l.image), about)
                })
                .collect();
            return Ok(Rendered {
                images,
                undrawn: array(&undrawn),
            });
        }
        let v = views(scene, &d.preview).map_err(failed)?;
        let undrawn = array(
            &v.undrawn
                .iter()
                .map(|(p, why)| object(&[("part", string(p)), ("why", string(why))]))
                .collect::<Vec<_>>(),
        );
        let cameras: Vec<String> = v.cameras.iter().map(camera).collect();
        let images = if d.each {
            v.images
                .iter()
                .zip(&cameras)
                .enumerate()
                .map(|(k, (im, c))| {
                    (
                        shape_tools::png(im),
                        object(&[("view", k.to_string()), ("camera", c.clone())]),
                    )
                })
                .collect()
        } else {
            let columns = if d.columns == 0 {
                v.images.len() as u32
            } else {
                d.columns
            };
            vec![(
                shape_tools::png(&shape_tools::sheet(&v.images, columns)),
                object(&[
                    ("columns", columns.to_string()),
                    ("cameras", array(&cameras)),
                ]),
            )]
        };
        Ok(Rendered { images, undrawn })
    })
}

/// Where image `k` of `n` goes: `out` itself when there is one, else
/// `<stem>.<k>.<extension>` beside it, `k` with leading zeros to the
/// width of the last index.
pub fn image_path(out: &Path, k: usize, n: usize) -> PathBuf {
    if n == 1 {
        return out.to_path_buf();
    }
    let stem = out
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("preview");
    let ext = out.extension().and_then(|s| s.to_str()).unwrap_or("png");
    // As many digits as the last index has, so the names sort as the
    // views turn (`turn.000.png` … `turn.119.png`).
    let digits = (n - 1).to_string().len();
    out.with_file_name(format!("{stem}.{k:0digits$}.{ext}"))
}

/// Writes the images and gives the report.
pub fn write(files: &dyn Files, out: &Path, r: &Rendered) -> Result<String, Failure> {
    let n = r.images.len();
    let mut listed = Vec::with_capacity(n);
    for (k, (png, about)) in r.images.iter().enumerate() {
        let path = image_path(out, k, n);
        files.write(&path, png)?;
        listed.push(object(&[
            ("output", written(&path, png.len())),
            ("shows", about.clone()),
        ]));
    }
    Ok(object(&[
        ("images", array(&listed)),
        ("undrawn", r.undrawn.clone()),
    ]))
}

/// The command line.
pub fn run(a: &Args) -> Result<String, Failure> {
    a.only(&[
        "-o",
        "--out",
        "--views",
        "--width",
        "--height",
        "--columns",
        "--elevation",
        "--azimuth",
        "--no-edges",
        "--no-ground",
        "--fov",
        "--unlit",
        "--each",
        "--uv",
        "--uv-size",
        "--uvs",
        "--clip",
        "--time",
        "--frames",
        IMPORT_OPTIONS[0],
        IMPORT_OPTIONS[1],
        IMPORT_OPTIONS[2],
        IMPORT_OPTIONS[3],
    ])?;
    let out = a.out()?;
    if a.value(&["--uv-size"])?.is_some() && !a.switch("--uv") {
        return Err(Failure::Usage("--uv-size goes with --uv".into()));
    }
    let d = PreviewDesc {
        preview: Preview {
            width: a.number("--width")?.unwrap_or(0),
            height: a.number("--height")?.unwrap_or(0),
            views: a.number("--views")?.unwrap_or(0),
            elevation: a.number("--elevation")?,
            azimuth: a.number("--azimuth")?.unwrap_or(0.0),
            no_edges: a.switch("--no-edges"),
            no_ground: a.switch("--no-ground"),
            fov: a.number("--fov")?,
            unlit: a.switch("--unlit"),
            frame: None,
        },
        columns: a.number("--columns")?.unwrap_or(0),
        each: a.switch("--each"),
        uv: if a.switch("--uv") {
            Some(UvView {
                size: a.number("--uv-size")?.unwrap_or(0),
            })
        } else {
            None
        },
        // `--time` or `--frames` alone take the file's first clip.
        clip: match a.value(&["--clip"])? {
            Some(c) => Some(c.to_string()),
            None if a.value(&["--time"])?.is_some() || a.value(&["--frames"])?.is_some() => {
                Some(String::new())
            }
            None => None,
        },
        time: a.number("--time")?.unwrap_or(0.0),
        frames: a.number("--frames")?.unwrap_or(0),
    };
    let source = load_uvs(&Local, &a.input()?, &a.import()?, a.switch("--uvs"))?;
    write(&Local, &out, &render(&source, &d)?)
}

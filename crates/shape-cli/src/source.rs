//! What a command reads: a model program or a mesh file.
//!
//! Entry: [`load`]. An [`Input`] is a path or a program's text. A path
//! whose extension is `glb`, `gltf`, `obj` or `fbx` is a mesh, imported by
//! `shape_tools::import` (glTF's external buffers and images and OBJ's
//! material libraries are read beside it, through the same [`Files`]);
//! any other path is a program (`{"name", "ops"}`) replayed by
//! `shape_model::program::run_with`, with the fonts its `text` profiles
//! name read through the same [`Files`], relative to the program.

use std::path::{Path, PathBuf};

use shape_mesh::Solid;
use shape_model::Model;
use shape_tools::{Import, Imported, Part, Scene, Texture, apply_transform, import, join};

use crate::{Failure, failed};

/// File access for a command. The command line reads and writes plain
/// files ([`Local`]); the MCP server confines both to its workspace.
pub trait Files {
    /// The bytes of a file.
    fn read(&self, path: &Path) -> Result<Vec<u8>, Failure>;
    /// Writes a file, replacing it.
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), Failure>;
}

/// Plain files, paths as given.
#[derive(Clone, Copy, Debug, Default)]
pub struct Local;

impl Files for Local {
    fn read(&self, path: &Path) -> Result<Vec<u8>, Failure> {
        std::fs::read(path).map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))
    }

    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), Failure> {
        std::fs::write(path, bytes).map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))
    }
}

/// What to read. The zero value is no input, and an error.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Input {
    /// Nothing given.
    #[default]
    None,
    /// A program (`.json`) or a mesh file (`.glb`, `.gltf`, `.obj`,
    /// `.fbx`).
    Path(PathBuf),
    /// A program's JSON text.
    Program(String),
}

/// The extensions read as meshes.
pub const MESH_EXTENSIONS: [&str; 4] = ["glb", "gltf", "obj", "fbx"];

/// A path's extension, lower case ("" if none).
pub fn extension(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

/// Whether a path names a mesh file, by its extension.
pub fn is_mesh(p: &Path) -> bool {
    MESH_EXTENSIONS.contains(&extension(p).as_str())
}

/// An imported mesh file.
#[derive(Clone, Debug, Default)]
pub struct Mesh {
    /// The file's stem: the name outputs get.
    pub name: String,
    /// The format, by the file's extension (`glb`, `gltf`, `obj`, `fbx`).
    pub format: String,
    /// What the importer read.
    pub imported: Imported,
}

/// A program or a mesh, read.
#[derive(Clone, Debug)]
pub enum Source {
    /// A replayed program.
    Program(Model),
    /// An imported mesh.
    Mesh(Mesh),
}

/// Reads an input. `d` says how to read an OBJ file's units and axes.
pub fn load(files: &dyn Files, input: &Input, d: &Import) -> Result<Source, Failure> {
    match input {
        Input::None => Err(Failure::Usage(
            "no input: give a program or a mesh file".into(),
        )),
        Input::Program(text) => program(files, text, "program", Path::new("")).map(Source::Program),
        Input::Path(p) if is_mesh(p) => mesh_file(files, p, d).map(Source::Mesh),
        Input::Path(p) => {
            let bytes = files.read(p)?;
            let text = String::from_utf8(bytes)
                .map_err(|_| Failure::Failed(format!("{}: not UTF-8 text", p.display())))?;
            let dir = p.parent().map(Path::to_path_buf).unwrap_or_default();
            program(files, &text, &p.display().to_string(), &dir).map(Source::Program)
        }
    }
}

/// Reads an input that must be a mesh file.
pub fn load_mesh(files: &dyn Files, input: &Input, d: &Import) -> Result<Mesh, Failure> {
    match input {
        Input::Path(p) if is_mesh(p) => mesh_file(files, p, d),
        Input::None => Err(Failure::Usage("no input: give a mesh file".into())),
        _ => Err(Failure::Usage(format!(
            "this tool takes a mesh file ({}), not a program",
            MESH_EXTENSIONS.join(", ")
        ))),
    }
}

/// Replays a program, reading the fonts its `text` profiles name as paths
/// relative to `dir` (the program file's directory; for a program given
/// as text, the current directory or the server's workspace root).
fn program(files: &dyn Files, text: &str, what: &str, dir: &Path) -> Result<Model, Failure> {
    let fail = |e: shape_model::ModelError| Failure::Failed(format!("{what}: {e}"));
    let mut fonts = shape_model::program::Fonts::new();
    for name in shape_model::program::fonts_named(text).map_err(fail)? {
        let bytes = files
            .read(&dir.join(&name))
            .map_err(|e| Failure::Failed(format!("{what}: font {name:?}: {e}")))?;
        fonts.insert(name, bytes);
    }
    shape_model::program::run_with(text, &fonts).map_err(fail)
}

fn mesh_file(files: &dyn Files, path: &Path, d: &Import) -> Result<Mesh, Failure> {
    let bytes = files.read(path)?;
    let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let resolve = |uri: &str| -> Result<Vec<u8>, String> {
        files.read(&dir.join(uri)).map_err(|e| e.to_string())
    };
    let imported = import(&bytes, d, &resolve)
        .map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))?;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("mesh")
        .to_string();
    Ok(Mesh {
        name,
        format: extension(path),
        imported,
    })
}

impl Mesh {
    /// Every part placed by its transform, named; a skinned part as it is
    /// (its bones place it, in its bind pose).
    pub fn placed(&self) -> Result<Vec<(String, Solid)>, Failure> {
        self.imported
            .parts
            .iter()
            .map(|p| {
                let s = if p.solid.skin.is_some() {
                    p.solid.clone()
                } else {
                    apply_transform(p).map_err(failed)?
                };
                Ok((p.name.clone(), s))
            })
            .collect()
    }

    /// Every part placed and joined into one solid (`shape_tools::join`);
    /// refused for a skinned part.
    pub fn joined(&self) -> Result<Solid, Failure> {
        if self.imported.parts.is_empty() {
            return Err(Failure::Failed(format!(
                "{}: no meshes in the file",
                self.name
            )));
        }
        join(&self.imported.parts).map_err(failed)
    }

    /// The base-colour texture the joined solid can carry: the one every
    /// textured part shares. `None` when no part has one, or when parts
    /// have different ones (a plain GLB has one material).
    pub fn map(&self) -> Option<&Texture> {
        let mut maps = self.imported.parts.iter().filter_map(|p| p.map.as_ref());
        let first = maps.next()?;
        maps.all(|m| m == first).then_some(first)
    }

    /// Whether any part is skinned.
    pub fn skinned(&self) -> bool {
        self.imported.parts.iter().any(|p| p.solid.skin.is_some())
    }
}

/// An export scene of named solids, all hanging on the root.
pub fn scene<'a>(name: &str, parts: &'a [(String, Solid)]) -> Scene<'a> {
    Scene {
        name: name.to_string(),
        parts: parts.iter().map(|(n, s)| Part::new(n, s)).collect(),
    }
}

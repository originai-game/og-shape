//! `shape`: the og-shape command line, for tests and for the platform
//! before the merge (`docs/design.md` §5 and §7).
//!
//! ```text
//! shape build    <program.json>
//! shape export   <input> -o <out.glb|out.obj|out.stl> [--atlas PX]
//! shape measure  <input> [--intersections]
//! shape preview  <input> -o <out.png> [--views N] [--width PX]
//!                [--height PX] [--columns N] [--elevation RAD]
//!                [--azimuth RAD] [--no-edges] [--no-ground] [--fov RAD]
//!                [--unlit] [--each] [--uv [--uv-size PX]]
//! shape import   <mesh> [-o out]
//! shape repair   <mesh> [-o out] [--weld MM] [--weld-all] [--floaters F]
//!                [--floater-size MM] [--non-manifold] [--holes N]
//!                [--thicken MM] [--fuse] [--min-edge MM] [--intersections]
//! shape decimate <mesh> (--ratio R | --target N | --lod-ratios A,B,…)
//!                [--max-error MM]
//!                [--lock-border] [--merge-charts UV] [--cross-color-borders]
//!                [--lods N] [-o out]
//! shape unwrap   <mesh> [-o out.glb] [--angle DEG] [--resolution PX]
//!                [--padding TEXELS] [--shared] [--conformal]
//! shape place    <mesh> [-o out] [--up AXIS] [--front AXIS] [--turn DEG]
//!                [--longest L | --height L | --width L | --depth L | --scale F]
//!                [--pivot base|center|X,Y,Z]
//! shape compress <file.glb> -o out.glb [--draco [--position-bits N] [--normal-bits N]
//!                [--uv-bits N] [--color-bits N]]
//! shape collide  <input> [-o out] [--hulls N] [--concavity MM]
//!                [--max-vertices N]
//! shape process  <mesh> [--budget N | --ratio R] [--merge-charts UV]
//!                [--cross-color-borders] [-o out.glb]
//!                [repair, unwrap and place options] [--intersections]
//!                [--unwrap] [--texture-max PX] [--ktx2 | --ktx2-rdo Q | --etc1s]
//!                [--bake PX [--bake-distance MM]]
//! ```
//!
//! An input is a program (the JSON a binding records,
//! `shape_model::program`) or a mesh file (`.glb`, `.gltf`, `.obj`,
//! `.fbx`; OBJ with `--unit-mm MM`; OBJ and glTF with `--up AXIS` and
//! `--front AXIS`, the file's axes that are the object's up and front,
//! or `--z-up` for `--up +z --front -y`). `--uvs` on `build`,
//! `export`, `measure` and `preview` replays a program as if it began
//! with the `uvs` operation, so every part it makes has UVs. Reports go to stdout
//! as JSON with no thresholds; errors go to stderr with exit status 1 (2
//! for a usage error). Each subcommand is one module of the `shape_cli`
//! library, which the MCP server (`shape-mcp`) calls too.

use std::process::ExitCode;

use shape_cli::Failure;
use shape_cli::args::Args;

const USAGE: &str = "usage: shape <command> <input> [options]
  an input is a program (.json) or a mesh file (.glb .gltf .obj .fbx);
  OBJ takes --unit-mm MM (default metres); OBJ and glTF take --up AXIS and
  --front AXIS (+x -x +y -y +z -z: the file's axes that are the object's up and
  front; default up +y, front +z) or --z-up (up +z, front -y); a program takes --uvs
  (build, export, measure, preview): every part it makes gets UVs
  build    <program.json>                       report the built model as JSON
  export   <input> -o <out.glb|obj|stl>         write the posed model or the mesh;
                                                OBJ and STL at the pose if one is set;
                                                --atlas PX: a program's parts' UVs in
                                                one atlas, alike materials shared
  measure  <input> [--intersections] [--posed] [--axes]
                                                measures as JSON; --posed: a program
                                                at its pose, its joints and interference;
                                                --axes: a mesh's extents, area facing
                                                each way and where its mass sits
  preview  <input> -o <out.png> [--views N] [--width PX] [--height PX]
           [--columns N] [--elevation RAD] [--azimuth RAD] [--no-edges] [--no-ground]
           [--fov RAD] [--unlit] [--each] [--uv [--uv-size PX]]
                                                draws base-colour maps and vertex colours;
                                                --each: a PNG a view with its camera;
                                                --uv: each material's UV layout instead
  import   <mesh> [-o out.glb|obj|stl]          parts, images and what was skipped
  repair   <mesh> [-o out] [--weld MM] [--weld-all] [--floaters FRACTION]
           [--floater-size MM] [--non-manifold] [--holes N] [--thicken MM] [--fuse]
           [--min-edge MM] [--intersections]
  decimate <mesh> (--ratio R | --target N | --lod-ratios A,B,…) [--max-error MM]
           [--lock-border]
           [--merge-charts UV] [--cross-color-borders] [--lods N] [-o out]
  unwrap   <mesh> [-o out.glb] [--angle DEG] [--resolution PX] [--padding TEXELS] [--shared] [--conformal]
  place    <mesh> [-o out] [--up AXIS] [--front AXIS] [--turn DEG]
           [--longest L | --height L | --width L | --depth L | --scale F]
           [--pivot base|center|X,Y,Z]      turn, size (L with a unit: 1.2m, 90cm,
                                                900mm) and origin, before and after
  compress <file.glb> -o out.glb             meshopt, lossless (EXT_meshopt_compression)
           [--draco [--position-bits N] [--normal-bits N] [--uv-bits N] [--color-bits N]]
                                                Draco, quantized (KHR_draco_mesh_compression)
  collide  <input> [-o out] [--hulls N] [--concavity MM] [--max-vertices N]
                                                convex hulls over each part: colliders
  process  <mesh> [--budget N | --ratio R] [--merge-charts UV] [--cross-color-borders]
           [-o out.glb]
           [repair, unwrap and place options]
           [--intersections] [--unwrap] [--texture-max PX] [--ktx2 | --ktx2-rdo Q | --etc1s]
           [--bake PX [--bake-distance MM]]    --bake: fresh UVs packed at PX², the
                                                look baked into new maps";

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let result = match argv.split_first() {
        Some((cmd, rest)) => {
            let a = Args::new(rest);
            match cmd.as_str() {
                "build" => shape_cli::build::run(&a),
                "export" => shape_cli::export::run(&a),
                "measure" => shape_cli::measure::run(&a),
                "preview" => shape_cli::preview::run(&a),
                "import" => shape_cli::import::run(&a),
                "repair" => shape_cli::repair::run(&a),
                "decimate" => shape_cli::decimate::run(&a),
                "unwrap" => shape_cli::unwrap::run(&a),
                "place" => shape_cli::place::run(&a),
                "collide" => shape_cli::collide::run(&a),
                "compress" => shape_cli::compress::run(&a),
                "process" => shape_cli::process::run(&a),
                "help" | "-h" | "--help" => {
                    println!("{USAGE}");
                    return ExitCode::SUCCESS;
                }
                other => Err(Failure::Usage(format!("no command {other:?}"))),
            }
        }
        None => Err(Failure::Usage("no command".into())),
    };
    match result {
        Ok(out) => {
            if !out.is_empty() {
                println!("{out}");
            }
            ExitCode::SUCCESS
        }
        Err(Failure::Usage(why)) => {
            eprintln!("shape: {why}\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Failed(why)) => {
            eprintln!("shape: {why}");
            ExitCode::from(1)
        }
    }
}

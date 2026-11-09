//! The `shape` binary on the committed washing-machine program and on a
//! committed mesh with defects: each subcommand runs, files match the
//! library's bytes, reports say what the kernel did, and mistakes exit 2.

use std::process::Command;

const PROGRAM: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../golden/programs/washing_machine.json"
);

const MESH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../golden/meshes/ball_defects.obj"
);

fn shape(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_shape"))
        .args(args)
        .output()
        .expect("the binary runs")
}

#[test]
fn subcommands_match_the_library() {
    let dir = std::env::temp_dir().join(format!("shape-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let model = shape_model::program::run(&std::fs::read_to_string(PROGRAM).unwrap()).unwrap();
    let glb = dir.join("m.glb");
    let out = shape(&["export", PROGRAM, "-o", glb.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read(&glb).unwrap(), model.glb().unwrap());
    let out = shape(&["measure", PROGRAM]);
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(
        text.trim(),
        shape_model::program::measures_json(&model).unwrap()
    );
    let png = dir.join("m.png");
    let out = shape(&[
        "preview",
        PROGRAM,
        "-o",
        png.to_str().unwrap(),
        "--width",
        "64",
        "--views",
        "2",
    ]);
    assert!(out.status.success());
    assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"));
    let out = shape(&["build", PROGRAM]);
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains("\"joint\":\"door_hinge\"")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn mistakes_exit_with_a_reason() {
    let usage = shape(&["export", PROGRAM, "-o", "x.fbx"]);
    assert_eq!(usage.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&usage.stderr).contains(".glb"));
    let missing = shape(&["measure", "no/such/program.json"]);
    assert_eq!(missing.status.code(), Some(1));
    let unknown = shape(&["preview", PROGRAM, "-o", "x.png", "--colour", "red"]);
    assert_eq!(unknown.status.code(), Some(2));
    // A mesh tool on a program, a program tool on a mesh, no decimation
    // asked for: usage errors.
    for args in [
        &["repair", PROGRAM][..],
        &["build", MESH][..],
        &["decimate", MESH][..],
    ] {
        let out = shape(args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
}

/// Runs a command that must succeed and parses its JSON report.
fn report(args: &[&str]) -> serde_json::Value {
    let out = shape(args);
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("a JSON report")
}

#[test]
fn mesh_tools_run_on_a_mesh_file() {
    let dir = std::env::temp_dir().join(format!("shape-cli-mesh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = |name: &str| dir.join(name).to_str().unwrap().to_string();

    let r = report(&["import", MESH, "-o", &path("in.glb")]);
    assert_eq!(r["import"]["parts"].as_array().unwrap().len(), 2);
    assert_eq!(r["import"]["triangles"], 1984);
    assert!(std::fs::read(path("in.glb")).unwrap().starts_with(b"glTF"));

    let r = report(&[
        "repair",
        MESH,
        "--weld",
        "0.001",
        "--floaters",
        "0.05",
        "-o",
        &path("fixed.obj"),
    ]);
    assert_eq!(r["repair"]["flipped"], 79);
    assert_eq!(r["repair"]["floaters"], 1);
    assert_eq!(r["measures"]["closed"], true);
    assert_eq!(r["measures"]["inconsistent_edges"], 0);
    // The repaired file is a mesh the tools read back.
    let back = report(&["measure", &path("fixed.obj")]);
    assert_eq!(back["whole"]["triangles"], 1980);

    let r = report(&[
        "decimate",
        &path("fixed.obj"),
        "--ratio",
        "0.5",
        "--lods",
        "2",
        "-o",
        &path("low.glb"),
    ]);
    let levels = r["levels"].as_array().unwrap();
    assert_eq!(levels.len(), 2);
    assert_eq!(levels[0]["decimate"]["triangles"], 990);
    assert_eq!(levels[1]["decimate"]["triangles_before"], 990);
    assert!(std::fs::metadata(path("low.lod1.glb")).is_ok());
    assert!(std::fs::metadata(path("low.lod2.glb")).is_ok());

    let r = report(&["unwrap", &path("low.lod1.glb"), "-o", &path("uv.glb")]);
    assert_eq!(r["measures"]["uv"]["overlapping_pairs"], 0);
    let back = report(&["import", &path("uv.glb")]);
    assert_eq!(back["import"]["parts"][0]["uvs"], true);

    let r = report(&[
        "process",
        MESH,
        "--budget",
        "1000",
        "--weld",
        "0.001",
        "--floaters",
        "0.05",
        "-o",
        &path("done.glb"),
    ]);
    assert_eq!(r["measures"]["triangles"], 1000);
    assert_eq!(r["measures"]["closed"], true);
    assert_eq!(r["measures"]["uv"]["outside"], 0);
    assert_eq!(
        r["output"]["bytes"],
        std::fs::metadata(path("done.glb")).unwrap().len()
    );

    let out = shape(&["preview", MESH, "-o", &path("m.png"), "--width", "64"]);
    assert!(out.status.success());
    assert!(
        std::fs::read(path("m.png"))
            .unwrap()
            .starts_with(b"\x89PNG")
    );
    let out = shape(&["export", MESH, "-o", &path("m.stl")]);
    assert!(out.status.success());
    // 80-byte header, a count, 50 bytes a triangle.
    assert_eq!(
        std::fs::metadata(path("m.stl")).unwrap().len(),
        84 + 50 * 1984
    );
    let _ = std::fs::remove_dir_all(&dir);
}

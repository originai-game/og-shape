//! The `shape` binary on the committed washing-machine program: each
//! subcommand runs, files match the library's bytes, and mistakes exit 2.

use std::process::Command;

const PROGRAM: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../golden/programs/washing_machine.json"
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
}

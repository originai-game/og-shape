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
    // The washing machine is posed: OBJ is written at the pose, and the
    // report says so.
    let obj = dir.join("m.obj");
    let out = shape(&["export", PROGRAM, "-o", obj.to_str().unwrap()]);
    assert!(
        String::from_utf8(out.stdout)
            .unwrap()
            .contains("\"geometry\":\"posed\"")
    );
    assert_eq!(
        std::fs::read(&obj).unwrap(),
        model.obj().unwrap().into_bytes()
    );
    let out = shape(&["measure", PROGRAM, "--posed"]);
    assert_eq!(
        String::from_utf8(out.stdout).unwrap().trim(),
        shape_model::program::posed_json(&model).unwrap()
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
    // Each level halves the one before (a collapse removes two).
    let second = levels[1]["decimate"]["triangles"].as_u64().unwrap();
    assert!((494..=495).contains(&second), "{second}");
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

/// A 12 × 12 grid of quads whose every triangle is its own UV chart, as
/// some image-to-3D generators write them: seams everywhere.
fn chart_per_triangle_grid() -> String {
    let n = 12;
    let mut obj = String::new();
    for j in 0..=n {
        for i in 0..=n {
            obj += &format!("v {i} {j} 0\n");
        }
    }
    let id = |i: usize, j: usize| j * (n + 1) + i + 1;
    let mut t = 0;
    for j in 0..n {
        for i in 0..n {
            for tri in [
                [id(i, j), id(i + 1, j), id(i + 1, j + 1)],
                [id(i, j), id(i + 1, j + 1), id(i, j + 1)],
            ] {
                // Each triangle its own corner of the square, apart.
                let (u, v) = ((t % 24) as f64 / 24.0, (t / 24) as f64 / 24.0);
                let d = 0.03;
                obj += &format!("vt {u} {v}\nvt {} {v}\nvt {} {}\n", u + d, u + d, v + d);
                obj += &format!(
                    "f {}/{} {}/{} {}/{}\n",
                    tri[0],
                    3 * t + 1,
                    tri[1],
                    3 * t + 2,
                    tri[2],
                    3 * t + 3
                );
                t += 1;
            }
        }
    }
    obj
}

#[test]
fn fresh_uvs_free_the_decimation_from_the_files_seams() {
    let dir = std::env::temp_dir().join(format!("shape-cli-seams-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("grid.obj");
    std::fs::write(&path, chart_per_triangle_grid()).unwrap();
    let path = path.to_str().unwrap();
    let out = dir.join("low.glb");
    let out = out.to_str().unwrap();
    // Keeping the file's UVs, the seams hold: hardly anything collapses.
    let kept = report(&["process", path, "--budget", "50"]);
    assert!(kept["decimate"]["triangles"].as_u64().unwrap() > 250);
    // With fresh UVs the old seams are gone before decimation.
    let fresh = report(&["process", path, "--budget", "50", "--unwrap", "-o", out]);
    assert!(fresh["decimate"]["triangles"].as_u64().unwrap() <= 50);
    assert!(std::fs::metadata(out).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn build_reads_fonts_beside_the_program_and_reports_booleans() {
    let dir = std::env::temp_dir().join(format!("shape-cli-fonts-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("fonts")).unwrap();
    let program = dir.join("p.json");
    std::fs::write(
        &program,
        r#"{"name": "p", "ops": [
          {"op": "box", "name": "a", "width": 4, "depth": 4, "height": 4},
          {"op": "box", "name": "b", "width": 1, "depth": 1, "height": 1,
           "on": {"origin": [0, 0, 1], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
          {"op": "cut", "target": 0, "tool": 1}
        ]}"#,
    )
    .unwrap();
    let out = shape(&["build", program.to_str().unwrap()]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains(r#""booleans":[{"op":"cut","target":"a","tool":"b","voids":1,"#),
        "{text}"
    );
    // The font is read relative to the program: missing, then not a font.
    std::fs::write(
        &program,
        r#"{"name": "p", "ops": [{"op": "extrude", "name": "t", "height": 1,
          "profile": {"text": {"text": "A", "font": "fonts/x.ttf", "size": 5}}}]}"#,
    )
    .unwrap();
    let out = shape(&["build", program.to_str().unwrap()]);
    let e = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(e.contains("font \"fonts/x.ttf\""), "{e}");
    std::fs::write(dir.join("fonts/x.ttf"), b"not a font").unwrap();
    let out = shape(&["build", program.to_str().unwrap()]);
    let e = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(e.contains("not a TrueType"), "{e}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn preview_writes_each_view_with_its_camera_and_the_uv_layout() {
    let dir = std::env::temp_dir().join(format!("shape-cli-views-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("v.png");
    let o = out.to_str().unwrap();
    let r = report(&[
        "preview", PROGRAM, "-o", o, "--each", "--views", "2", "--width", "32", "--fov", "0.6",
    ]);
    let images = r["images"].as_array().unwrap();
    assert_eq!(images.len(), 2);
    for (k, im) in images.iter().enumerate() {
        let path = im["output"]["path"].as_str().unwrap();
        assert!(path.ends_with(&format!("v.{k}.png")), "{path}");
        assert!(std::fs::read(path).unwrap().starts_with(b"\x89PNG"));
        let cam = &im["shows"]["camera"];
        assert_eq!(cam["fov"], 0.6);
        assert_eq!(cam["position"].as_array().unwrap().len(), 3);
        // The glTF frame: metres, Y up.
        assert_eq!(
            cam["gltf"]["target"][1].as_f64().unwrap(),
            cam["target"][2].as_f64().unwrap() / 1000.0
        );
    }
    // The UV view: the program asked for UVs, one image a material.
    let r = report(&[
        "preview",
        PROGRAM,
        "--uvs",
        "-o",
        o,
        "--uv",
        "--uv-size",
        "64",
    ]);
    let images = r["images"].as_array().unwrap();
    assert!(!images.is_empty());
    assert_eq!(images[0]["shows"]["size"], 64);
    assert_eq!(images[0]["shows"]["under"], "checker");
    // Without UVs there is nothing to lay out.
    let failed = shape(&["preview", PROGRAM, "-o", o, "--uv"]);
    assert!(!failed.status.success());
}

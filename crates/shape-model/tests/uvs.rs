//! UVs through the program (`docs/research.md` §21): the model and its
//! parts ask for them, every edit carries them or refuses, a material
//! names a texture the caller supplies, and the GLB carries both.

use serde_json::{Value, json};
use shape_core::predicates::orient2d;
use shape_model::program::{Fonts, Session, run_files, textures_named};

/// A 64×64 checker, eight squares a side.
fn checker() -> Vec<u8> {
    let mut rgba = Vec::new();
    for y in 0..64u32 {
        for x in 0..64u32 {
            let on = (x / 8 + y / 8) % 2 == 0;
            let c = if on { 230 } else { 40 };
            rgba.extend_from_slice(&[c, c, if on { 60 } else { 200 }, 255]);
        }
    }
    shape_tools::png(&shape_tools::Image {
        width: 64,
        height: 64,
        rgba,
    })
}

/// The acceptance program: a plate with a hole and a fillet, a lofted
/// handle joined on, a checker map 20 mm a side.
pub fn plate_program() -> Value {
    let up = |x: f64, y: f64, z: f64| json!({"origin": [x, y, z], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]});
    let rr = |w: f64, h: f64, r: f64| json!({"rect": {"width": w, "height": h, "radius": r}});
    json!({"name": "uv_plate", "ops": [
        {"op": "uvs"},
        {"op": "extrude", "name": "plate", "profile": rr(80.0, 50.0, 4.0), "height": 6},
        {"op": "hole", "id": 0, "diameter": 8, "at": [up(-28.0, 0.0, 10.0), up(28.0, 0.0, 10.0)]},
        {"op": "fillet", "id": 0, "radius": 1.5, "edges": ["top"]},
        {"op": "loft", "name": "handle", "on": up(0.0, 0.0, 5.0), "sections": [
            {"profile": rr(30.0, 12.0, 3.0), "z": 0},
            {"profile": {"ellipse": {"rx": 9, "ry": 5}}, "z": 12},
            {"profile": rr(16.0, 8.0, 2.0), "z": 24}
        ], "steps": 4},
        {"op": "join", "target": 0, "tool": 1},
        {"op": "material", "ids": [0], "color": "#ffffff", "map": "checker.png", "mapSize": 20}
    ]})
}

#[test]
fn a_plate_with_a_hole_a_fillet_and_a_lofted_handle_exports_uvs_everywhere() {
    let program = plate_program().to_string();
    assert_eq!(textures_named(&program).unwrap(), vec!["checker.png"]);
    let mut textures = Fonts::new();
    textures.insert("checker.png".into(), checker());
    let s = Session::replay_files(&program, &Fonts::new(), &textures).unwrap();
    let m = s.model();
    // Measures: every triangle has UVs, none turned over or flat.
    let measures: Value =
        serde_json::from_str(&shape_model::program::measures_json(m).unwrap()).unwrap();
    let part = &measures["parts"][0];
    assert_eq!(part["name"], "plate");
    let uv = &part["uv"];
    assert_eq!(uv["flipped"], 0, "{uv}");
    assert_eq!(uv["zeroArea"], 0, "{uv}");
    assert!(uv["coverage"].as_f64().unwrap() > 0.9, "{uv}");
    assert_eq!(measures["uvCoverage"], uv["coverage"]);
    assert_eq!(part["closed"], true);
    // The GLB: TEXCOORD_0 on every vertex, the PNG embedded.
    let glb = m.glb().unwrap();
    let imported = shape_tools::import(
        &glb,
        &shape_tools::Import::default(),
        &shape_tools::no_files,
    )
    .unwrap();
    let p = &imported.parts[0];
    let uvs = p.solid.uvs.as_ref().expect("UVs in the GLB");
    assert_eq!(uvs.len(), p.solid.triangles.len());
    assert_eq!(p.map.as_ref().unwrap().bytes, checker());
    // Replayed through `run_files`: the same bytes.
    let again = run_files(&program, &Fonts::new(), &textures).unwrap();
    assert_eq!(again.glb().unwrap(), glb);
    // Without the texture: an error at the material, naming the file.
    let e = run_files(&program, &Fonts::new(), &Fonts::new())
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("op 6 material") && e.contains("checker.png"),
        "{e}"
    );
}

/// Every triangle has UVs that read the way its face does, but needles a
/// boolean leaves (two corners an ulp apart, area about 1e-15 mm²),
/// whose two near corners take one UV.
fn check(w: &shape_mesh::Solid, what: &str) {
    let uvs = w.uvs.as_ref().unwrap();
    assert_eq!(uvs.len(), w.triangles.len());
    for (i, u) in uvs.iter().enumerate() {
        let p = w.triangles[i].map(|v| w.positions[v as usize]);
        if (p[1] - p[0]).cross(p[2] - p[0]).length() > 1e-9 {
            assert_eq!(
                orient2d(u[0], u[1], u[2]),
                core::cmp::Ordering::Greater,
                "{what}: triangle {i} {u:?}"
            );
        }
    }
}

fn apply(s: &mut Session, op: Value) -> Result<Value, String> {
    s.apply_value(&op).map_err(|e| e.to_string())
}

#[test]
fn uvs_are_opt_in_and_never_dropped_silently() {
    let rect = json!({"rect": {"width": 20, "height": 10}});
    // A part made without UVs: the model may no longer ask for them.
    let mut s = Session::new("t");
    apply(
        &mut s,
        json!({"op": "box", "name": "a", "width": 1, "depth": 1, "height": 1}),
    )
    .unwrap();
    let e = apply(&mut s, json!({"op": "uvs"})).unwrap_err();
    assert!(e.contains("made without UVs"), "{e}");
    // ... but a part may.
    apply(
        &mut s,
        json!({"op": "extrude", "name": "b", "profile": rect, "height": 4, "uvs": true}),
    )
    .unwrap();
    let names: Value = apply(&mut s, json!({"op": "measure"})).unwrap();
    assert!(names["parts"][0].get("uv").is_none());
    assert!(names["parts"][1]["uv"].is_object());
    // Merging a part with UVs and one without is refused.
    let e = apply(&mut s, json!({"op": "merge", "target": 1, "tool": 0})).unwrap_err();
    assert!(e.contains("no UVs"), "{e}");
    // A cut by a tool without UVs is counted in the answer.
    apply(
        &mut s,
        json!({"op": "box", "name": "c", "width": 4, "depth": 4, "height": 10,
        "on": {"origin": [0, 0, -2], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}}),
    )
    .unwrap();
    let r = apply(&mut s, json!({"op": "cut", "target": 1, "tool": 2})).unwrap();
    assert!(r["facesWithoutUvs"].as_u64().unwrap() > 0, "{r}");
    // A map on a part without UVs is refused; `uvs` is a boolean.
    s.texture("t.png", checker());
    let e = apply(
        &mut s,
        json!({"op": "material", "ids": [0], "map": "t.png"}),
    )
    .unwrap_err();
    assert!(e.contains("needs UVs"), "{e}");
    let e = apply(
        &mut s,
        json!({"op": "box", "name": "d", "width": 1, "depth": 1, "height": 1,
        "uvs": "yes"}),
    )
    .unwrap_err();
    assert!(e.contains("uvs"), "{e}");
    // With no UVs asked for, nothing has them: the GLB has no TEXCOORD_0.
    let mut plain = Session::new("p");
    apply(
        &mut plain,
        json!({"op": "box", "name": "a", "width": 1, "depth": 1, "height": 1}),
    )
    .unwrap();
    let glb = plain.model().glb().unwrap();
    assert!(!String::from_utf8_lossy(&glb).contains("TEXCOORD_0"));
}

#[test]
fn every_edit_carries_uvs() {
    // Each edit on a part with UVs leaves every triangle with UVs that are
    // neither flat nor turned over (a mirror turns them over: it is a
    // mirror image).
    let up = |x: f64, y: f64, z: f64| json!({"origin": [x, y, z], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]});
    let rect = |w: f64| json!({"rect": {"width": w, "height": w}});
    let edits = [
        json!({"op": "hole", "id": 0, "diameter": 4, "at": [up(0.0, 0.0, 20.0)], "depth": 5,
               "counterbore": [7, 2]}),
        json!({"op": "pocket", "id": 0, "profile": rect(6.0), "depth": 3,
               "at": [up(0.0, 0.0, 20.0)]}),
        json!({"op": "split", "id": 0, "frame": up(0.0, 0.0, 7.0), "keep": "below"}),
        json!({"op": "shell", "id": 0, "thickness": 1, "open": ["top"]}),
        json!({"op": "dent", "id": 0, "at": [[0, 0, 10]], "radius": 4, "depth": 1}),
        json!({"op": "chamfer", "id": 0, "distance": 1, "edges": ["top"]}),
        json!({"op": "soften", "id": 0, "radius": 0.5}),
        json!({"op": "scale", "id": 0, "x": 2}),
        json!({"op": "at", "id": 0, "frame": up(3.0, 1.0, 2.0)}),
    ];
    for edit in edits {
        let mut s = Session::new("e");
        apply(&mut s, json!({"op": "uvs"})).unwrap();
        apply(
            &mut s,
            json!({"op": "extrude", "name": "a", "profile": rect(20.0), "height": 10}),
        )
        .unwrap();
        apply(&mut s, edit.clone()).unwrap_or_else(|e| panic!("{edit}: {e}"));
        check(
            &s.model().world_solid(shape_model::Id(0)).unwrap(),
            &edit.to_string(),
        );
    }
    // Seat with a part that has UVs; a mirror; a copy.
    let mut s = Session::new("e");
    apply(&mut s, json!({"op": "uvs"})).unwrap();
    apply(
        &mut s,
        json!({"op": "extrude", "name": "a", "profile": rect(20.0), "height": 10}),
    )
    .unwrap();
    apply(
        &mut s,
        json!({"op": "cylinder", "name": "pin", "radius": 2, "height": 20,
        "on": up(0.0, 0.0, -5.0)}),
    )
    .unwrap();
    apply(&mut s, json!({"op": "seat", "host": 0, "part": 1})).unwrap();
    apply(
        &mut s,
        json!({"op": "mirror", "id": 0, "name": "b",
        "plane": {"origin": [30, 0, 0], "x": [0, 1, 0], "y": [0, 0, 1], "z": [1, 0, 0]}}),
    )
    .unwrap();
    apply(&mut s, json!({"op": "copy", "id": 0, "name": "c"})).unwrap();
    let m: Value = apply(&mut s, json!({"op": "measure"})).unwrap();
    let parts = m["parts"].as_array().unwrap();
    let uv = |name: &str| {
        parts
            .iter()
            .find(|p| p["name"] == name)
            .map(|p| p["uv"].clone())
            .unwrap()
    };
    check(&s.model().world_solid(shape_model::Id(0)).unwrap(), "seat");
    assert_eq!(uv("c"), uv("a"));
    // The mirror image's UVs read mirrored: every triangle that read
    // the right way is turned over, and the other way round.
    let b = parts.iter().find(|p| p["name"] == "b").unwrap();
    let n = |name: &str, k: &str| uv(name)[k].as_u64().unwrap();
    assert_eq!(
        n("b", "flipped") + n("b", "zeroArea") + n("a", "flipped"),
        b["triangles"].as_u64().unwrap()
    );
}

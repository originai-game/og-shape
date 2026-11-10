//! The authoring surface: unknown fields refused, errors located, and the
//! generators the program gained (`docs/research.md` §14).

use serde_json::{Value, json};
use shape_model::ModelError;
use shape_model::program::{Session, run};

fn apply(s: &mut Session, op: Value) -> Result<Value, ModelError> {
    s.apply_value(&op)
}

fn volume(program: &Value, part: &str) -> f64 {
    let m = run(&program.to_string()).unwrap();
    let p = m.measure().unwrap();
    let p = p.iter().find(|p| p.name == part).unwrap();
    assert!(p.measures.closed, "{part} is open");
    p.measures.volume
}

#[test]
fn unknown_fields_are_refused_naming_the_field_and_the_accepted_ones() {
    let mut s = Session::new("t");
    let e = apply(
        &mut s,
        json!({"op": "cylinder", "name": "c", "radius": 1, "height": 2, "segmentz": 8}),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("\"segmentz\""), "{e}");
    assert!(e.contains("radiusTop, height, segments, on"), "{e}");
    // Inside a profile, a loft section, a frame and a mimic.
    for (op, field) in [
        (
            json!({"op": "extrude", "name": "e", "height": 1,
                   "profile": {"rect": {"width": 1, "height": 1, "radiuz": 0.1}}}),
            "radiuz",
        ),
        (
            json!({"op": "extrude", "name": "e", "height": 1,
                   "profile": {"subtract": [{"rect": {"width": 2, "height": 2}},
                                            {"circle": {"radius": 0.5, "sides": 6}}]}}),
            "sides",
        ),
        (
            json!({"op": "loft", "name": "l", "sections": [
                {"profile": {"circle": {"radius": 1}}, "z": 0, "twist": 1},
                {"profile": {"circle": {"radius": 1}}, "z": 1}]}),
            "twist",
        ),
        (
            json!({"op": "box", "name": "b", "width": 1, "depth": 1, "height": 1,
                   "on": {"origin": [0, 0, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1],
                          "w": [0, 0, 0]}}),
            "w",
        ),
    ] {
        let e = apply(&mut s, op).unwrap_err().to_string();
        assert!(e.contains(&format!("{field:?}")), "{e}");
    }
    // Nothing was recorded, and a known field set to null is absent.
    assert_eq!(s.program(), r#"{"name":"t","ops":[]}"#);
    apply(
        &mut s,
        json!({"op": "box", "name": "b", "width": 1, "depth": 1, "height": 1, "on": null}),
    )
    .unwrap();
}

#[test]
fn errors_name_the_op_its_index_and_the_part() {
    let program = json!({"name": "t", "ops": [
        {"op": "box", "name": "plate", "width": 10, "depth": 10, "height": 2},
        {"op": "box", "name": "pin", "width": 1, "depth": 1, "height": 5},
        {"op": "fillet", "id": 0, "radius": 40, "edges": "top"}
    ]});
    let e = run(&program.to_string()).unwrap_err();
    match &e {
        ModelError::At {
            index, op, part, ..
        } => {
            assert_eq!(
                (*index, op.as_str(), part.as_deref()),
                (2, "fillet", Some("plate"))
            );
        }
        other => panic!("not located: {other:?}"),
    }
    assert!(e.to_string().starts_with("op 2 fillet \"plate\": "), "{e}");
    // A live session: the index the op would be recorded at; a reader
    // says so.
    let mut s = Session::new("t");
    apply(
        &mut s,
        json!({"op": "box", "name": "plate", "width": 10, "depth": 10, "height": 2}),
    )
    .unwrap();
    let e = apply(
        &mut s,
        json!({"op": "cylinder", "name": "rod", "radius": -1, "height": 2}),
    )
    .unwrap_err()
    .to_string();
    assert!(e.starts_with("op 1 cylinder \"rod\": "), "{e}");
    let e = apply(&mut s, json!({"op": "face", "id": 0, "name": "lid"}))
        .unwrap_err()
        .to_string();
    assert!(
        e.starts_with("face \"plate\" (a reader, before op 1): "),
        "{e}"
    );
}

#[test]
fn new_generators_build_closed_solids() {
    let on = |x: f64| json!({"origin": [x, 0, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]});
    let program = json!({"name": "t", "ops": [
        {"op": "wire", "name": "wire", "diameter": 2, "fillet": 3,
         "path": [[0, 0, 0], [20, 0, 0], [20, 20, 0]]},
        {"op": "body", "name": "body", "plan": [[0, 4], [30, 10], [60, 4]],
         "top": {"points": [[0, 2], [30, 6], [60, 2]], "smooth": true}, "bottom": -2, "on": on(40.0)},
        {"op": "knurl", "name": "knurl", "diameter": 10, "height": 8, "straight": true, "on": on(120.0)},
        {"op": "gear", "name": "gear", "teeth": 16, "module": 1.5, "width": 5, "bore": 4,
         "on": on(160.0)},
        {"op": "dome", "name": "dome", "radius": 6, "height": 3, "axis": "+Z", "on": on(200.0)},
        {"op": "torus", "name": "torus", "radius": 8, "tube": 2, "angle": 3.0, "on": on(240.0)}
    ]});
    let m = run(&program.to_string()).unwrap();
    for p in m.measure().unwrap() {
        assert!(p.measures.closed, "{} is open", p.name);
        assert_eq!(p.measures.degenerate_triangles, 0, "{}", p.name);
    }
}

#[test]
fn segments_set_the_count_and_zero_keeps_the_chord_rule() {
    let tris = |op: Value| {
        let m = run(&json!({"name": "t", "ops": [op]}).to_string()).unwrap();
        m.measure().unwrap()[0].measures.triangles
    };
    // 8 sides: 8 walls of 2 and two caps of 6.
    assert_eq!(
        tris(json!({"op": "cylinder", "name": "c", "radius": 4, "height": 1, "segments": 8})),
        28
    );
    // 12 round, 6 pole to pole: 12 · 6 · 2 less the 24 at the poles.
    assert_eq!(
        tris(json!({"op": "sphere", "name": "s", "radius": 4, "segments": 12})),
        120
    );
    // A torus 16 round and 8 round its tube.
    assert_eq!(
        tris(
            json!({"op": "torus", "name": "t", "radius": 8, "tube": 2, "segments": 16,
                    "tubeSegments": 8})
        ),
        256
    );
    // A straight wire 8 round: 8 walls of 2 and two caps of 6.
    assert_eq!(
        tris(
            json!({"op": "wire", "name": "w", "diameter": 2, "segments": 8,
                    "path": [[0, 0, 0], [0, 0, 10]]})
        ),
        28
    );
    assert_eq!(
        tris(json!({"op": "sphere", "name": "s", "radius": 4, "segments": 0})),
        tris(json!({"op": "sphere", "name": "s", "radius": 4}))
    );
    let e = run(
        &json!({"name": "t", "ops": [{"op": "cone", "name": "c", "radius": 1, "height": 1,
                                      "segments": 2}]})
        .to_string(),
    )
    .unwrap_err()
    .to_string();
    assert!(e.contains("segments is 2"), "{e}");
}

#[test]
fn scale_multiplies_the_volume_and_keeps_the_names() {
    let b = json!({"op": "box", "name": "b", "width": 2, "depth": 3, "height": 4});
    let plain = volume(&json!({"name": "t", "ops": [b]}), "b");
    let scaled = json!({"name": "t", "ops": [b, {"op": "scale", "id": 0, "x": 2, "z": 0.5}]});
    assert!((volume(&scaled, "b") - plain).abs() < 1e-9);
    let tall = json!({"name": "t", "ops": [b, {"op": "scale", "id": 0, "z": 3}]});
    assert!((volume(&tall, "b") - 3.0 * plain).abs() < 1e-9);
    let mut s = Session::replay(&tall.to_string()).unwrap();
    let top = apply(&mut s, json!({"op": "face", "id": 0, "name": "top"})).unwrap();
    assert_eq!(top["origin"], json!([0.0, 0.0, 12.0]));
    let e = run(&json!({"name": "t", "ops": [b, {"op": "scale", "id": 0, "y": -1}]}).to_string())
        .unwrap_err()
        .to_string();
    assert!(e.contains("scale"), "{e}");
}

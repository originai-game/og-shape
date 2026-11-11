//! The rest of the authoring surface (`docs/research.md` §16): profiles
//! from paths, `until`, paths of pieces, `merge`, `delete`, boolean
//! reports and fonts.

use serde_json::{Value, json};
use shape_core::trig;
use shape_model::program::{Session, fonts_named, run};

fn err(program: Value) -> String {
    run(&program.to_string()).unwrap_err().to_string()
}

fn part(program: &Value, name: &str) -> shape_mesh::Measures {
    let m = run(&program.to_string()).unwrap();
    let p = m.measure().unwrap();
    let p = p.into_iter().find(|p| p.name == name).unwrap();
    assert!(p.measures.closed, "{name} is open");
    p.measures
}

const UP8: &str = r#"{"origin": [0, 0, 8], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}"#;

#[test]
fn until_a_parallel_face_is_its_height_and_a_tilted_one_cuts_the_top() {
    let on: Value = serde_json::from_str(UP8).unwrap();
    let flat = json!({"name": "t", "ops": [
        {"op": "box", "name": "shelf", "width": 10, "depth": 10, "height": 1, "on": on},
        {"op": "extrude", "name": "leg", "profile": {"rect": {"width": 2, "height": 2}},
         "until": {"part": 0, "face": "bottom"}}
    ]});
    let m = part(&flat, "leg");
    assert_eq!(m.bounds_max.z, 8.0);
    assert!((m.volume - 32.0).abs() < 1e-9, "{}", m.volume);
    // A roof tilted 0.2 in x over the post's axis at z = 10: the top
    // follows the plane, and is named `top`.
    let (s, c) = (0.196_116_135_138_184_04, 0.980_580_675_690_920_2);
    let tilted = json!({"name": "t", "ops": [
        {"op": "box", "name": "roof", "width": 20, "depth": 20, "height": 2,
         "on": {"origin": [0, 0, 10], "x": [c, 0, -s], "y": [0, 1, 0], "z": [s, 0, c]}},
        {"op": "extrude", "name": "post", "profile": {"rect": {"width": 4, "height": 4}},
         "until": {"part": 0, "face": "bottom"}}
    ]});
    let m = part(&tilted, "post");
    assert!((m.bounds_max.z - 10.4).abs() < 1e-9, "{}", m.bounds_max.z);
    assert!((m.bounds_min.z).abs() < 1e-12);
    // The prism under the plane z = 10 + 0.2·x: 16 · 10.
    assert!((m.volume - 160.0).abs() < 1e-9, "{}", m.volume);
    let mut session = Session::replay(&tilted.to_string()).unwrap();
    let top = session.apply_value(&json!({"op": "face", "id": 1, "name": "top"}));
    let f: Value = top.unwrap();
    assert!(
        (f["z"][0].as_f64().unwrap() + s).abs() > 0.1,
        "the top faces up the slope"
    );
    // Refused: with a height, or a plane below the profile.
    let e = err(json!({"name": "t", "ops": [
        {"op": "box", "name": "shelf", "width": 10, "depth": 10, "height": 1, "on": on},
        {"op": "extrude", "name": "leg", "profile": {"circle": {"radius": 1}}, "height": 3,
         "until": {"part": 0, "face": "bottom"}}
    ]}));
    assert!(e.contains("height") && e.contains("until"), "{e}");
    let e = err(json!({"name": "t", "ops": [
        {"op": "box", "name": "shelf", "width": 10, "depth": 10, "height": 1},
        {"op": "extrude", "name": "leg", "profile": {"circle": {"radius": 1}},
         "until": {"part": 0, "face": "bottom"}, "on": on}
    ]}));
    assert!(e.contains("op 1 extrude") && e.contains("until"), "{e}");
}

#[test]
fn path_profiles_follow_their_arcs_and_refuse_what_they_cannot_build() {
    // A D: a three-point half circle of radius 5, closed by its diameter.
    let d = json!({"name": "t", "ops": [{"op": "extrude", "name": "d", "height": 1,
        "profile": {"path": [[0, -5], {"arc": {"through": [5, 0], "to": [0, 5]}}]}}]});
    let v = part(&d, "d").volume;
    let half = trig::PI * 12.5;
    assert!((v - half).abs() / half < 2e-2, "{v}");
    assert!(v < half);
    for (path, want) in [
        (
            json!([[0, 0], {"arc": {"through": [1, 0], "to": [2, 0]}}, [0, 3]]),
            "one line",
        ),
        (
            json!([[0, 0], {"tangentArc": {"to": [2, 2]}}, [0, 3]]),
            "tangent",
        ),
        (json!([[0, 0], {"bezier": {"to": [2, 2]}}, [0, 3]]), "arc"),
    ] {
        let e = err(json!({"name": "t", "ops": [{"op": "extrude", "name": "p",
            "height": 1, "profile": {"path": path}}]}));
        assert!(e.contains(want) && e.contains("path[1]"), "{e}");
    }
    // A stroke is as wide as asked: a straight bar 10 × 2 with square ends.
    let bar = json!({"name": "t", "ops": [{"op": "extrude", "name": "b", "height": 1,
        "profile": {"stroke": {"path": [[0, 0], [10, 0]], "width": 2}}}]});
    assert!((part(&bar, "b").volume - 20.0).abs() < 1e-9);
    // A hull of two squares is the box round both.
    let hull = json!({"name": "t", "ops": [{"op": "extrude", "name": "h", "height": 1,
        "profile": {"hull": [{"rect": {"width": 2, "height": 2}},
            {"translate": {"profile": {"rect": {"width": 2, "height": 2}}, "x": 6, "y": 4}}]}}]});
    let v = part(&hull, "h").volume;
    assert!(
        (v - (8.0 * 6.0 - 2.0 * 0.5 * 6.0 * 4.0)).abs() < 1e-9,
        "{v}"
    );
    // A mirror across x keeps the area and flips y.
    let m = json!({"name": "t", "ops": [{"op": "extrude", "name": "m", "height": 1,
        "profile": {"mirror": {"profile": {"poly": {"points": [[0, 0], [4, 0], [0, 3]]}},
            "axis": "x"}}}]});
    let m = part(&m, "m");
    assert_eq!((m.volume, m.bounds_min.y, m.bounds_max.y), (6.0, -3.0, 0.0));
    // Segment counts below a profile's least are refused, not ignored.
    let e = err(
        json!({"name": "t", "ops": [{"op": "extrude", "name": "s", "height": 1,
        "profile": {"superellipse": {"rx": 2, "ry": 1, "exponent": 4, "segments": 3}}}]}),
    );
    assert!(e.contains("superellipse.segments"), "{e}");
}

#[test]
fn paths_of_pieces_and_helices_build_sweeps_and_wires() {
    // A spring: a wire along a helix, as tall as its turns.
    let spring = json!({"name": "t", "ops": [{"op": "wire", "name": "s", "diameter": 1,
        "path": {"helix": {"radius": 4, "pitch": 2, "turns": 3}}}]});
    let m = part(&spring, "s");
    // The end caps lean with the helix, so the rod reaches a little short.
    assert!(
        (m.bounds_max.z - 6.5).abs() < 1e-2 && (m.bounds_min.z + 0.5).abs() < 1e-2,
        "{m:?}"
    );
    // A wire along a tangent arc is no wider than its rod round the bend.
    let crook = json!({"name": "t", "ops": [{"op": "wire", "name": "c", "diameter": 1,
        "fillet": 1, "path": [[0, 0, 0], [0, 0, 8], {"tangentArc": {"to": [6, 0, 8]}}]}]});
    let m = part(&crook, "c");
    assert!((m.bounds_max.z - 11.5).abs() < 1e-2, "{}", m.bounds_max.z);
    // `fillets` is one radius a point: with curves it is refused.
    let e = err(
        json!({"name": "t", "ops": [{"op": "wire", "name": "c", "diameter": 1,
        "fillets": [0, 1, 0], "path": [[0, 0, 0], [0, 0, 8], {"tangentArc": {"to": [6, 0, 8]}}]}]}),
    );
    assert!(e.contains("fillets"), "{e}");
    // A sweep along a three-point arc in 3D stays on its circle.
    let ring = json!({"name": "t", "ops": [{"op": "sweep", "name": "r",
        "profile": {"circle": {"radius": 0.5}},
        "path": [[10, 0, 0], {"arc": {"through": [0, 10, 0], "to": [-10, 0, 0]}}]}]});
    let m = part(&ring, "r");
    assert!((m.bounds_max.y - 10.5).abs() < 1e-2, "{}", m.bounds_max.y);
}

#[test]
fn merge_keeps_both_shells_and_delete_removes_a_subtree() {
    let program = json!({"name": "t", "ops": [
        {"op": "box", "name": "a", "width": 2, "depth": 2, "height": 2},
        {"op": "box", "name": "b", "width": 2, "depth": 2, "height": 2,
         "on": {"origin": [1, 0, 1], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
        {"op": "merge", "target": 0, "tool": 1}
    ]});
    let m = run(&program.to_string()).unwrap();
    let p = m.measure().unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!((p[0].measures.components, p[0].measures.volume), (2, 16.0));

    let mut s = Session::new("t");
    let box_ = |name: &str| json!({"op": "box", "name": name, "width": 1, "depth": 1, "height": 1});
    for n in ["body", "door", "knob"] {
        s.apply_value(&box_(n)).unwrap();
    }
    s.apply_value(&json!({"op": "group", "name": "front", "children": [1, 2]}))
        .unwrap();
    let pivot = json!({"origin": [0, 0, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]});
    s.apply_value(
        &json!({"op": "joint", "child": 1, "parent": 0, "name": "hinge",
        "on": pivot, "range": [0, 1]}),
    )
    .unwrap();
    s.apply_value(
        &json!({"op": "joint", "child": 2, "parent": 1, "name": "turn",
        "kind": "continuous", "on": pivot}),
    )
    .unwrap();
    // The body holds the hinge of a door that stays: refused, unchanged.
    let e = s
        .apply_value(&json!({"op": "delete", "id": 0}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("hinge") && e.contains("op 6 delete"), "{e}");
    // The group goes with its parts and both joints.
    s.apply_value(&json!({"op": "delete", "id": 3})).unwrap();
    let m = s.model();
    assert!(m.joint_names().is_empty());
    assert_eq!(m.measure().unwrap().len(), 1);
    let e = s
        .apply_value(&json!({"op": "copy", "id": 1, "name": "again"}))
        .unwrap_err();
    assert!(e.to_string().contains("deleted"), "{e}");
    // The name is free again, and the program replays.
    s.apply_value(&box_("door")).unwrap();
    run(&s.program()).unwrap();
}

#[test]
fn booleans_answer_their_report_and_check_refuses_a_folded_operand() {
    let mut s = Session::new("t");
    let block = json!({"op": "box", "name": "block", "width": 10, "depth": 10, "height": 10});
    s.apply_value(&block).unwrap();
    s.apply_value(
        &json!({"op": "box", "name": "void", "width": 2, "depth": 2, "height": 2,
        "on": {"origin": [0, 0, 4], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}}),
    )
    .unwrap();
    let r = s
        .apply_value(&json!({"op": "cut", "target": 0, "tool": 1}))
        .unwrap();
    assert_eq!(
        r,
        json!({"voids": 1, "touchingEdges": 0, "touching": false, "facesWithoutUvs": 0, "crossingPairs": 0})
    );
    assert_eq!(s.model().booleans().len(), 1);
    // A wire whose path crosses itself passes through itself: `check`
    // refuses it before any work; without it the join goes on.
    let knot = json!({"op": "wire", "name": "knot", "diameter": 1, "closed": true,
        "path": [[-4, -4, 12], [4, 4, 12], [4, -4, 12], [-4, 4, 12]]});
    s.apply_value(&knot).unwrap();
    let e = s
        .apply_value(&json!({"op": "join", "target": 0, "tool": 2, "check": true}))
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("passes through itself") || e.contains("crosses itself"),
        "{e}"
    );
    assert_eq!(
        s.model().booleans().len(),
        1,
        "a refused boolean is not kept"
    );
}

#[test]
fn text_names_a_font_the_caller_supplies() {
    let program = json!({"name": "t", "ops": [
        {"op": "extrude", "name": "label", "height": 1,
         "profile": {"text": {"text": "Hi", "font": "fonts/a.ttf", "size": 5}}},
        {"op": "extrude", "name": "other", "height": 1,
         "profile": {"union": [{"text": {"text": "x", "font": "b.otf", "size": 5}}]}}
    ]})
    .to_string();
    assert_eq!(fonts_named(&program).unwrap(), ["b.otf", "fonts/a.ttf"]);
    let e = run(&program).unwrap_err().to_string();
    assert!(
        e.contains("\"fonts/a.ttf\" was not") && e.contains("none"),
        "{e}"
    );
    let mut s = Session::new("t");
    s.font("fonts/a.ttf", b"not a font".to_vec());
    let e = s
        .apply_value(&json!({"op": "extrude", "name": "label", "height": 1,
            "profile": {"text": {"text": "Hi", "font": "fonts/a.ttf", "size": 5}}}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("not a TrueType"), "{e}");
}

#[test]
fn edits_answer_the_crossing_pairs_they_leave() {
    let shell = |radius: f64, thickness: f64| {
        let mut s = Session::new("t");
        s.apply_value(&json!({"op": "extrude", "name": "case", "height": 80,
            "profile": {"rect": {"width": 80, "height": 50, "radius": radius}}}))
            .unwrap();
        s.apply_value(&json!({"op": "shell", "id": 0, "thickness": thickness}))
            .unwrap()
    };
    assert_eq!(shell(15.0, 3.0), json!({"crossingPairs": 0}));
    // A wall thicker than the corners' radius folds the inner solid
    // (`docs/research.md` §27.1): the part comes back, and the answer says
    // it passes through itself.
    let folded = shell(5.0, 10.0);
    assert!(folded["crossingPairs"].as_u64().unwrap() > 0, "{folded}");
}

#[test]
fn wrap_is_the_convex_hull_of_parts_where_they_stand() {
    // Two posts 40 apart, wrapped: a slab 50 long, 10 wide, 30 tall, with
    // eight corners and no vertex of the posts' inner faces.
    let post = |name: &str, x: f64| {
        json!({"op": "box", "name": name, "width": 10, "depth": 10, "height": 30,
            "on": {"origin": [x, 0, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}})
    };
    let p = json!({"name": "t", "ops": [
        post("a", -20.0), post("b", 20.0),
        {"op": "wrap", "name": "skin", "parts": [0, 1]},
    ]});
    let m = part(&p, "skin");
    assert!((m.volume - 50.0 * 10.0 * 30.0).abs() < 1e-9, "{}", m.volume);
    assert_eq!((m.vertices, m.triangles), (8, 12));
    // The posts stay.
    assert!((part(&p, "a").volume - 3000.0).abs() < 1e-9);
    // Flat input is refused with the reason, located.
    let flat = json!({"name": "t", "ops": [
        {"op": "extrude", "name": "sheet", "height": 1, "profile": {"rect": {"width": 4, "height": 4}}},
        {"op": "wrap", "name": "w", "parts": [5]},
    ]});
    let e = err(flat);
    assert!(e.contains("op 1 wrap"), "{e}");
}

#[test]
fn subdivide_rounds_a_cage_and_keeps_a_sharp_base() {
    let p = |sharp: Value| {
        json!({"name": "t", "ops": [
            {"op": "box", "name": "cushion", "width": 60, "depth": 60, "height": 20},
            {"op": "subdivide", "id": 0, "levels": 2, "sharp": sharp},
        ]})
    };
    let soft = part(&p(json!([])), "cushion");
    let based = part(&p(json!("bottom")), "cushion");
    assert_eq!((soft.triangles, based.triangles), (192, 192));
    // Rounded all over, it lifts off the floor; with its base sharp it
    // stands on it, and keeps more of the box.
    assert!(soft.bounds_min.z > 1.0 && based.bounds_min.z == 0.0);
    assert!(soft.volume < based.volume && based.volume < 60.0 * 60.0 * 20.0);
    // A name no edge carries is refused, with the names there are.
    let e = err(p(json!("rim")));
    assert!(e.contains("op 1 subdivide") && e.contains("bottom"), "{e}");
}

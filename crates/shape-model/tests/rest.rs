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
    // A case 80 × 50 × 80 with corners of radius 5, a square pocket of
    // side `pocket` 6 deep in its top (0: none), hollowed.
    let shell = |thickness: f64, pocket: f64| {
        let mut s = Session::new("t");
        s.apply_value(&json!({"op": "extrude", "name": "case", "height": 80,
            "profile": {"rect": {"width": 80, "height": 50, "radius": 5}}}))
            .unwrap();
        if pocket > 0.0 {
            s.apply_value(&json!({"op": "pocket", "id": 0, "depth": 6,
                "profile": {"rect": {"width": pocket, "height": pocket}},
                "at": {"origin": [0, 0, 80], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}}))
                .unwrap();
        }
        let answer = s
            .apply_value(&json!({"op": "shell", "id": 0, "thickness": thickness}))
            .unwrap();
        let built = s.model().build().unwrap();
        (answer, built.scene().parts[0].solid.measure().volume)
    };
    assert_eq!(shell(3.0, 0.0).0, json!({"crossingPairs": 0}));
    // A wall thicker than the corners' radius folds the inner copy at the
    // corners (`docs/research.md` §27.1, §35, §37). Convex, the hollow is
    // cut from the planes; with a pocket the case is not convex, and the
    // faces that turned over are collapsed: the hollow is the box 10 in,
    // 60 × 30 × 60, less the pocket's own wall, 26 × 26 × 6.
    assert_eq!(shell(10.0, 0.0).0, json!({"crossingPairs": 0}));
    let (answer, volume) = shell(10.0, 6.0);
    assert_eq!(answer, json!({"crossingPairs": 0}));
    let (_, solid) = shell(3.0, 6.0);
    let hollow3 = 74.0 * 44.0 * 74.0 - 12.0 * 12.0 * 6.0;
    let hollow10 = 60.0 * 30.0 * 60.0 - 26.0 * 26.0 * 6.0;
    // Against the 3 mm shell of the same case (its hollow has corners of
    // radius 2: 74 × 44 less (4 − π)·2², 74 tall, less the pocket's wall).
    let hollow3 = hollow3 - (4.0 - trig::PI) * 4.0 * 74.0;
    let chord = 0.02 * hollow3;
    assert!(
        ((volume - solid) - (hollow3 - hollow10)).abs() < chord,
        "{volume} {solid}"
    );
    // A case whose top rim is rounded 2, with a pocket, hollowed 6: the
    // rounded rim's edge round the flat top is no hole in it, and the
    // case comes apart at the pocket's mouth alone
    // (`docs/research.md` §48).
    let build = |ops: Vec<serde_json::Value>| {
        let mut s = Session::new("t");
        let mut last = json!(null);
        for op in ops {
            last = s.apply_value(&op).unwrap();
        }
        let built = s.model().build().unwrap();
        let m = built.scene().parts[0].solid.measure();
        (last, m.closed, m.volume)
    };
    let (answer, closed, volume) = build(vec![
        json!({"op": "extrude", "name": "case", "height": 40,
            "profile": {"rect": {"width": 80, "height": 50, "radius": 4}}}),
        json!({"op": "pocket", "id": 0, "depth": 12,
            "profile": {"rect": {"width": 30, "height": 20, "radius": 3}},
            "at": {"origin": [0, 0, 40], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}}),
        json!({"op": "fillet", "id": 0, "radius": 2, "edges": "top"}),
        json!({"op": "shell", "id": 0, "thickness": 6}),
    ]);
    assert_eq!(answer, json!({"crossingPairs": 0}));
    assert!(closed && (volume - 94_959.0).abs() < 100.0, "{volume}");
    // Holes that cross inside the part are one pocket that is not
    // convex: its walls, moved out, fold where they meet. Each hole's
    // wall is grown by itself and the two cut from the hollow: a tube
    // of wall round each hole, crossing in the middle.
    let block = json!({"op": "extrude", "name": "block", "height": 40,
        "profile": {"rect": {"width": 60, "height": 60}}});
    let along_x = json!({"op": "hole", "id": 0, "diameter": 10,
        "at": {"origin": [30, 0, 20], "x": [0, 1, 0], "y": [0, 0, 1], "z": [1, 0, 0]}});
    let (answer, closed, volume) = build(vec![
        block.clone(),
        along_x.clone(),
        json!({"op": "hole", "id": 0, "diameter": 10,
            "at": {"origin": [0, 30, 20], "x": [1, 0, 0], "y": [0, 0, -1], "z": [0, 1, 0]}}),
        json!({"op": "shell", "id": 0, "thickness": 3}),
    ]);
    assert_eq!(answer, json!({"crossingPairs": 0}));
    // The walls: the box's (144 000 − 54 · 54 · 34), and two tubes of
    // outer radius 8 through the hollow, less the holes themselves.
    assert!(closed && (volume - 55_090.0).abs() < 60.0, "{volume}");
    // A blind hole that ends inside the other: its floor is two
    // crescents whose rims touch the other hole's at a point each.
    let (answer, closed, volume) = build(vec![
        block,
        along_x,
        json!({"op": "hole", "id": 0, "diameter": 8, "depth": 23,
            "at": {"origin": [0, 0, 40], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}}),
        json!({"op": "shell", "id": 0, "thickness": 3}),
    ]);
    assert_eq!(answer, json!({"crossingPairs": 0}));
    assert!(closed && volume > 50_000.0 && volume < 56_000.0, "{volume}");
    // A blind hole of the other hole's own diameter, ending a millimetre
    // past its axis: the two inner copies are tangent along a line,
    // which no untangling decides; with each vertex's move nudged by up
    // to a millionth of itself they cross, and the shell builds
    // (`docs/research.md` §64).
    let through = json!({"op": "hole", "id": 0, "diameter": 8,
        "at": {"origin": [30, 0, 20], "x": [0, 1, 0], "y": [0, 0, 1], "z": [1, 0, 0]}});
    let blind = |diameter: f64, depth: f64| {
        json!({"op": "hole", "id": 0, "diameter": diameter, "depth": depth,
            "at": {"origin": [0, 0, 40], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}})
    };
    let block = json!({"op": "extrude", "name": "block", "height": 40,
        "profile": {"rect": {"width": 60, "height": 60}}});
    let (answer, closed, volume) = build(vec![
        block.clone(),
        through.clone(),
        blind(8.0, 21.0),
        json!({"op": "shell", "id": 0, "thickness": 3}),
    ]);
    assert_eq!(answer, json!({"crossingPairs": 0}));
    assert!(closed && (volume - 51_050.0).abs() < 5.0, "{volume}");
    // A narrower blind hole reaching the far wall of the other: the cut
    // leaves triangles of no area, which have no plane to move.
    let (answer, closed, _) = build(vec![
        block.clone(),
        through.clone(),
        blind(6.0, 30.0),
        json!({"op": "shell", "id": 0, "thickness": 5}),
    ]);
    assert_eq!(answer, json!({"crossingPairs": 0}));
    assert!(closed);
    // A blind hole ending the wall's own thickness below the other
    // hole: faces of the inner copy lie on each other, and the shell
    // is built at a wall a millionth off (`docs/research.md` §76). Its
    // volume lies between its neighbours' in depth (35 061 and 35 467).
    let (answer, closed, volume) = build(vec![
        block,
        through,
        blind(8.0, 26.0),
        json!({"op": "shell", "id": 0, "thickness": 2}),
    ]);
    assert_eq!(answer, json!({"crossingPairs": 0}));
    assert!(closed && (volume - 35_216.0).abs() < 5.0, "{volume}");
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

#[test]
fn a_clip_is_checked_as_poses_are_and_poses_the_model_at_a_time() {
    let lid = json!({"origin": [0, 20, 10], "x": [1, 0, 0], "y": [0, 0, 1], "z": [0, -1, 0]});
    let base = |extra: Vec<Value>| {
        let mut ops = vec![
            json!({"op": "box", "name": "chest", "width": 60, "depth": 40, "height": 10}),
            json!({"op": "box", "name": "lid", "width": 60, "depth": 40, "height": 2,
                "on": {"origin": [0, 0, 10], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}}),
            json!({"op": "joint", "child": 1, "parent": 0, "name": "hinge", "on": lid,
                "range": [0, 1.5]}),
        ];
        ops.extend(extra);
        json!({"name": "t", "ops": ops})
    };
    let open = json!({"op": "clip", "name": "open", "keys": [
        {"time": 0, "pose": {"hinge": 0}},
        {"time": 2, "pose": {"hinge": 1.5}},
    ]});
    // Declaring a clip leaves the pose alone; posing at a time moves it.
    let m = run(&base(vec![open.clone()]).to_string()).unwrap();
    assert!(!m.posed() && m.clips() == ["open"]);
    let glb = m.glb().unwrap();
    let text = String::from_utf8_lossy(&glb);
    assert!(text.contains("\"animations\":[{\"name\":\"open\""));
    let half = base(vec![
        open.clone(),
        json!({"op": "pose", "clip": "open", "time": 1}),
    ]);
    let m = run(&half.to_string()).unwrap();
    assert_eq!(m.pose_values(), [("hinge".to_string(), 0.75)]);
    // Past its last key a clip holds its last value.
    let late = base(vec![open, json!({"op": "pose", "clip": "open", "time": 9})]);
    assert_eq!(run(&late.to_string()).unwrap().pose_values()[0].1, 1.5);
    // A value out of range, a joint that is not there, times that do not
    // rise: refused where the clip is declared.
    let bad = |keys: Value| err(base(vec![json!({"op": "clip", "name": "c", "keys": keys})]));
    let e = bad(json!([{"time": 0, "pose": {"hinge": 2}}]));
    assert!(e.contains("op 3 clip") && e.contains("range"), "{e}");
    let e = bad(json!([{"time": 0, "pose": {"latch": 0}}]));
    assert!(e.contains("hinge"), "{e}");
    let e = bad(json!([{"time": 1, "pose": {"hinge": 0}}, {"time": 1, "pose": {"hinge": 1}}]));
    assert!(e.contains("rise"), "{e}");
}

#[test]
fn loft_sections_placed_by_frames_bend_and_close_into_a_loop() {
    // Sections round a ring of radius `r` about z: x outward, z along it.
    let ring = |r: f64, n: usize, radius: f64| -> Vec<Value> {
        (0..n)
            .map(|i| {
                let (s, c) = trig::sin_cos(trig::TAU * i as f64 / n as f64);
                json!({"profile": {"circle": {"radius": radius}},
                    "at": {"origin": [r * c, r * s, 0], "x": [c, s, 0], "y": [0, 0, -1], "z": [-s, c, 0]}})
            })
            .collect()
    };
    let p = |sections: Vec<Value>, closed: bool| {
        json!({"name": "t", "ops": [
            {"op": "loft", "name": "ring", "sections": sections, "closed": closed},
        ]})
    };
    // A loop of eight circles is a torus, near enough: closed, no ends.
    let m = part(&p(ring(20.0, 8, 3.0), true), "ring");
    let torus = 2.0 * trig::PI * trig::PI * 20.0 * 9.0;
    assert!(
        m.closed && (m.volume - torus).abs() < 0.03 * torus,
        "{}",
        m.volume
    );
    // Open, the first three sections are a bend with two ends.
    let bend = ring(20.0, 8, 3.0)[..3].to_vec();
    let m = part(&p(bend, false), "ring");
    assert!(m.closed && m.volume > 0.0 && m.volume < torus / 3.0);
    // A bend tighter than the section is wide folds: refused.
    let e = err(p(ring(2.0, 8, 3.0), true));
    assert!(e.contains("passes through itself"), "{e}");
    // A loop needs frames; heights and frames do not mix.
    let stacked = json!([
        {"profile": {"circle": {"radius": 3}}, "z": 0},
        {"profile": {"circle": {"radius": 3}}, "z": 5},
    ]);
    let e = err(json!({"name": "t", "ops": [
        {"op": "loft", "name": "l", "sections": stacked, "closed": true}]}));
    assert!(e.contains("placed by at"), "{e}");
    let mut mixed = ring(20.0, 8, 3.0);
    mixed[0] = json!({"profile": {"circle": {"radius": 3}}, "z": 0});
    let e = err(p(mixed, false));
    assert!(
        e.contains("every section by z, or every section by at"),
        "{e}"
    );
}

/// A shell's hollow where it pinches off, or where a hole or a boss
/// meets it: the part is taken apart at its holes, each piece's inner
/// solid found, and the pockets' cut from the bodies' (`shape-edit`'s
/// `pieces`, `docs/research.md` §38). The hollows are the closed forms.
#[test]
fn a_shell_is_hollowed_piece_by_piece_where_its_hollow_pinches_off() {
    let up = |x: f64, y: f64, z: f64| json!({"origin": [x, y, z], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]});
    // The hollow's volume, after the ops and a shell `t` thick.
    let hollow = |ops: Vec<Value>, t: f64, open: Value| -> f64 {
        let mut s = Session::new("t");
        for op in ops {
            s.apply_value(&op).unwrap();
        }
        let solid = s.model().build().unwrap().scene().parts[0]
            .solid
            .measure()
            .volume;
        let answer = s
            .apply_value(&json!({"op": "shell", "id": 0, "thickness": t, "open": open}))
            .unwrap();
        assert_eq!(answer, json!({"crossingPairs": 0}));
        let m = s.model().build().unwrap().scene().parts[0].solid.measure();
        assert!(m.closed, "{m:?}");
        solid - m.volume
    };
    let case = |height: f64, radius: f64| {
        json!({"op": "extrude", "name": "case", "height": height,
            "profile": {"rect": {"width": 80, "height": 50, "radius": radius}}})
    };
    let pocket = |side: f64, depth: f64, z: f64| {
        json!({"op": "pocket", "id": 0, "depth": depth,
            "profile": {"rect": {"width": side, "height": side}}, "at": up(0.0, 0.0, z)})
    };
    let near = |v: f64, want: f64| assert!((v - want).abs() < 1e-6 * want, "{v} {want}");
    // A pocket 30 wide, 40 deep, walls 6: the pocket's own wall (42 wide)
    // reaches through the hollow (38 deep), which is two chambers.
    near(
        hollow(
            vec![case(80.0, 5.0), pocket(30.0, 40.0, 80.0)],
            6.0,
            json!([]),
        ),
        68.0 * 38.0 * 68.0 - 42.0 * 38.0 * 40.0,
    );
    // Walls exactly twice the thickness beside the pocket: cut through.
    near(
        hollow(
            vec![case(80.0, 5.0), pocket(10.0, 6.0, 80.0)],
            10.0,
            json!([]),
        ),
        60.0 * 30.0 * 60.0 - 30.0 * 30.0 * 6.0,
    );
    // A boss thinner than two walls is solid through: the hollow is the
    // case's alone.
    let boss = vec![
        case(40.0, 4.0),
        json!({"op": "cylinder", "name": "boss", "radius": 8, "height": 15, "on": up(0.0, 0.0, 39.0)}),
        json!({"op": "join", "target": 0, "tool": 1}),
    ];
    near(hollow(boss, 9.0, json!([])), 62.0 * 32.0 * 22.0);
    // An L-shaped pocket 12 deep, walls 6: the L grown by 6, where it
    // lies in the hollow.
    let l = json!({"op": "pocket", "id": 0, "depth": 12, "at": up(0.0, 0.0, 40.0),
        "profile": {"poly": {"points": [[-20, -10], [20, -10], [20, 0], [-8, 0], [-8, 14], [-20, 14]]}}});
    near(
        hollow(vec![case(40.0, 4.0), l], 6.0, json!([])),
        68.0 * 38.0 * 28.0 - (52.0 * 22.0 + 24.0 * 13.0) * 12.0,
    );
    // A mug, walls 6: its handle (a tube of radius 4) is solid through,
    // the hollow the can's (a 14 mm circle in chords, 44 tall).
    let mug = vec![
        json!({"op": "cylinder", "name": "mug", "radius": 20, "height": 50}),
        json!({"op": "torus", "name": "handle", "radius": 12, "tube": 4, "axis": "+Y", "on": up(22.0, 0.0, 25.0)}),
        json!({"op": "join", "target": 0, "tool": 1}),
    ];
    let v = hollow(mug, 6.0, json!(["top"]));
    let round = trig::PI * 14.0 * 14.0 * 44.0;
    assert!(v < round && v > 0.99 * round, "{v} {round}");
    // A can with a hole through its side (a hole in a curved wall), and
    // holes, blind and through, in an open face: clean.
    let side = json!({"origin": [21, 0, 20], "x": [0, 1, 0], "y": [0, 0, 1], "z": [1, 0, 0]});
    let can = vec![
        json!({"op": "cylinder", "name": "can", "radius": 20, "height": 40}),
        json!({"op": "hole", "id": 0, "diameter": 6, "at": [side]}),
    ];
    assert!(hollow(can, 8.0, json!([])) > 0.0);
    let holes = vec![
        case(40.0, 4.0),
        json!({"op": "hole", "id": 0, "diameter": 8, "depth": 15, "at": [up(-20.0, 5.0, 40.0)]}),
        json!({"op": "hole", "id": 0, "diameter": 5, "at": [up(20.0, -8.0, 40.0), up(0.0, 12.0, 40.0)]}),
    ];
    assert!(hollow(holes, 9.0, json!(["top"])) > 0.0);
}

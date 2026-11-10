//! Every part the evaluation's programs build is closed and does not pass
//! through itself (`docs/research.md` §26): a standing check on what an
//! agent's programs actually make.

use shape_model::program::run;

/// Parts known to cross themselves, with the section that explains why,
/// for a cause another stream owns. This list may only shrink.
const KNOWN: &[(&str, &str, &str)] = &[];

#[test]
fn every_part_of_the_evaluation_programs_is_closed_and_uncrossed() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../eval/phase6/Z/Z1");
    let mut dirs: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.join("program.json").is_file())
        .collect();
    dirs.sort();
    assert_eq!(dirs.len(), 20, "the evaluation's 20 programs");
    let (mut entries, mut parts, mut bad) = (0, 0, Vec::new());
    for d in &dirs {
        let program = d.file_name().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(d.join("program.json")).unwrap();
        let m = run(&src).unwrap_or_else(|e| panic!("{program}: {e}"));
        let built = m.build().unwrap();
        for p in built.scene().parts {
            entries += 1;
            if p.solid.triangles.is_empty() {
                continue;
            }
            parts += 1;
            let closed = p.solid.measure().closed;
            let pairs = shape_bool::crossing_pairs(p.solid);
            let known = KNOWN.iter().any(|k| k.0 == program && k.1 == p.name);
            if (!closed || pairs > 0) != known {
                bad.push(format!(
                    "{program} {}: closed {closed}, {pairs} crossing pairs",
                    p.name
                ));
            }
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
    // 140 scene entries, 21 of them links and groups with no mesh.
    assert_eq!((entries, parts), (140, 119), "every built solid checked");
}

/// A part in its own frame and the same part moved into the world.
fn here_and_moved(json: &str, part: &str, origin: [f64; 3]) -> [usize; 2] {
    let mut v: serde_json::Value = serde_json::from_str(json).unwrap();
    let moved = run(&v.to_string()).unwrap();
    // The same ops with every frame moved back by `origin`.
    for op in v["ops"].as_array_mut().unwrap() {
        if let Some(o) = op.get_mut("on").and_then(|on| on.get_mut("origin")) {
            for (k, x) in o.as_array_mut().unwrap().iter_mut().enumerate() {
                *x = serde_json::json!(x.as_f64().unwrap() - origin[k]);
            }
        }
    }
    let here = run(&v.to_string()).unwrap();
    [here, moved].map(|m| {
        let s = m.world_solid(m.find(part).unwrap()).unwrap();
        assert!(s.measure().closed, "{part} is open");
        shape_bool::crossing_pairs(&s)
    })
}

#[test]
fn a_part_moved_into_the_world_does_not_cross_itself() {
    // The mouse's shell, its wheel slot cut in its own frame: no crossing
    // there; moved by (10, 15, 4), two cut points an ulp apart folded a
    // needle over the slot's end wall (8 pairs) until `placed` repaired it.
    let slot = include_str!("../../../golden/programs/placed_slot.json");
    assert_eq!(
        here_and_moved(slot, "mouse_shell", [10.0, 15.0, 4.0]),
        [0, 0]
    );
    // The cup of §25.3: its rim joined in its own frame, turned 22.5° into
    // the world (5 pairs on its top face before).
    let rim = include_str!("../../../golden/programs/touching_rim.json");
    let m = run(rim).unwrap();
    let s = m.world_solid(m.find("cup").unwrap()).unwrap();
    assert!(s.measure().closed);
    assert_eq!(shape_bool::crossing_pairs(&s), 0);
}

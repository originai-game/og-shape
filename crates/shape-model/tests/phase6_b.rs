//! Every program of the phase 6 evaluation's run B
//! (`eval/phase6/B/*/program.json`, written by an agent with the guides)
//! builds natively, every part closed and none passing through itself
//! (`docs/research.md` §27): a change that refuses or breaks an ordinary
//! program shows in `cargo test`, not only in the binding's test.

use shape_model::program;

/// What is wrong with one program's build, if anything.
fn check(dir: &str, o: &str) -> Vec<String> {
    let src = std::fs::read_to_string(format!("{dir}/{o}/program.json")).unwrap();
    let model = match program::run(&src) {
        Ok(m) => m,
        Err(e) => return vec![format!("{o}: {e}")],
    };
    let built = match model.build() {
        Ok(b) => b,
        Err(e) => return vec![format!("{o}: {e}")],
    };
    let mut wrong = Vec::new();
    // Groups and assemblies hold no triangles of their own.
    for p in built
        .scene()
        .parts
        .iter()
        .filter(|p| !p.solid.triangles.is_empty())
    {
        let m = p.solid.measure();
        if !m.closed || m.non_manifold_edges > 0 {
            wrong.push(format!("{o} {}: {m:?}", p.name));
        }
        let pairs = shape_bool::crossing_pairs(p.solid);
        if pairs > 0 {
            wrong.push(format!("{o} {}: {pairs} crossing pairs", p.name));
        }
    }
    wrong
}

#[test]
fn every_run_b_program_builds_closed_and_uncrossed() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../eval/phase6/B");
    let mut objects: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().join("program.json").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    objects.sort();
    assert_eq!(objects.len(), 20);
    let wrong: Vec<String> = objects.iter().flat_map(|o| check(dir, o)).collect();
    assert!(wrong.is_empty(), "{wrong:#?}");
}

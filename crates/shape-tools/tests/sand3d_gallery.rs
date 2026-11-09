//! Phase 1's acceptance: ten sand3d programs rebuilt with existing
//! generators only. Each part is a closed solid, the GLB reads back with
//! its joints, and the preview renders. `cargo run --release -p
//! shape-tools --example phase1 -- docs/previews/phase1` writes the
//! previews for a person to look at.

#[path = "phase1/mod.rs"]
mod phase1;

use phase1::programs;
use shape_tools::{Preview, glb, preview};

#[test]
fn every_program_builds_exports_and_previews() {
    let models = programs::all();
    assert_eq!(models.len(), 10);
    for m in &models {
        for (name, s) in m.solids() {
            let ms = s.measure();
            assert!(ms.closed, "{}: {name} is open", m.name);
            assert!(ms.volume > 0.0, "{}: {name} faces in", m.name);
        }
        let scene = m.scene();
        let bytes = glb(&scene).unwrap_or_else(|e| panic!("{}: {e}", m.name));
        let doc = gltf::Gltf::from_slice(&bytes).unwrap_or_else(|e| panic!("{}: {e}", m.name));
        let joints = doc
            .nodes()
            .filter(|n| {
                n.extras()
                    .as_ref()
                    .is_some_and(|x| x.get().contains("s3d_joint"))
            })
            .count();
        let declared = scene.parts.iter().filter(|p| p.joint.is_some()).count();
        assert_eq!(joints, declared, "{}", m.name);
        assert!(joints > 0, "{}", m.name);
        let views = preview(
            &scene,
            &Preview {
                width: 48,
                views: 1,
                ..Default::default()
            },
        )
        .unwrap();
        // Something other than background and ground was drawn.
        let px = &views[0].rgba;
        let distinct = px
            .chunks(4)
            .filter(|c| c[..3] != [246, 246, 244] && c[..3] != [222, 222, 218])
            .count();
        assert!(distinct > 48, "{}: an empty preview", m.name);
    }
}

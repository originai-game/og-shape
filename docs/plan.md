# og-shape plan

Each phase ends when its acceptance is met, with the gate green. Phases run
in order. Phase M (the merge) needs the owner's go-ahead.

## Where it runs

- **No GPU is needed.** Development runs on x86-64 Linux (an Amp orb, a
  cloud container or WSL), plus wasm32 under wasmtime and bun.
- **The other targets run on GitHub Actions** (`windows-latest`,
  `ubuntu-24.04-arm`), so they never take a slot or a lane on macrunner and
  never touch origin-engine's gate or landing queue.
- **aarch64 macOS is deferred.** It joins the gate as its own change
  (`macos-latest`), before phase 1 ends.

## Gate

`scripts/check.sh`, run locally and in GitHub Actions, does:
- fmt and clippy (`-D warnings`);
- the unit tests;
- the licence check (cargo-deny) and the determinism lint (`clippy.toml`);
- the golden output hashes of every operation on x86-64 Linux,
  x86-64 Windows, aarch64 Linux and wasm32 (wasmtime); aarch64 macOS joins
  later (§Where it runs).

One hash file is committed. Every target must reproduce it byte for byte.
A change that moves a hash says why in its commit and changelog.

## Phase 0: foundations and the boolean decision

- Workspace, `shape-core`, the determinism lint and the CI matrix.
- Copy og-geom from origin-engine at a pinned commit into `shape-gen` and
  `shape-bool`, recorded in `third_party/og-geom/SOURCE`.
- Benchmark og-geom's exact-predicate CSG against manifold-rust (pinned,
  sequential) on three suites:
  - the sand3d programs' booleans, rebuilt by hand as fixtures;
  - Thingi10K's closed meshes;
  - synthetic AI-mesh defects: clean meshes from the first two suites,
    rebuilt by marching cubes from their distance field, then given seeded
    noise, floaters, holes, flipped faces, duplicate vertices and
    self-intersections. No provider or gateway is needed. Real provider
    outputs re-check the decision in phase 5.
  
  Measure robustness (closed output, no lost faces), speed, and hash
  identity on all targets. Keep one boolean engine. Write the decision and
  its numbers into `docs/research.md`.
- **Acceptance:** the gate runs on the four targets of §Gate, and the
  decision is recorded.

## Phase 1: profiles and generators

- `shape-2d`: all profile constructors, 2D booleans, offset, stroke,
  triangulation, text via skrifa.
- `shape-mesh` names, and canonical ordering.
- `shape-gen`: every generator in `docs/design.md` §2, with sand3d's
  naming of caps and walls.
- **Acceptance:** every generator has a golden hash on every target. Rebuild
  ten sand3d gallery programs (no booleans or bevels) and inspect their
  previews.

## Phase 2: booleans and edits

- cut, join, intersect, split, hole, pocket, seat, shell, dent, merge, with
  names carried through.
- The CSG follow-ups of `docs/research.md` §5.4: operand validation, a
  broad phase, self-union.
- **Acceptance:** sand3d's edit examples rebuild with the same names on the
  same edges. Shell walls measure the requested thickness within
  tolerance.

## Phase 3: fillet and chamfer

- `shape-bevel`, clean-room from Blender's published Bevel V2 design
  (`docs/research.md` §2.2):
  - four offsets per edge;
  - a superellipse profile;
  - segments;
  - miters;
  - corner fill by valence (octant patch, the ADJ quad pattern by
    boundary-constrained Catmull-Clark, Cutoff fallback);
  - clamp per vertex with exact predicates;
  - UVs by mean-value interpolation.
- **Acceptance:** fillets and chamfers on the sand3d examples (bracket,
  washer, enclosure corners) produce all-quad corner patches, closed
  output, and the names `fillet` and `chamfer`. Inspect their previews.

## Phase 4: parts, joints, materials, export, preview

- Parts, groups, assemblies, joints, pose, materials.
- GLB export with joint extras; OBJ and STL.
- The CPU turntable preview.
- `shape-cli` and the wasm build with its TypeScript binding.
- **Acceptance:** sand3d's washing machine, rebuilt as a TypeScript program,
  exports a GLB that three.js loads with working joints. Its preview
  renders with no GPU.

## Phase 5: mesh tools for the platform

- import (glb/gltf/obj; fbx if a Rust-only path exists);
- repair (weld, floaters, degenerate faces, transforms, triangulate, join);
- decimate with LODs (meshoptimizer-rs);
- UV unwrap;
- measure.
- **Acceptance:**
  - Re-check the phase 0 boolean decision on real provider outputs.
  - Take provider outputs (Meshy, Tripo, Hunyuan), run the full chain
    `import → repair → decimate → unwrap → measure → preview → export`, and
    meet the Hunyuan texturing input contract (`docs/research.md` §3).
  - Everything the platform's `asset_process` does today is covered.

## Phase 6: the authoring guide

- `guides/shape.md`: how to plan an object (parts first, then details),
  naming, joints, budgets, and the review loop with previews and measures.
  It follows sand3d's eight assembly rules where they fit, with no platform
  names.
- Evaluate it: an agent with only the guide and the API rebuilds 20
  objects from sand3d's datasets (CC BY 4.0). Record the success rate and
  the token counts.
- **Acceptance:** the evaluation results are recorded in `docs/research.md`.

## Phase M: merge into origin-engine (needs the owner's go-ahead)

As in `docs/design.md` §8. It runs as one origin-engine stream from
origin/main, landed through `scripts/land.sh`, and draws the platform ask to
drop Blender from the asset image.

## Status

| Phase | State |
|---|---|
| Research | Done (2026-11-09): `docs/research.md` |
| 0 | Done (2026-11-09): gate on four targets; booleans: og-geom's CSG (`docs/research.md` §5) |
| 1 | In progress: `shape-mesh`, `shape-2d` (with gear and text), extrude, revolve, primitives, sweep, loft, helix, thread. Left: body, wire, knurl (needs phase 2 booleans), the ten sand3d gallery rebuilds and their previews (needs the phase 4 preview) |
| 2 | Nearly done: named `boolean`, operand validation, broad phase, `fuse`; split, hole (counterbore, countersink), pocket, seat, shell, dent, merge in `shape-edit`, sand3d's edit cases rebuilt as tests. Left: folded single surfaces |

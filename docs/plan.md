# og-shape plan

Each phase ends when its acceptance is met, with the gate green. Phases run
in order. Phase M (the merge) needs the owner's go-ahead.

## Where it runs

- **No GPU is needed.** Development runs on x86-64 Linux (an Amp orb, a
  cloud container or WSL), plus wasm32 under wasmtime and bun.
- **The other targets run on GitHub Actions** (`windows-latest`,
  `ubuntu-24.04-arm`, `macos-latest`), so they never take a slot or a lane
  on macrunner and never touch origin-engine's gate or landing queue.

## Gate

`scripts/check.sh`, run locally and in GitHub Actions, does:
- fmt and clippy (`-D warnings`);
- the unit tests;
- the licence check (cargo-deny) and the determinism lint (`clippy.toml`);
- the golden output hashes of every operation on x86-64 Linux,
  x86-64 Windows, aarch64 Linux, aarch64 macOS and wasm32 (wasmtime).

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

## Phase 5: mesh tools

- import (glb/gltf/obj; fbx if a Rust-only path exists);
- repair (weld, floaters, degenerate faces, transforms, triangulate, join);
- decimate with LODs (meshoptimizer-rs);
- UV unwrap;
- measure.
- A skinned input is refused by any tool that would break its skin
  (phase 5b makes them carry it).
- **Acceptance:**
  - Meshes from outside the kernel go through the full chain
    `import → repair → decimate → unwrap → measure → preview → export`:
    - scans and models like Thingi10K's;
    - the synthetic generated-mesh defects of phase 0;
    - other tools' GLB.
    
    They come out closed, joined, triangulated, under a triangle budget,
    and unwrapped.
  - Re-check the phase 0 boolean decision on outside meshes.
  - Every tool of `docs/research.md` §3.1's Blender pipeline has an
    equivalent here.

## Phase 5b: skinning

- Skin in the data model (`docs/design.md` §4): bones, inverse bind
  matrices, four weights per vertex.
- Bones from the program: parts and named regions become bones, joints
  become pivots, and weights blend across the seams the program names.
- Imported skins are kept through repair, decimation, unwrap and export;
  decimation interpolates weights and keeps bone-influence borders.
- GLB export of a glTF skin; the preview poses it.
- **Acceptance:**
  - A stylised figure (body, limbs, head) written as a program exports a
    skinned GLB that three.js loads and poses without tearing.
  - An imported skinned GLB comes back out with the same bones and its
    weights, after decimation to half.

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
| 1 | Done (2026-11-09): ten sand3d programs rebuilt and previewed (`docs/previews/phase1/`). The gaps they found (§6.4) are closed: placement (stream E), loft holes and splines, +z axes, body, wire, knurl, gear solid (stream F, §10) |
| 2 | Done (2026-11-09): named `boolean`, operand validation, broad phase, `fuse`, the CSG rounding fixes; split, hole (counterbore, countersink), pocket, seat, shell, dent, merge in `shape-edit`; sand3d's edit cases rebuilt as tests, shell walls exact. A single surface folded through itself is refused; repairing it is phase 5's |
| 3 | Done (2026-11-09): `shape-bevel` fillet, chamfer, soften; sand3d's bracket, washer and enclosure corners closed with all-quad patches. Left: UVs (Solid has none yet), arc and patch mitres, pipe joints, loop slide (`docs/research.md` §7) |
| 4 | Done (2026-11-09): export, preview, `shape-model` (parts, groups, placement, joints, pose, materials), `shape-cli`, `shape-wasm` and the TypeScript binding; sand3d's washing machine rebuilt in TypeScript, GLB with joints, preview (`docs/previews/phase4/`). Gaps in `docs/research.md` §8.5 |
| 5 | Done (2026-11-09): import (glTF, OBJ, FBX), repair, own QEM decimation with LODs, UV unwrap, measure as JSON, plain GLB; the full chain on outside meshes (`docs/research.md` §9). Skinned inputs are detected and kept on import; tools that would break a skin refuse it. A uniform armature scale (Mixamo's 0.01) is taken out of the bones; Soldier, Xbot and RobotExpressive import and pose (`docs/research.md` §9.9) |
| MCP | Done (2026-11-09): `shape` CLI mesh-tool commands; `shape-mcp` server on stdio (build, preview, measure, export, import, repair, decimate, unwrap, process, TypeScript programs; docs as resources); `guides/mcp.md`, `guides/typescript.md` |
| 5b | Done (2026-11-09): skin in `Solid`; bones from the program (`shape-skin`), posing, glTF skin export; imported skins (glTF, FBX, uniform armature scale stripped) kept through repair, join, apply_transform, decimate (bone borders kept by a penalty) and unwrap. Acceptance: a figure from a program posed; Soldier, Xbot, RobotExpressive decimated to half with the same bones and weights summing to 1, no skin drift (`docs/research.md` §11) |
| 6 | Done (2026-11-09): `guides/shape.md` + `guides/typescript.md` (about 6 200 tokens together, samples built by a test); evaluation on 20 sand3d dataset objects (12 articulated): 20/20 built and closed in every run, joints and sizes as the reference, failed builds 9 → 3 with the revised guide (`docs/research.md` §13). Kernel gaps found: §13.6 |
| Follow-ups | Done: self-intersection check and self-union (stream J, §5.4 item 3); bevel mitres, pipes, loop slide (stream K, §7.4). Done for most of §13.6: booleans on touching, coplanar, tangent operands close or are refused naming the faces (stream L, §5.4); every generator and field in the program, binding, CLI and MCP, segment control, strict fields, located errors (stream M, §14); loft corners, refused folds, interference measure (stream N, §15). Then stream O: collinear slivers removed from boolean output, exact axis entry for holes (§13.6 items 12, 13); stream P: two cones at a vertex, in-line edges across curved gaps, the untested bevel cases (§7) |

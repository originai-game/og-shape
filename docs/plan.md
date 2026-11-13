# og-shape plan

Each phase ends when its acceptance is met, with the gate green. Phases run
in order. Phase M (the merge into origin-engine) is dropped: og-shape is its
own project (the owner, 2026-11-11).

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
  triangulation, text via ttf-parser.
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

## Phase M: merge into origin-engine (dropped, 2026-11-11)

Not going ahead: og-shape stays its own project. What it would have been:

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
| Follow-ups | Done: self-intersection check and self-union (stream J, §5.4 item 3); bevel mitres, pipes, loop slide (stream K, §7.4). Done for most of §13.6: booleans on touching, coplanar, tangent operands close or are refused naming the faces (stream L, §5.4); every generator and field in the program, binding, CLI and MCP, segment control, strict fields, located errors (stream M, §14); loft corners, refused folds, interference measure (stream N, §15). Then stream O: collinear slivers removed from boolean output, exact axis entry for holes (§13.6 items 12, 13); stream P: two cones at a vertex, in-line edges across curved gaps, the untested bevel cases (§7); stream Q: bevels wider than a curved face's segment, `TooLarge` 1 233 → 446 of 17 516 in the scan (§7.6). Then six streams together (2026-11-10): R, the rest of the authoring surface in the program, binding, CLI and MCP (§16); S, UVs per triangle corner on `Solid`, carried by import, export, repair, decimation and booleans (§17; left: bevels refuse a solid with UVs, loft and sweep give none); T, turned elbows join, winding numbers by a tree, the degenerate-triangle rule (§18); U, the binding under node and Chromium, three.js, a real MCP client, progress and cancellation (§19); V, preview opacity and creases, joints through `copy` and `mirror`, posed OBJ, STL and measures (§20); W, a bevel never returns a crossing result as ok (§7.7; left: an edge that tapers to a tip at both ends is refused, 10 812 of the scan's 17 516). Then four more (2026-11-10): X, lone crease pieces taper to tips and twisted faces are cut on their folds, the scan's ok 6 012 → 16 564 (§7.8); Y, UVs from every generator, through bevels and edits, asked for by the program, with a texture map on a material (§21); Z, the evaluation on the finished surface through MCP: 20/20 built and closed, 24 joints as the reference, 2.0 builds an object (§22; kernel gaps left: a sweep's ends are square to the chord, not the path; a fillet of a curved loft's top is refused); AA, the countersink's cone turned half a segment, and booleans repair a result that crosses itself by a rounding error (§23). Then three small ones (2026-11-10): AB, sweep and wire ends square to a curve's tangent, and a helix segmented on every turn (§24); AC, the fillet of a curved loft's top (§7.9); AD, the counterbore and the repair measured, (§25; left: a boolean result the repair leaves crossing itself is still returned without an error, as a cup's `shell` and faces a few ulps apart show, a placed part's rounding crossings, a tangent arc after a curve). Then (2026-11-10): AE, a part moved into the world no longer folds a needle through itself, and every evaluation program's parts are tested closed and uncrossed (§26); AF, `shell` untangles a folded inner solid, faces a few ulps apart are taken as one surface, and every boolean and edit reports the crossing pairs it leaves (§27; left: 76 of 4 920 swept shells still cross, reported, so the refusal stays off); AG, 54 published outputs of eight image- and text-to-3D generators through the mesh chain at three budgets: every run reports, 50 of 54 end closed, and the gaps for games are ranked (§28.7). Then the gaps §28.7 ranked (2026-11-10): AH, material factors, all five maps and vertex colours through the mesh chain, and a texture budget (§29); AI, the preview draws base-colour maps and vertex colours, a UV view and unlit views with their cameras (§30); AJ, `place`: pivot, real size and the 24 axis turns for imported meshes (§31); AK, decimation that does not fold, non-manifold repair, pockets, the fuse step faster (§32); AL, the four merged on one decoder, emissive in the preview, `cross_color_borders` (§33; left: keeping a texture, 14 of 54 generated meshes miss 2 000 triangles without atlas repacking or baking; 3 GiB on vertex-coloured meshes of 600 000 triangles and more); AM, atlas packing and baking: `process --bake` decimates with fresh UVs and bakes the source's base colour, data maps, vertex colours and normals into one atlas, and all 54 generated meshes reach 2 000 and 10 000 triangles with their look, 52 of them closed (§34; left: at 10 000 keeping UVs looks closer where it reaches the budget, the preview draws no normal map) |
| Additions (2026-11-11) | Done, each its own commit: convex hull and `wrap`; colliders (`shape collide`, convex decomposition); `subdivide` (Catmull-Clark, sharp edges); animation clips in the program and the GLB; the preview reads normal maps; `shell` of a convex solid cut from its planes when the copy folds, and refused when the walls meet (`docs/research.md` §35: 75 crossing shells of the sweep to 3); `repair` holds one mesh at a time (§36: 2.7 GiB to 0.9 GiB on 619 000 coloured triangles); `loft` through sections placed by frames, closed into a loop; `thicken` for open sheets (`shape repair --thicken`). Then: shells of solids that are not convex have their turned faces collapsed, and a fillet as wide as its round corner ends in a point (§37: 1 of 7 280 swept shells crossing). Then: a hollow that pinches off, and parts with holes, bosses and handles, are hollowed piece by piece (§38). Then: holes that cross inside a part and a rounded rim with pockets build (§48: 120 of 126 swept; §64: 125, the one left a limit of the boolean). Then, 2026-11-12: meshopt and Draco reading (§42, §44), morph targets (§43), FBX eased curves, layers and blend shapes (§45, §46), meshes moved by their nodes (§47), `unwrap --shared` (§49). Then: KTX2 of UASTC blocks is read and written (`process --ktx2`, §50). Then: ETC1S under BasisLZ read and written (`--etc1s`, §57), meshopt written (`shape compress`, §51), URDF and USD export (§52, §54), the preview's metal and maps (§53), conformal charts (§55), MikkTSpace (§56). Then: WebP decoded (§58); a block-copy rate-distortion pass for UASTC (§59); Draco written (§60); quantized attributes before meshopt (§61); morph targets through the kernel's operations (§62); repair's chains and large holes (§63); 125 of the 126 swept shells (§64). Then, 2026-11-13: seams cut by the unwrap (§65); a skin through the kernel's operations, and targets exact after a bevel or a subdivision (§66); Draco written as an EdgeBreaker walk (§67); meshopt's triangle streams fixed, a vertex-cache order, and skinned, morphing and animated meshes quantized (§68); ETC1S runs and WebP written (§69); three holes through one point (§70); large holes closed round and the bench of 54 again (§71); URDF inertials and limits (§72); USD skeletons, clips, joints and `.usdz` (§73). Left (README, "Not done"): outputs no outside tool has read (KTX2, Draco, USD, URDF); `Solid` itself is 300 MiB at 600 000 coloured triangles (§36). |

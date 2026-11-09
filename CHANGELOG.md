# Changelog

## Unreleased

- Design: og-shape is "3D modeling in Rust, built for LLMs". It owns its
  capabilities, not where an agent's other meshes come from. Skinning is
  planned (phase 5b): bones from the program, imported skins carried
  through every tool, never broken silently.
- Licence list: Unicode-3.0 allowed (Unicode data tables; reached through
  `gltf` in tests), in `AGENTS.md` and `deny.toml`.
- Repository created: design, plan and verified research.
- Cargo workspace and `shape-core`: `DVec3` and `Frame`, `trig` (libm),
  exact predicates (`orient2d`, `orient3d`, `incircle`, `insphere`),
  canonical ordering, and the SHA-256 output hash, which refuses NaN.
- The determinism lint: `clippy.toml` denies `std` transcendental maths,
  `mul_add`, `powi`, threads, `HashMap`/`HashSet` and glam's SIMD types.
- The gate: `scripts/check.sh` (fmt, clippy, cargo-deny licences and bans,
  tests, golden hashes natively and as wasm32 under wasmtime), and GitHub
  Actions on x86-64 Linux, x86-64 Windows and aarch64 Linux. The first
  golden hashes cover `shape-core`.
- `shape-gen` and `shape-bool`: origin-engine's og-geom copied at
  `d4f4d4d` (`third_party/og-geom/SOURCE`): three.js's extrude, lathe,
  sweep, polyhedra, merge, and exact-predicate CSG and fracture. og-geom's
  own contract tests pass unchanged, including its recorded output hashes,
  so the copy gives the engine's bytes. Golden cases added for each.
- Phase 0 decision: booleans stay on og-geom's exact-predicate CSG;
  manifold-rust is dropped (`docs/research.md` §5). The benchmark is in
  `bench/booleans`.
- `shape-mesh`: `Solid` (f64 triangles with surfaces, face names and edge
  names), name lookup that lists existing names on a miss, canonical
  ordering, measures, and the output hash.
- `shape-2d`: profiles (`rect` with corner radius, `circle`, `ellipse`,
  `slot`, `polygon`, `poly` with per-corner fillets, `superellipse`, and a
  `Path` with lines, three-point and tangent arcs, Béziers and splines),
  each point flagged sharp or smooth; union, subtract, intersect, xor,
  offset, stroke (i_overlay, integer core), mirror, hull (exact
  predicates), transforms; triangulation by spade's constrained Delaunay
  on the exact contour points. Curves follow one rule: 0.05 mm chord, 24
  to 64 segments a turn.
- `shape-gen::solids`: named generators. `extrude` (twist, taper, both
  sides) names `bottom`, `top`, `side` (and `inner` on hole walls) and the
  edges between them; `revolve` names `side`, `rim`, `start`, `end`;
  `cuboid` (also `left`, `right`, `front`, `back`), `cylinder`, `cone`,
  `tube`, `sphere`, `dome`, `torus`, `wedge`. Every output is canonical.
- Golden cases for profiles, 2D booleans, offsets, triangulation and the
  named generators.
- `shape-2d::gear` (involute spur gear, ISO 53 proportions) and `text`:
  glyph outlines from a font file the caller supplies (ttf-parser; no font
  ships), filled by the non-zero rule, corners where segments turn more
  than about 11°. `outline_profile` takes outline commands directly.
- `shape-gen::paths`: `sweep` along a polyline with rotation-minimizing
  frames (double reflection), twist and end scale (`start`, `end`,
  `side`); `helix`; `thread` (a 60° ridge swept on a helix, faces also
  `thread`); `loft` through stacked sections, straight or Catmull-Rom
  (`bottom`, `top`, `side`).
- `shape-bool::boolean` (phase 2): join, cut and intersect on named solids
  in f64. Faces keep their names, also as `<part>.<tag>`; a cutting
  tool's faces get `cut`; edges where operands meet get `cut`,
  `<target>.cut`, the tool's name and the cap names; pieces of named input
  edges keep their names; named edges between coplanar faces lose them.
  An operand that is not closed and oriented after welding is refused
  with an error value (`docs/research.md` §5.4, follow-up 1).
- `shape-bool::csg_points`: the f64 core of og-geom's CSG on welded
  operands; `csg` and its output bytes are unchanged.
- CSG broad phase: a sweep along x replaces the all-pairs box test, with
  the output unchanged. Two 65 536-triangle spheres: 9.0 s → 1.15 s.
- `shape-edit` (phase 2), after sand3d's verbs:
  - `split` by a frame's plane, keeping above or below;
  - `hole` at frames or points (`above(x, y)`): through or to a depth,
    with a counterbore or countersink, starting where the axis enters.
    Mouth edges are named `hole`, bores `hole.side`, floors `hole.bottom`;
  - `pocket`: a profile cut in from a face, named `pocket.bottom` and
    `pocket.side`;
  - `seat`: the volume a part fills, refused when they share none;
  - `shell`: hollowing by an inner solid offset through a least-squares
    fit of the planes at each vertex. Faces are opened by name or by
    direction (`+z`); curved faces are refused;
  - `dent`: round hollows pressed in, with crack-free local subdivision;
  - `merge`: no boolean.
  sand3d's own holes, pockets and shell cases are rebuilt as tests,
  including the wall thickness.
- `shape-bool::fuse` and `components`: a solid of several shells is fused
  before a boolean.
- CSG fix: a plate minus a pin whose vertex columns lie on the plate
  caps' 45° diagonal is now closed (it had open or non-manifold edges
  around the bore; `docs/research.md` §5.4, item 4). Crossing points that
  round onto or past a triangle's outline are placed just inside it for
  that triangle's triangulation, and crossings that round to one position
  are one point. Triangle normals take their components from `orient2d`.
  Every output hash is unchanged.
- `hole`'s tools are no longer turned half a segment (a countersink's cone
  a quarter segment) off the frame's axes: a hole's polygon now starts on
  its frame's x axis. The `edit.holes.pocket` golden hash moves for that
  reason alone.
- Plan: aarch64 macOS joins the gate later; phase 0 benchmarks booleans on
  synthetic AI-mesh defects instead of provider outputs, which re-check the
  decision in phase 5.
- `shape-tools` (phase 4, export and preview): `Scene` of parts, each a
  solid with a `Material` (three.js names: `color`, `metalness`,
  `roughness`, `emissive`, `opacity`) and an optional `Joint`.
  - `glb`: glTF 2.0 binary, metres, Y up; normals smooth within a surface
    and split at real edges; one node, mesh and material per part; joints
    as `s3d_joint` node extras (`docs/research.md` §1.5), posed at their
    `value`.
  - `obj` (Y up) and binary `stl` (Z up), metres, meshes only.
  - `preview`: a turntable of views from a deterministic CPU rasterizer
    (integer coverage, Lambert, real edges, ground shadow) as RGBA images;
    `png` encodes one (miniz_oxide), `sheet` tiles them.
  - Golden cases `tools.glb`, `tools.obj`, `tools.stl`, `tools.preview`
    (framebuffer bytes) and `tools.preview.png`.
- Phase 1 acceptance: ten sand3d programs (gallery A2–A7, A9, A10, A12, and
  `docs/examples/gate.py`) rebuilt with existing generators, exported and
  previewed; previews in `docs/previews/phase1/`, gaps in
  `docs/research.md` §6.4.
- `shape-bevel` (phase 3): `fillet`, `chamfer` (`distance2` or `angle`) and
  `soften` on named edges, clean-room from Blender's published Bevel
  design (`docs/research.md` §7). Superellipse profiles, segments from the
  chord rule, mitred two-edge corners, octant patches at three-edge
  corners, the ADJ pattern by boundary-constrained Catmull-Clark above
  three; planar and curved neighbours (cylinder rims, hole rims). New
  faces are named `fillet` or `chamfer`; their sharp edges take the kind
  and the cap names they touch. An offset larger than the faces allow is
  `BevelError::TooLarge`, never a clipped result. `quad_pairs` measures
  how much of a patch is quads. Golden cases `bevel.*` added.
- `shape-gen`, the generators the phase 1 rebuilds found missing
  (`docs/research.md` §10):
  - `loft` takes sections with holes (one region each, as many holes in
    every section, matched by nearest centre; hole walls `side` +
    `inner`) and a `curve` field: `Auto` (as before), `Straight`,
    `CatmullRom`, or `Spline`, sand3d's natural cubic spline through
    every section by height. `loft` moves to `shape_gen::loft` and is
    re-exported from `paths`.
  - `Revolve` gains `axis`: `Axis::Y` (default, as before) or `Axis::Z`
    (sand3d's: profile y is height). `sphere_desc`, `torus_desc` and
    `dome_desc` take an `axis` too, and with `Axis::Z` rest on z = 0;
    `DomeDesc::height` makes sand3d's spherical cap.
  - `body`: sand3d's solid along +y from plan, top and bottom views and a
    squareness; superellipse sections, straight or spline views, steps,
    pointed or capped (`start`, `end`) ends.
  - `wire`: a round wire along a path, corners bent round a `fillet`
    radius (one, or one per point) or mitred (`seam` edges); open (`start`,
    `end`) or closed.
  - `knurl`: sand3d's diamond or straight knurl on a cylinder.
  - `gear_solid`: an involute gear with a `bore` and a `hub`, no boolean.
  - `GenError::Invalid` carries a message for inputs that cannot make a
    solid.
  - Golden cases `gen2.*` added; every existing hash is unchanged. A
    preview sheet is in `docs/previews/gen2/`
    (`cargo run -p shape-tools --example gen2_preview -- <dir>`).
- Placement (phase 4): `Frame` gains `facing` (CadQuery's face axes),
  `with_up`, `offset`, `translate`, `turn` (radians, own X then Y then Z,
  quarter turns exact), `inverse` and `is_rigid`. `shape_model::place`
  reads frames off solids: `bbox("+X-Y")`, `face(name or "+X")` on the
  face's centroid with +Z its normal, `edge(name)` as a path, and the
  distributors `grid`, `ring`, `along` and `snap`.
- `shape-model` (phase 4): `Model` holds named parts (a solid, a
  placement, a material), groups, assemblies with anchors and `attach`,
  `copy` and `mirror`, units (mm, cm, m; metres on export). Booleans,
  `seat`, holes, pockets, `split`, `shell`, `dent`, fillets and chamfers
  run on parts with world frames; a boolean uses its tool up. Joints:
  revolute, continuous, prismatic, fixed and floating, with axis, range
  and mimic, several on one child (composed in order); `pose` within the
  range. `Model::glb`, `obj`, `stl` and `preview` export the posed model
  through `shape-tools`, a jointed group as a node with no mesh, a second
  joint on one child through a `<child>:<joint>` link.
- `shape-tools`: `JointKind::Fixed`, written `"type": "fixed"`, and
  `ExportError::FixedConstrained`. Existing outputs are unchanged.
- `shape_model::program`: the model API as JSON operations; the
  operations that change a model are recorded as a program that replays
  to the same bytes.
- `shape-cli` (the `shape` binary): `build`, `export` (GLB, OBJ, STL),
  `measure` (JSON) and `preview` (PNG) on a recorded program.
- `shape-wasm` and the private `og-shape` TypeScript binding
  (`crates/shape-wasm/ts`): `load()`, `og.model({ name })`, operations
  with desc objects, frames with `offset`, `move` and `turn` computed in
  wasm. The gate builds it for wasm32 and runs its tests when
  wasm-bindgen and bun are installed.
- Phase 4 acceptance: sand3d's washing machine rebuilt as a TypeScript
  program through the binding; its GLB's four joints checked in three.js
  (headless), its preview in `docs/previews/phase4/`, its program in
  `golden/programs/` (`docs/research.md` §8).
- Golden cases added (no existing hash moved): `model.frame`,
  `model.place`, `model.joints.glb`, `model.joints.preview`,
  `model.washing_machine.glb`, `.png` and `.measure`.
- Skins in the data model (phase 5b, part 1): `Solid` has an optional
  `skin` (`shape_mesh::Skin`): named bones with a parent, a rest frame and
  an inverse bind matrix, and up to four normalized `(bone, weight)` pairs
  per vertex (`Weights`, with `from_raw` and `blend` for interpolation).
  `canonicalize` carries weights through sorting; welded vertices take the
  mean of their weights. An unskinned solid hashes as before.
- Booleans (`boolean`, `fuse`), the `shape-edit` edits and the bevels
  refuse a skinned solid with a new `Skinned` error; they never drop a
  skin. `append` and `merge` keep a skin when the skeletons match, and
  otherwise leave vertices unweighted for `Skin::check` to report.
- `shape-skin`: `skin(solid, &Rig)` makes a bone of each named face region
  and blends weights across the seams between regions over `blend`
  millimetres of surface distance, splitting long edges in the band
  first; `pose` (linear blend skinning from bone turns), `skin_matrices`,
  and `measure_pose` (closed, inverted triangles with their area and best
  shape, area and volume ratios). `docs/research.md` §11.
- `glb` writes a skinned part's glTF skin: bone nodes under the part's
  node, inverse bind matrices, `JOINTS_0` and `WEIGHTS_0`. A skinned part
  on a joint is `ExportError::BadSkin`. Unskinned scenes write the same
  bytes as before.
- Phase 5b part 1 acceptance: a stylised figure (torso, head, arms, legs)
  built from loft, sweep, sphere, cuboid and boolean joins, skinned with
  ten bones and posed with an elbow and a knee at 90°; previews in
  `docs/previews/phase5b/`. Golden cases `skin.weld`, `skin.weights`,
  `skin.pose` and `skin.glb` added; no existing hash moved.
- `boolean` never returns open output: a result whose edges do not all
  pair up is `BoolError::OpenResult`. A closed, oriented operand that
  passes through itself used to give open output with no error.
- `shape-tools` mesh tools (phase 5) for meshes from outside the kernel
  (`docs/research.md` §9):
  - `import`, `import_gltf`, `import_obj`, `import_fbx`: glTF 2.0 (GLB,
    and `.gltf` with embedded or external buffers through a `Resolve`
    callback), OBJ (with MTL texture maps) and FBX 7.x binary and ASCII,
    into `ImportedPart`s (a solid each, its `Affine` transform, per-corner
    UVs, material name) and the textures' sizes (`ImageInfo`). Polygons
    are triangulated on the way in; faces carry their part's name.
    Draco and meshopt compression, and FBX 6, are `Unsupported` errors.
  - Skinned inputs fill `shape_mesh::Skin` completely (`docs/research.md`
    §11.4): glTF skin joints as bones, parents first, with world rest
    frames and the file's inverse bind matrices, `JOINTS_0`/`WEIGHTS_0`
    through `Weights::from_raw`; FBX skin deformers the same from their
    clusters' `Transform`/`TransformLink`, `Indexes` and `Weights`. A bone
    that is scaled or sheared (including through a scaled armature), or a
    skin that does not check, is refused at import. `measure` reports
    the skin. Repair's steps, `join`, `apply_transform` (now a `Result`),
    `decimate`, `lods`, `collapse_short_edges` and `glb_plain` refuse a
    skinned solid with an error value until part 2 carries skins through
    them; `glb` writes the skin, and `unwrap`, `measure` and `preview`
    take it.
  - `repair` with `Repair`: `weld` (closes cracks, or all vertices with
    `weld_all`), `remove_degenerate` (exact zero area, duplicates),
    `remove_floaters` (`Floaters` by size or fraction), `orient`
    (consistent winding, closed pieces facing out), `close_holes` (least
    area fill, pinched rims split, fills named `fill`), `fuse` of
    interpenetrating shells (taken only when the union is closed), and
    `collapse_short_edges` (`min_edge`, keeping the mesh manifold);
    `RepairReport` says what each step did.
    `apply_transform` and `join` place and merge imported parts.
  - `decimate` and `lods` with `Decimate` (ratio or triangle target, error
    limit, locked borders): quadric error metrics in f64 with the link
    condition, so closed meshes stay closed; features and names kept.
  - `unwrap` with `Unwrap`: charts grown within an angle with an exact
    no-overlap test, planar projection, smallest-box turn, skyline packing
    at one texel density, padding in texels; `Uvs` per triangle corner.
  - `measure` with `Measure`, `MeshMeasures::to_json`: counts, bounds,
    pivot, area, volume, closed, boundary loops, non-manifold and
    inconsistent edges, shortest edge, degenerate and duplicate triangles,
    pieces and floaters, exact self-intersections, the skin, UV coverage
    and overlap, texture memory once decoded (with mips). No thresholds.
  - `glb_plain` with `PlainGlb`: one node, one mesh, one primitive with
    positions, normals and `TEXCOORD_0`, what texturing tools take.
  - Golden cases `meshtools.*` added; no existing hash moved. The
    acceptance chain is `bench/meshtools` (`--glb` runs other tools'
    files); `guides/meshtools.md` says what the measures mean.
- Skinned imports under a uniformly scaled armature (Mixamo's 0.01) are
  accepted: the scale is taken out of the bones, their rest frames come
  from the file's bind pose (its inverse bind matrices) with the scale
  divided out, the inverse bind matrices are recomputed from them, and
  the vertices stay where the bind pose puts them in the world. glTF and
  FBX. `Imported::skin_scales` (`SkinScale`) reports the scale, how well
  the file's inverse bind matrices agree, the rounding taken as uniform
  (`UNEVEN` = 1e-4) and how far the file's nodes stand from the bind
  pose. An uneven or sheared scale is still refused. Golden case
  `meshtools.import.skinned.scaled`; `bench/meshtools` gains
  `skin-check`, which poses imported skins against the files' own
  matrices.
- `shape` CLI: the mesh tools. `shape import <mesh> [-o out]` reports the
  parts, materials, UVs, bones, images and what was skipped;
  `shape repair`, `shape decimate` (`--ratio` or `--target`, `--lods N`
  writing `<stem>.lod<k>.<ext>`), `shape unwrap` and `shape process` (the
  chain under `--budget`) report what they did and the measures after, and
  write a plain GLB (OBJ, STL) with `-o`. `measure`, `preview` and
  `export` also take a mesh file (glb, gltf, obj, fbx; OBJ with
  `--unit-mm` and `--z-up`). The commands are now the `shape_cli`
  library, one module each. `build`'s report escapes control characters
  in names as serde_json does.
- `shape-mcp`: an MCP server on stdio (`guides/mcp.md`). Tools `build`,
  `preview` (PNG image content), `measure`, `export`, `import`, `repair`,
  `decimate`, `unwrap`, `process` and `run_typescript` (a TypeScript
  program run through the binding under bun; the program it records is
  replayed natively). Resources: the program JSON, the TypeScript API and
  the mesh tools guides. Paths are confined to `--root`; reads are capped
  by `--max-bytes`.
- Guides: `guides/program.md` (the program JSON and every operation),
  `guides/typescript.md` (the binding's API), `guides/mcp.md` (building
  and registering the server).
- Golden case `mcp.process.ball_defects` (the `process` chain's report
  and GLB on the committed `golden/meshes/ball_defects.obj`); no existing
  hash moved.

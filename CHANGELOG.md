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
- The gate runs on aarch64 macOS too (`macos-latest`): five targets.
- Imported skins are carried through the mesh tools (phase 5b part 2).
  - Repair keeps them: weights follow the vertices through welding,
    degenerate and floater removal, orientation and hole filling (a new
    centre vertex takes its rim's blend); `collapse_short_edges` blends.
  - `apply_transform` bakes a rigid transform into the vertices, rest
    frames and inverse bind matrices. A scaled or mirroring transform on a
    skinned part is `RepairError::Skinned`.
  - `join` merges skeletons by bone name. Bones of one name that
    disagree, or skinned and unskinned parts, are `RepairError::Skinned`
    naming them. `RepairError::Skinned`'s message now says what cannot be
    kept.
  - `decimate` and `lods` blend weights by the collapse and keep
    bone-weight borders with a penalty (0.5 % of the diagonal per unit of
    weight change). New field `Decimate::cross_bone_borders` (off) drops
    it. `DecimateError::Skinned` is no longer returned.
  - Unwrap leaves weights alone; `glb_plain` still refuses skins.
  - Golden cases `meshtools.skin.repair`, `meshtools.skin.decimate` and
    `meshtools.skin.join`; no existing hash moved. `bench/meshtools`
    gains `skin-decimate` (decimate to half, pose, compare); results in
    `docs/research.md` §11.5, previews in `docs/previews/phase5b/`.
- `shape-bool`: an exact self-crossing check and a self-union
  (`docs/research.md` §5.4, item 3).
  - `selfcheck` holds the exact triangle-pair tests, now shared with
    `shape-tools`' measure and repair.
  - `crossing_pairs` counts a solid's triangle pairs that cross within one
    shell.
  - `check_operands` refuses a self-crossing operand before any work
    (`BoolError::SelfIntersecting`). `boolean` reports that error instead
    of `OpenResult` when its result is open for that reason. It does not
    refuse up front by default: the kernel's own outputs can cross
    themselves by an ulp and still give closed booleans.
  - `self_union` untangles a closed surface that passes through itself,
    and joins overlapping shells, carrying names like `fuse`.
    `fuse` untangles self-crossing shells first.
  - `repair`'s fuse step untangles a single closed shell that crosses
    itself.
- `bench/booleans`: engines `og-shape-checked` and `og-shape-untangled`,
  and an `--engine` filter. Synthetic defects: 0 wrong, against og-geom's
  21.
- Golden cases `selfunion.*` (folded ball, noisy ball, overlapping
  copies, the check, repair's fuse). No existing hash moved.
- `shape-bevel` closes the configurations it refused as `Unsupported`
  (`docs/research.md` §7.4):
  - a gap with two or more unbevelled edges between bevelled ones (and a
    lone edge ending among three or more faces): the faces between the
    unbevelled edges are cut on a profile around the vertex and the hole
    takes an ADJ patch (Blender's Full Vertex Bevel);
  - a lone bevelled edge whose two faces meet again at its end (part of
    a rim): it tapers to a tip at the vertex;
  - pipe joints: where two of three or more bevelled edges are in line,
    the corner patch lies on the round running through;
  - `Fillet` and `Chamfer` gain `outer_miter`, `inner_miter` (`Miter::
    Sharp`, `Arc`, `Patch`; inner takes no `Patch`, refused as the new
    `BevelError::Invalid`), `spread` and `loop_slide`. Zero values keep
    the old results.
- `shape-bevel`: a bevel that did not fit because a face was cut into
  slivers (a cylinder's cap under part of its rim, a boolean's faces)
  now rebuilds that planar face from its outline, holes bridged, and a
  straight run of bevelled edges that a boolean left in short pieces is
  bevelled as one edge. Results that fitted before are unchanged.
- Golden cases `bevel.gaps`, `bevel.miters`, `bevel.pipe`,
  `bevel.slide` and `bevel.slivers`; no existing hash moved. Previews in
  `docs/previews/phase3/` (`cargo run -p shape-bevel --example phase3`).
- Guides: `guides/shape.md`, the authoring guide (plan parts first, names,
  placement with frames, joints, budgets, the review loop and a table of
  symptoms and fixes), and `guides/typescript.md` rewritten against the
  binding: each generator's own space, the facing rule for frame axes,
  `ring`/`grid`/`bbox` frames, profile transforms, hole, pocket and shell
  details, `taper` as a scale. Together about 6 200 tokens.
- Every ```ts and ```json sample in `guides/` is built by the binding's
  tests (`crates/shape-wasm/ts/test/guides.test.ts`); the gate's binding
  step runs it, or says `binding: NOT RUN` without bun.
- Phase 6 evaluation (`eval/phase6/`, `docs/research.md` §13): 20 sand3d
  dataset objects rebuilt by agents from a description and a picture,
  and again by translating the sand3d program; programs, previews and the
  runner (`eval/phase6/run.ts`) committed, datasets attributed.
- `shape-bool`: booleans on operands that only touch (faces on faces,
  vertices within an ulp of a face or an edge) close, or are refused
  saying why (`docs/research.md` §5.4, item 5; §13.6, items 1, 2, 5).
  When og-geom's arrangement comes out open, a second pass builds it
  again. It orders points on an edge exactly (expansion arithmetic),
  triangulates in an exact layout where the rounded outline is not
  simple, makes points at one position one point, and classifies faces
  lying on the other operand by orientation. A sliver no probe can
  resolve is kept or dropped to close the surface. Closed first-pass
  results are unchanged. Stacked lofts, a plate along a tube's vertex
  columns, a box through a torus's ring of vertices, a cup rim on its
  shelled wall and untilted elbows sharing a ring face now join closed.
- `shape-bool`: a boolean never returns a result that `measure` reports
  not closed, except a union that is non-manifold where the operands
  touch along an edge (two boxes sharing only an edge). Before, an edge
  used twice each way passed the result check, so `join` could return
  `closed: false` with no error. Other non-manifold results are refused
  as the new `BoolError::NotManifold`.
- `shape-bool`: `boolean_report` returns the result with a `BoolReport`:
  the edges where the operands touch, the voids left (a cut whose tool
  lies wholly inside keeps the void, which is the correct result), and
  whether the operands touch. `fuse` cuts an inward-facing shell from the
  rest instead of joining it, so later booleans keep a void.
  `csg_points_traced` returns where the operands lie in one plane or
  touch, and the result's edge counts.
- `shape-bool`: an open result is diagnosed before blame. The
  self-crossing check runs only then; operands that touch give the new
  `BoolError::OpenTouching`, which names up to six faces of each
  ("…where the operands touch along coplanar or tangent faces (target:
  …; tool: …)"). `OpenResult` no longer says an operand most likely
  passes through itself.
- Golden cases `touching.*` (11, of which three replay
  `golden/programs/touching_*.json`); no existing hash moved.
  `bench/booleans` gains a `touching` suite.
- Loft: sections with corners (polygons, sharp `rect`, `poly` corners)
  keep them as samples, and corner `i` runs to corner `i` of the next
  section, which starts at its corner nearest the previous start by
  angle; holes too. Smooth sections (circles, ellipses, rounded
  rectangles) are resampled as before.
- Loft: a Catmull-Rom curve whose z would run back past a section (uneven
  heights) runs z straight instead, so the solid stays between its end
  sections. Unchanged where z already rose.
- `shape-2d` cap fills are exact: a sliver between nearly collinear
  contour points no longer closes a cap twice. Polygon lofts with
  `samples` (and any profile with such points) came out open.
- `shape_model::loft`, used by the program's `loft`: a loft whose surface
  passes through itself is refused (`GenError::Invalid`), naming the
  sections it folds between.
- Interference: `shape_tools::interference` (`Interfering`: crossings,
  touching, inside, where), `Model::interference(pose)` and
  `program::report_json(model, poses)`, the measures plus an added
  `interference` field at rest and at each pose. `measures_json` is
  unchanged; the CLI, MCP and binding do not expose it yet.
- Golden cases `loft.corners`, `loft.folds` and
  `interference.washing_machine`; no existing hash moved.
- Program JSON, binding, CLI and MCP (`docs/research.md` §14): new
  generator ops `wire`, `body`, `knurl` and `gear` (the gear solid, with
  `bore` and `hub`); `dome` takes `height`; `loft` takes `curve`
  (`"auto"`, `"straight"`, `"catmullRom"`, `"spline"`); `revolve`,
  `sphere`, `dome` and `torus` take `axis` (`"+Y"`, `"+Z"`); `torus`
  takes `angle` (a part ring with `start`/`end` caps) and
  `tubeSegments`; a `scale` op (a part along its own axes); profiles
  `scale`, `polygon.flats` and a `poly` point's `bulge` (an arc edge).
  The binding adds the generators, `Shape.scale`, and `og.translate`,
  `og.rotate`, `og.offset`, `og.scale`.
- `segments` (a turn; 0 keeps the chord rule) on `cylinder`, `cone`,
  `tube`, `sphere`, `dome`, `torus`, `revolve` and `wire`. In `shape-gen`:
  `segments` on `CylinderDesc`, `SphereDesc`, `DomeDesc`, `Revolve`, `Wire`;
  `angle`, `segments`, `tube_segments` on `TorusDesc`; new `ConeDesc`,
  `TubeDesc` (`cone_desc`, `tube_desc`). In `shape-2d`: `Polygon::flats`,
  `PolyPoint::bulge`, `ProfileError::ArcCorner` and `Conflict`. Zero
  values keep every earlier result.
- **Breaking for programs:** a field an operation, profile, loft
  section, body view, frame or mimic does not take is now an error that
  names it and lists the fields taken (it was ignored). Every program in
  `eval/`, `golden/` and `guides/` already complied.
- Errors name where they happened: `ModelError::At` gives the op's
  index in the program (or the index it would be recorded at, live), its
  verb and its part (`op 3 fillet "lid": …`; readers say `(a reader,
  before op N)`). The binding appends the author's call site from the
  stack, read only when a call fails. New `ModelError::BadField` and
  `UnknownField`; field errors no longer repeat the op.
- Fixed: a generator whose `on` frame was refused had already added its
  part, so the model changed on an error and a replay diverged.
- Golden cases `surface.program.glb` and `surface.program.measure`; no
  existing hash moved.
- Guides: `typescript.md` and `program.md` cover the above;
  `shape.md` points bends at `wire`, hulls at `body`, outline arcs at
  `bulge`, budgets at `segments`. The pair is about 6 600 tokens.
  Run `M` (`eval/phase6/M/`, `docs/research.md` §14.4): five objects
  re-built with them.
- Merging streams L, M and N moved four of their own new golden cases,
  each re-checked: `touching.lofts.hexagon` and `surface.program.{glb,
  measure}` (N's loft corners line up a hexagon's and a rectangle's
  corners; N's interference field in the measures), and
  `touching.program.elbows_turned` (still refused; M's errors now name the
  operation). No hash that was on main before moved.
- Fixed (`docs/research.md` §13.6, item 12; §15.4): joining tapered
  extrudes left a zero-area triangle in a closed result. Cause: one
  operand's edge passes a rounding error off the other's (the lever knob's
  `band_7` corner edge off `band_6`'s top diagonal), so the edge through
  the face and the diagonal through the side are two crossings, distinct
  by the exact predicates, less than an ulp apart; with a third point of
  the cut they rounded onto one line. A boolean's output triangle whose
  corners are distinct positions on one line (exactly, `orient2d` in each
  axis plane) is now flipped into its neighbour across its long edge,
  which is split at the middle corner; no tolerance. The same runs when a
  model part is moved (`at`, `transform`, export), since rounding there
  can put a needle a rounding error wide onto a line. New
  `shape_bool::drop_slivers`. The phase 6 knob measures closed with no
  degenerate triangle (it had 2).
- Two golden hashes moved, each because one such triangle is gone:
  `touching.strap` (on the plate's top edge, z = 38.14 at x = 457.2) and
  `edit.holes.pocket` (from the countersink hole on, at the countersink's
  rim, z = 4 to 8). Checked: triangle counts, `closed`, volume and area
  are as before (to the last digit printed), and the degenerate count goes
  from 1 to 0.
- Fixed (`docs/research.md` §13.6, item 13): where a hole's or pocket's
  axis meets the solid is decided by `orient3d` of the axis with each
  triangle's edges, so an axis through an edge or a corner meets the
  triangles there; the floating point `u, v` test missed both triangles
  of a turned face's diagonal. Phase 6's error ("the axis from [≈0, ≈0,
  0] does not meet the solid") was correct: a through hole ran down the
  boss, and a pocket at the bottom face's centre had its axis inside the
  bore. **Breaking:** `EditError::Misses` is now `{ origin, direction,
  clearance }`, and its message names the direction into the part and the
  distance from the axis to the nearest edge (the bore's radius there).
- Golden cases `smallgaps.knob.bands`, `smallgaps.knob.program`,
  `smallgaps.hole.diagonal` and `smallgaps.pocket.through_hole`.
- `shape-bevel`: a vertex where two cones of faces touch (two boxes
  joined at a corner) is bevelled as two corners instead of
  `Unsupported`. A fan that does not close is `NotClosed`: it cannot
  happen on a closed solid (`docs/research.md` §7.5). Golden case
  `bevel.cones`; no existing hash moved.
- `shape-bevel`: a face corner of zero angle is `TooLarge` (no offset
  fits it) instead of `Unsupported`; an edge in line with its unbevelled
  neighbour across a curved face is bevelled, stepping across by half the
  offset as collinear edges on a plane do. A bevelled edge that is the
  only real edge at its vertex cannot happen and is no longer a case.
  Golden case `bevel.inline`; no existing hash moved.
- Fixed (`shape-bevel`): chamfers of several segments on regular prisms
  and pyramids, and chamfers at the corners of a bored block, were
  refused as `TooLarge` although they fit: a face was clipped into a
  triangle of zero area. Such a face is now clipped again avoiding it;
  results that were accepted are unchanged. Golden case
  `bevel.chamfers`; no existing hash moved.
- `shape-bevel` tests: pipes through curved faces, corner patches of 7
  to 12 sides, loop slide around a cone, and arc and patch miters with
  chamfers of 2 to 5 segments, on straight and curved outlines
  (`docs/research.md` §7.5). Golden case `bevel.untested`; no existing
  hash moved.
- Previews of the §7.5 cases in `docs/previews/phase3/` (two cones, the
  in-line ridge, a bored block's chamfer, a seven-sided apex, a pipe
  through curved faces, loop slide around a cone, a patch miter on a
  curved outline).
- `shape-bevel`: bevels wider than the segments of their faces fit
  instead of `TooLarge` (`docs/research.md` §7.6): the part revolve's
  bore at r 1.5, a hole's rims from `shape-edit` (whose rims come in
  pieces down to 1e-15 long: now the same volume as a plain plate's
  hole), a slice of a cylinder chamfered across its cut faces, and a
  twisted prism's top edges at r 5 (its faces cut exactly where the
  offset line crosses their rows). Tried only where the old layout gave
  `TooLarge`, so every result it gave is unchanged. In the bevel scan,
  `TooLarge` fell from 1 233 to 446 of 17 516 bevels; nothing that fitted
  fails, and no new result has crossing triangles.
- Golden hash `bevel.slivers` moved: its part revolve filleted 1.5 used to
  hash the `TooLarge` error and now hashes the bevel. No other hash
  moved. Golden case `bevel.wide` added.
- `crates/shape-bevel/examples/scan.rs`: the bevel scan of §7.5 and §7.6,
  one line per bevel, so two runs compare line by line.
- Previews of the §7.6 cases in `docs/previews/phase3/` (`wide_bore`,
  `wide_hole_rim`, `wide_cylinder_slice`, `wide_twisted_top`).
- Program and binding profiles `superellipse`, `spline` (closed, smooth;
  new `shape_2d::spline`), `path` (a start point, then points,
  three-point `arc`s, `tangentArc`s and `spline` pieces), `stroke`,
  `hull`, 2D `mirror` and `text` (`docs/research.md` §16). `segments`
  below what a `circle`, `ellipse` or `superellipse` can take is refused
  instead of ignored.
- Fonts: a `text` profile names a font file the caller supplies; no
  font ships. `shape` and the MCP tools read it relative to the program
  (MCP within the workspace); the binding takes bytes in
  `og.model({ fonts })` and `og.replay(program, { fonts })`;
  `program::run_with`, `fonts_named`, `Session::font`. A file that is not
  a font is `ProfileError::Font` (was `Empty`).
- `sweep` and `wire` paths take pieces (arcs, tangent arcs, splines,
  flattened by the curve rule; `shape_gen::path3`) or a whole
  `{"helix": …}` (`og.helix`). On a curved path `wire`'s `fillet` bends
  the corners between pieces; `fillets` is refused there.
- `extrude` `until: {part, face}`: up to a face's plane; a tilted face
  cuts the top, still named `top`.
- `merge` (no boolean) and `delete` (a node and everything under it,
  with the joints hanging what goes) in the program, the binding and
  `Model`.
- `cut`, `join`, `intersect` answer `{voids, touchingEdges, touching}`
  (the binding returns it), take `check` to refuse an operand that
  passes through itself first, and are listed with their reports as
  `booleans` in `shape build` and MCP `build`. `Model::cut`, `join` and
  `intersect` return `BoolReport` (was `()`); `Model::combine`,
  `Model::booleans`; `ModelError::Deleted`.
- MCP serves `guides/shape.md` as `og-shape://guides/shape.md` (§13.6
  item 15).
- Golden cases `rest.program.glb`, `rest.program.measure`,
  `rest.program.booleans`; no existing hash moved.
- The preview blends transparent materials (`docs/research.md` §20):
  opaque parts are drawn first; then the triangles of parts with
  `opacity` below 1, back to front by the sum of their corners' depths
  (ties to the earlier part, then the earlier triangle), blended over the
  pixel and tested against the opaque depth.
  `opacity` 0 is not drawn. The shadow under a transparent part is lighter
  by its opacity. The washing machine's glass is now see-through.
- The preview shades smooth only across facets of one surface that meet
  at less than 35° (`shape_tools::preview::CREASE_DEGREES`): a hexagonal
  loft shows its facets, a 12-sided or finer turn stays round. The GLB's
  normals are unchanged.
- Golden hashes moved, all three preview lines and nothing else:
  `tools.preview` and `tools.preview.png` (the wheel's material has
  opacity 0.8, now blended; the twisted riser's facets past 35° now
  flat), `model.washing_machine.png` (the glass at opacity 0.4 is
  blended, and facets of one surface meeting at 35° or more are flat). `model.joints.preview` held:
  its boxes and cylinders shade the same under the crease rule, and it
  has no transparent part. New cases: `tools.preview.facets`,
  `model.joints.obj`, `model.joints.stl`, `model.joints.copies`,
  `model.washing_machine.posed`.
- Previews regenerated under `docs/previews/`: `phase4/washing_machine.png`;
  in `phase1/` the binocular, keyboard and gear transfer; in `phase3/`
  ten bevel cases; `phase5b/bend.png` and `wave.png`; `gen2/`. The
  others render the same bytes. `phase1/bevel_to_spur_transfer.png` was
  already stale on `main`. `phase5/` was not regenerated (it needs the
  mesh-tools bench's external inputs).
- `copy` and `mirror` bring the joints of what they copy, named with a
  free suffix (`hinge.001`), at the same pose; a copied part hangs on its
  parent's copy when the parent was copied too, a follower follows its
  leader's copy. A mirrored joint moves as the image of the original's
  motion: pivot mirrored (z the cross product of its mirrored x and y),
  a slide's axis mirrored, a turn's axis mirrored and negated; ranges,
  values and mimics kept, so the same value poses the image.
- Frames are read at rest only: `face`, `bbox`, `edge`, `snap`,
  `placement`, `local` and `anchor` of a part that a posed joint moves are
  refused with `ModelError::Posed` (the read, the part, the joint and its
  value). Parts the pose does not move read as before.
- `Model::obj` and `Model::stl` write the pose when any joint has a value
  other than 0 (the header ends `, posed`), at rest otherwise; parts no
  joint moves keep their rest positions bit for bit. GLB is unchanged:
  rest meshes, the pose in the joint nodes. `shape_tools::obj_at`,
  `stl_at` (`Placement::Rest` or `Pose`) and `posed`; `obj` and `stl`
  still write the rest. `Model::mesh_placement`, `Model::posed`,
  `Model::pose_values`.
- `shape export` prints its report; for a program it says `geometry`:
  `"posed"` or `"rest"` (the MCP `export` tool's report gains the field).
- Measures at the pose: `Model::measure_posed`, `Model::seating`
  (per joint, at rest: the pivot's distance to the parent's and the
  child's surfaces, and how far the parent has moved and turned off the
  pivot since the joint was declared), `program::posed_json`, the
  `measure` reader's `posed: true`, `shape measure --posed` and the
  binding's `measure({ posed: true })`: `measures_json`'s fields at the
  pose, with `pose`, `posed`, `joints` and `interference` at the pose.
  `measures_json` and `report_json` are unchanged.
- `Interfering::as_built`: whether a pair was compared where built (both
  parts moved by the very same motion) or posed, where faces flush under
  different joints can read as crossing by rounding.
- Guides: `shell`'s `open` directions are the part's own axes, as built;
  copies and mirrors carry joints; reads before posing; the posed
  measure.
- UVs in the data model (`docs/research.md` §17): `Solid::uvs`, one
  texture coordinate per triangle corner, f64 inside and f32 on output,
  optional. Canonical order and the output hash include them only when
  present, so no golden hash moved. `Solid::push_triangle_uv` and
  `Solid::reverse_triangle` keep them with their triangles.
- Import keeps UVs and textures: glTF `TEXCOORD_0` (v flipped to the
  kernel's v-up), OBJ `vt` and FBX's first UV layer now go into
  `Solid::uvs` (`ImportedPart::uvs` is gone), and each part's
  base-colour texture is kept as `ImportedPart::map` (`Texture`: the
  file's bytes and MIME type; glTF `baseColorTexture`, OBJ `map_Kd`, FBX
  `DiffuseColor`). `Imported::skipped` names a part with UVs on only some
  faces, a texture on a second UV set, a `KHR_texture_transform`, and the
  material maps that are not kept (normal, occlusion, emissive,
  metallic-roughness).
- Export writes them: `glb` writes a solid's UVs as `TEXCOORD_0` and
  `Material::map` as an embedded PNG or JPEG; `glb_plain` writes the
  solid's UVs and `PlainGlb::map` (`PlainGlb::uvs` is gone). A map on a
  solid without UVs, or not PNG or JPEG, is `ExportError::BadTexture` /
  `PlainGlbError::BadTexture`.
- `unwrap` takes `&mut Solid` and sets its `uvs`; `measure` reads the
  solid's UVs (`Measure::uvs` is gone), so coverage and overlap are
  measured on carried UVs too.
- Repair, `apply_transform`, `join` and decimation carry UVs: welds keep
  each triangle's UVs, turned triangles turn theirs, hole fills take
  their rim's (`RepairReport::uv_seam_fills` counts fills across a UV
  seam; `close_holes` returns that count too), parts without UVs joined
  to parts with them get (0, 0). Decimation keeps UV seams as feature
  lines and refuses a collapse that cannot match a corner's UV side or
  would turn a UV triangle over. Measured on Khronos's Duck, Avocado,
  BoxTextured and DamagedHelmet decimated to half: mean UV error at most
  a third of a texel, p99 at most 2.2 texels; the texture reaches
  three.js.
- Booleans and `fuse` interpolate UVs inside each source face, so cut
  faces keep their mapping; `BoolReport::faces_without_uvs` counts faces
  from an operand without UVs when the other has them.
- `shape_gen::solids::textured(&Generator)`: extrude, revolve, box,
  cylinder, cone, tube, sphere, torus and dome with UVs in millimetres
  (caps planar; extrude walls by contour arc length and height; turned
  walls by radius × angle and profile arc length; unmirrored). The
  generators themselves are unchanged.
- `shape-bevel`: `fillet`, `chamfer` and `soften` refuse a solid with
  UVs (`BevelError::HasUvs`) until bevels interpolate them.
- CLI and MCP: `repair`, `decimate` and `export` of a mesh write its UVs
  and texture; `process` keeps a textured mesh's UVs and texture and
  skips the unwrap (`"unwrap": null`) unless `--unwrap` (MCP
  `fresh_uvs`) asks for new UVs. The import report shows `"map"` on a
  part that has a texture.
- Golden cases added: `uvs.generators`, `uvs.chain`, `uvs.boolean`,
  `uvs.glb`. `bench/meshtools` gains `uv-check`; three.js renders of the
  full and halved Duck and DamagedHelmet in `docs/previews/uv/`.
- `shape-mcp`: long calls report progress and can be cancelled. A
  request with `_meta.progressToken` gets `notifications/progress` before
  each step of `process` (import, repair, decimate, unwrap, measure,
  export) and of `run_typescript`; `notifications/cancelled` stops a call
  before its next step (the kernel is never interrupted mid-step), stops
  bun while a TypeScript program runs, and drops a request still queued.
  A cancelled request is not answered. A `ping` is answered while a call
  runs. The server reads its input on a thread of its own; every kernel
  call stays on one thread. `shape_cli::process::process_steps` and
  `STEPS` give the chain's steps to a front end.
- `shape-mcp`: `guides/shape.md` (how to model an object) is served as
  `og-shape://guides/shape.md`, listed first, and the server's
  instructions say to read it first (research §13.6 item 15).
- `shape-mcp`: workspace I/O checks the file it opened, not only the path
  before it: a read confirms the opened file is the one at its real path
  under the root (device and inode on Unix), and a write creates a new
  file beside the target, confirms it, and renames it over the target.
  A directory or link swapped in between the check and the I/O is now
  refused, and a link at a write's target is replaced, not written
  through. A Windows test of `\\?\` paths runs in CI.
- The TypeScript binding (`crates/shape-wasm/ts/src/index.ts`) has no
  TypeScript-only syntax left (parameter properties became fields), so
  node 22.18 or later imports it as it is. Checked: node, an esbuild
  bundle, and headless Chromium each give the washing machine's golden
  GLB and PNG (`tools/check_consumers.ts`; the gate's binding step runs
  the node check when node 22.18+ is installed, and CI installs node
  22.23.4 for it).
- `crates/shape-wasm/ts/tools/check_render.ts`: three.js's GLTFLoader and
  WebGLRenderer in headless Chromium on the washing machine and the
  skinned figure; joints, re-posing and skinning are measured against the
  kernel, and screenshots go to `docs/previews/validate/`.
  `cargo run -p shape-skin --example figure_poses` writes the figure and
  the kernel's posed vertices for it.
- `crates/shape-mcp/ts/client/client.ts`: the official MCP TypeScript SDK
  client drives every tool and resource of the release server, including
  an FBX, a `.gltf` with an external buffer and image, progress and
  cancellation. `cargo run -p shape-mcp --example fixtures` writes the FBX.
- `crates/shape-wasm/ts/tools/scale.ts`: times and peak memory of the mesh
  chain on a mesh of a million triangles (research §19.6).
- `measure`'s self-intersection count welds vertices at equal positions
  first. A mesh read from a file has a vertex split wherever its UVs or
  normals differ, and the triangles either side of such a seam were
  counted as crossing: a processed GLB of 1 000 triangles reported 789
  pairs for a part whose welded whole has none (`docs/research.md`
  §19.6). Golden line `meshtools.measure.json` moved for this, with the
  owner's leave: its raw mesh has split vertices too.
- Golden line `rest.program.booleans` moved when streams R and S met: it
  hashes the debug text of each boolean's report, which gained
  `faces_without_uvs`. The geometry (`rest.program.glb`) is unchanged.

- `shape-bool`: booleans whose operands lie a few ulps apart without being
  exactly one surface (elbows of tori turned about their axis, the tool
  brought to the target's frame through the world) join closed instead of
  `OpenTouching`. When both passes leave a result open, or closed with
  edges used four times, the tool's vertices within 2⁻⁴⁰ of the operands'
  size of a target vertex are snapped onto it (`docs/design.md` §6) and
  the passes run again; the snapped result is taken only if cleaner
  (`CsgOutput::snapped`). A union that the second pass left empty is no
  longer taken (it answered `Empty`). Every result that was closed is
  unchanged (`docs/research.md` §18.1).
- `shape-bool`: a countersink whose narrow end is the bore's own polygon,
  cut after the bore, was refused as "a position that is not finite" (a
  crossing computed as 0/0); it now closes in either order
  (`docs/research.md` §18.4).
- `shape-bool`: `self_union` (and so `fuse`, repair's fuse step and the
  bench's untangled engine) classifies regions with a hierarchical
  winding number with a rigorous bound, falling back to the direct sum
  where the bound leaves a decision in doubt: the same output, untangling
  5–9× faster on heavy noise (`docs/research.md` §18.3, §18.7).
- `measure` (the program's measures) counts a triangle as degenerate only
  when its corners are exactly in line, as the mesh tools' measure does;
  a needle whose f64 cross product rounds to zero is no longer counted
  (`docs/research.md` §18.6).
- Golden: `touching.program.elbows_turned` moved, from hashing the
  `OpenTouching` refusal to the closed join. New cases
  `touching.elbows.turned` and `touching.countersink.ring`. No other line
  moved.
- `bench/booleans --crossings`: counts the pairs of triangles of each
  og-shape output that cross within one shell, outside the timing.
- `shape-bevel` never returns a result that crosses itself
  (`docs/research.md` §7.7). An exact crossing check on every result
  (always on; 40–60 % of a bevel's time) turns a crossing layout into
  `TooLarge` at the nearest bevelled vertex, so the straight-run retry and
  the wide bevel get their try. Of the 18 such bevels in the scan (a
  boolean's cut edges in pieces beside a bore, a notch, a join and a cross
  bar, and two part revolves), 10 now fit and 8 are refused because their
  selection holds a lone rim piece (below).
- `shape-bevel` checks its input: a solid that passes through itself is
  welded where a boolean left points 1e-15 apart (the post joined to a
  plate: its 440 results in the scan crossed; 72 now fit, the other 368
  select a lone rim piece), and otherwise refused
  with the new `BevelError::SelfCrossing(at)`. An edge with more than two
  faces (two boxes joined along an edge) is the new
  `BevelError::NotManifold(a, b)`, whose message says to bevel the parts
  before joining them; it was `NotClosed`.
- `shape-bevel` no longer tilts old faces off their planes. Where the
  faces at a bevelled vertex are not one plane (a twisted prism's rows, a
  cone's fan at its apex), moving a triangle's corner tilted the whole
  triangle, far from the bevel: the twisted prism's `fillet(top)` at
  r 0.5 added 11 mm³. Such bands are now cut exactly as wide bands are, so
  the twisted top removes 15–19 r² at every radius with its sides on their
  old triangles; what cannot be cut is
  `BevelError::Unsupported(at, "a bevel along a face whose triangles at
  this vertex are not in one plane …")` (the twisted prism's side edges,
  the apex revolve's caps, §7.5's hand-built ridge).
- `shape-bevel` refuses a bevelled edge that tapers to a tip at both ends
  (one piece of a cylinder's rim, or of a twisted prism's side edge,
  selected alone): it came back unbevelled. The error says to select the
  whole crease. In the scan, 10 160 results that silently left such a
  piece unbevelled are now this error.
- Golden: `bevel.slivers` moved (its twisted prism's fillet of every edge
  removed 14.7 mm³ where the tapered one removes 101; now the refusal),
  `bevel.inline` moved (the ridge's result lifted its flat face by 0.11 of
  the offset; now the refusal). No other hash moved. Golden case
  `bevel.sound` added.
- The bevel scan prints each result's output hash, so two runs show which
  results changed their bytes. Previews `twisted_top`,
  `twisted_top_corner` and `bore_bottom_chamfer` added in
  `docs/previews/phase3/`; `twisted` and `inline_ridge` removed (their
  results are refused now).
- `repair`'s self-intersection count (`check_intersections`) welds equal
  positions first, as `measure` does. No golden line moved.
- `hole`: a countersink's cone is turned half a segment about the hole's
  axis, so its generators run between the bore's columns and its rim no
  longer crosses itself by an ulp (53 of 96 countersinks in a sweep of
  positions, diameters, angles and depths crossed themselves; none do
  now). Golden: `edit.holes.pocket` moved for this; no other line moved
  (`docs/research.md` §23).
- Booleans: a region of one operand that meets the other's surface
  nowhere is classified exactly (the signed crossings of a segment from
  one of its corners, by `orient3d`) instead of by probes a millionth of
  the operands' size off its face. Slabs stacked two ulps apart lost one
  slab in a union, closed and with no error; they now join whole.
- Booleans: a closed result left in several shells is built again with the
  tool's vertices snapped onto the target's within 2⁻⁴⁰ of the operands'
  size, and taken if it is closed and in fewer shells (solids stacked a
  few ulps apart join as one shell). No golden line moved by these two.
- `shape_bool::uncross_output` (with `Uncrossed`): repairs a boolean's
  closed output that crosses itself by a rounding error, by merging a
  constructed point into a neighbour within 2⁻⁴⁰ of the operands' size or
  splitting a needle across its long edge, each kept only if exact
  predicates find fewer crossing pairs there and every edge still used
  once each way. An output that does not cross itself is returned byte
  for byte. Not yet on in `boolean` (seven golden lines would move); the
  bench's `--uncross` applies it. Golden cases `touching.gap.slabs` and
  `touching.uncross` added.
- The winding tree's remainder bound is proved (`|∂²K| ≤ 6/|y|⁴`, sharp)
  and used as proved (3 S/ρ⁴ for 4 S/ρ⁴). Decisions are the direct sum's,
  so no output changed.
- `shape-bevel` bevels a lone piece of a longer crease (one segment of a
  rim, a piece of a boolean's cut edge, a straight edge meeting arcs
  tangentially), which since stream W was refused as "tapers to a tip at
  both ends": the bevel now tapers to a tip at each end inside the edge,
  its width rising at 45° to the offset (a piece shorter than twice the
  offset peaks at half its length in the middle), its new points in the
  planes of the edge's own triangles, so no old face tilts
  (`docs/research.md` §7.8). Where that taper does not fit, the error is
  `BevelError::LonePiece(a, b, names)`: it names the piece and its names
  and asks for the whole crease. In the scan, 10 358 of the 10 812 such
  refusals are now results and 46 are this error; the rest fail on the
  selection's other edges, as they do without the piece.
- `shape-bevel` cuts a corner on a face whose triangles there are not one
  plane (a loft's or a twisted prism's rows, a cone's fan at its apex) on
  that face's folds, each fold taken as a real edge and each triangle cut
  in its own plane, where the classic layout would tilt it and the cut
  bands of §7.6 do not reach. Run A's toaster, `fillet(14, top)` on a
  loft, refused since stream W, builds again; the twisted prism's side
  edges and every edge, the apex revolve's start cap and §7.5's ridge
  build. 194 of the scan's 216 refusals of this kind are now results.
- `shape-bevel`'s final crossing check tests only pairs with a triangle
  the bevel made or moved, found by a tree of boxes: the box's twelve
  edges at 32 segments 42 → 16–18 ms for the check. Every result and
  error is the same.
- Golden: `bevel.sound` moved (its last case, one piece of a cylinder's
  rim, hashed the tip refusal; now the tapered result); `bevel.inline`
  moved (the ridge's fillet and chamfer hashed the tilt refusal; now
  results, every old face in its plane; the needle's refusal unchanged);
  `bevel.slivers` moved (the twisted prism's fillet of every edge hashed
  the tilt refusal; now a result removing 108.5 mm³, the tapered prism
  101; its other six results unchanged). Golden cases `bevel.tips` and
  `bevel.folds` added. Previews `lone_rim_piece`, `lone_straight_edge`,
  `loft_top_folds` and `twisted_all` added in `docs/previews/phase3/`.
- UVs everywhere a program can reach (stream Y, `docs/research.md` §21):
  - The program asks for UVs: the `uvs` operation (every part made after
    it; refused once a part without UVs exists) and `uvs` on every
    generator; `og.model({ uvs: true })` and `uvs?` on generators in the
    binding; `--uvs` on `shape build`, `export`, `measure`, `preview`;
    `uvs` on the MCP `build`, `preview`, `measure`, `export` tools. UVs
    stay opt-in: nothing changes without them.
  - A material names a base-colour image (`map`, PNG or JPEG) the caller
    supplies, and `mapSize`, the units it spans. The CLI and MCP read it
    relative to the program (as fonts); the binding takes
    `og.model({ textures })`; `Session::texture`, `textures_named`,
    `run_files`. GLB export embeds it and writes the UVs divided by
    `mapSize`. A map on a part without UVs is refused.
  - `textured` builds `loft`, `sweep`, `thread`, `wire`, `body`, `knurl`,
    the gear solid and `wedge` with UVs (seams in `shape_gen::solids`'s
    docs).
  - `fillet`, `chamfer` and `soften` carry UVs: untouched faces keep
    theirs, trimmed faces are re-fitted, new faces take the map of the
    old face they face most. `BevelError::HasUvs` is gone.
  - `split`, `hole`, `pocket`, `shell` build their tools with UVs on a
    solid that has them; `dent` and skin refinement interpolate them;
    `drop_slivers` interpolates them (it left them on the wrong
    triangles). A cut's tool UVs are mirrored so the cavity reads as the
    tool's outside did. `seat` with a part without UVs is
    `EditError::NoUvs`; the model's `merge` of a part with UVs and one
    without is refused. A `mirror` copy's UVs read mirrored.
  - Booleans answer `facesWithoutUvs` (program answer, `shape build`,
    MCP, `BoolReport` in the binding). `measure` adds `uv` per part with
    UVs and fills `uvCoverage` and `uvOverlap`.
  - Golden: `uvs.generators2`, `uvs.edits`, `uvs.program` added
    (`golden/programs/uv_plate.json`, `checker.png`); no line moved.
    `tools/check_uvs.ts` loads the plate in three.js; screenshots in
    `docs/previews/uv/`.
- MCP: `preview`, `measure` and `export` take `pose: {joint: value}`
  (radians, or mm for a slide), posing a program's joints for that call
  only; `measure` takes `posed` (implied by `pose`) and answers the posed
  measures with each joint's seating and the interference. `preview`
  refuses an elevation of 0 or below as out of range, where the kernel
  called it "not finite". The `run_typescript` description says the
  `path` file must exist and that `console.log` comes back as `logs`.
- Guides (run `Z`, research §22): 3D paths for `sweep` and `wire` are
  the list itself, not `og.path`; a swept end on an arc tilts by half a
  segment (lead in with a straight piece); `body`'s `plan` is the full
  width, an end closes only where width and height both reach 0, smooth
  lines can overshoot; what `touching`, the posed `interference` fields
  and its box mean; `grid`'s frame order; `og.frame`'s x with `up`;
  `snap`; `bulge` in mm; a flat on a polygon's +x; a hollow or square
  bent bar as a `sweep`; a cylinder along +X; posing through MCP.
- `eval/phase6`: run `Z` (`Z/Z1`, `Z/Z2`); `mcp_run.ts` takes `--run`
  and `--jobs` and writes each failed build's whole error to
  `errors.log`; `references.json` records each reference's OpenUSD path
  and SHA-256; `Z/distance.ts` measures a symmetric surface distance
  between a built program and its reference mesh (also written for runs
  `B` and `U`).
- `boolean` (cut, join, intersect) repairs a result that crosses itself by
  a rounding error (`shape_bool::uncross_output`, `docs/research.md`
  §23.2): constructed points a rounding error apart are merged and needles
  split, each change kept only where exact predicates find fewer crossing
  pairs. A result that does not cross itself is returned as built. Golden
  lines moved, with the owner's leave, each from an output that crossed
  itself to one that does not, the volume equal to 1e-13:
  `smallgaps.hole.diagonal`, `smallgaps.knob.program`,
  `touching.countersink.ring`, `touching.lofts.cut`, `touching.lofts.join`,
  `touching.program.elbows`, `touching.torus.ring`.
- `sweep` and `wire` along a path that ends in a curve piece (an arc, a
  tangent arc, a spline, the helix) end square to the curve's tangent
  there, not to the first or last chord: the caps sit at the path's end
  points with the path's direction as their normal (`docs/research.md`
  §24). A plain point keeps the chord. `shape_gen::path3::Flat` carries
  `tangents`; `paths::Sweep` and `wire::Wire` take `tangents` (zero: the
  chord); `paths::rmf_ends`, `paths::helix_tangents`. `thread` ends
  square to its helix too. Golden lines moved: `rest.program.glb`,
  `rest.program.measure`, `solid.thread`, `uvs.generators2`; new
  `gen2.sweep.ends`. The guides' lead-in workaround is gone.
- `shape-bevel` bevels a run of edges along a smooth wall whose rows are
  shallower than the offset (`docs/research.md` §7.9): run Z2's toaster,
  `fillet(10, top)` on a spline loft of rounded rectangles (8 rows a
  span, the top row 6.9 deep), was "a bevel along a face whose triangles
  at this vertex are not in one plane … not supported" and now builds,
  as do the same fillet at 14 and 20, `chamfer`, a straight loft, the
  bottom rim, and a loft of a circle to rounded rectangles. Each gap
  point is found on the wall a fillet's offset below the run, the wall is
  cut across its rows between them and the band above the cut is
  replaced by the strip; what is left of the wall keeps its triangles'
  planes. A plain multi-row wall (a loft of one section twice), which
  was `TooLarge`, now removes what its single-row twin does to 1e-9.
  Where the wall is too short for the bevel, the error is `TooLarge` at
  the vertex. New golden line `bevel.rows`; no other line moved.
- `bench/booleans` repairs the og-shape engines' outputs by default, as
  `boolean` does; `--no-uncross` replaces `--uncross` and leaves them as
  built. `--hashes` with `--crossings` writes each boolean's crossing
  pairs after its hash (`docs/research.md` §25.2).
- Tests (research §25): a counterbore whose floor crossed itself by a
  rounding error (now repaired by `boolean`), and two prisms stacked on a
  tilted ring triangulated two ways or with midpoints. No golden line
  moved.
- `helix` has the chord rule's segments for every turn. It had one
  turn's count in all, so a thread of 7.1 turns spanned 107° a segment
  (`docs/research.md` §24.5). Golden lines moved, with the owner's
  leave, each a solid swept along a helix: `gen2.sweep.ends`,
  `rest.program.glb`, `rest.program.measure`, `solid.sweep.helix`,
  `solid.thread`, `uvs.generators2`.
- A part moved into the world (`at`, `on`, a turn, a unit other than
  millimetres) or mirrored no longer crosses itself where the rounding
  folded a needle between two points a rounding error apart. Where the
  moved part has an edge or a triangle that thin and crosses itself, the
  boolean's repair runs on it (`docs/research.md` §26). The mouse's shell
  in `eval/phase6/Z/Z1/sim2_mouse_and_pad` went from 8 crossing pairs to
  none, and §25.3's cup from 5 to none. Golden: `touching.program.rim`
  moved (the cup: two needle triangles fewer, the same volume and names);
  new `model.placed.slot`. A test builds every `eval/phase6/Z/Z1` program
  and checks every part closed with no crossing pair. Over every
  committed program, parts crossing themselves went from 7 to 2 (the
  octagonal cup's `shell`, §25.4).
- `shell` no longer cuts with an inner solid that folds through itself.
  Where a face is narrower than the wall (a chamfer, fillet or rounded
  corner), the inner solid's offset folds over, and the cut kept the
  fold: an octagonal cup chamfered 10 and hollowed 20 thick
  (`eval/phase6/B/obj1_octagonal_cup`) had a lip 1.7 high crossing its
  walls in 44 pairs, and 750 mm³ too much. The inner solid's outside
  (`self_union`) is cut now, when that result is closed; otherwise the
  folded one, as before. Swept over 7 220 shells: 950 crossed themselves
  before, 76 after (`docs/research.md` §27); none that built before is
  refused. No golden line moved.
- `boolean` builds faces of the operands that lie within a rounding error
  of each other, without being one plane, as one surface where cutting
  them by each other leaves the result open, not manifold, or crossing
  itself: such pairs of triangles are left uncut, and the result is taken
  only if it is closed, manifold and crosses itself less
  (`shape_bool::csg_points_repaired`, `docs/research.md` §27). Two
  prisms on a tilted ring triangulated two ways now join and cut (both
  were refused); the cut of a tilted ring with midpoints no longer
  crosses itself (259 pairs). `bench/booleans`: verdicts and all 921
  hashes unchanged.
- `BoolReport::crossing_pairs`, shown as `crossingPairs` in a boolean's
  answer, `shape build`, the MCP `build` tool and the binding's
  `BoolReport`: pairs of the result's triangles that still cross after
  the repair, a measurement (`docs/research.md` §27). Edits report it
  too: `shape_edit::EditReport` from `hole_report`, `pocket_report`,
  `seat_report`, `shell_report` and `dent_report`. Golden line
  `rest.program.booleans` moved: it hashes the report's text. A result
  that still crosses itself is not refused (`BoolError::CrossingResult`
  stays off): 76 swept shells still do.
- Test: every program of the evaluation's run B builds natively, every
  part closed and none crossing itself (`shape-model`'s `phase6_b`).
  Examples `shell_sweep` and `edit_sweep` (`shape-edit`).
- `seat`, `hole`, `pocket`, `shell` and `dent` answer `crossingPairs` in
  a program and return `{ crossingPairs }` from the binding, as a boolean
  does; `Model`'s five methods return `shape_edit::EditReport`. A `shell`
  whose inner wall folds through itself comes back with the count, never
  silently.
- The importer says when it reads past vertex colours: glTF `COLOR_0`
  and OBJ `v x y z r g b` are listed in `skipped` (they were dropped
  silently; TripoSR, InstantMesh and Unique3D outputs carry their only
  colour there, `docs/research.md` §28).
- `repair`'s fuse step keeps the file's UVs on every vertex it keeps:
  a corner on an input vertex takes that vertex's own UV, not the
  boolean's interpolation, which rounded differently in each triangle
  and made almost every edge a UV seam (on a 150 000-triangle Hunyuan3D
  mesh, 31 611 seam edges for 6 237; decimation keeping the UVs then
  stopped at 31 304 triangles for a budget of 2 000). No golden line
  moved.
- `shape process --unwrap` drops the file's UVs before decimating, so
  their seams no longer hold the decimation above the budget.
- `--unwrap` (`shape process`) and `--posed` (`shape measure`) took the
  next argument as their value: `process m.glb --unwrap -o out.glb`
  wrote nothing, and `measure p.json --posed --intersections` counted
  no intersections.
- `shape decimate --lods N` (and the MCP tool's `lods`): level k keeps
  `ratio`^k of the triangles, or the target and then half the level
  before. Every level had the same count.
- `shape preview` of a mesh file welds its parts by exact position
  first: the vertices a file splits at UV seams were drawn as edges and
  shaded as creases.
- `bench/generated`: 54 meshes from seven image-to-3D and text-to-3D
  generators (a manifest and a fetch script, nothing committed), the
  game chain at three budgets, LODs and booleans on them
  (`docs/research.md` §28). `crates/shape-wasm/ts/tools/render_glb.ts`
  renders mesh files side by side in three.js with all their maps.
- `guides/meshtools.md`: a section on generated meshes (scale and
  pivot, up axis, the UV-island floor of decimation, fills across seams,
  what is not kept).
- `shape repair` and `shape process` (and the MCP tools) report
  `uv_seam_fills`: hole fills that cross a UV seam and stretch the
  texture. `guides/meshtools.md` told authors to read it, and the report
  left it out. Golden line `mcp.process.ball_defects` moved for the new
  field (its value there is 0), with the owner's leave.
- Materials, maps and vertex colours through the mesh chain (stream AH,
  `docs/research.md` §29):
  - `shape_mesh::Solid::colors`: vertex colours, linear RGBA per triangle
    corner, beside `uvs`; present only when the input has them, so no
    hash of a solid without them moved. `Solid::set_colors`,
    `Solid::color_at`, `shape_mesh::WHITE`.
  - Import reads glTF `COLOR_0` (float or normalised integer, RGB or
    RGBA) and OBJ `v x y z r g b` (sRGB, made linear) into it; they were
    only listed in `skipped` before.
  - `ImportedPart::material` is now a `shape_tools::Material` (was the
    material's name; `ImportedPart::map` is now `material.map`): factors
    as the file means them (glTF: base colour, metallic, roughness,
    emissive with `KHR_materials_emissive_strength`, alpha mode and
    cutoff, double-sided; MTL: `Kd`, `d`/`Tr`, `Ke`, `Pr`, `Pm`; FBX:
    diffuse, emissive, opacity) and every map: `map`, `metalness_map`,
    `normal_map`, `emissive_map`, `ao_map`. `ImportedPart::defaults`
    lists the glTF factors the file leaves unset (an unset
    `metallicFactor` is 1, as glTF says).
  - `Material` gains `color_space` (`ColorSpace::Linear` writes glTF's
    factors unchanged), `emissive_intensity`, `transparent`,
    `alpha_test`, `double_sided`, `metalness_map`, `normal_map`,
    `normal_scale`, `emissive_map`, `ao_map`, `ao_map_intensity`; `glb`
    and `glb_plain` (new `PlainGlb::material`) write them, and
    `COLOR_0`. A zero-value material writes the JSON it always did.
  - Repair, join, the fuse step, decimation, booleans (`shape_bool`) and
    unwrap carry the colours as they carry UVs: hole fills take their
    rim's, a cut face interpolates them, decimation keeps hard colour
    borders as feature lines. `BoolReport::faces_without_colors` counts faces from
    an operand without colours (its `Debug` text shows the field only
    when it is not 0).
  - `shape_tools::downsize` and `decode`: a texture budget. PNG and
    baseline JPEG are decoded (integer inverse DCT), box-filtered down
    in linear light for colour maps and as stored for data maps, and
    written as PNG; the same bytes on every target. `shape process
    --texture-max PX` and the MCP `process` tool's `texture_max` apply it
    to every map kept and report each map's size and decoded bytes
    before and after (`textures`).
  - `measure` with `Measure::materials` reports each material's maps and
    sizes and texture memory per kind of map (`texture.materials`,
    `texture.by_kind`), and on a coloured solid `colors.borders`.
  - The CLI's and MCP's `import` report gives each part's `colors` and
    `look` (factors, maps, `defaults`); `repair`, `decimate`, `unwrap`
    and `process` report the `material` written, with what was
    `dropped` (every map when the UVs are replaced, or parts whose
    materials differ), and an OBJ or STL output's `not_written` lists
    the UVs, colours and material it leaves out. `process` has a
    `textures` step (seven steps now), and its `measures.texture` counts
    the maps it writes (it counted every image the file references).
  - Golden: `mcp.process.ball_defects` moved for the new report fields
    only (its GLB is byte for byte the same). New cases
    `meshtools.look.import`, `meshtools.look.chain`,
    `meshtools.look.texture`, `mcp.process.look`; fixture
    `golden/meshes/stripes_420.jpg` (32 × 16, made here with libjpeg).
  - `bench/generated/look.ts` measures what of a file's look reaches the
    output; `bench/generated/src/bin/normals.rs` what the chain does to a
    normal map's shading.
- The preview draws colour (`docs/research.md` §30): a part's base-colour
  map (PNG or JPEG; a program's `map`, an imported file's) sampled per
  pixel from a box-filtered mip chain, and `Solid::colors` (linear RGBA
  per triangle corner, a new field) interpolated, both times the
  material's `color` in linear light. A part with neither looks as
  before, to the byte. A map that cannot be drawn is listed in
  `Views::undrawn` and the CLI's and MCP's `undrawn`. Metalness,
  roughness and emissive still do not change the picture.
- `Preview::fov` (perspective, up to π/3; orthographic stays the
  default) and `Preview::unlit` (colour only); `shape_tools::views`
  gives each view's camera (`ViewCamera`). CLI `shape preview --fov RAD
  --unlit --each`, MCP `preview` `fov`, `unlit`, `each`; the binding's
  `preview({ fov, unlit })`. `shape preview` now prints a JSON report:
  the files written, what each shows (cameras in the scene's frame and
  the GLB's), and the maps not drawn.
- The UV view: `shape_tools::uv_view`, `shape preview --uv [--uv-size
  PX]`, MCP `preview` `uv`, `uv_size`: one image per material, chart
  outlines over its map or a checker, covering whole UV tiles
  (`uv_min`, `tiles` in the report).
- `shape preview` of a mesh file draws each part's imported map.
- The preview decodes its maps with the texture budget's decoder
  (`shape_tools::decode`: PNG, baseline and extended sequential JPEG);
  a progressive JPEG is listed in `undrawn`.
- Golden: new `tools.preview.textured`, `tools.preview.colors`,
  `tools.preview.uv`, `uvs.program.preview`.
- `place` (stream AJ, `docs/research.md` §31): `shape place <mesh>`, the
  MCP tool `place` and `shape_tools::place` (`Place`, zero value changes
  nothing) turn a mesh so a stated axis is up and another faces front
  (one of the 24 exact axis turns, then `--turn DEG` about up), scale it
  uniformly (`--longest`, `--height`, `--width`, `--depth` with a unit,
  `1.2m`, or `--scale`), and move its origin (`--pivot base`, `center`
  or a point). Skins move with the mesh (bones rigid, inverse bind
  matrices `S · IBM · A⁻¹`); UVs and textures are kept. The report gives
  what was done and the axis measures before and after. `process` takes
  the same options (MCP `place`) and places after decimation; the
  report has `place` only when asked.
- `measure --axes` (MCP `axes`): a mesh's extents, `pivot_from_base`,
  longest and shortest axis, the area facing each way and where its
  mass sits in the box (`shape_tools::axis_measures`); added only when
  asked.
- Import: `up` and `front` (`--up`, `--front`, MCP `up`, `front`) name
  an OBJ's or glTF's own axes that are up and front, so InstantMesh's
  OBJs (`--up -z --front +x`) can be read. `z_up` now applies to glTF
  too and is refused together with `up` or `front`; FBX refuses all
  three (it says its axes). No golden line moved; new cases `place.*`.
- `decimate` no longer folds the surface through itself: a collapse
  whose rewritten triangles would cross another triangle (exact,
  `shape_bool::selfcheck::crossing`, against the triangles near them in
  a spatial hash) is not taken. On an input that already crosses itself
  the count never rises. A decimation that misses its target says why:
  `DecimateReport::blocked` counts the edges left by the rule that
  refuses each (`fixed`, `error`, `link`, `turn`, `uv`, `fold`,
  `not_retried`), and `folds_refused` the collapses refused for folding
  (`docs/research.md` §32). The report also gives `uv_islands` and
  `uv_charts`.
- `Decimate::merge_charts` (`--merge-charts UV` on `shape decimate` and
  `shape process`, `merge_charts` in the MCP tools): UV islands that meet
  along an edge and agree there up to a rigid move in UV space are one
  chart for the seam rule, so their shared seam no longer pins the
  decimation; the texture is not rewritten. Off by default.
- `repair`: `Repair::non_manifold` (`--non-manifold`, MCP
  `non_manifold`) splits edges with more than two faces into manifold
  sheets (`split_non_manifold`): faces the surface already joins round
  an end of the edge pair up, the rest by their exact order round it;
  every extra sheet through a vertex gets its own copy of it, moved 1 %
  of its shortest edge into its own fan so a file does not weld it back.
  The pieces that come apart and are floaters are dropped. The report's
  `non_manifold` says what was split, left and moved.
- `close_holes` fills a hole over the edge limit (up to 512 edges) when
  its least-area fill is flat (no triangle against the mean normal) and
  crosses nothing; `RepairReport::large_holes_filled` counts them.
  `close_holes` returns a `Holes` report.
- A hole fill whose rim crosses a UV seam takes one UV at every corner
  (its first rim vertex's): it no longer fans across the atlas (51–55 %
  of texels overlapped on two generated meshes, back to their level
  without fills). `uv_seam_fills` counts these fills.
- The fuse step untangles each shell on its own and joins only the
  shells that meet or may nest, the small ones among themselves first:
  133 s → 24 s on a 619 000-triangle chair. The shells an untangling
  leaves besides its largest are reported (`pockets`) and dropped when
  they are floaters by the repair's `floaters` (`pockets_dropped`,
  `pocket_triangles_dropped`).
- Golden: `mcp.process.ball_defects` moved for the report's new fields
  (`folds_refused`, `blocked`, `uv_islands`, `uv_charts`,
  `non_manifold`, `large_holes_filled`, `pockets`, `pockets_dropped`,
  `pocket_triangles_dropped`); its mesh is the same bytes. New cases
  `meshtools.decimate.fold`, `meshtools.decimate.charts`,
  `meshtools.repair.non_manifold`, `meshtools.repair.large_hole`,
  `meshtools.repair.seam_fill`, `meshtools.repair.pockets`.
- `bench/generated`: the guide's repair adds `--non-manifold`;
  `run.ts --merge-charts UV --modes keep`; `compare.ts` prints two runs
  side by side (`docs/research.md` §32).
- The four mesh-tool streams merged (stream AL, `docs/research.md` §33):
  - Decimation carries a corner's UV and colour together and keeps
    AK's refusals, fold check and chart merging: a corner across a
    merged seam takes its UV through the charts' moves and its colour
    from its own side; a seam inside a merged chart stays a feature
    line where the colours differ across it. Repair's colours run
    through the large and seam-crossing fills, and after the
    shell-by-shell fuse as after the plain one.
  - One image decoder: the preview reads maps with the texture budget's
    (`shape_tools::decode`); `jpeg-decoder` is no longer a dependency. A
    progressive JPEG is listed in the preview's `undrawn`.
  - `shape preview` of a mesh file draws each part's whole imported
    material (it drew the base-colour map only), reading its linear
    colour factors as such; `shape place` writes each part's whole
    material (it wrote the base-colour map only).
  - Golden: `mcp.process.ball_defects`, `place.process`, `place.command`
    and `mcp.process.look` moved for the report fields the other streams
    added (each GLB written is the same bytes); `tools.preview.textured`
    and `tools.preview.uv` for the JPEG decoder (within 3 levels of
    AI's on `label.jpg`).
- `Decimate::cross_color_borders` (CLI `--cross-color-borders` on
  `decimate` and `process`, MCP `cross_color_borders`): collapse across
  hard vertex-colour borders, off by default. Unique3D's chair reaches
  2 000 triangles with it (8 078 without) and 10 000 at 2.0 mm of error
  (5.8 mm without).
- The preview adds a material's emissive colour (times
  `emissive_intensity`) to the lit colour, in linear light. Golden
  `tools.preview` and `tools.preview.png` moved for it (the tools
  scene's wheel glows red); no other line.
- Atlas packing (`shape_tools::pack`, `unwrap_atlas`, `repack`,
  `atlas_measures`; `Atlas`, `AtlasReport`): charts packed tallest first
  by a bottom-left skyline on an integer texel grid, padding in texels,
  optional quarter turns, one texel density (or a weight per chart), and
  measured: coverage, overlap, overlapping pairs, texel density min,
  median and max. `Model::glb_atlas` and `shape export --uvs --atlas PX`
  pack every part's UVs into one atlas and let parts whose materials
  agree share one material (`glb_shared`); a part with a map keeps its
  UVs.
- Baking (`shape_tools::bake`; `Bake`, `BakeSource`, `BakeReport`): a
  source mesh's base colour (times its vertex colours),
  metallic-roughness, emissive and occlusion maps and its shading
  normals (its normal map, or its shape) baked into a low mesh's atlas:
  conservative rasterisation in UV space, the source point along the
  low normal with the facing agreeing (exact crossings) or the nearest
  point, dilation, PNG. The report counts texels without a source, those
  taken from the nearest point, those whose nearest crossing faced away,
  ambiguous ones, the largest distance, and the normal map's angle to the
  source's shading with and without it.
- `PlainGlb::tangents` writes `TANGENT`: per-vertex sums of ∂P/∂u and
  ∂P/∂v (not MikkTSpace), the frame the bake uses.
- `shape process --bake PX [--bake-distance MM]` (MCP `process`
  `bake`, `bake_distance`): repair, decimate without UVs or vertex
  colours to the budget, pack at PX², bake, write the GLB with the baked
  maps, the factors and `TANGENT`; the report's `unwrap` is the atlas's
  and `bake` is new. Without it nothing changes. Golden: new
  `atlas.pack`, `atlas.model`, `bake.fixture`, `mcp.process.bake`; no
  existing line moved.
- `shape_gen::hull::convex_hull`: the convex hull of points in space, by
  exact predicates, with no vertex but its corners; faces in one plane
  are one surface. The program operation `wrap` (`name`, `parts`) and the
  binding's `wrap({ name, parts })` make a new part, the hull of parts as
  they stand; faces and edges are named `hull`. Golden case `gen2.hull`.
- Colliders: `shape_tools::colliders`, `shape collide` and the MCP tool
  `collide` cover each part of a program or a mesh file with convex
  hulls. One hull a part by default; `hulls` cuts a part with hollows
  where they are deepest, `concavity` says when to stop, `max_vertices`
  thins a hull a vertex at a time where the error is largest. The report
  gives each hull's vertices, triangles, volume, concavity and how far
  the part pokes out of a thinned hull; `-o` writes the hulls as
  `<part>.collider.<k>`. `shape-tools` now depends on `shape-gen` (the
  hull). Golden case `collide.hulls`; no existing line moved.
- `subdivide`: Catmull and Clark's subdivision (`shape_edit::subdivide`,
  the program operation `subdivide` with `levels` and `sharp`, the
  binding's `subdivide(part, { levels?, sharp? })`). A solid's flat faces
  are the cage (triangles of one surface in one plane are one face,
  decided exactly); edges named in `sharp` stay sharp and keep their
  names, smoothed edges lose theirs and their faces become one surface.
  Refuses an open solid, and one with a skin, UVs or vertex colours.
  New `EditError` variants `NotClosed`, `NoSuchEdge`, `Unsupported`.
  Golden case `edit.subdivide`; no existing line moved.

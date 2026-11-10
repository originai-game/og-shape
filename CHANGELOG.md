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

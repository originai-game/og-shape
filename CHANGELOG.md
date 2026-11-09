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

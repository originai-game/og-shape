# og-shape research (verified 2026-11-09)

What has been checked so far, with sources. Claims marked **[unverified]**
come from a project's own documentation and still have to pass this
repository's gate.

## 1. sand3d's real Blender dependencies

Read at Sandflow-ai/sand3d `84c980a523adcb0474420ed4e3990681d403dce3`
(v1.6.3, Apache-2.0). The package imports only `bpy`, `bmesh`,
`mathutils` and the standard library.

### 1.1 What needs Blender

Only four things:

| Need | Blender call | Verbs |
|---|---|---|
| Mesh boolean | Boolean modifier, solver `EXACT` (`edit.py#L60-L123`) | cut, join, intersect, split, hole, pocket, seat, shell |
| Edge bevel | `bmesh.ops.bevel` (offset, profile 0.5, clamp overlap, loop slide), Bevel modifier fallback (`edit.py#L834-L899`) | fillet, chamfer, soften |
| Glyph outlines | FONT curve evaluated to a mesh (`profile.py#L892-L947`) | text |
| GLB export | `bpy.ops.export_scene.gltf` (`assembly.py#L97-L121`) | export .glb |

Everything else is sand3d's own Python plus two building blocks:
- a constrained Delaunay triangulation (`mathutils.geometry.delaunay_2d_cdt`),
  used by every profile and every cap;
- BVH and KD-tree queries (`extrude(until=)`, `snap`, the entry search of
  hole and pocket, `dent`).

sand3d does **not** use Solidify, Remesh, Screw, curve-to-mesh or geometry
nodes:
- `sweep` uses rotation-minimizing frames by double reflection
  (`solids.py#L344-L463`);
- `loft` uses a natural cubic spline through the sections;
- `shell` offsets each vertex by a least-squares fit over the planes it
  touches, then runs a self-boolean (`edit.py#L1085-L1189`).

### 1.2 Pure math (no kernel)

Frames, at, transform, local, bbox, grid, ring, along, helix, gear
(involute), wrap (hull), joint, pose, group, assembly, attach, units,
material (a tag), scale, copy, merge, delete, and the OBJ, STL, URDF and
USDA writers.

### 1.3 Names

- Each name is a boolean attribute: `s3d_face:<tag>` on faces,
  `s3d_edge:<tag>` on edges, each also qualified as `<object>.<tag>`
  (`_core.py#L134-L250`).
- Generators: faces carry a surface id. An edge between faces of different
  ids is a real edge, named after the caps it bounds (`top`, `bottom`,
  `start`, `end`) or after the verb's kind (`side`, `rim`, `seam`).
- Booleans: faces carry an operand id through the modifier. Edges between
  operands get `cut`, `<target>.cut`, the tool's name and the cap names.
  Faces left by a cutting tool get `cut`. A named edge left flat between
  coplanar faces (within 0.5°) loses its names (`edit.py#L335-L375`).
- Bevels: new faces lose inherited names and take `fillet` or `chamfer`.
  Edges touching them are re-tagged at more than 10° (40° between two
  chamfer faces) with the cap names they touch (`edit.py#L737-L770`).
- Lookup takes every element with any of the names given and suggests the
  nearest names on a miss.

### 1.4 Determinism

- Canonical order after every boolean and bevel: vertices by coordinate,
  then edges and faces by vertex index (`edit.py#L939-L949`).
- Segment counts depend only on the radius in mm (0.05 mm chord, 24 to 64
  per turn, `_core.py#L280-L296`).
- Matrix entries within 1e-6 of an integer are snapped; tool coordinates
  are snapped onto the target's; a weld at 1e-5 mm after booleans.
- All tolerances run in mm whatever the unit; double precision where f32
  hurts.
- Tested only as two processes on the same machine and build, never across
  platforms.

### 1.5 GLB joint extras

A node per joint at its pivot, the child under it. The node's extras:

```json
{ "s3d_joint": { "name": "...", "type": "revolute|continuous|prismatic|floating",
  "value": 0.0, "axis": [x, y, z], "lower": 0.0, "upper": 1.0,
  "mimic": { "joint": "...", "multiplier": 1.0 } } }
```

Units are SI (radians or metres); the axis is Y-up in the joint node's
frame. A revolute joint with no range is written as `continuous`. Face and
edge names are not written to the GLB.

## 2. Library survey

Constraints: pure Rust, permissive licence, deterministic on x86-64,
aarch64 and wasm32.

### 2.1 Mesh booleans

| Option | Licence | Finding |
|---|---|---|
| **manifold-rust** 0.19.2 (2026-11-01) | Apache-2.0 | Line-by-line port of Manifold 3.5 (the algorithm in Blender 4.5+/5.x, Godot, OpenSCAD). Manifold output guaranteed for manifold input. Split, SplitByPlane, TrimByPlane. Deterministic trig; optional parallel feature claims bit-identical results **[unverified]**. Builds to wasm. Also ships an exact-rational arrangement engine (Zhou 2016) for self-intersecting input. One maintainer, four months old, some `std::HashMap` to audit. Inherits Manifold's coincident-coplanar-face weakness (#1430). |
| **og-geom CSG** (origin-engine) | MIT | Already deterministic: Shewchuk predicates (`robust`), spade CDT, winding numbers per region, f64 with `+ - * /`, `sqrt` and own `atan2`. Not yet measured on large or AI-generated meshes. |
| Manifold C++ 3.5.4 + bindings | Apache-2.0 | Needs a C++ toolchain; the TBB CSG-tree order is not deterministic. |
| boolmesh 0.2.2 | MPL-2.0 | Excluded (licence). |
| csgrs 0.20.1 | MIT | BSP; no manifold guarantee. |
| hypermesh 0.1.0 | MIT | Exact reals; too young (2026-05). Watch. |
| Cherchi 2022 | MIT, depends on LGPL predicates | Ideas only. |
| EMBER 2022 | No source | Paper only; the model for a later integer-exact kernel. |
| trueform 2026 | Non-commercial | Excluded. |
| CGAL, libigl copyleft, MeshLib | GPL / commercial | Excluded. |

Blender's Exact solver (Trickey, GPL) follows Zhou 2016 with GMP rationals;
Blender has since adopted Manifold. Phase 0 chooses between og-geom's CSG
and manifold-rust on measurements.

### 2.2 Fillet and chamfer

No permissive library gives a Blender-equivalent bevel. Write our own,
clean-room from Blender's published design:
- Trickey's notes: http://archive.blender.org/wiki/2024/wiki/User:Howardt/Bevel.html
- Bevel V2 design issue: https://projects.blender.org/blender/blender/issues/98674
- Blender 5.2's Mesh Bevel node (PR #158151) is GPL: design reference only.

The design to follow:
- four offsets per edge (start/end × left/right), superellipse profile
  (0.5 a circle, 0.25 straight), segments, inner and outer miters;
- corner fill by valence: an octant patch for three beveled edges, the ADJ
  quad pattern placed by boundary-constrained Catmull-Clark (Levin), then
  resampled; a Cutoff fallback;
- clamp overlap per vertex or edge with exact predicates; loop slide and
  global clamp as optional passes outside the core;
- UVs by mean-value interpolation.

No 2022–2026 paper improves on this for mesh bevels. B-rep fillets (Open
CASCADE LGPL, truck master, monstertruck-fillet, vcad) and SDF or Minkowski
rounding are out of scope: wrong licence, no edge selection, or dense
topology.

### 2.3 Everything else

| Need | Choice | Licence / version |
|---|---|---|
| 2D booleans, offset, stroke | i_overlay | MIT/Apache, 9.1.0 (2026-10), integer core |
| Triangulation with holes | i_triangle | MIT/Apache, 0.51.0 (2026-10), integer core |
| Curves | kurbo with `default-features = false, features = ["libm"]` | Apache/MIT, 0.13.1 |
| Glyph outlines | skrifa (fontations); harfrust if shaping is needed | MIT/Apache, 0.49 |
| Decimation, weld, LODs | meshoptimizer-rs | MIT, 0.2.19 (2026-11-06), claims byte-identical output **[unverified]** |
| Sweep, loft, helix, thread, shell | write our own | double-reflection RMF (Wang et al., TOG 2008); twist spreading (Mossman et al., CGF 2023) |
| Large or variable offsets | later | Zint et al. CGF 2023; Feature-Preserving Offset Meshing, TOG 45(3) 2026 |
| Arc-exact offsets (optional) | cavalier_contours | MIT/Apache; its `std` trig must be replaced |

Open: BSL-1.0 crates (clipper2-rust, boostvoronoi) are permissive but not
on the list.

## 3. The platform's 3D pipeline today

Read at origin-agent and origingame `main` on 2026-11-09.

### 3.1 Blender in the platform

- The asset image installs Blender 5.1.2, trimmed (no Rigify, USD, Cycles
  libraries): origin-agent `server/scripts/sandbox/Dockerfile:84-104`,
  `build-images.ts:43-87`. Its 1.8 GB size is an estimate, never measured.
- The `asset_process` runner tool (origin-agent
  `runner/crates/runner-tools/src/asset_blender.py`) does only:
  import glb/gltf/obj/fbx, weld, collapse decimation to a ratio or target,
  smart-project UVs, export up to 4 LODs, measure triangles and texture
  memory, render an Eevee turntable.
- Promised but never built: retopology, PBR baking, Rigify, retargeting,
  atlas packing (origingame `docs/game-pipeline.md:650-655`).
- InstaBox development sandboxes have no Blender.
- Hunyuan3D texturing expects a repaired, decimated (≤ 40k triangles),
  joined, triangulated plain GLB (origingame
  `ai-gateway/docs/hunyuan3d-workflow.md:28-66`).

### 3.2 Generation

- Through the AI gateway: Meshy, Tripo and Hunyuan3D (on Modal with
  trimesh, pymeshlab, xatlas). Rigging is Meshy's or Tripo's.
- A game's `assets/` holds uncompressed GLB with PNG/JPEG and `.ogt`
  import settings; `og cook` makes LODs and compression.
- Models as code must be the engine's `type model` (origin-agent
  `knowledge/game/instructions.md:212-218`), but a recorded run built its
  models as scaled icosahedra (`server/src/core/skill-pages.ts:55-67`):
  the gap this kernel closes.
- No platform plan mentions CAD, parametric or articulated modeling, or
  replacing Blender.

### 3.3 What origin-engine already covers

FBX/OBJ import (ufbx), meshopt simplification and LOD chains, KTX2 and
meshopt compression, xatlas UVs (lightmaps and projection painting),
`og bake projection` GLB export, og-geom (shape, extrude with bevel and
holes, lathe, sweep, polyhedra, merge, exact CSG, fracture), `type model`
(materials, pivots, parts, mirror), probe turntables and `og compare`.

### 3.4 What is still missing for the platform to drop Blender

1. A standalone GLB export of a model.
2. Repair of generated meshes (weld, floaters, degenerate faces, apply
   transforms, triangulate, join into one node).
3. Decimation as a tool output (GLB plus measures).
4. General UV unwrap for texturing.
5. A turntable preview with no GPU.
6. Measures as JSON (triangles, texture memory, bounds, pivot).
7. Modeling og-geom lacks: loft, fillet and chamfer on chosen edges,
   shell, text, named parts and joints, robust booleans on AI meshes.

Out of scope, left to providers or later: 3D auto-rigging and
retargeting, retopology, high-to-low baking, atlas packing.

## 4. Work chains verified so far

- **Program → GLB with joints → engine:** sand3d writes joints as GLB node
  extras (§1.5); og-cook already reads glTF extras
  (`crates/og-cook/src/cook/model/primitive.rs`). The chain is open only at
  mapping `s3d_joint` onto pivots and joint metadata.
- **Program → engine model blob:** `type model` already cooks og-geom
  generators into the same blob a glTF cooks to
  (`crates/og-cook/tests/model_text.rs`). og-shape extends this path.
- **Determinism without a GPU:** og-geom's generators and CSG are already
  deterministic across hosts by the same rules (`crates/og-geom/src/lib.rs`,
  `csg.rs`). No step in the plan needs a GPU.
- **Generated mesh → texturing:** provider output → repair → decimate →
  unwrap → GLB is the Hunyuan contract (§3.1); today it runs in Blender by
  hand or in `asset_process`.

## 5. Phase 0: the boolean decision (2026-11-09)

**Decision: keep og-geom's exact-predicate CSG (`shape-bool`).** Drop
manifold-rust. Three follow-ups are part of phase 2 (below).

### 5.1 How it was measured

`bench/booleans` (outside the kernel workspace) runs union, intersect and
subtract per case with og-geom's CSG, manifold-rust 0.19.2's exact engine
and its robust engine (sequential, no `parallel` feature). Per case and
engine there is one verdict:
- **ok**: every output is closed and oriented (each edge used as often one
  way as the other, after exact welding) and, where the inputs have a
  volume, V(A∪B) + V(A∩B) = V(A) + V(B) and V(A−B) = V(A) − V(A∩B) to
  1e-6 of the operands' size;
- **refused**: an error value;
- **wrong**: an output that is open or breaks an identity;
- **panic**, **timeout** (120 s): none occurred.

Suites:
- **sand3d**: 17 cases rebuilt by hand from sand3d's edit verbs (hole,
  counterbore, pocket, seat, join, split, shell, slot, washer), including
  flush and touching coplanar faces, a shared edge, tangent tubes and a
  rotated tool.
- **Thingi10K**: 40 closed, manifold, single-component meshes of 1k–20k
  faces with CC BY, CC BY-SA or public-domain licences
  (`bench/booleans/thingi10k.txt`, downloaded by `fetch-thingi10k.sh`,
  never committed). A = the mesh, B = the mesh rotated (30, 45, 60)° and
  moved 0.3 (manifold-rust's own pairing).
- **synthetic AI defects**: five shapes (sphere, rounded box, torus,
  blob, mug) polygonised by marching tetrahedra (32³), then each defect
  alone: clean, degenerate (vertices rounded onto grid corners), noise
  (0.3 cell), floaters, holes (1 % of triangles), flipped (2 %), triangle
  soup, self-intersecting (two overlapping copies). Cut by a tilted
  cylinder.
- **scaling**: two dense spheres, 256 to 65 536 triangles each.

### 5.2 Results

| suite | og-geom | manifold exact | manifold robust |
|---|---|---|---|
| sand3d (17) | 17 ok | 16 ok, 1 wrong | 16 ok, 1 wrong |
| Thingi10K (40) | 40 ok | 40 ok | 40 ok |
| synthetic (40) | 19 ok, 21 wrong | 27 ok, 10 refused, 3 wrong | 25 ok, 10 refused, 5 wrong |

The one sand3d failure of manifold-rust is the tangent case (two tubes of
one radius crossing): the identities are off by 2.5e-4 (exact) and 2.0
(robust). og-geom gets it right, with a non-manifold union where the
surfaces touch.

Synthetic, by defect (five shapes each):

| defect | og-geom | manifold exact | manifold robust |
|---|---|---|---|
| clean, floaters, soup | ok | ok | ok |
| degenerate | 4 ok, 1 wrong | ok | ok |
| noise | wrong | 2 ok, 3 wrong | wrong |
| holes, flipped | **wrong** (open output, no error) | refused | refused |
| self-intersecting | wrong (open) | closed | closed |

Speed (mean of the three operations per case):

| | og-geom | manifold exact | manifold robust |
|---|---|---|---|
| sand3d, mean | 4.6 ms | 3.3 ms | 36 ms |
| Thingi10K, total / max | 9.1 s / 1.2 s | 8.4 s / 1.0 s | 43.6 s / 4.8 s |
| spheres 4 096² | 82 ms | 52 ms | 313 ms |
| spheres 16 384² | 554 ms | 182 ms | 1 184 ms |
| spheres 65 536² | 9 018 ms | 1 040 ms | 4 464 ms |

Hash identity: every output hash of both engines on the sand3d and
synthetic suites is the same natively (x86-64) and as wasm32-wasip1 under
wasmtime. og-geom's outputs are also in the gate on x86-64 Windows and
aarch64 Linux.

### 5.3 Why og-geom

- **Licence.** manifold-rust depends unconditionally on clipper2-rust,
  which is BSL-1.0. BSL-1.0 is not on this repository's licence list.
  Using manifold-rust would need an exception or a fork without its
  cross-section code.
- **Determinism is already proven.** og-geom's output is in the gate on
  four targets, and its own recorded hashes match the engine's. In
  manifold-rust, the following would have to be audited and gated:
  - std `sin` in `interp_tri.rs`;
  - `std` and rustc-hash maps in about 20 files;
  - a process-global default engine (hidden state).
  
  It is 34.5k lines with one maintainer.
- **Robustness on the inputs the kernel builds.** og-geom passes all of
  sand3d, including the tangent case manifold-rust gets wrong, and all of
  Thingi10K.
- **Speed is close up to about 16k triangles a side.** Beyond that,
  og-geom's quadratic pair loop dominates. Its algorithm is not the cause,
  so this is fixable (below).
- **AI meshes are a repair problem, not a choice between these engines.**
  Neither engine repairs noise, holes, flips or self-intersections.
  manifold-rust refuses the cases it cannot do, which is the right
  behaviour. og-geom returns open output instead.

### 5.4 Follow-ups (phase 2)

1. **Validate operands.** `csg` returns an error value for an operand that
   is open or not oriented, after welding. It never returns open output
   silently (`docs/design.md` §6).
   **Done** for the kernel's entries (`csg_points`, `boolean`). og-geom's
   `csg` on f32 meshes keeps its old behaviour, so its output still
   matches the engine's.
2. **A broad phase.** Replace the all-pairs loop with a sort-and-sweep or a
   BVH over triangle boxes, with a deterministic visiting order. Target:
   within 2× of manifold-exact at 65 536².
   **Done:** a sweep along x, with pairs visited in the old order, so the
   output is unchanged. Spheres 16 384² go from 554 to 222 ms, and
   65 536² from 9 018 to 1 149 ms (manifold-exact: 182 and 1 040 ms).
3. **Self-intersecting operands.** These need a self-union, the same
   arrangement as CSG with one operand. Until it exists, `repair`
   (phase 5) runs before booleans on imported meshes. Phase 5 re-checks
   this decision on real provider outputs (`docs/plan.md`).
   **Partly done:** `fuse` joins the separate shells of a solid (`merge`
   output, interpenetrating parts) before any boolean.
   - A single closed surface that folds through itself passes the operand
     check and used to give open output with no error (found by stream D).
   - `boolean` now checks its result and returns `BoolError::OpenResult`
     instead.
   - Still to do: an exact self-intersection check on the operands, to
     refuse up front, and a self-union to untangle such a surface.
4. **Exact edge-on-edge crossings (found in phase 2).** The minimal case:
   `cuboid(40, 40, 10)` minus a centred `cylinder(r = 2, 24 segments)`
   through it gave an open result with 100 boundary edges; a blind one
   (z from 4) gave 46 non-manifold edges; r = 3 was closed. The plate's cap
   diagonal runs within rounding of the cylinder's vertex columns.
   **Done.** The diagnosis did not hold up as recorded: in the minimal case
   no crossing is edge-on-edge. The diagonal misses the column by an ulp, so
   the exact predicates see two `EdgeTri` crossings 4·10⁻¹⁶ apart: the
   column through a cap triangle, and the diagonal through a wall triangle.
   Three faults in `csg.rs` turned that into cracks, all in rounding of
   constructed points, not in the predicates:
   - A point made inside a triangle (an edge through it) can round to a
     position past its outline in the projection: past a corner's edge, or
     past a point on an edge that itself rounded inward. Its segments then
     run through an outline point, spade merges them into an edge that no
     crossing names, and the region of step 3 leaks across it (the bore
     wall joined the region below the plate). `pieces` now places such a
     point, for that triangle's triangulation only, at the smallest step
     towards the centroid that an exact winding test (`orient2d`) puts
     strictly inside the outline. Its shared position, and the segments
     that end at it, are unchanged.
   - Two crossings a rounding error apart can round to the same position
     (a countersink edge passing a bore column). spade merged them, a
     segment was lost, and a region leaked. `add_point` now makes them one
     point, on the edges of both.
   - A legitimate sliver from an earlier cut (6·10⁻¹⁴ in area) has an f64
     cross product that cancels to zero, so `drop_axis` chose a degenerate
     projection. `tri_normal` now takes its components from `orient2d`.

   Every golden hash and og-geom's contract tests are unchanged. The test is
   `crates/shape-bool/tests/edge_on_edge.rs` (through and blind, both
   closed and of the analytic volume). `shape-edit`'s round tools are no
   longer turned. A countersink's cone still runs on below the bore, so it
   crosses the bore wall rather than touching it along a ring: a ring of
   coincident edges is a different case and has not been tested unturned.

## 6. Export and the CPU preview (phase 4, stream B; 2026-11-09)

### 6.1 Decisions

- **One crate, `shape-tools`.** Its entry is `Scene`: parts, each a
  `(name, &Solid, Material, Option<Joint>)` plus the name of the part it
  hangs on, parents first. Parts and assemblies come later (phase 4); the
  scene is the small desc they will fill.
- **Axes and units.** Programs are millimetres, Z up. GLB and OBJ are
  metres, Y up (kernel Z → Y, Y → −Z, as sand3d's exporter does); STL is
  metres, Z up (`meshfile.py` does the same).
- **GLB, written by hand.** One node, one mesh, one material per part;
  POSITION and NORMAL as f32, u32 indices, one buffer. Normals: one vertex
  per (position, surface id), its normal the area-weighted sum of that
  surface's triangle normals around it, so real edges split and a surface
  stays smooth. JSON keys are in a fixed order with no whitespace; numbers
  are Rust's shortest round-trip form (the same on every host); `-0` is
  written `0`.
- **Joints (§1.5's shape).** A jointed part's node hangs under a node named
  after the joint, at its pivot, which hangs under the parent part's node.
  The part's node undoes the pivot, so meshes stay in world coordinates at
  rest. Extras: `{"s3d_joint": {name, type, value, axis, lower, upper,
  mimic}}`, SI units, the axis in the joint node's frame (Y up). As in
  sand3d, a revolute joint with no range is written `continuous`, and a
  follower with no range takes its leader's × its multiplier. The joint
  node carries the pivot *after* the joint's motion, so `value` poses the
  part as sand3d's exporter does; the preview places parts by the same
  forward kinematics. Kinds: revolute, continuous, prismatic, floating;
  a part with no joint is welded (sand3d has no `fixed` kind either).
- **Materials** take three.js `MeshStandardMaterial` names (`color`,
  `metalness`, `roughness`, `emissive`, `opacity`). The zero value is
  three.js's default (white, roughness 1, opaque). Colours are sRGB and
  written linear (`baseColorFactor`); opacity below 1 writes
  `alphaMode: BLEND`.
- **The rasterizer.** Orthographic views at one scale for all azimuths
  (the scene's bounding sphere fills 92 % of the short side). Vertices snap
  to 1/256 pixel in i64; coverage is three integer edge functions with a
  top-left rule (an antisymmetric tie-break, so a pixel on a shared edge
  goes to one triangle only). Depth and light are interpolated in f64 from
  the integer barycentrics. Lambert from one world light (ambient 0.38,
  diffuse 0.62), lit from the side the camera sees, smooth per surface.
  Real edges are one-pixel integer DDA lines, depth-tested with a bias of
  0.6 % of the scene's radius. The ground is a square 1.15 radii across at
  the scene's lowest z; the shadow is every triangle pressed onto it along
  the light, marked only where the ground is the nearest thing. No GPU,
  threads or SIMD; `sin`/`cos` come from `trig` (camera azimuths, joint
  turns), `pow` from `trig` (sRGB to linear).
- **PNG.** RGBA8, every row filtered `Up`, zlib from `miniz_oxide` at level
  9, CRC-32 written here. The gate hashes PNG bytes as well as the
  framebuffer (`tools.preview.png`), which checks `miniz_oxide` on every
  target.

### 6.2 Dependencies

| crate | version | licence | where |
|---|---|---|---|
| miniz_oxide | 0.9.1, `default-features = false`, `with-alloc` | MIT OR Zlib OR Apache-2.0 | kernel |
| adler2 (via miniz_oxide) | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | kernel |
| gltf | 1.4.1, `default-features = false`, `utils`, `names`, `extras` | MIT OR Apache-2.0 | dev only: reads the GLB back in a test |

`gltf` brings serde, serde_json, gltf-json, gltf-derive, syn and their
helpers (MIT and/or Apache-2.0, two Unlicense OR MIT), and
**unicode-ident** 1.0.27, `(MIT OR Apache-2.0) AND Unicode-3.0`.
Unicode-3.0 is not on the licence list. It never ships (dev-dependency,
build-time proc macro), and the gate does not see it: `cargo deny check`
as configured leaves dev-dependencies out (`cargo deny list` shows none of
`gltf`'s tree). Whether the licence gate should cover dev-dependencies,
and whether Unicode-3.0 may be allowed there, is the owner's call.

### 6.3 Measurements (x86-64, release)

- Golden cases `tools.glb`, `tools.obj`, `tools.stl`, `tools.preview`
  (four 256² views) and `tools.preview.png`: identical natively and as
  wasm32-wasip1 under wasmtime. The whole golden run takes 0.24 s.
- The ten phase 1 rebuilds, four 320² views each plus PNG encoding:
  0.36–0.42 s for all ten. Two runs give identical PNG bytes.

| program | parts | joints | triangles | GLB bytes | PNG bytes |
|---|---|---|---|---|---|
| xyz_cartesian_stage (A7) | 29 | 3 | 348 | 46 700 | 31 950 |
| floor_task_lamp (A6) | 18 | 3 | 3 228 | 135 920 | 26 319 |
| double_leaf_garden_gate (A2) | 37 | 3 | 928 | 76 552 | 22 320 |
| compact_apartment_refrigerator (A3) | 34 | 3 | 1 472 | 91 628 | 13 558 |
| skateboard (A5) | 11 | 6 | 1 332 | 61 412 | 24 646 |
| compact_theatre_binocular (A9) | 30 | 4 | 4 528 | 191 572 | 63 523 |
| compact_pickup_tailgate (A10) | 43 | 4 | 1 492 | 103 012 | 27 734 |
| compact_trackpoint_keyboard (A4) | 59 | 43 | 13 596 | 456 940 | 51 181 |
| bevel_to_spur_transfer (A12) | 30 | 3 | 11 240 | 443 056 | 54 184 |
| gate (`docs/examples/gate.py`) | 6 | 2 | 296 | 17 384 | 11 486 |

Each program's joint count is the one sand3d's gallery lists.

### 6.4 Phase 1 acceptance: the ten rebuilds

Read at sand3d `84c980a`: the gallery's twelve articulated programs
(`site/gallery.html`) and `docs/examples`. Eight gallery programs need no
boolean and no bevel: A2, A3, A4, A5, A6, A7, A9, A10. A12 needs `cut` only
for its two bevel gears' bores; it is rebuilt without them (they sit inside
a hub). The tenth is `docs/examples/gate.py`. Left out: the washing
machine (`pocket`, `cut`, `join`, `seat`, `fillet`), A8 (`cut`, `hole`,
`fillet`), A11 (`cut`, `join`), and the other examples (`fillet`, `hole`,
`shell`, `chamfer`).

The programs are in `crates/shape-tools/tests/phase1/programs.rs`, line
for line, over a test-side subset of sand3d's vocabulary
(`tests/phase1/sand3d.rs`): sand3d's primitives' conventions, `at` with
Blender's XYZ Euler, frames with `offset`, `move`, `turn`, `bbox` and
`face` frames, mirror, materials from sand3d's presets, joints read on a
frame. In `gate.py`, sand3d's `copy` of the jointed leaf also copies its
hinge; the rebuild declares the image's hinge by hand (`hinge.001`, on the
other post, about −Z, so it also opens toward +Y). The previews are in
`docs/previews/phase1/`.

Gaps found (none worked around in the generators):
- **`loft` takes single-contour sections.** A section with a hole (A12's
  bevel gear with its bore) cannot be lofted; sand3d lofts the outline and
  cuts the bore.
- **`revolve`, `sphere` and `torus` are about +Y**, while `extrude`,
  `cylinder` and sand3d's revolve are about +Z, and sand3d's sphere and
  torus rest on z = 0. Every rebuild turns them a quarter turn and lifts
  them.
- **No placement layer yet:** `Frame` has no `offset`, `move` or `turn`;
  there is no `at`, `bbox`, `face(name)` frame, `copy` or `mirror` of a
  part. Phase 4's parts need them; the rebuilds carry their own.
- **No solid gear:** sand3d's `gear(teeth, module, width, bore)` is
  `extrude(gear − circle)` here.
- **`loft` interpolates straight or by Catmull-Rom**, sand3d's by a natural
  cubic spline: the binocular shell (four sections) differs slightly.
- **One joint per part** in the export desc. sand3d composes several on one
  child (A11's cap turns and lifts); not needed by the ten.
- Still missing from phase 1, not needed by these ten: `body`, `wire`,
  `knurl`.

What remains unchecked: the GLB has been read by the `gltf` crate only,
not by three.js (phase 4's acceptance); the joint extras' meaning is
checked by unit tests, not by posing in a viewer.
## 7. Phase 3: fillet and chamfer (`shape-bevel`, 2026-11-09)

### 7.1 Sources (clean-room)

Only Blender's published design was read, never its source or the Mesh
Bevel node PR (#158151):
- Trickey's notes, http://archive.blender.org/wiki/2024/wiki/User:Howardt/Bevel.html:
  offset kinds, boundary points where offset lines meet, the superellipse
  profile (parameter about a quarter of the exponent) at even chords, the
  ADJ vertex mesh by Catmull-Clark after Levin with the boundary
  resampled, generalized barycentric UVs.
- The Bevel V2 design issue, https://projects.blender.org/blender/blender/issues/98674
  (read through `projects.blender.org/api/v1/…/issues/98674`; the web page
  answers 403): four offsets per edge, the profile defined in the unit
  square and mapped into the skewed square of each corner, terminal
  edges ending on their end face, two bevelled edges joining their
  profiles, the ADJ pattern from one centre vertex with "fullness",
  Cutoff, miters.
- Levin, *Filling N-sided holes by combined subdivision schemes* (2000),
  for the idea of subdividing with the boundary held on given curves.
- sand3d (Apache-2.0, `84c980a`) for the cases only: `docs/examples/bracket.py`,
  `tests/cases.py` (`edge_treatments`, `fillet_on_a_chamfer`, the shell
  case's `fillet(box, 4, edges="bottom side")`).

### 7.2 What was built and the decisions taken

- **Model.** Every triangle corner at a bevelled vertex is replaced by
  the boundary point of its gap (the run of faces between two bevelled
  edges). Faces are not re-meshed: a replaced triangle is re-triangulated
  only where a corner became a profile (the end face of a terminal edge),
  by ear clipping. This keeps every unbevelled vertex and edge, so the
  cut-up faces stay welded to their unbevelled neighbours.
- **Offsets.** `fillet` uses the offset along each face (Blender's default
  and what sand3d passes), so `radius` is the true radius only where the
  faces meet square. `chamfer` takes `distance` on the cap face (the face
  carrying the edge's name) and `distance2` or `angle` on the other;
  `angle` gives `d2 = d·sin θ / sin(α + θ)` per edge end (α between the
  faces), which is where the four offsets per edge differ.
- **Boundary points.** On a planar face: where the two offset lines meet
  (an unbevelled edge is an offset line at 0, so the point slides along
  it). On a curved face (a cylinder wall, a bore): slide along the inner
  edge most across the rim, by `d / sin`. Collinear rims on a plane step
  straight in. A gap with one unbevelled edge between two faces takes the
  mean of the two faces' points on it (Blender's compromise, without
  loop slide).
- **Profiles.** Superellipse exponent `4 × profile`; equal chords in the
  unit square by a 2048-step table, made exactly symmetric. Each chain is
  `O + (A − O)x + (B − O)y` with `O = A + B − C`; `C` is the vertex for one
  or two bevelled edges (the two profiles then coincide, which is the
  mitre), `A + B − O` with `O = v + ½Σ(B_j − v)` at three, and the
  perpendicular cut at four or more.
- **Corners by valence.** One edge: the profile lies on the end face.
  Two: shared profile. Three: the octant patch, the affine image of the
  superellipsoid octant on `B_1, B_2, B_3` around `O`, gridded like a cube
  corner's three faces projected onto it (on a box it is a sphere
  octant). Four or more: the ADJ pattern by Catmull-Clark from one centre
  vertex at `2·mean(M_j) − mean(B_j)` (Blender's "fullness" stand-in),
  boundary re-sampled from the profiles at every level, to the next power
  of two, then bilinearly resampled. One segment: a flat fan. Odd counts
  above one at a corner: the cutoff fallback, a fan from the hole's
  centroid (not Blender's Cutoff, which cuts each profile flat).
- **Segments.** `shape_2d::curve::segments(max offset, max turn)`, rounded
  up to even for fillets so every corner is quads; chamfers take one.
- **Clamp.** Every rebuilt triangle must turn counter-clockwise by
  `orient2d` in the projection of its old normal, and the result must be
  closed with no degenerate triangle. Anything else is
  `BevelError::TooLarge(at)`: two opposite fillets that meet exactly are
  refused too. No loop slide and no global clamp.
- **Names.** New faces carry only `fillet` or `chamfer`. Strips joined
  through a smooth two-edge vertex share one surface, and fillet patches
  join their strips. An edge between a new and an old face turning more
  than 10° is named by the kind plus the old face's cap names; between
  new faces of different surfaces, more than 10° (fillet) or 40°
  (chamfer), by the kind. So sand3d's `fillet(…, edges="chamfer")` after
  a chamfer finds both chamfer rims.
- **Not done:** UVs (a `Solid` carries none yet; mean-value interpolation
  waits for them), arc and patch miters, pipe joints, gaps with two or
  more unbevelled edges between bevelled ones, and a bevelled edge that
  ends among smooth faces. These are `BevelError::Unsupported`.

### 7.3 Measurements

All closed and oriented, no degenerate triangles. `quad_pairs` pairs
triangles of one surface across a shared edge into convex quads, the most
rectangular first; "quads" below is paired / new triangles.

| case | volume | analytic | quads | worst fold |
|---|---|---|---|---|
| box 40×30×20, `fillet(3, top)`, 32 seg | 23 739.583 | 23 739.956 (gap = 32-gon deficit 0.37) | 256/256 | 0° |
| same, `chamfer(2, d2 = 4, top)` | 23 461.333 | 23 461.333 exact | 8/8 | 0° |
| same, `soften(3)`, 32 seg | 23 339.920 | 23 340.849 (deficit) | 13 056/13 056 | 1.5° |
| same, `fillet(4, bottom side)` (enclosure corners), 6 seg | 23 259.14 | | 312/312 | 6.1° |
| one box edge, `fillet(3)`, 32 seg | 23 922.630 | 23 922.743 (deficit) | 64/64 | 0° |
| L-prism, `fillet(2, side)`, concave edge included | within the deficit of the analytic | | all | |
| cylinder r 15, `fillet(2, top)` | 13 994.49 | | 468/468 | 0° |
| washer plate 60×40×8, hole 10, `chamfer(1, hole)` | loses 33.51 | Pappus on the 24-gon rims 33.51 | 200/200 | 0° |
| bracket (rect 80×40 r 4, two 6.6 holes), `fillet(1, top)` | loses 60.59 | (1 − π/4)r²·length 58.9, ±3 % | 1 984/1 984 | 0° |
| square pyramid apex (4 edges), 4 and 6 seg | | | apex patch all quads | ~20° |

On the pyramid, the strips of the side edges twist from the flat end
profile on the base to the apex and one quad per strip is not convex, so
8 strip triangles of 120 stay unpaired; the apex patch pairs fully.
sand3d's `fillet_on_a_chamfer` (`chamfer(9, d2 = 3)` on a disc 6 thick,
then `fillet(4, chamfer)`) is refused: 4 mm down a wall 3 mm tall does not
fit. Blender clamps it silently. `fillet(2)` there succeeds.

Every output hash is the same natively (x86-64) and as wasm32-wasip1
under wasmtime (golden cases `bevel.box`, `bevel.curved`, `bevel.sand3d`,
`bevel.adj`).

## 8. Phase 4: parts, joints, the CLI and the wasm binding (stream E, 2026-11-09)

### 8.1 Sources

sand3d (Apache-2.0, `84c980a`) for semantics only: `src/sand3d/frames.py`
(frames, `face`, `edge`, `bbox`, `local`, `at`, `grid`, `ring`, `along`,
`snap`), `joints.py` (kinds, ranges, mimic, several joints per child,
posing), `assembly.py` (group, assembly, attach, units, export), and
`programs/washing_machine.py` (the acceptance). No code was copied; the
washing machine is rebuilt line for line in TypeScript.

### 8.2 Decisions

- **Placement lives in two places.** `shape_core::Frame` gains the frame
  operations as added methods: `facing` (CadQuery's axes, as sand3d: on a
  ±Z face X is world +X, else X = Z × normal; components within 1e-6 of
  zero cleaned off the normal), `with_up`, `offset`, `translate` (sand3d's
  `move`; `move` is a Rust keyword, the binding calls it `move`), `turn`
  (radians about own X, then Y, then Z: three.js's Euler `XYZ`; sines and
  cosines within 1e-12 of 0 or ±1 snapped, so quarter turns are exact),
  `inverse`, `is_rigid`. Everything that reads solids is
  `shape_model::place`: `bbox`, `face`, `edge`, `grid`, `ring`, `along`,
  `snap`.
- **`face(name)` sits on the centroid**, as this task specified, not on
  the parent's origin projected onto the face (sand3d's CadQuery rule):
  the origin is the area-weighted centroid of the chosen triangles, +Z
  their area-weighted normal. By name, the chosen triangles are the
  largest flat group carrying it (normal and plane offset rounded to
  1e-4 as integer keys); more than six groups with the largest under half
  the named area is `NotFlat` (sand3d's rule). By direction (`"+X"`), the
  triangles facing it within 1° and furthest along it; none is
  `NotFacing`. A program ported from sand3d moves the frame down to
  sand3d's origin once (`base = front.move(0, -h / 2)` in the washing
  machine).
- **`edge(name)` is a path**, as in sand3d: the longest run of the named
  edges (ties to the lowest point), closed if it closes, anticlockwise
  about its dominant axis from the point nearest that view's first axis;
  forks are refused. Frames come from `along(path, …)`.
- **`snap`** is brute force: nearest point (Ericson) or Möller–Trumbore
  ray, every triangle, ties to the lower solid then triangle. No BVH: the
  cost is triangles × points, fine at model scale.
- **One world.** Every frame, pivot and axis the model API takes or gives
  is in the world at rest. A part's solid stays in its own space with a
  rigid placement; `at` sets it, `transform` composes a motion, and both
  move everything under the node and the pivots of joints whose child is
  among them (sand3d: "a jointed part moves with its pivot"). Edits take
  world frames and run in the part's space, so a boolean tool is mapped
  into the target's space. Geometry is always read at rest; `pose` only
  changes export and preview (sand3d reads the posed scene).
- **Booleans use their tool up** (sand3d); the tool's id then answers
  `UsedUp`. A tool may not be a group, a jointed part or a joint's parent.
  `seat` leaves its part.
- **Mirror bakes.** A mirror is no rigid placement, so `mirror` writes the
  image into each copied part's solid (triangles flipped) and gives it the
  world placement. `copy` keeps placements. Neither copies joints (sand3d's
  copy does); declare the copy's own.
- **Joints.** Kinds revolute, continuous, prismatic, fixed, floating
  (sand3d's `free`). Revolute and prismatic need a range containing 0;
  continuous, fixed, floating and followers take none; fixed and floating
  take no axis or mimic. The axis is a world direction (or the pivot's
  normal), stored as a world vector and written in the pivot's frame. The
  parent is explicit (`joint(child, parent, desc)`), where sand3d reads it
  off the frame; every joint of one child hangs on one parent; cycles are
  refused through the joints' parents and the scene tree. A follower may
  not be posed, nor follow a fixed or floating joint, and a slide may not
  follow an endless turn (sand3d's rules). Poses are checked against the
  range with sand3d's 1e-9 slack.
- **Several joints on one child** compose in declaration order, the first
  nearest the parent (sand3d). The export desc keeps one joint per part,
  so the model writes a link per joint before the last: a part with no
  mesh named `<child>:<joint>`, chained from the parent. The joint nodes'
  pivots are all world-at-rest frames, so `P0 M0 P0⁻¹ · P1 M1 P1⁻¹` is
  exactly "the k-th rides on the motions of those before it".
- **A jointed group** (the washing machine's door) becomes a part with no
  mesh named after the group; its parts hang on it welded. Parts with no
  joint hang on the body they move with (their nearest jointed ancestor),
  or on the scene root.
- **`fixed`** is a new `JointKind` in `shape-tools` (an added variant, and
  `ExportError::FixedConstrained`): written `"type": "fixed"` with no
  axis or limits. sand3d has no fixed kind; URDF does. Existing outputs
  are unchanged (every earlier golden hash holds).
- **Units.** `units` (mm, cm, m) before the first part; the built scene is
  scaled to millimetres (positions, pivots, slide values and ranges, and
  mimic multipliers across kinds), and export writes metres.
- **Materials** stay `shape_tools::Material` (three.js
  `MeshStandardMaterial` names). A group's material reaches every part
  under it that sets none (the nearest wins). No presets in the kernel:
  sand3d's presets are written out as values in the program.
- **The program.** `shape_model::program::Session` is the whole API as
  JSON operations (`{"op": "box", …}`, camelCase fields), one call at a
  time. Operations that change the model are recorded; readers are not,
  because the frames they answered are written as literals into the
  operations that used them. The recorded program replays to the same
  bytes (tested in Rust, in bun and in the gate). serde_json is the only
  new kernel dependency; its float printing (`zmij`) and parsing are
  exact, and maps are `BTreeMap` (sorted keys).
- **The wasm boundary is strings and bytes.** `shape-wasm` exports one
  class: `apply(op_json) → answer_json`, `program`, `glb`, `obj`, `stl`,
  `measure`, `preview`. Frames are turned in wasm (with `trig`), never with
  JavaScript's `Math.sin`; JavaScript does only `+ - * /` on lengths
  (`og.deg` is `d * π / 180`). The crate builds under the workspace lints,
  `unsafe_code = "forbid"` included.
- **The binding** (`crates/shape-wasm/ts`, npm `og-shape`, private):
  `load()` → `og`; `og.model({ name })` → a `Shape` whose methods are the
  operations with desc objects, as in `docs/design.md` §3; `Part` and
  `Frame` serialise themselves; errors are thrown `Error`s with the
  kernel's message. wasm-bindgen `--target web`, loaded with `initSync`
  from bytes, so bun and node run it unchanged.
- **The CLI** replays a program: `shape build | export | measure | preview
  <program.json>`, each subcommand a module, measures as JSON (the
  design §7 list; UV coverage and overlap `null` and texture bytes 0 until
  phase 5 adds UVs and textures). No argument-parsing dependency.
- **Gate.** The `wasm` step builds `shape-wasm` for wasm32-unknown-unknown
  and ends with `binding`: when wasm-bindgen 0.2.130 and bun are
  installed it builds the module and runs the binding's tests, which
  compare the washing machine's GLB and PNG with `golden/hashes.txt`.
  Without them it says `binding: NOT RUN` and passes. CI installs both on
  the x86-64 Linux job.

### 8.3 Dependencies

| crate | version | licence | where |
|---|---|---|---|
| serde_json | 1.0.154, `default-features = false`, `std` | MIT OR Apache-2.0 | kernel (`shape-model`) |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | via serde_json |
| zmij | 1.0.23 | MIT | via serde_json (float printing) |
| itoa | 1.0.19 | MIT OR Apache-2.0 | via serde_json |
| memchr | 2.8.3 | Unlicense OR MIT | via serde_json |
| wasm-bindgen (+ `-macro`, `-macro-support`, `-shared`) | 0.2.130, pinned `=` | MIT OR Apache-2.0 | `shape-wasm` only |
| bumpalo, rustversion, proc-macro2, quote, syn, unicode-ident | as locked | MIT OR Apache-2.0; unicode-ident also Unicode-3.0 | build-time, via wasm-bindgen's macro |
| gltf | 1.4.1 | MIT OR Apache-2.0 | dev only (`shape-model` tests) |
| wasm-bindgen-cli | 0.2.130 | MIT OR Apache-2.0 | tool, not a dependency |
| three (npm) | 0.186.1 | MIT | dev only, `tools/check_three.ts` |

`cargo deny check licenses bans sources` passes.

### 8.4 Acceptance: the washing machine

`crates/shape-wasm/ts/examples/washing_machine.ts` rebuilds
`programs/washing_machine.py` through the binding: `box`, `fillet(side)`,
`pocket`, `cut`, `join`, `seat` (three times), `tube`, `group`, materials,
four joints (door revolute 0–143° about −Z, drum and dial continuous,
drawer prismatic 0–200 mm along +X), `assembly` with a `front` anchor,
then `washer_open()`'s pose (door 92°, drawer 120 mm, dial 30°). Its
recorded program is `golden/programs/washing_machine.json`; its preview
is `docs/previews/phase4/washing_machine.png`.

| part | triangles | closed | volume (mm³) |
|---|---|---|---|
| cabinet (filleted, porthole pocket, drawer bay and dial seat) | 1 258 | yes | 225 360 665 |
| drum (bore cut, axle joined) | 868 | yes | 6 216 515 |
| ring (glass groove seated) | 1 368 | yes | 4 798 606 |
| glass | 252 | yes | 1 284 730 |
| handle | 12 | yes | 79 200 |
| drawer (front joined) | 40 | yes | 4 996 000 |
| dial | 216 | yes | 84 639 |

GLB 150 740 bytes, PNG (four 320² views) 18 650 bytes. Times: in bun
(wasm32-unknown-unknown) build 193 ms, export 12 ms, preview 68 ms; the
native CLI replays and exports in about 0.1 s.

**Joints, read back by three.js** (`tools/check_three.ts`, three 0.186.1
`GLTFLoader` under bun, headless): four joint nodes with
`userData.s3d_joint` of the right kinds, limits in radians and metres,
posed values written. Re-posing in three.js (undo the node's exported
value about or along its axis, apply another) matches the kernel's own
export at that pose to 1.2e-16 m (open file posed to rest vs the rest
file) and 1.9e-16 m (rest file posed open vs the open file). At rest the
door sits closed on the front (x 0.300–0.375 m) and the drawer flush (its
front at x 0.315 m); at 143° the door swings out to x 0.670 m; the drawer
slides out 0.120 m; the dial turns in place.

**Bytes on every host available here:** the GLB, the PNG and the measures
are the same natively (x86-64) and as wasm32-wasip1 under wasmtime (the
golden run). As wasm32-unknown-unknown, the binding's test under bun
(JavaScriptCore) compares the GLB's and the PNG's hashes with
`golden/hashes.txt`, and a run under node 22.23 (V8,
`--experimental-transform-types`) gave the GLB's hash; both match.

### 8.5 Gaps and what is untested

- **Not run here:** x86-64 Windows and aarch64 Linux (CI only); the
  new golden lines are expected to hold there by the same rules, which
  the CI gate checks. The binding in a browser. three.js was run headless
  under bun only, with no renderer: the scene graph and matrices, not
  pixels.
- **OBJ and STL are written at rest** (`shape-tools` ignores joints for
  meshes), where sand3d writes the current pose.
- **Joints are not copied** by `copy` or `mirror`. A mirrored jointed
  part's motion (sand3d: "the image of the motion") is not modelled.
- **Geometry is read at rest**, never posed: `bbox` or `face` of a posed
  part answers its rest place (sand3d reads the posed scene and refuses to
  declare joints off rest).
- **Directions in `shell(open)`** are the part's own axes, not the world's.
- **Frames do not remember the part they were read on** (sand3d's
  `Frame.part`); the joint's parent is given explicitly, and nothing checks
  that a child still sits where its frame put it.
- **No URDF or USD** (not in the first release), no gear or knurl parts
  in the binding, no text profiles in the program (`shape_2d::text` needs
  font bytes).
- **The washing machine's look**: the glass is a transparent material in
  the GLB, but the preview draws it opaque (the rasterizer ignores
  opacity); sand3d's glass preset also has transmission, which glTF
  core does not carry.
## 9. Phase 5: mesh tools (stream D, 2026-11-09)

### 9.1 The Blender pipeline of §3.1, and its equivalents here

| `asset_process` (Blender, §3.1) | og-shape (`shape-tools`) |
|---|---|
| import glb / gltf | `import_gltf`: GLB, `.gltf` with `data:` or external buffers (a `Resolve` callback, no file system in the kernel), all accessor types, normalized and quantized attributes, sparse accessors, strips and fans, the scene's node tree |
| import obj | `import_obj`: `o`/`g` parts, `usemtl` surfaces, n-gons, negative indices, `vt`, MTL texture maps |
| import fbx | `import_fbx`: FBX 7.x binary and ASCII (§9.3) |
| weld | `weld` (`Weld`: distance; cracks only, or every vertex) |
| collapse decimation to a ratio or a target | `decimate` (`Decimate`: ratio, target, error limit, locked borders) |
| export up to 4 LODs | `lods`: any number of levels, each from the last |
| smart-project UVs | `unwrap` (`Unwrap`: angle, resolution, padding) |
| measure triangles and texture memory | `measure` → `MeshMeasures::to_json` (§9.5) |
| Eevee turntable | `preview` (phase 4's CPU rasterizer) |
| export | `glb` (parts, joints) and `glb_plain` (one node, one mesh, with UVs) |

Added beyond it, for generated meshes (§3.4 item 2): degenerate and
duplicate triangles, floaters, consistent orientation, small holes, apply
transforms, join, self-union of interpenetrating shells, exact
self-intersection and UV overlap counts.

### 9.2 Decisions

- **Everything in `shape-tools`, new modules only:** `import.rs`,
  `fbx.rs`, `jsonread.rs`, `image.rs`, `polygon.rs`, `repair.rs`,
  `intersect.rs`, `decimate.rs`, `unwrap.rs`, `measure.rs`,
  `meshglb.rs`. The only change to an existing module is two variants
  (`Null`, `Bool`) on the JSON writer's value.
- **The data.** Import gives `ImportedPart`s: a `Solid` in the part's own
  coordinates (kernel units: mm, Z up), an `Affine` that places it, UVs
  per triangle corner as the file has them, the material's name. Faces
  carry the part's name; each glTF primitive, OBJ material run and FBX
  material is a surface. Triangles stay in file order and nothing is
  welded on import, so UVs line up; repair welds and canonicalizes.
  `Solid` has no UVs, so UVs live beside it (`Uvs`, per corner) and
  `glb_plain` writes them.
- **Axes and units.** glTF is metres, Y up by its specification; OBJ is
  read the same way unless `Import` says otherwise (`unit_mm`, `z_up`);
  FBX by its `GlobalSettings` (axes, `UnitScaleFactor` in cm). The axis
  change is a signed permutation, applied to transforms by conjugation
  (signs and swaps only) and to translations with the unit, so no
  rounding enters there.
- **No JSON or glTF crate in the kernel.** A 300-line JSON reader whose
  numbers go through `core`'s correctly rounded `f64` parser. `gltf` stays
  a dev-dependency (it reads `glb_plain`'s output back in a test).
- **Images are measured, never decoded:** sizes from PNG, JPEG, WebP and
  KTX2 headers. "Texture memory once decoded" is width × height × 4
  (RGBA8), and with a full mip chain.
- **Repair never guesses silently.** Every step reports what it changed
  (`RepairReport`), and what it could not do is a count, not a hidden
  best effort:
  - `weld` merges, by default, only vertices on open edges: it closes the
    cracks a seam-split or soup export leaves, and never pinches a closed
    surface. With a 1 µm weld of every vertex, Thingi10K #42545 (closed)
    got 4 non-manifold edges from two vertices of separate sheets; the
    crack-only weld leaves it closed. `weld_all` is Blender's merge by
    distance, for parity.
  - Kept vertices never move (each joins the nearest kept one within
    reach, visited in position order), so the result depends only on the
    positions.
  - Degenerate means exactly zero area: all three coordinate-plane
    projections collinear by `orient2d`.
  - `orient` propagates winding across manifold edges, then turns each
    piece over if its signed volume about its own centre is negative.
  - `close_holes` traces rims along open edges; a rim that passes a vertex
    twice (two holes touching at a corner) is cut there into simple
    loops. The fill is the least-area triangulation of the rim (O(n³)
    dynamic programming); a chord that already exists in the mesh is not
    allowed, and when no fill avoids one (holes next to holes), the rim is
    fanned from a new centre vertex, whose edges are all new. Fills are
    named `fill` plus the names around the hole. Before both fixes, 107
    of 1 525 holes on a 146k-triangle sphere stayed open; after, none.
  - `fuse` (shape-bool's self-union of closed shells) is taken only when
    its result is closed. og-geom's union of two copies of a Thingi10K
    mesh can touch along edges (2 and 5 103 non-manifold edges on #128482
    and #472050); the shells are then kept apart, and the report says why.
- **Decimation is written here: QEM in f64, not meshoptimizer-rs.**
  `meshoptimizer-rs` 0.2.19 (2026-11-06) *is* pure Rust (MIT; a port of
  meshoptimizer 1.3; dependencies `libm` and optional `rayon`; SIMD only
  in its codecs, behind a feature) and it builds for wasm32. It was wired
  in first. On the closed 145 760-triangle marching-tetrahedra sphere of
  phase 0 it left non-manifold edges and self-intersections:

  | target | meshoptimizer-rs: non-manifold / self-intersecting | og-shape QEM: closed, self-intersecting |
  |---|---|---|
  | 100 000 | 1 / 0 | closed, 0 |
  | 70 000 | 5 / 2 | closed, 0 |
  | 40 000 | 3 / 2 | closed, 0 |
  | 20 000 | 1 / 0 | closed, 0 |
  | 5 000 | 0 / 0 | closed, 0 |

  Its simplifier has no link-condition check, and phase 5's acceptance
  needs closed output, so it was dropped. The decimator here: area-weighted
  plane quadrics; borders and features (surfaces, face names, named
  edges) as lines with heavy quadrics of the plane square to the face;
  line vertices slide only along their line, corners stay; a collapse goes
  to the kept end, or for two interior vertices to the cheaper of the two
  ends and the midpoint; refused if it breaks the link condition, turns a
  triangle by more than 90° or empties it, or exceeds `max_error`;
  cheapest first, ties by edge key, then vertex. Every triangle left is
  an input triangle with moved corners, so names carry through.
- **UV unwrap is written here, not a port of xatlas.** xatlas is MIT, so a
  port is allowed, but it is about ten thousand lines of C++. Ours does
  what `asset_process`'s smart project does, with guarantees:
  - charts grow from the largest triangles across shared edges while the
    normal stays within `angle` (66°, Blender's default) of the seed's;
  - a triangle joins only if its projection overlaps no triangle already
    in the chart, decided exactly (`orient2d` separating axes over a
    grid), so no chart folds;
  - each chart is projected along its seed's normal, turned to its
    smallest box (rotating calipers on its exact hull), laid wide, and
    packed tallest first by a bottom-left skyline at one texel density,
    the largest that fits found by bisection, `padding` texels apart;
  - the final UVs are checked exactly: rounding in the turn and the scale
    can push two triangles that only touched (thin fans around a vertex,
    collinear edges on CAD meshes) into an overlap of about 10⁻¹⁷; such a
    triangle leaves for its own chart and the charts are repacked (after
    three rounds both triangles of a pair leave, so it ends). Before this
    check, 12 overlapping pairs were reported on Thingi10K #42545.
- **Measures with no thresholds**, as JSON with keys in a fixed order:
  counts, bounds and size, the pivot relative to the box's base centre,
  area, signed volume, closed, open edges and their loops, non-manifold
  and inconsistently wound edges, degenerate and duplicate triangles, the
  pieces largest first (all but the first are floaters for the agent to
  judge), exact self-intersecting pairs (triangles sharing no vertex that
  meet, by `orient3d`, an x-sweep picking pairs), and for UVs the summed
  area, coverage and overlap of texel centres (exact inside tests with a
  top-left rule), exact overlapping pairs, flipped, zero-area and outside
  triangles; texture memory per image and in total.

### 9.3 FBX

There is a pure-Rust, permissive path, so FBX is in:
- `ufbx` (the crate) binds ufbx's C: excluded.
- `fbxcel` 0.9.0 and `fbxcel-dom` 0.0.10 (MIT OR Apache-2.0, pure Rust)
  parse FBX but have had no release since 2022/2023 and expose a low-level
  tree. The tree is the small part, so the reader is written here (about
  950 lines with its tests) on `miniz_oxide`, which the crate already
  uses for PNG: binary 7.x (32- and 64-bit records, every property type,
  zlib arrays) and ASCII 7.x. FBX 6 and older are `Unsupported`.
- Read: `Geometry` meshes (polygons triangulated, the first UV layer,
  per-polygon materials as surfaces), the model tree with FBX's full local
  transform `T·Roff·Rp·Rpre·R·Rpost⁻¹·Rp⁻¹·Soff·Sp·S·Sp⁻¹` (all six Euler
  orders) and the geometric transform, `GlobalSettings` axes and unit,
  `Video` textures (embedded content, or files by the resolver), and
  skin deformers as `Solid::skin` (§9.9). Skipped and listed: blend
  shapes, NURBS and other geometry.
- Checked on files not written here (three.js r160's examples, not
  committed): `Samba Dancing.fbx` (binary 7.x from Mixamo, skinned) reads
  as 2 parts, 55 320 triangles, a closed T-posed figure 1 803 mm tall and
  1 947 mm across the arms, 95 pieces, in 133 ms (its preview read with
  the skins set aside: `bench/meshtools`'s `import-check`); `stanford-bunny.fbx`
  (ASCII 7.5) as 30 338 triangles with its 4 open rims, in 119 ms, at the
  file's own scale (vertices ±250 cm under a `Lcl Scaling` of 100: 500 m
  across; three.js's example scales it down itself); `nurbs.fbx` is
  `Unsupported` (NURBS only). Their previews were inspected: both upright
  and whole. A hand-written binary file (`tests/support/fbx.rs`: pivots,
  Euler order 4, a parent, zlib arrays, two materials, an embedded PNG)
  lands where the transform formula puts it, to the µm, and is a golden
  case.

### 9.4 Dependencies

No new kernel dependency. `meshoptimizer-rs` was tried and removed
(above). `shape-golden` gains `miniz_oxide` (already in the tree) to write
its FBX case's compressed arrays. `bench/meshtools` is its own workspace,
like `bench/booleans`.

### 9.5 Acceptance (`bench/meshtools`)

Meshes from outside the kernel (`docs/plan.md` phase 5):
- the synthetic generated-mesh shapes of phase 0 (`bench/booleans/src/suites.rs`:
  sphere, rounded box, torus, blob, mug), polygonised at 96³ (82k–146k
  triangles), and the 40 Thingi10K meshes of `thingi10k.txt` (fetched by
  `fetch-thingi10k.sh`, not committed), subdivided once 1-to-4;
- each given one defect at a time: clean, triangle soup, 1 % of triangles
  removed, 2 % flipped, three floaters, and two overlapping copies written
  as two nodes (interpenetrating parts);
- each written as a GLB by og-shape's own exporter, then read back by the
  importer;
- other tools' GLB, read as they are (§9.9).

The chain per input: `import → join → repair (weld 1 µm, floaters below
5 % of the largest piece, holes up to 64 edges, fuse) → decimate (target
40 000) → unwrap → measure → preview (4 views, 256²) → glb_plain`. The
result is checked on the GLB bytes, read back: one node, one mesh, one
triangle primitive, at most 40 000 triangles, closed, with UVs (the
budget is one example, a texturing tool's input limit; "the contract"
below). It also runs with `min_edge` 1 µm. After
that, a boolean cut by phase 0's tilted cylinder re-checks §5.3's finding
that generated meshes are a repair problem.

### 9.6 A surface folded through itself (§5.4, item 3)

What repair can do:
- **Separate shells that pass through each other** (an outside model
  made of interpenetrating parts): `fuse` joins them by self-union, and
  the result is taken only if it is closed. With `min_edge`, the slivers
  the union leaves along its seams are collapsed, so f32 export does not
  merge their vertices into non-manifold edges (Thingi10K #128482 doubled:
  2 such edges without it, none with it).
- **Report it, exactly.** `measure` (and `Repair::check_intersections`)
  count pairs of triangles sharing no vertex that meet, decided with
  `orient3d`; nothing is hidden.

What it cannot do:
- **A single closed, oriented surface that folds through itself** has no
  inside to keep. It is one piece, so `fuse` returns it unchanged, and no
  step here untangles it: that needs a self-union over one operand (the
  arrangement CSG builds for two), which does not exist yet. Repair
  leaves it and reports the pairs. Measured: a 20 mm sphere with one
  vertex pulled through the far side is closed, oriented, one piece, with
  8 intersecting pairs; repair changes nothing and says 8.
- **Finding: shape-bool does not refuse it.** Phase 2's status says such a
  surface is refused; it is not. It passes `csg_points`' check (closed and
  oriented after welding), and `boolean` then returns `Ok` with open
  output: on the sphere above, a cut and a join by a 10 × 10 × 60 box are
  open, and an intersection is open with 16 intersecting pairs. That is a
  silent best effort, against `docs/design.md` §6. The fix is an exact
  self-intersection check in operand validation (the one in
  `shape-tools/src/intersect.rs` would do, moved where both crates can
  use it). It is outside this stream's modules and is left as a separate
  task.
- Self-intersections the input already has stay: on Thingi10K #62593,
  the bench's own 1-to-4 subdivision in f32 creates 14 (the raw mesh has
  none; its shortest edge is 0.4 µm at this scale), and the f32 GLB round
  trip 24. Repair does not add any.

### 9.7 Acceptance results (x86-64, release, one thread)

270 runs: 45 inputs (5 synthetic at 96³, 82k–161k triangles; 40
Thingi10K, subdivided, 4k–67k) × 6 defects; up to 322k triangles in (two
overlapping copies).

| defect | runs | contract met | closed out | cut after repair ok | self-intersecting out |
|---|---|---|---|---|---|
| clean | 45 | 45 | 45 | 45 | 1 |
| soup | 45 | 45 | 45 | 45 | 1 |
| holes (1 %) | 45 | 45 | 45 | 45 | 3 |
| flipped (2 %) | 45 | 45 | 45 | 45 | 1 |
| floaters | 45 | 45 | 45 | 45 | 1 |
| overlapping parts (two nodes) | 45 | 45 | 45 | 44 | 15 |

- **The contract is met on all 270**: one node, one mesh, one triangle
  primitive, ≤ 40 000 triangles, closed, with UVs, read back from the GLB.
- **Holes:** 13 346 filled, none left. **Flips:** 26 967 triangles turned,
  every run closed. **Soup:** welded and closed in every run.
- **Floaters:** the three specks (13.9 mm diagonal) were dropped on 23
  inputs and kept on 22, where 5 % of the largest piece's diagonal is
  below 13.9 mm. Correct for the setting, and `measure` lists them
  (`floaters`, `largest_pieces`); an agent sets `size` when it knows the
  scale. A threshold is the guide's call, not the tool's.
- **Overlapping parts:** fused into one closed solid on 44; on #472050
  the union was not closed (5 102 non-manifold edges) and the two closed
  shells were kept, with 5 835 intersecting pairs reported. 15 outputs
  self-intersect: fused shells whose union kept some (og-geom's union of
  near-coincident copies), and #472050.
- **Booleans after repair (§5 re-check):** a cut by phase 0's tilted
  cylinder gives a closed result on 269 of 270. Phase 0 had og-geom
  wrong on every hole and flip case before repair; after repair they
  pass. The one failure is #500114's overlapping copies, which fused
  closed but still self-intersect; the cut returns open output there
  without an error (§9.6's finding).
- **Self-intersections out besides those:** #62593 (24, from the input:
  §9.6, which also makes its holes run the third) and one fill each on
  torus.holes and #500114.holes, where a least-area fill crosses a
  nearby face.
- **Decimation error** (largest per collapse): at most 0.014 mm on these
  meshes (about 240 mm across), 0.0103 mm on the sphere from 146k to 40k.
- **UVs:** 113 charts on average, coverage 0.11–0.79 (mean 0.50; low on
  thin, many-chart Thingi parts), no overlapping, flipped or outside
  triangles anywhere.
- **Time**, all 270 runs: import 5.6 s, repair 89.3 s (max 4.1 s, the
  fuse of two 161k-triangle copies), decimate 33.8 s (max 3.3 s), unwrap
  32.7 s, measure 36.1 s (with exact self-intersections), preview 8.5 s,
  export 2.5 s. Larger: the blob at 414k triangles runs repair 3.2 s,
  decimate 3.6 s, the rest under 0.6 s; two copies (829k) repair in
  11.2 s (fuse) and decimate in 4.6 s.
- Previews of four runs are in `docs/previews/phase5/`.

### 9.8 Gaps and what is untested

- Outside meshes tried: synthetic defects, Thingi10K, our own GLB, and
  eight GLB and three FBX files from three.js's and Khronos's examples
  (§9.3, §9.9). Not tried: scans of millions of triangles, and other
  tools' meshes with heavy defects; the boolean re-check rests on the
  synthetic and Thingi10K runs (§9.7).
- Not read: Draco and meshopt-compressed glTF (refused with a message),
  FBX 6 and older, morph targets and blend shapes, FBX NURBS,
  the second and later UV layers, vertex colours. Imported UVs and
  textures are measured but not carried through repair and decimation:
  the chain makes new UVs.
- FBX was checked on two real files and hand-written ones only.
- Skins are read and written (`glb`) but not carried through repair,
  decimation or `glb_plain`, which refuse them (§9.9); that is part 2.
- Hole fills are least-area, not fair: a large hole on a curved surface
  closes flat. A fill can cross a nearby face (2 of 45 hole runs).
- A single surface folded through itself is left as it is (§9.6); booleans
  on it give open output silently, a shape-bool fix still to do.
- The unwrap is planar per chart (smart project), not conformal: stretch
  up to 1/cos 66° within a chart; no chart merging, so coverage on thin
  parts is low. Decimation keeps no UVs (the chain unwraps after it).
- Determinism was proven natively on x86-64 and as wasm32 under wasmtime
  locally; x86-64 Windows and aarch64 Linux run in CI.

### 9.9 Skinned inputs, and other tools' GLB

Stream G (phase 5b part 1, §11) defines `shape_mesh::Skin` (bones with a
parent, a rest `Frame` and an inverse bind matrix; per-vertex
`Weights`), and `glb` writes it. Import fills it as §11.4 asks:
- **glTF.** A node's skin: its joints become bones, ordered parents
  first (by depth in the node tree, then file order), each parented to
  its nearest jointed ancestor and named after its node (made unique);
  rest frames are the joints' world transforms in kernel space; inverse
  bind matrices are the file's, conjugated into kernel space exactly
  (identity when the file has none); `JOINTS_0`/`WEIGHTS_0` go through
  `Weights::from_raw`. `MAT4` accessors are read for this. The skinned
  mesh's own node transform is not used, as glTF says (a note is
  recorded when it is not the identity). Bone weights on a mesh whose
  node has no skin are listed as skipped.
- **FBX.** A geometry's `Skin` deformer: each `Cluster` is a bone named
  after its model, parented to its nearest bone ancestor; with `M` the
  cluster's `Transform` (the mesh at bind time) and `B` its
  `TransformLink` (the bone), the rest frame is `M⁻¹·B` and the inverse
  bind matrix its inverse, in the mesh's own coordinates; `Indexes` and
  `Weights` give the weights. A skinned mesh with a geometric transform
  or mirrored axes is refused.
- **Refused at import, with an error value:** a bone whose world
  transform is scaled or sheared (a rest `Frame` must be rigid), a
  projective inverse bind matrix, and a skin that does not pass
  `Skin::check` (an unweighted vertex, for one).
- **Refused by the tools that cannot carry a skin yet** (part 2):
  repair's steps, `join` and `apply_transform` (`RepairError::Skinned`),
  `decimate`, `lods`, `collapse_short_edges` (`DecimateError::Skinned`),
  `glb_plain` (`PlainGlbError::Skinned`, a plain GLB has no skin).
  `glb` writes the skin; `obj` and `stl` write the mesh, as on main;
  `unwrap` (changes no vertex), `measure` (reports the bones and the
  weighted vertices) and `preview` (the bind pose) take it.
- **Tests.** A hand-written skinned GLB is read with its bones, rest
  frames, inverse bind matrices and weights, refused by each tool above,
  and survives `glb` → import with the same bones, parents, rest frames,
  inverse bind matrices and weights (`tests/meshtools.rs`); golden case
  `meshtools.import.skinned`.

On real files:
- `Samba Dancing.fbx` (Mixamo, binary): two skins of 52 bones, every
  vertex weighted, `Skin::check` passes; refused by join, repair,
  decimate and `glb_plain`; `glb` → import keeps the bones.
- Soldier.glb, Xbot.glb and RobotExpressive.glb: their bones hang under
  an armature scaled 0.01 (Mixamo's centimetres) or 100, so no bone's
  world transform is rigid. They are accepted by taking the scale out
  (below).

**Taking a uniform armature scale out of the bones** (the owner's
decision on the point §11.4 left open):
- When every bone's world transform is a rotation times one uniform
  scale (to `UNEVEN` = 10⁻⁴ relative: exporters write scales like
  (1, 1.0000018, 1.0000156), as RobotExpressive's `UpperLeg.L`), the
  scale comes out. An uneven or sheared scale, or bones scaled
  differently, is still refused with an error value.
- The bind pose is the file's inverse bind matrices, not its node
  transforms, since a file can be saved posed. `bind` is
  `world · inverse bind` of the bone that weights the most vertices: where
  the file puts the mesh. Each bone's rest frame is `bind · IBM⁻¹`, its
  scale divided out and its axes made exactly orthonormal; its inverse
  bind matrix is recomputed as that frame's inverse; each vertex goes to
  `bind · v`, so it stays where the file's bind pose puts it in the world.
- Reported per part in `Imported::skin_scales` (`SkinScale`): the scale;
  `bind_error`, how far the file's inverse bind matrices with the scale
  factored back in put a vertex from the recomputed ones (they agree to
  rounding); `uneven`, the largest departure from a uniform scale that
  was rounded away; `node_offset`, how far the file's nodes stand from
  the bind pose at the vertices their bones weight (0 when the file is
  saved at its bind pose).
- FBX does the same with its clusters (`TransformLink` scaled
  uniformly): the vertices go where the clusters' mesh bind transforms
  put them.
- A file already rigid keeps the earlier reading: rest frames from its
  nodes, inverse bind matrices as given (no hash moved).

Checked (`bench/meshtools`, `skin-check`; the files are fetched, not
committed: three.js r160's `examples/models/gltf/Soldier.glb` (2.2 MB),
`Xbot.glb` (2.9 MB) and `RobotExpressive/RobotExpressive.glb` (0.46 MB)
from raw.githubusercontent.com/mrdoob/three.js). Posing turns the bone
with the most bones below it by 0.5 rad about its own +X and compares
with the file's bind pose turned the same way, computed from its own
node and inverse bind matrices by the independent `gltf` crate:

| file, part | bones | scale out | IBMs agree to | nodes off the bind pose | our pose vs the file's |
|---|---|---|---|---|---|
| Soldier, vanguard_Mesh | 49 | 0.01 | 4.1·10⁻⁵ mm | 6.6·10⁻⁴ mm | 2.8·10⁻⁴ mm (Spine, moves 335 mm) |
| Soldier, vanguard_visor | 2 | 0.01 | 8.6·10⁻⁶ mm | 0 | 2.0·10⁻⁴ mm |
| Xbot, Beta_Joints / Beta_Surface | 67 | 0.01 | 1.2·10⁻⁵ mm | 1.2·10⁻³ mm | 1.3·10⁻¹¹ mm (Spine, moves 329 mm) |
| RobotExpressive, Hand.R / Hand.L | 43 | 100 | 6.0·10⁻⁴ mm | 514 and 572 mm (saved posed) | 7.1·10⁻⁴ mm (Body, moves 731 mm) |

All four files also go through the chain: refused by join, repair,
decimate and `glb_plain`, and `glb` → import gives the same bones. The
hand-written test (`tests/meshtools.rs`) puts the legs under a 0.01
armature: the import equals the unscaled file's to f32, posing the knee
matches the file's own matrices to 10⁻³ mm, the `glb` round trip keeps
the bones and weights, a file saved with the knee turned gives the same
bind pose and reports the offset (380.8 mm), and a (0.01, 0.02, 0.01)
armature is refused. Golden case `meshtools.import.skinned.scaled`.

Other tools' files through the chain (`bench/meshtools --glb`; three.js
r160's examples and Khronos's DamagedHelmet, not committed):

| file | triangles | result |
|---|---|---|
| Horse, Flamingo, Parrot | 984, 542, 626 | budget met; morph targets reported as skipped; Flamingo and Parrot keep 21 and 36 intersecting pairs of their own |
| ShaderBall | 88 264, 4 pieces | decimated to 40 000 (error 0.76 mm), budget met |
| DamagedHelmet | 15 452, 55 pieces | not closed: open shells with 12 rims over 64 edges stay open (646 open edges), reported; the fuse is refused (its pieces are open) |
| RobotExpressive, Soldier, Xbot | 3 237, 11 376, 49 112 | skinned, scale taken out (above); refused by join, repair, decimate and `glb_plain`; `glb` round trip keeps the bones |

## 10. The generator gaps of §6.4 (stream F, 2026-11-09)

Read at sand3d `84c980a` (Apache-2.0): `src/sand3d/solids.py` (`revolve`,
`wire`, `_filleted`, `loft`, `_through`, `body`, `_view`, the
primitives), `src/sand3d/parts/gear.py`, `src/sand3d/parts/turned.py`
(`knurl`, `_lathe`) and `_core.py` (`Builder.finish`, the edge-naming
rule). All new code is in `shape-gen`: `loft.rs`, `spline.rs`, `body.rs`,
`wire.rs`, `parts.rs`, and the axis in `solids.rs`. No dependency was
added. Golden cases: `gen2.loft.holes`, `gen2.loft.spline`,
`gen2.axis.z`, `gen2.body`, `gen2.wire`, `gen2.knurl`, `gen2.gear`; every
earlier hash is unchanged, natively and under wasmtime. Every output is
closed, with no degenerate triangle, and canonical. Previews:
`docs/previews/gen2/`.

### 10.1 Decisions

- **Desc fields, not new signatures.** `Loft` gains `curve`, `Revolve`
  gains `axis`; both default to the old behaviour, so the old hashes stay.
  `sphere`, `torus` and `dome` take bare numbers, so they gain desc
  siblings instead (`sphere_desc`, `torus_desc`, `dome_desc`); the old
  functions are unchanged. At the merge the IDL can expose only the desc
  forms.
- **Axis.** `Axis::Z` builds about +y as before and turns the result by
  `(x, y, z) → (x, −z, y)`, a negation and a swap, so it is exact and the
  volume is the +y solid's to rounding of the sum. The round primitives
  then rest on z = 0 as sand3d's do (sphere centre at `r`, torus at
  `tube_radius`). `DomeDesc::height` is sand3d's `dome(d, h)` spherical
  cap; above `radius` it is more than half a sphere.
- **Loft through holes.** Each section is one region; every section has
  as many holes, else `GenError::Invalid`. Each hole slot is matched to
  the next section's hole with the nearest centroid, greedily in slot
  order (deterministic, ties to the lower index). Every contour is
  resampled as the outline is (evenly by length from its largest-x
  point), to `samples` or the largest count that slot has. Hole walls
  are `side` + `inner`, one surface per hole; caps are regions with the
  resampled holes. sand3d refuses holes in a loft section (it lofts the
  outline and cuts the bore); the rebuilds' gap was exactly that.
- **Loft curves.** `Spline` is sand3d's `_through`: a natural cubic
  spline per point and coordinate, parameter = section height (sand3d's
  chord length between origins, which for sections stacked on z is the
  height). z runs linearly, so every ring stays flat. With `steps` 0 it
  takes sand3d's step rule (one step for two sections, else
  `max(2, round(6 · span / mean span))`). sand3d's corner-to-corner
  correspondence (`_correspond`, matching shifts and directions) is not
  ported: sections still start at their largest-x point.
- **Body.** As sand3d's `body`: views are numbers or `(y, value)` points,
  straight or smooth; sections at every view point, plus 48
  cosine-spaced ones when a view is smooth or `n` varies; a step (two
  points at one y) is two sections 0.05 mm apart; an end where both views
  close is a point, any other end a flat cap; one view closing alone is an
  error. A smooth view here is a natural cubic spline in y between steps,
  not sand3d's 2D `spline` path read along y, so it cannot overshoot back
  along y (sand3d's `SPLINE_BACK` tolerance is not needed), but a nose
  whose width has a vertical tangent is only approximated. Every section
  has one point count, from the widest by the chord rule.
- **Wire.** sand3d's `_filleted`: duplicate points merge keeping the
  larger radius; a bend's setback is `r tan(θ/2)`; a radius under the
  wire's own, a turn of 180°, or setbacks longer than a side are
  `GenError::Invalid`. Unlike sand3d, a corner with no fillet is mitred:
  the ring lies in the plane normal to `u + v` and each point moves along
  the run onto it, which keeps the circle's area times the centre line's
  length exactly (checked to 1e-9). The runs meeting at a mitre are
  separate surfaces, so the mitre ring is a real edge, named `seam`
  (sand3d's kind for swept solids). Rings are carried from run to run by
  two reflections (normal `u + v`, then `v`), the discrete
  rotation-minimizing frame; a closed wire spreads the angle it gains
  round the loop over its length (`atan2` from `trig`). The cap rims are
  `start` and `end`.
- **Knurl.** sand3d's `_lathe` and V-wave radius, from the same counts
  (`π d / pitch` teeth, at least 6; four points per tooth round; four
  rows per 30° helix lead, or one row per pitch for straight ridges).
  sand3d splits every grid quad along one diagonal, so one helix's
  creases cross the triangles and the diamonds shade as stripes; here
  each quad is split along the diagonal whose midpoint radius is nearer
  the radius at the quad's centre. Straight ridges tie and keep the
  default diagonal. No boolean is needed.
- **Gear solid.** `gear_solid` extrudes the `shape-2d` involute outline
  with a `bore` and a `hub` (diameters, as sand3d's `bore`). No boolean:
  every cap is a `Region` whose hole is the exact ring the walls use, so
  caps and walls share points bit for bit. The bore must be narrower than
  the hub (or the root circle), the hub narrower than the root circle.
  Faces: `bottom`, `top`, `side`; the hub's wall `side` + `hub`, its top
  `top` + `hub`; the bore `side` + `inner` + `bore`. Hub rims carry `top`
  + `hub`.
- **Errors.** `GenError::Invalid(String)` is new, for inputs that cannot
  make a solid, with the place and the reason in sand3d's words.

### 10.2 Measurements

| case | measured | analytic | note |
|---|---|---|---|
| loft, square frame 20/10 to itself, 16 samples | 3 000 | 3 000 | exact (corners on samples) |
| loft, frame 20/10 to 10/4, 6 high | prismatoid | `h(a₀² + a₀a₁ + a₁²)/3` per square | within 1e-9 |
| spline loft through four squares of half-width `5 + z/10` | frustum | 2 906.67 | relative 1e-12: a natural spline of linear data is that line |
| gear 20 × m2, width 8, bore 6, hub 20 × 5 | polygon areas × heights | same | within 1e-9 |
| straight knurl d 10, h 6, pitch 1 | star polygon area × 6 | same | within 1e-9 |
| diamond knurl d 16, h 12 | 2 218.07 | between 2 120.58 (root) and 2 412.74 (crest) | 11 596 triangles |
| mitred wire, three runs, d 6 | 24-gon area × centre length | same | within 1e-9 |
| sand3d's stool leg, d 33, fillet 60 | 744 787.29 | 744 819.54 (polygon area × centre length) | −4.3e-5: the bend's chords |
| body, constant views | 24-gon ellipse area × 100 | same | within 1e-9 |
| body, linear views from a point | ellipse polygon × L / 3 | same | within 1e-9 |
| dome cap (4, 2), (3, 3), (3, 5) | −3.7 %, −2.8 %, −2.6 % | `π h²(3R − h)/3` | the chord rule's 24-a-turn floor on small radii |

### 10.3 Gaps and what is untested

- Loft: no `follow` or `loop` (sand3d's), no placed (non-z) sections, no
  corner-to-corner correspondence; holes that swap places between sections
  are matched by centre only. Sections with more than one region are
  refused.
- Body: the smooth views are splines in y, not sand3d's 2D splines; no
  per-section frame (it always runs along +y; turn it with the placement
  layer, stream E).
- Wire: a mitre at a very sharp turn on short runs can fold the tube
  through itself; it is not detected. Paths given as splines, arcs or
  helices (sand3d accepts them) are points here: flatten them first
  (`helix` gives points).
- Knurl: the grid is sand3d's; the diamond tips are sampled, not exact.
  No knurl on a non-cylindrical surface.
- Gear: the hub is on the top face only, and round; no keyway, no hub on
  both sides. Teeth are `shape-2d`'s outline (8 flank points), not
  sand3d's (6).
- Every new hash is checked natively (x86-64) and as wasm32 under
  wasmtime here; aarch64 and Windows run in CI only.
## 11. Phase 5b part 1: skins from the program (`shape-skin`, 2026-11-09)

### 11.1 Decisions

- **Data model.** `Solid.skin: Option<Skin>`. A `Bone` has a name, a
  parent index (parents first), a rest `Frame` (its pivot, rigid) and an
  inverse bind matrix (4×4, column-major, kernel units). The two are kept
  apart because an imported skin's inverse bind matrix need not be its
  node's inverse (glTF allows it). `Weights` holds four `(bone, f64)`
  pairs in canonical form (largest first, ties to the lower bone, unused
  `(0, 0)`, sum 1). It is plain data, so interpolation is
  `Weights::blend` (sum per bone, keep four, normalize): what welding uses
  now and decimation will use in part 2.
- **Welding rule.** When `canonicalize` welds vertices at one position,
  the vertex takes the mean of the copies' weights. The copies are sorted
  first (`Weights::cmp_total`), so the bits do not depend on build order.
  Copies with no weights do not count. Equal copies, the common case,
  pass through unchanged.
- **Refuse, never drop.** `boolean`, `fuse`, `split`, `hole`, `pocket`,
  `seat`, `shell`, `dent`, `fillet`, `chamfer` and `soften` return a
  `Skinned` error for a skinned operand. `components` carries the skin.
  `append` and `merge` keep it when the skeletons are equal and otherwise
  leave the new vertices unweighted, which `Skin::check` reports.
  `map_positions` moves weights with their vertices but not bones (its map
  need not be rigid), so a program places a solid before skinning it.
  `obj` and `stl` hold meshes only, as before; `preview` draws what it is
  given (pose first).
- **Regions.** A bone's `regions` are face names. A face belongs to a
  bone if it carries `r` or `r.*` (a named boolean's qualified tags). A
  face with no bone, or with two, is an error value that lists its names.
  So is a region no face carries. The usual program tags each part
  (`tag_all_faces`) before joining them, so the seam is the boolean's cut
  curve.
- **Distance.** Dijkstra over the mesh's edges (edge lengths), multi-
  source from a bone's faces, cut off at `blend / 2`. The heap orders by
  distance and then vertex index, and the adjacency is sorted, so the bits
  are the same everywhere. Graph distance is an upper bound on geodesic
  distance: about 8 % long across a regular triangulation, exact along
  edges. A smoother distance (heat method, or exact geodesics after
  Mitchell–Mount–Papadimitriou) needs a linear solve or much more code,
  and the bands here are refined finely, so the graph metric is enough for
  part 1.
- **Blend.** A bone's raw weight at distance `d` is
  `1 − smoothstep(2d / blend)` within the band and 1 on its own faces. The
  raw weights are normalized. Across a two-bone seam this is
  `f/(1+f) → ½ → 1/(1+f)`, with equal slopes on both sides, so it is C¹.
  `blend = 0` gives hard seams: a seam vertex is ½–½.
- **Refinement.** In a band, rigid long triangles cannot bend: a cylinder
  side from `extrude` has one triangle from end to end. So edges longer
  than `max_edge` (default `blend / 4`) with an end inside a band are
  split at their midpoints, round after round. A triangle with two split
  edges cuts its remaining quad along the shorter diagonal. The surface
  does not move, and names and surfaces follow the pieces.
- **Posing: linear blend skinning.** For each bone, the skinning matrix
  is `posed world · inverse bind`, where `posed world = parent posed world
  · parent rest⁻¹ · rest · turn`. A `Turn` is an axis in the bone's rest
  axes and an angle in radians, the kernel's units. Dual quaternions fix
  the "candy wrapper" collapse of a joint twisted about its own axis. The
  figure's joints bend about hinges. With a blend of about four limb radii,
  the 90° bends invert no triangle except slivers (§11.3), so dual
  quaternions are not needed yet. They would change the GLB too: glTF
  skins are linear, and three.js poses them linearly.
- **GLB.** A skinned part (no joint) gets a glTF skin.
  - Its bones are nodes under the part's node, each at its rest frame
    relative to its parent bone, in glTF space.
  - Inverse bind matrices are `C · IBM · C⁻¹`, with `C(p) = y_up(p) ·
    0.001`.
  - Per vertex: `JOINTS_0` as u16 and `WEIGHTS_0` as f32, the largest
    weight set to one minus the others so the f32 sum is 1.
  - Vertices split at real edges take their weights from one source
    vertex, so the exported mesh cannot tear along a crease.
  - A skinned part hung on a joint is refused: glTF ignores a skinned
    node's transform, but three.js binds with it, so the two would
    disagree.
- **Determinism.** Only `+ - * /` and `sqrt` are used; `sin_cos` comes
  from `trig` (turns) and `acos` from `trig` (the measure). The data
  structures are `BTreeMap`, `BTreeSet` and a `BinaryHeap` with a total
  order. The golden cases `skin.weld`, `skin.weights`, `skin.pose` and
  `skin.glb` are the same natively and on wasm32 under wasmtime.

### 11.2 Measure

`measure_pose` reports, with no thresholds:
- closed and non-manifold edges of the posed mesh;
- triangles whose posed normal is more than 90° from their rest normal
  as the skin moves it (each corner's blend of bone matrices, averaged):
  their count, their total rest area, and the best shape among them
  (twice the area over the longest edge squared: 0.866 for equilateral,
  near 0 for a sliver);
- collapsed triangles;
- the range of posed-to-rest area ratios, and the volume ratio.

A thin sliver can flip under any non-rigid map: its middle vertex moves
off the line by more than the sliver is wide. The shape figure separates
those slivers from real folds.

### 11.3 Measurements (x86-64, release)

An arm: an upper cylinder (r 10, z 0–60) joined to a forearm (r 9,
z 55–120), the elbow pivot at z 57.5, bent 90°:

| blend | triangles | inverted | their area mm² | best shape | min area ratio | volume ratio |
|---|---|---|---|---|---|---|
| 10 | 5 790 | 320 | 87.8 | 0.711 | 0.026 | 0.986 |
| 20 | 4 312 | 15 | 15.6 | 0.463 | 0.139 | 0.970 |
| 30 | 4 082 | 113 | 36.6 | 0.016 | 0.160 | 0.956 |
| 40 | 2 900 | 0 | 0 | — | 0.369 | 0.937 |
| 60 | 2 652 | 96 | 116.2 | 0.016 | 0.165 | 0.909 |

- At 10 and 20 mm (one and two radii), well-shaped triangles invert: the
  inner crease folds.
- At 30 and 60 mm, only slivers invert (best shape 0.016). They come from
  the boolean's seam: the upper arm's cap diagonals cross the forearm's
  circle 0.02 mm from its vertices, which leaves strips 0.04 mm wide
  along the forearm.
- At 40 mm (four radii), nothing inverts at 45°, 90° or 120°.
- Guide note for later: blend about four limb radii.

The acceptance figure (`crates/shape-skin/tests/figure/mod.rs`):
- Built from a lofted torso, a sphere head, swept limbs, sphere hands and
  cuboid feet: 6 124 triangles, one closed shell.
- Skinned with ten bones and a blend of 24 mm (limb radii 5–8): 26 150
  triangles, 13 077 vertices.
- Skinning it and measuring one pose take about 0.2 s.

| pose | closed | inverted | their area mm² | best shape | max turn | area ratio | volume ratio |
|---|---|---|---|---|---|---|---|
| rest | yes | 0 | 0 | — | 20.8° | 1.000–2.63 | 1.000 |
| bend: left elbow −90° about x, right knee +90° | yes | 11 | 1.58 | 0.023 | 165.7° | 0.179–9.56 | 0.989 |
| wave: the bend, plus right arm raised 100°, a step, the head turned 25° | yes | 45 | 9.25 | 0.122 | 167.2° | 0.179–52.0 | 1.030 |

- **Rest.** The 20.8° and the 2.63 area ratio at rest are rounding on
  boolean slivers of near-zero area. The skinning matrices are exactly
  the identity.
- **Bend.** The 11 inverted triangles are slivers (best shape 0.023,
  1.6 mm² in all, out of 24 376 mm² of surface). No triangle of
  better shape turns past 90°.
- **Wave.** The 100° shoulder raise also folds a few triangles in the
  armpit (best shape 0.12).
- **GLB.** About 1.0 MB. It is not committed: the example writes it on
  request.
- **Read back with the `gltf` crate** and skinned from the file's own
  nodes, inverse bind matrices, `JOINTS_0` and `WEIGHTS_0`, it matches the
  kernel's pose to under 5 µm for all three poses. No two copies of a
  vertex separate.
- **three.js r170** (`GLTFLoader.parse` under Node 22, headless) loads
  one `SkinnedMesh` with the ten bones. Posed like the bend through the
  bones' quaternions, then skinned with `SkinnedMesh.applyBoneTransform`:
  - 14 581 vertices, 0 torn copies;
  - the 1 368 vertices fully on a turned bone are within 7.8e-10 m of
    the rigid turn about their pivot.

  The script is not in the repository: the gate has no Node, and `three`
  would be an npm dependency. It was run by hand.

Previews: `docs/previews/phase5b/rest.png`, `bend.png`, `wave.png`.

### 11.4 For part 2 (imported skins)

- Import, from stream D's glTF reader:
  - `JOINTS_0` and `WEIGHTS_0` into `Weights::from_raw`, which drops
    zeros and renormalizes;
  - skin joints into `Bone`s, ordered parents first and with world rest
    frames from the node hierarchy;
  - inverse bind matrices as given.

  Non-rigid bone nodes (scale) do not fit `Frame` and need a decision.
  The mesh's node transform must be applied or refused.
- Repair: welding already carries weights (`canonicalize`). Floater and
  degenerate removal drop vertices, so remap the weights with them.
- Decimation: each collapse sets the new vertex's weights with
  `Weights::blend` of the two ends (by the collapse parameter). Keeping
  bone-influence borders means a cost term or a lock on edges whose ends
  have different main bones.
- Unwrap: seams split vertices, so the copies take their source's
  weights, as the GLB export already does through `split`.
- Every stream D tool that does not carry skins yet must refuse a skinned
  input with an error value, as the booleans do.

### 11.5 Part 2: imported skins through the mesh tools (stream D, 2026-11-09)

Decisions:

- **Weights follow vertices.** Every repair step that drops or merges
  vertices remaps the skin with them (`remap_skin`). Vertices merged into
  one take the mean of their weights (`Skin::weld`, as `canonicalize`
  does). A hole filled around a new centre vertex gives it the even blend
  of its rim's weights. `collapse_short_edges` blends as decimation does.
- **Rigid placement only.** `apply_transform` bakes a rigid transform
  into the vertices, the bones' rest frames and their inverse bind
  matrices (`IBM' = IBM · M⁻¹`, so rest · IBM stays the identity). A
  scaled or mirroring transform on a skinned part is
  `RepairError::Skinned`: scale does not fit `Frame` (§11.4).
- **Join merges skeletons by bone name.** Bones of one name must agree on
  their parent's name, their rest frame (1e-6 mm, scaled by the
  coordinate's size, and 1e-9 on the axes) and their inverse bind matrix
  (1e-6, scaled likewise). Otherwise the error names every bone that
  disagrees. Skinned and unskinned parts are not joined.
- **Decimation blends by where the vertex lands.** A collapse onto the
  kept vertex keeps its weights; one to the midpoint takes half of each
  (`Weights::blend`, in `cmp_total` order so the result does not depend
  on which end is which).
- **Bone borders by a penalty, not a ban.** Removing vertex `v` costs, on
  top of its quadric error, the quadric's area weight times
  `(Δ · 0.005 · diagonal)²`, where `Δ` is the largest difference, on any
  bone, between `v`'s weights and any neighbour's. In other words, a
  change in weights counts as moving the surface by `Δ` × 0.5 % of the
  bounding-box diagonal. A vertex inside a region weighted to one bone
  costs nothing extra; the rows across a joint cost the most. The
  reported `error` leaves the penalty out.
  `Decimate::cross_bone_borders` drops it.
  - Banning collapses across a change of dominant bone (the first
    attempt) left 3.3 mm and 21 inverted triangles on the tube below. It
    also cannot meet a triangle target on a mesh weighted everywhere
    (Soldier: 198 mm rest error when forced to half).
  - A penalty on the step between the edge's two ends only, rather than
    across `v`'s whole ring, still left 0.84 mm on the tube.
  - Scale swept over 0.005, 0.01, 0.02, 0.05 and 0.1 of the diagonal. All
    keep the tube exact. The rest error at half grows with the scale:
    Soldier 13.0, 17.9, 21.3, 24.8, 33.9 mm; RobotExpressive's hands 59.0,
    69.3, 127.5, 128.4, 163.8 mm. The smallest, 0.005, is the default.
- **Unwrap** changes no vertex, so weights are untouched. The GLB writer
  splits vertices at UV seams and copies the weights (`split`).
- **`glb_plain` still refuses skins:** it promises one mesh with no skin.

Acceptance:

- The skinned tube (`tests/meshtools.rs`): a cylinder r 10 mm and 100 mm
  tall, rings every 2 mm, with `root` and `knee` blending over
  z ∈ [40, 60]. It is written to a GLB, read back, repaired, decimated to
  half and bent 0.8 rad at the knee.

  | borders | triangles | posed Hausdorff to the full mesh | inverted |
  |---|---|---|---|
  | kept | 3264 → 1632 | 0.0000 mm | 0 |
  | crossed | 3264 → 1632 | 5.806 mm | 41 |

  With borders kept, the test asserts the same bones, `Skin::check`, no
  stray weights, at most half the triangles (+2), under 0.5 mm and none
  inverted.
- Other tools' files, `bench/meshtools` `skin-decimate` (three.js r160
  example models). Each skinned part is decimated to half, then the bone
  with the most descendants is turned 0.5 rad about X:
  - **Stray weights:** half-mesh vertices weighted to a bone that weights
    no full-mesh vertex within the half mesh's longest edge.
  - **Skin drift:** for vertices whose rest position the decimation kept
    exactly, how far the posed half and the posed full mesh put them
    apart. It isolates the weights from the geometric error.

  | part | triangles | rest Hausdorff, kept / crossed | posed Hausdorff, kept | skin drift | inverted (full) |
  |---|---|---|---|---|---|
  | Soldier vanguard_Mesh | 11186 → 5592 | 12.95 / 9.90 mm | 12.95 mm | 0.000 mm | 1 (1) |
  | Soldier vanguard_visor | 190 → 94 | 2.66 / 2.66 mm | 2.64 mm | 0.000 mm | 0 (0) |
  | Xbot Beta_Joints | 20840 → 10420 | 1.71 / 1.71 mm | 1.71 mm | 0.000 mm | 0 (0) |
  | Xbot Beta_Surface | 28272 → 14136 | 2.46 / 2.46 mm | 2.46 mm | 0.000 mm | 0 (0) |
  | RobotExpressive Hand.L/R | 272 → 136 | 59.0 / 58.7 mm | 59.0 mm | 0.000 mm | 0 (0) |

  Every part keeps its bones, its weights sum to 1, and none has stray
  weights. The posed error equals the rest error: posing adds nothing to
  what decimation already moved.
- The cost of keeping borders: no triangles, since every target was met
  (Soldier, borders kept, ends one under it: 5592 for 5593). The cost is in
  rest accuracy at the same count: +3.05 mm on Soldier (31 %), +0.33 mm
  on the Robot's hands, nothing on Xbot. The tube's 0.8 rad bend (5.8 mm
  and 41 inverted triangles when crossed) is what it buys.
  Measured threshold: posed Hausdorff to the posed full mesh at most the
  rest Hausdorff (plus 0.02 mm), and skin drift 0.000 mm.
- Previews (Soldier, at rest and with the spine turned, full and half):
  `docs/previews/phase5b/soldier-{full,half}-{rest,posed}.png`.
- Golden: `meshtools.skin.repair` (a cut cap and a seam-split ring,
  welded, filled and short edges collapsed), `meshtools.skin.decimate`
  (half with borders kept and crossed, LODs 0.5/0.25/0.1),
  `meshtools.skin.join` (a rigid quarter turn baked, two parts merged,
  parts placed apart refused).

Gaps: a weight-aware quadric (weights as extra quadric dimensions,
Hoppe 1999) would replace the penalty's scale with a principled one. It
is untested here. Non-rigid placement of a skinned part stays refused.

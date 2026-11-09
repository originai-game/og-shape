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
   output, interpenetrating provider parts) before any boolean. A single
   surface that folds through itself is still refused.
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

## 9. Phase 5: mesh tools for the platform (stream D, 2026-11-09)

### 9.1 What the platform's `asset_process` does, and where it is now

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
| export | `glb` (parts, joints) and `glb_plain` (the texturing contract) |

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

  Its simplifier has no link-condition check, and the texturing contract
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
  `Video` textures (embedded content, or files by the resolver). Skipped
  and listed: deformers (skins, blend shapes: meshes stay in their bind
  pose), NURBS and other geometry.
- Checked on files not written here (three.js r160's examples, not
  committed): `Samba Dancing.fbx` (binary 7.x from Mixamo, skinned) reads
  as 2 parts, 55 320 triangles, a closed T-posed figure 1 803 mm tall and
  1 947 mm across the arms, 95 pieces, in 133 ms; `stanford-bunny.fbx`
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

**No provider output was available.** The inputs stand in for them:
- the synthetic AI-mesh shapes of phase 0 (`bench/booleans/src/suites.rs`:
  sphere, rounded box, torus, blob, mug), polygonised at 96³ (82k–146k
  triangles), and the 40 Thingi10K meshes of `thingi10k.txt` (fetched by
  `fetch-thingi10k.sh`, not committed), subdivided once 1-to-4;
- each given one defect at a time: clean, triangle soup, 1 % of triangles
  removed, 2 % flipped, three floaters, and two overlapping copies written
  as two nodes (interpenetrating parts);
- each written as a GLB by og-shape's own exporter, then read back by the
  importer: a GLB we export ourselves, not one a provider wrote.

The chain per input: `import → join → repair (weld 1 µm, floaters below
5 % of the largest piece, holes up to 64 edges, fuse) → decimate (target
40 000) → unwrap → measure → preview (4 views, 256²) → glb_plain`. The
contract is checked on the GLB bytes, read back: one node, one mesh, one
triangle primitive, at most 40 000 triangles, closed, with UVs. After
that, a boolean cut by phase 0's tilted cylinder re-checks §5.3's finding
that AI meshes are a repair problem.

### 9.6 A surface folded through itself (§5.4, item 3)

What repair can do:
- **Separate shells that pass through each other** (a provider's model
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

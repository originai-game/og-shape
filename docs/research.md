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

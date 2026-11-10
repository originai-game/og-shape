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

After §5.4 item 3 (stream J), two engines over the kernel's f64 entry
`csg_points` were added to the bench:
- **og-shape-checked**: it refuses an operand with a shell that crosses
  itself (`check_operands`), joins separate shells by self-union, and
  refuses an open result;
- **og-shape-untangled**: it self-unions each operand first.

An engine that changed its operands is held to the identities on the
volumes of the operands it used. og-geom's own verdicts are unchanged.

| synthetic (40) | ok | refused | wrong |
|---|---|---|---|
| og-geom (before) | 19 | 0 | 21 |
| og-shape-checked | 24 | 16 | 0 |
| og-shape-untangled | 29 | 11 | 0 |

| defect | og-geom | og-shape-checked | og-shape-untangled |
|---|---|---|---|
| clean, floaters, soup | ok | ok | ok |
| degenerate | 4 ok, 1 wrong | 4 ok, 1 refused | 4 ok, 1 refused |
| noise | wrong | refused (3 700–7 400 pairs) | **ok** |
| holes, flipped | wrong | refused (open) | refused (open) |
| self-intersecting | wrong | **ok** | **ok** |

Cost of the check (og-shape-checked on the scaling spheres, both
operands checked for each of the three operations; `operands` includes
the bench's exact weld; two runs, x86-64, release):

| spheres | og-geom, 3 ops | og-shape-checked, 3 ops | of which operands | check / rest |
|---|---|---|---|---|
| 4 096² | 42 ms | 90 ms | 25 ms | 38 % |
| 16 384² | 177–220 ms | 361–405 ms | 104–110 ms | 35–40 % |
| 65 536² | 749–833 ms | 1 322–1 497 ms | 404–419 ms | 37–44 % |

A timing of the check on its own, a 16 128-triangle sphere, gives about
9 ms of broad phase plus 9 ms of pair tests per operand, against 55–65 ms
for the union. That is about 55 % for both operands, over the 25 % the
task allowed, so the check stays opt-in (`check_operands`). It also has
to stay opt-in for the ulp finding above.

The refused degenerate case is blob, whose rounded vertices make it cross
itself (10 pairs) and leave it non-manifold after welding.

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
   **Done** (stream J), except that the up-front refusal is not on by
   default in `boolean` (below).
   - **One exact check** (`shape_bool::selfcheck`, moved from
     `shape-tools/src/intersect.rs`, which now calls it). Two triangles
     cross improperly when they share no vertex and meet, share one vertex
     and meet anywhere else, share an edge and fold flat onto each other,
     or share all three corners. Every side is `orient3d`/`orient2d`.
     `crossing_pairs(&Solid)` counts such pairs within one shell, after
     exact welding (separate shells crossing each other are `fuse`'s
     job). The broad phase is a uniform grid of cells a mean triangle
     across, each pair found in one cell only.
   - **Refusal.** `BoolError::SelfIntersecting { operand, pairs }`.
     `check_operands(target, tool)` returns it before any work.
     `boolean` returns it instead of `OpenResult` when the result comes
     out open and an operand crosses itself. The folded sphere of §9.6 is
     refused either way.
   - **Finding: the kernel's own outputs cross themselves by an ulp.**
     With the check on by default, `edit.holes.pocket` moved: after the
     countersink hole, the plate has 9 crossing pairs, all along the
     countersink's rim, where crossing points rounded to z =
     7.999999999999998, 7.999999999999999 and 8.0. These are real
     crossings by the exact predicates. The pocket's boolean on that plate
     comes out closed. Refusing it would break the edit tools' own chains,
     so the up-front check is opt-in (`check_operands`) and every golden
     hash is unchanged. Whether to turn it on for imported meshes only
     (the CLI, MCP) is left to the integrator.
   - **Another implementation, reviewed and not merged.** Branch
     `claude/stoic-brown-bhuout` (e3b7189, on an older main) refused such
     operands inside `csg_points`, always on. Its check lived in
     `shape_mesh::self_intersections`, and pairs with vertices within
     1e-9 of the operand's size counted as sharing a vertex, which kept
     `edit.holes.pocket` unchanged. Stream J keeps one implementation,
     this one, for four reasons:
     - it also catches improper crossings between triangles that share a
       vertex or an edge;
     - it needs no distance threshold;
     - `self_union` uses it too;
     - it does not add the check's cost to every boolean.
     
     Taken from that branch: its regression case, a sphere's pole pulled
     through, refused as either operand
     (`a_pole_pulled_through_is_refused_as_either_operand`).
     
     Measured on this branch: with that 1e-9 rule added to
     `crossing_pairs` and `check_operands` called first in `boolean`, no
     pre-existing golden hash moves. Refusing by default is therefore
     possible. It would cost 35–55 % of each boolean and need an opt-out
     field on `Boolean`, whose struct literals sit in other crates; the
     integrator decides.
   - **Self-union** (`self_union(&Solid)`, `csg::self_union_points`): the
     arrangement CSG builds for two operands, built for one. Pairs whose
     boxes overlap are intersected: those sharing no vertex, and those
     sharing a vertex or an edge that cross improperly. Pieces are cut,
     grouped into regions, and each region is classified by the mesh's own
     winding number. What bounds the space of winding ≥ 1 is kept, facing
     out of it.
     - **Triple points.** Two operands never need them, but one surface
       can be crossed by two triangles that also cross each other. Their
       two seams inside a third triangle then cross at a point the
       pairwise step does not make. spade dropped one constraint and
       regions leaked: the bench's noise defect untangled open. Each such
       point is now made once, named by its three triangles and placed by
       their three planes (Cramer's rule, fixed order), and all three
       triangles split their seams there.
     - **Classification.** A region not lying on another face is decided
       by the winding number of every *other* triangle at its largest
       piece's centre, a half-integer, so a sliver needs no step off its
       surface. Faces lying on one another use the two-probe test, and
       the piece of the lowest-numbered triangle stands for them (two
       copies of a plate give one plate).
     - Operands may use a directed edge more than once if its reverse is
       used as often (coincident copies).
     - Names are carried as `fuse` carries them: faces keep their names
       and surfaces, and seams are named `cut`.
   - **`fuse`** first untangles each shell that crosses itself; if that
     fails, it joins the shell as before. **`repair`'s fuse step**
     untangles a single closed shell that crosses itself. One condition in
     `repair.rs`; open shells are left alone, as before.
   - **Cost of the check:** 35–55 % of a boolean at 16k triangles a side
     (table in §5.2). It is opt-in.
   - **Findings left open:**
     - The untangled folded sphere is closed, manifold and crossing-free,
       but booleans on it with the centred 10 × 10 × 60 bar come out open
       (140 edges, refused as `OpenResult`). They come out closed with the
       bar moved by 0.3 mm. The open edges lie where the bar's faces meet
       the fold's cut pieces, slivers down to 7·10⁻¹⁶ mm². This is the
       two-operand CSG on sliver-laden input (follow-up 4's kind), not the
       self-union.
     - Untangling is slow on heavy noise: about 1.7 s per 31k-triangle
       operand with 6 000 crossing pairs. Each region's winding number is
       a sum over every triangle, and the noise makes thousands of
       regions. A hierarchical winding number would fix this.

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

5. **Operands that only touch (stream L, 2026-11-09; §13.6, items 1, 2
   and 5).** Faces lying on faces, vertices within an ulp of a face or an
   edge, edges on edges. Reproduced from `eval/phase6/` with the kernel's
   generators (`crates/shape-bool/tests/touching.rs`, golden
   `touching.*`):
   - two lofts stacked on a shared face of different samplings (40 and
     32 points on one 32-gon): open, 71 unmatched edges;
   - hexagonal or octagonal lofts stacked the same way, and straight lofts
     with `steps: 3`: `closed: false` (4–6 edges used four times) and no
     error, failure 1 exactly;
   - a box over a torus's quarter, whose 90° ring of vertices lies
     8·10⁻¹⁴ off the box's face: open, 152–167 unmatched edges;
   - phase 6's hinged strap (run `A`'s first build), a plate whose top
     edges lie on a tube's vertex columns and whose ends are flush with
     the tube's: open;
   - phase 6's cup rim coplanar with its shelled wall's top: open, 83;
   - phase 6's two elbows sharing a ring face, as the agents built them
     (tori turned half a facet): open, 67 (128 in their full program).

   **Why it failed.** Three faults, none in the predicates:
   - *Rounded order.* Where an edge passes within rounding of a vertex,
     its crossings with that vertex's edges are made a few ulps apart, and
     their rounded positions put them out of order along an edge or past
     an outline. spade then drops or splits a constraint, and a region
     leaks. Follow-up 4's merge of crossings that round to one position
     made this worse here: two crossings merged while a third lies, in
     exact order, between them.
   - *Classification.* A sliver region a rounding error wide is
     classified by probes 10⁻¹⁶ off its surface, which no winding number
     resolves.
   - *The result check.* `open_edges` counts *directed* edges that do
     not balance. An edge used four times, twice each way, balances: a
     fin (an A and a B sliver on the same three positions, facing
     opposite ways, both kept) or a pinch. `measure` counts it as
     non-manifold, so `join` returned `closed: false` with no error
     (§13.6, item 2).

   **What was done.** `csg_points` keeps og-geom's arrangement as the
   first pass; every closed result is the first pass's, so no golden
   hash moved. When the first result is open, the arrangement is built
   again by a second pass, and taken if it has fewer unmatched edges:
   - crossings that round to one position stay separate points;
   - points on an edge are ordered by their exact place on it. Where two
     rounded places lie within 2⁻³⁰ of the edge of each other, the order
     is decided by Shewchuk's expansion arithmetic (`exact.rs`: `+ − ×`
     on f64, no fused multiply-add): a crossing of the edge with a
     triangle's plane is `orient3d(T, p) / (orient3d(T, p) −
     orient3d(T, q))`, with another edge `orient2d` likewise, an
     operand's vertex its coordinate along the edge, and two such
     fractions are compared by cross-multiplying. Both triangles that
     share the edge get one order;
   - a triangle whose rounded triangulation is not faithful (a
     constraint dropped or split, an outline edge not bordering one
     piece, a segment not two) is triangulated again in an exact layout:
     corners at (0, 0), (1, 0), (0, 1), points on an edge at distinct
     multiples of 2⁻⁵² along it in exact order, points inside by their
     barycentric coordinates. Points at one position share a place. Two
     points at different positions that the layout cannot tell apart are
     separated by the smallest step along one of their segments, in that
     layout only;
   - points at one position are made one point (the result is welded by
     position anyway). Where one position comes back along an outline,
     the run between is collapsed too, if it is no longer than 2⁻³⁰ of
     the operands' size. Each step collapses edges, so a closed surface
     stays closed;
   - a region whose boundary is, edge for edge, the boundary of a region
     of the other operand, every triangle of both lying in one plane with
     a triangle of the other (exactly, `coplanar`), lies on the other
     surface. It is classified by orientation, with no probe: the other
     solid lies behind a face facing the same way, in front of one facing
     the other way. This is the first pass's rule for coincident faces,
     decided symbolically;
   - as a last resort, a region whose largest piece has an inradius under
     2⁻³⁰ of the operands' size keeps its probe's answer unless the
     result is open along it. Then, in each cluster of at most 12 such
     regions on the open edges, the keep-or-drop choice that leaves the
     fewest unmatched edges is taken: fewest changes first, then the
     lowest-numbered. No region a probe can resolve is changed;
   - in both passes, pairs of output triangles on the same three
     positions facing opposite ways (fins) are dropped: they bound no
     volume, and their edges cancel.

   **What a boolean may return** (`named.rs`, `boolean_report`):
   - a closed result;
   - or one that is non-manifold only where the operands touch along an
     edge: each such edge is used by two faces of each operand, one each
     way. Two boxes sharing only an edge, or a box face on a tube's vertex
     column, make such a union, which is correct. `BoolReport::
     touching_edges` counts the edges, and `measure` reports the result
     not closed. A pinch at the scale of rounding that has this shape is
     reported the same way;
   - anything else non-manifold is refused (`BoolError::NotManifold`),
     and an open result always is.

   **Voids.** A cut whose tool lies wholly inside the target leaves a
   closed inner shell facing in. That is the correct result (sand3d's
   no-op is not): the volume and mass drop by the tool's.
   `BoolReport::voids` counts such shells. `fuse`, which `boolean` runs
   on an operand of several shells, used to join an inward shell as if it
   were a solid, which lost the void; it now cuts it from the rest, so
   later booleans keep it.

   **Errors.** An open result is diagnosed before blame, in this order:
   - `crossing_pairs` runs only now; an operand that passes through
     itself is `SelfIntersecting`, as before;
   - operands that touch (the arrangement found triangles in one plane,
     or meeting without crossing) give `OpenTouching`: "the result is
     open (N unmatched edges) where the operands touch along coplanar or
     tangent faces (target: …; tool: …)", naming up to six face names of
     each;
   - otherwise `OpenResult`, which no longer blames self-intersection.

   **Results.** Every reproduction above closes, with the volume
   identities to 1e-9, except the turned elbows. Their tori are turned
   about their axis, and shape-model hands the boolean its operands in
   the target's frame, so the two ring faces are tilted and only nearly
   coplanar. They cross each other exactly at 10⁻¹⁶ angles, which makes
   large regions no probe can classify. 67 unmatched edges became 8, and
   the join is refused as `OpenTouching`, naming `qa.bottom` and
   `qb.top`. The same elbows with the tori unturned (exactly coplanar
   rings) join closed.

   **Benchmark** (`bench/booleans`, new `touching` suite of seven cases:
   the lofts, hexagonal lofts, the torus quarter, the strap, boxes
   sharing an edge, the tube's vertex column on a box face, a tool
   inside; x86-64, release, one thread):

   | suite | engine | before: ok / refused / wrong | after |
   |---|---|---|---|
   | sand3d (17) | og-geom; og-shape-checked; -untangled | 17/0/0; 17/0/0; 16/1/0 | unchanged |
   | Thingi10K (40) | the same | 40/0/0; 38/2/0; 40/0/0 | unchanged |
   | synthetic (40) | the same | 19/0/21; 24/16/0; 29/11/0 | unchanged |
   | touching (7) | og-geom | 4/0/3 | 4/0/3 (it is og-geom's f32 `csg`, unchanged) |
   | touching (7) | og-shape-checked | 4/3/0 | **7/0/0** |
   | touching (7) | og-shape-untangled | 4/3/0 | **7/0/0** |

   No suite got worse. Of the 864 output hashes the og-shape engines and
   og-geom give on these suites, the six that moved are the touching
   cases refused before. Mean times are within run-to-run noise (Thingi10K,
   og-shape-checked: 241.6 and 242.0 ms; on sand3d, og-geom, whose code
   did not change, moved from 2.9 to 3.7 ms, og-shape-checked from 4.8 to
   5.7), since the second pass runs only on an open result. On the
   touching suite it costs 27–40 ms a case. The bench's own `closed` is the directed check, so
   it scores the hexagonal lofts `ok` before (in f32 they come out
   without fins).

   Not done: a loft folding through itself, and loft corner
   correspondence (§13.6, items 3–4), are stream N's.

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

### 7.4 The gaps C left (stream K, 2026-11-09)

**Sources.** The same two published design notes as §7.1, nothing else
from Blender: Trickey's page (Bevel Ends; one of two edges bevelled, the
choices (a)–(e) and the slide distance; Preserve Widths, Preserve
Angles and the Loop Slide option; Full and Partial Vertex Bevel; the
uneven widths a per-vertex slide leaves and the need for a global width
pass) and the Bevel V2 issue #98674, read through
`projects.blender.org/api/v1/repos/blender/blender/issues/98674` (pipe
joints "snapped to the pipe profile"; arc and patch miters, spread, the
attachment of unbevelled edges to the split points; the terminal-edge
patterns). Blender's source and the Mesh Bevel node PR (#158151) were not
opened. The figures of the issue were not fetched; where the text leaves
a shape open, the choice below is ours and says so.

**One path for the rest.** The classic layout of §7.2 is kept, byte for
byte, for every vertex it handled; a vertex it refused goes through
`general.rs`. There each gap between bevelled edges gets a run of
boundary points `Q_0 … Q_r` instead of one, consecutive points joined by
a **face chain**: a profile of `n` segments lying on the gap's faces,
around a corner of its own. The gap's triangles take the run, each the
piece in front of it (split by spoke direction, as the end face of a
terminal edge already was). The corner hole then has `m + Σr` sides and
is filled by `patch.rs` like any corner: the octant only when every side
is a bevelled edge (a face chain's corner does not fit the octant's
single centre: on a pyramid apex it bulged 0.1 outside the face), the ADJ
pattern otherwise.

- **Two or more unbevelled edges in a gap** (and a lone edge with three
  or more faces past its end): Blender's Full Vertex Bevel. The first
  and last points slide along the first and last unbevelled edges to
  where the bevels' offset lines meet them; the others along their edges
  by distances evenly between ("the amount the previous edge slid" in
  the notes; even spacing keeps it symmetric). Each face between two
  unbevelled edges is cut on a profile around the vertex. Partial Vertex
  Bevel (keep the vertex) is not offered.
- **A lone edge whose two faces meet again at its end** (one unbevelled
  edge, usually its continuation: part of a rim): the design's choice (b),
  a tip, with a slide of **zero**. Blender slides the tip by the offset;
  here that tilted every face triangle at the vertex, and on a cylinder's
  faceted wall it shaved a wedge off the wall's whole height (5 mm³ too
  much on a quarter rim, r = 1.5). The strip's last profile is one
  point, so its last row is triangles.
- **Pipe joints**: when two of three or more bevelled edges at a vertex
  are within 1° of opposite, the hole takes the ADJ pattern and its
  inner points, with those of the two edges' chains, are moved onto the
  pipe (the first edge's profile at the vertex, swept along it) from the
  profile's centre in its cross-section. With planar faces two in-line
  edges leave the edges across them flat, so the case arises at seams
  (coplanar faces of different surfaces, as joins and split contours
  leave) and on curved faces.
- **Miters** (`inner_miter`, `outer_miter`, `spread`; zero values are the
  old sharp miter). They apply to a gap with no unbevelled edge whose
  faces are one plane, at a vertex of two or more bevelled edges; it is
  outer when its angle at the vertex is above 180°, inner below.
  - Arc: the gap's point `P` splits in two, slid by the spread (0: the
    offset) along the bevelled edges; an arc around `P` joins them on
    the face. It is tangent to both bevels' edge lines, rounds the face's
    corner (cutting a convex one, filling a reflex one) and adds a side
    to the hole.
  - Patch (outer only; `inner_miter: Patch` is `BevelError::Invalid`): a
    third point between, slid from `P` toward the vertex, and two arcs
    whose corners lie on the bevels' edge lines, so the outline is
    tangent at all three points. Our reading: a first try with the middle
    point slid away from the vertex (into the face) cannot be tangent to
    the edge lines without the two arcs crossing near `P`.
- **Loop slide** (`loop_slide`): an optional pass over the four offsets
  per edge before any vertex is built. At each gap with one unbevelled
  edge the two faces' offset lines meet that edge at `t_l` and `t_r`; the
  classic layout takes their mean. The pass sets each bevel side's width,
  at both its ends, to the mean of the widths that would put its line
  through those means, and repeats until no width moves by more than
  1e-12 of the largest (at most 32 times), so strips keep an even width
  along their edges. Mismatch left over (a loop of such corners whose
  angles do not close) is still averaged. Blender solves this as least
  squares; this is the cheap fixed point.
- **Faces in slivers.** C's model rebuilds each old triangle alone. A
  cap's ear-cut fan, or a boolean's faces, have slivers whose three
  corners all move, and one flipped is `TooLarge` although the face fits.
  Now a triangle that does not fit is rebuilt with its whole planar face
  (same surface, normals within the planar tolerance, reached across
  edges): its outline loops, holes bridged to the outer loop by the
  nearest visible vertex (exact `orient2d`), ear-clipped. If that fails
  too, the error is the first triangle's, as before.
- **Straight runs.** A boolean leaves its cut edges in pieces; on the
  join of §7.4's table a piece 1 long sat under a 1.5 bevel's corner. When
  a bevel is `TooLarge`, it is tried once more with each vertex inside a
  straight run of bevelled edges (two in line within 1e-9, only diagonals
  of its two planar faces besides) dissolved: its faces re-triangulated
  without it, its two edges one, with both edges' names. The original
  error is returned if the retry fails.

Both fallbacks run only where C's code returned `TooLarge`, so every
result it produced is unchanged; the four `bevel.*` hashes did not move.

**Measurements** (`cargo run --release -p shape-bevel --example phase3`,
6 segments unless said; previews in `docs/previews/phase3/`, the close-ups
are the result intersected with a cube around the vertex). All closed,
no degenerate triangle. Quads are `quad_pairs` over the new faces.

| case | volume | quads | worst fold |
|---|---|---|---|
| pyramid 20 × 20 × 20, `fillet(1.5)` on two adjacent side edges | 2 635.096 | 84/90 | 30.4° |
| same, one side edge (three faces past its apex end) | 2 654.836 | 64/66 | 118.9° |
| cylinder r 15, `fillet(1.5)` on the rim's first quadrant: tips | 14 068.163 | 72/84 | 0° |
| L-prism `fillet(2, top)`, sharp | 4 895.976 | 70/72 | 15.0° |
| same, outer arc | 4 896.090 | 126/126 | 127.4° |
| same, outer patch | 4 896.062 | 138/144 | 130.9° |
| same, inner arc (five convex top corners) | 4 889.493 | 332/342 | 25.7° |
| box 40 × 30 × 20, `fillet(3)` all, inner arc: six-sided corners | 22 976.457 | 1 008/1 008 | 31.3° |
| split-front block, `fillet(3, top seam)`, 8 seg: pipe | 23 734.013 | 190/192 | 17.0° |
| tapered uneven quad, `chamfer(2, top)`, averaged | 3 737.510 | 8/8 | 3.0° |
| same, loop slide | 3 737.058 | 8/8 | 0° |
| box 20 joined with box 8, `fillet(1)` all | 4 464.698 | 1 144/1 152 | 75.0° |
| ring 5–10 × 8 turned 2 rad, `fillet(0.3)` all | 594.491 | 1 052/1 056 | 15.0° |
| square 20 twisted 1 rad, `fillet(1.5)` all | 7 584.722 | 760/864 | 83.1° |
| square 20 tapered 0.5, `fillet(1.5)` all | 4 569.921 | 568/576 | 14.3° |

- The pipe joint lies on the round to 1e-9 and its volume equals the
  plain top fillet's: the seam's bevel is flat.
- The tips' last rows are the unpaired triangles of the rim case; the
  removed volume is within the 4-segment section times the arc's length.
- The large folds are where the geometry has a crease the patch must
  carry: the pyramid's unbevelled side edges (78° between faces) running
  into the apex patch along a quad's diagonal, and on the L the point on
  the concave vertical edge where the two top fillets' profiles meet at
  an angle and the hole narrows to it. The twisted block's strips pair
  three quarters: its walls are twisted quads and a strip quad across a
  wall's diagonal is not convex.
- Loop slide flattens the averaged case's strips (3° → 0°) and makes the
  front wall's bevel the same width at both ends (a test).
- The join failed before (`TooLarge` at the small box's foot); its
  chamfer too. The part revolve at r 0.3 worked before and is unchanged.

**Still errors, and untested.**
- A bevel wider than a segment of a curved face (the part revolve's bore
  at r 1.5) is `TooLarge`: its offset line would have to cross the
  face's own edges, which needs the inset-and-intersect re-meshing of
  the V2 notes ("handle overlaps").
- `Unsupported` remains for a vertex whose fan does not close, two cones
  at one vertex, a face corner of zero angle, an edge in line with its
  unbevelled neighbour on a curved gap, and a bevelled edge that is the
  only real edge at its vertex (impossible on a closed solid).
- Miters only on planar gaps without unbevelled edges; Partial Vertex
  Bevel, Blender's Cutoff and the custom/other vertex-mesh patterns, a
  per-corner spread and a slide distance for tips are not offered.
- Untested: pipes with curved through-faces (no generator makes one with
  a real edge across a round), patches of seven or more sides, loop
  slide on fillets around curved faces, miters with chamfers of several
  segments at reflex corners of curved solids, and very small spreads.
- On the join, the classic three-edge corners where a convex vertical
  fillet meets the concave seam fillets show a small light streak in the
  preview; the octant there is fitted to a saddle, not a cube corner.

### 7.5 The cases §7.4 left (stream P, 2026-11-10)

**Sources.** The same two published design notes as §7.1 and §7.4 and
nothing else from Blender: Trickey's page
(http://archive.blender.org/wiki/2024/wiki/User:Howardt/Bevel.html) and
the Bevel V2 issue #98674, read through
`projects.blender.org/api/v1/repos/blender/blender/issues/98674`.
Blender's source and the Mesh Bevel node PR (#158151) were not opened.
Neither note covers the cases below (they are mesh-topology corners the
notes do not reach); every choice here is ours and says so.

**How the cases were found.** A scratch scan bevelled about 19 000
selections (all edges, each name, each single edge, random subsets;
fillet and chamfer at 0.5 and 1.5) on generator solids (boxes,
cylinders, cones, tubes, wedges, domes, pyramids and prisms of 3–8
sides, twisted, tapered, D-slots, rounded rectangles, full and part
revolves with seams and apexes) and booleans (joins touching at a
corner or an edge, bores, notches, posts, cross bars and cross bores).
Of the `Unsupported` cases §7.4 lists, only two cones at one vertex
occurred (256 times); the others are shown below to be unreachable on a
closed solid or were built by hand for their tests.

- **A vertex whose fan does not close: unreachable.** On a closed solid
  each directed edge has one triangle and its twin one more, so the walk
  around a vertex is a permutation of its triangles and returns to its
  start. The guard stays, as `NotClosed`.
- **Two cones at one vertex** (two surfaces that only touch at a point,
  as a join of boxes meeting at a corner leaves): each such vertex at the
  end of a bevelled edge is split before bevelling, one copy per fan at
  the same position, each with its cone's triangles, edges and edge
  names (`cones.rs`). Each cone is then an ordinary corner. Copies left
  unbevelled weld back when the result is canonicalized. Two boxes
  joined at a corner, `fillet(2)` all, 4 segments, has exactly the volume
  of the two boxes filleted apart, all quads; with one box's edges only,
  the other keeps its corner.
- **A face corner of zero angle: `TooLarge`.** Two edges leaving a
  vertex in the same direction have offset lines parallel on either side
  of one ray, which meet nowhere; a needle of angle α needs room
  `d / sin(α/2)` along it, so zero is the limit of a corner too sharp for
  any offset, and the error is the one a sharp needle already gave. The
  same on a curved gap. A needle prism of 1e-10 rad refuses `fillet` and
  `chamfer` with `TooLarge` at its tip.
- **An edge in line with its unbevelled neighbour across a curved gap.**
  Sliding along the unbevelled edge to the offset line never arrives (the
  two are parallel). The point takes the rule of a curved gap between two
  bevelled edges instead: it slides along the inner spoke most across
  the line until its distance from it is half the offset, as collinear
  edges on a plane step straight in by the mean of the two offsets
  (ours: neither note covers it). Built by hand for the test, since no
  generator or boolean of the scan made one: a block whose top has a
  straight real edge split at a vertex, a curved surface behind it, two
  flat faces split by a third edge in front; one piece and the split are
  bevelled. The point lands where the rule says to 1e-12; fillet and
  chamfer are closed and all quads.
- **A bevelled edge that is the only real edge at its vertex:
  unreachable.** Around a vertex the surface id changes exactly at its
  real edges, and a cycle cannot change exactly once. The arm now joins
  the tip's (a lone edge whose faces meet again).
- **Flat triangles from straight profiles** (found by the scan, not in
  §7.4's list). A chamfer of two or more segments puts collinear points
  on an end face. The ear clip of `finish` could leave three of them as
  one triangle: counter-clockwise by the exact predicate, but of zero
  area in floating point, so the whole result was `TooLarge` (one bottom
  edge of an octagonal prism, `chamfer(0.5, segments 2)`; the side edges
  of 7-, 9- and 11-gon prisms and pyramids). The same happened beside a
  boolean's bore, whose rim had two points 2e-15 apart, for chamfers of
  one or two segments at the block's corners. Now, only where the first
  clip leaves such a triangle (zero area as `Solid::measure` computes
  it: from the corner first in position order, which is where
  `canonicalize` starts it), the polygon is clipped again choosing ears
  in this order: no flat ear while another fits, an ear beside a flat
  vertex first (so that vertex gets a new neighbour), then the most
  nearly equilateral. Results without a flat triangle are the first
  pass's, so no hash moved. In the scan, `TooLarge` fell from 2 173 to
  2 102 and the selections where a chamfer of 2 or 4 segments failed but
  the fillet and the one-segment chamfer fitted from 30 to 1.

**The untested items of §7.4, now tested** (`cargo test -p shape-bevel`;
golden case `bevel.untested`). Nothing in the code had to change for
them beyond the flat-triangle fix above.

| case | result | quads | worst fold |
|---|---|---|---|
| pipe through curved faces: part revolve r 5–10, outer wall split at z 4; seam and both post pieces, `fillet(1)`, 4 seg | volume 594.8315, post alone 594.8304 | 126/128 | 26.4° |
| same, 8 seg | 594.9501 / 594.9498 | 302/304 | 17.2° |
| pyramid of 7 sides, `fillet(1.5, side)`, 4 seg | 7 sectors, all quads, inside the pyramid | 112/112 | 11.9° |
| pyramid of 12 sides, 6 seg | 12 sectors | 360/360 | 8.5° |
| part revolve with a cone wall (r 10 → 7), `fillet(0.3)` on the top, averaged | cap depths 0.2905 / 0.3000 at the edge's two ends | 190/192 | 22.5° |
| same, loop slide | 0.29526 / 0.29526 (equal to 1e-11) | 190/192 | 22.5° |
| L-prism `chamfer(2, top)`, 2–5 seg, every inner/outer pair of arc and patch, spread 0, 1e-6, 0.5 | all closed | worst 64/81 (3 seg, inner arc: odd counts fill corners with fans) | 32.2° |
| outline with arcs and a reflex corner, the same | all closed but one class below | worst 330/411 | 32.2° |

- The pipe's through-faces are the start cap (flat) and the two wall
  bands (curved). The seam's faces are tangent, so its bevel is all but
  flat and the joint keeps the post's round: the volume differs from the
  post's alone by 2e-6 of it, the faceting of the bands. A pipe whose
  faces on both sides are curved needs a straight real edge between two
  curved surfaces, which no generator makes.
- Patches of 7 to 12 sides are the ADJ pattern; each has exactly
  `k (n/2)²` quads.
- An inner arc miter at the outline's corner where its arc meets the
  closing line (nearly 180°, the arc's last chord 0.65 long) is
  `TooLarge` at the default spread (the offset, 2) and at 0.5: the split
  points slide past the next point of the arc. A spread of 1e-6 fits. It
  stays an error value; a clamp would be a silent change.

**Previews** (`cargo run --release -p shape-bevel --example phase3 --
docs/previews/phase3`, the §7.4 images unchanged): `two_cones`,
`inline_ridge`, `bored_chamfer`, `apex_seven`, `pipe_curved`,
`cone_loop_slide` and `curvy_patch_chamfer`. All closed; quads 1 152/1 152,
24/24, 8/8, 210/210, 302/304, 284/288 and 176/180.

**Still errors, and untested.**
- A bevel wider than a segment of a curved face (the part revolve's bore
  at r 1.5) is still `TooLarge`. Not attempted: the golden case
  `bevel.slivers` hashes exactly that error, and the task forbids moving
  an existing hash, so closing it needs that hash re-blessed first. The
  route is the V2 notes' inset-and-intersect re-meshing of the face.
- The scan still finds selections where a fillet fits and a chamfer of
  the same offset does not, or the reverse (about 80 of 19 000, on part
  revolves with seams, posts joined to boxes, bores and twisted prisms),
  all `TooLarge`; not analysed.
- Unchanged from §7.4: miters only on planar gaps without unbevelled
  edges; no Partial Vertex Bevel, Cutoff, custom vertex meshes,
  per-corner spread or tip slide. A pipe with curved faces on both sides
  is untested (no generator makes one).

### 7.6 Bevels wider than a face's segments (stream Q, 2026-11-10)

**Sources.** The Bevel V2 issue #98674, read again through
`projects.blender.org/api/v1/repos/blender/blender/issues/98674`: "the
offset lines may cross over other edge lines of the adjacent faces and eat
into adjacent-to-adjacent faces"; "inset into the adjacent geometry
regarded as one big face", then "intersect the outside edges of the bevel
with all the existing edges"; clamping as the alternative it hoped to
avoid. Trickey's page as in §7.1. Blender's source and the Mesh Bevel node
PR (#158151) were not opened. The notes give the idea only; the passes
below and their limits are ours.

**What was built** (`crates/shape-bevel/src/wide.rs`). A fallback, tried
only where the classic layout and the straight-run retry return
`TooLarge`, so every result they gave is byte for byte the same. Three
passes; the classic layout runs after the first two, and on the third's
cut bands:

0. **Weld.** Edges shorter than 1e-9 of the largest offset are collapsed.
   The hole tool's boolean leaves rim points 1e-15 apart with two
   zero-area triangles on them, and the final check refused every bevel
   of that solid, however far from the hole.
1. **Swallowed runs** (the inset's edge event). For a bevelled edge
   `f = u → x` and another bevelled edge `e` at `u`, on each face of `f`:
   `e`'s offset line crosses `f` at `(d_e + d_f cos θ) / sin θ` from `u`,
   and `x`'s own point lands where `f`'s offset line meets the next run
   edge `g`'s (`(d_f cos φ − d_g) / sin φ` from `x`, φ the run's turn in
   `f`'s plane). If the first is past the second, `f`'s strip has no
   length there. Where `x` is a smooth run vertex (two real edges, both
   bevelled, the same two faces), it slides along `g`, on the old edge,
   to a tenth of the offset past the reach; or, if that would leave less
   than a tenth before `g`'s end, it merges into `u`. This is approximate:
   the faces move by `x`'s distance from the chord that replaces its two
   edges and from the planes of the triangles it leaves. That distance is
   measured, and over a tenth of the offset the pass is refused.
2. **Cut bands** (the inset and intersect). For a side of a bevelled edge
   whose band (its face between the edge and the offset line) holds
   another vertex of that face, the offset line is the plane through the
   offset points at both ends, square to the face's first triangle.
   `orient3d` decides each vertex's side. Every edge of the face from
   inside to outside is split where it crosses, so the geometry is
   unchanged. A swallowed vertex on an unbevelled real edge collapses
   forward along it into the cut there; one inside the face collapses into
   the edge's nearer end. A swallowed corner, a bevelled edge across a
   band, or cuts on both sides of one edge are refused. The band's
   triangles are not rebuilt. The gap points at the edge's ends are the
   cut points on the unbevelled edges there (snapped, no new point), and
   the strip gets a profile at each cut point between, with its other
   side split to match. Two bands meeting across one unbevelled edge
   share the first band's cut on it (the classic layout gives that gap one
   point), so the second face takes the corner there.

**Measurements** (`cargo test -p shape-bevel`, golden case `bevel.wide`;
fillets 4 segments). All results are closed, uncrossed
(`shape_bool::selfcheck`) and canonical, and the new faces carry only
`fillet` or `chamfer`, with edges re-tagged with the cap names as in
§1.3.

| case | before | now | quads | worst fold |
|---|---|---|---|---|
| part revolve r 5–10 × 8, 2 rad, `fillet(1.5)` all (`bevel.slivers`) | `TooLarge` | removed 51.17; smooth analytic 49.44 | 598/608 | 25.4° |
| same, `fillet(1.0)` | `TooLarge` | removed 23.45; analytic 22.52 | 596/608 | 30.5° |
| same, `chamfer(1.5)` | `TooLarge` | removed 104.36 | 104/112 | 29.2° |
| plate 30 × 30 × 10, `hole(6)`, `fillet(1.5, hole)` | `TooLarge` (any radius) | equal to a plain plate with the same 24-gon hole to 1e-9 (removed 22.20) | 384/384 at 2.5 | 0° |
| same, `fillet(1.5, top)` (outer edges and one rim) | `TooLarge` | fits | | |
| cylinder r 3, 12 seg, `chamfer(2, top, 3 seg)` | fitted | unchanged; the frustum's volume to 1e-9 | all | 0° |
| cylinder slice r 5 × 8, 2 rad, `chamfer(1.5, 3 seg)` all | `TooLarge` | removed 57.33 | 170/192 | 0° |
| same, `fillet(1.5)` | `TooLarge` | removed 30.31 | 324/328 | 22.8° |
| square 20 twisted 1 rad, `fillet(5, top)` | `TooLarge` | removed 470.0; 18.7 r² for r 4.5–5.5 | 58/64 | 67.5° |

- The analytic ring value is the smooth body rounded by a rolling ball:
  every edge is square, so it is the eroded body grown back (Steiner's
  formula). Where the classic layout fits (r 0.5), the 4-segment profile
  removes 1.095 times it. At 1.0 and 1.5 the result removes 1.041 and
  1.035 times it: pass 1 adds back 0.26 and 0.98 mm³ before bevelling,
  mostly inside the bands. The slice's pass 1 removes 0.29 (d 1) and
  1.99 mm³ (d 1.5), at the wall's chords next to the cut faces.
- On the hole, the pieces merged lie on the rims' own lines, so the
  result is the plain plate's.
- The plain cylinder's rim was never too wide: nothing crosses a rim's
  band on a cylinder (a scan of radii, segment counts, depths and
  profile segments refused the same cases before and after, all of them
  offsets that do not fit). The slice is the wide case: its cut faces'
  vertical edges swallow the rims' first vertices.
- On the twisted prism, every vertex of a side face in the result lies
  on the original side triangles to 1e-9 (a test). The quads left
  unpaired and the 67.5° fold are where a strip meets a twisted row's
  diagonal.

**The scan** (`cargo run --release -p shape-bevel --example scan -- out.txt`).
The scratch scan of §7.5 was not kept; this is a rebuild along its
description: 45 solids (boxes, cylinders, a cone, a tube, a wedge, a dome,
prisms and pyramids of 3–8 sides, twisted, tapered, a D-slot, a rounded
rectangle, a slot, full and part revolves with seams and apexes, joins at
a corner and an edge, bores, a notch, a post, a cross bar, a cross bore,
and three `shape-edit` holes), selected by all edges, each name, each
real edge (at most 200 a solid) and 50 seeded random subsets, then
filleted (4 segments) and chamfered at 0.5 and 1.5: 17 516 bevels. An
outcome is `ok` only if the result is closed, has no zero-area triangle
and no crossing triangles.

| outcome | before (`d7d9207`) | now |
|---|---|---|
| ok | 15 517 | 16 304 |
| `TooLarge` | 1 233 | 446 |
| crossing triangles | 458 | 458 (the same lines) |
| `NotClosed` (the join along an edge is not manifold) | 308 | 308 |

787 bevels went from `TooLarge` to `ok`. Nothing went from fitting to
failing, and no line changed in any other way. The 672 on the `shape-edit`
hole are all gone. What is left: random subsets on the part revolves
(134, a bevelled rim piece across a band), the notch, cross bar and join
(160), the apex revolve (46), the counterbore and cross bore (38), and
offsets that do not fit (the thin box
and the tube's 3 mm top at 1.5, 68).

**Still errors, and untested.**
- Pass 1 is approximate by up to a tenth of the offset at a swallowed run
  vertex. The exact answer starts the next edge's strip at a station on
  that edge, so the corner patch attaches mid-edge; that is not built.
- Pass 2 refuses a band crossed by a bevelled edge, a swallowed corner,
  and cuts on both sides of one edge. Where two bands meet across an
  unbevelled edge at different depths, the second face's corner
  triangles take the first band's cut (exact vertices, faces off their
  old planes near that corner).
- Found, not changed (the classic layout, as before): on the twisted
  prism, `fillet(top)` at r 1.5–3 removes 0.4–11 r², against 18.7 r²
  above, with a fin at one corner; a single helical side piece filleted
  alone removes zero or adds 0.83 mm³ (its tips). The 458 crossing
  results in the scan crossed before this stream too.
- Untested: wide bands on doubly curved faces (domes, tori), and loop
  slide or miters together with a wide band.

### 7.7 Bevel results that crossed themselves (stream W, 2026-11-10)

**Sources.** Nothing new from Blender: the two published design notes of
§7.1 only (Trickey's Bevel page and the Bevel V2 issue #98674). Neither
covers what follows (checks on a result, inputs that cross themselves,
faces that are not one plane at a vertex, a strip of no width); every
choice below is ours. Blender's source and the Mesh Bevel node PR were not
opened.

**Reproduced.** The scan of §7.6 on `a4b6046` gives its numbers exactly:
16 304 ok, 446 `TooLarge`, 458 crossing, 308 `NotClosed` of 17 516. The
scan now also prints each result's output hash, so a second run shows
which results changed their bytes.

**Causes, smallest first.**

| cause | scan lines | what it was | now |
|---|---|---|---|
| the input crosses itself | 440, all on the post (a pin joined onto a plate) | the boolean leaves two rim points 1e-15 apart and a sliver lying across the plate's top triangle; every bevel of that solid kept it | weld first (below); 72 fit, 368 select a lone rim piece |
| a straight run in pieces | 16: bore (5), notch (6), join (2), cross bar (2), ring 4 (1) | a boolean's cut edge left in pieces; a piece shorter than the next bevel sits under it, and the classic layout's triangles crossed there, all at 1.5 | the final check (below) hands it to the straight-run retry of §7.4; 10 fit, 6 select a lone piece |
| a band wider than a segment | 2, ring 2 `subset 15` | crossing at a rim vertex the bevel swallowed | the wide bevel of §7.6; the selection holds a lone piece |
| four faces on one edge | 308 `NotClosed`, all on "join edge" (two boxes joined along an edge) | outside the domain: the solid is not closed | `NotManifold(a, b)`: bevel the parts before joining them, or let them overlap |

**The final check (the backstop).** The input is checked first: closed,
two faces on every edge, and not crossing itself (`self_crossings` of
`shape_bool::selfcheck`, exact). An input that crosses itself is welded as
a wide bevel's pass 0 welds (edges shorter than 1e-9 of the largest offset
collapsed); if it still crosses, `SelfCrossing(at)`. Then every result is
checked the same way: two crossing triangles can only be the bevel's own,
and the attempt fails `TooLarge` at the bevelled vertex nearest the
crossing pair. Because it is an attempt's failure and not the final
answer, the straight-run retry and the wide bevel are tried as for any
`TooLarge`. The check is always on. Its cost (x86-64, release): a box's
twelve edges filleted at 8 segments 1.4 ms of 3.7 ms (38 %), at 32
segments 49 ms of 79 ms (62 %); a 64-segment cylinder's rims at 8
segments 4.8 ms of 10.5 ms (46 %). Nearly all of it is the broad phase
(`box_pairs`): the result's big flat faces cover many of its grid cells.
That function belongs to `shape-bool` and was not changed here.

**Faces tilted off their planes** (item 3 of the stream). The classic
layout keeps every old triangle and moves its corner at a bevelled vertex
to the gap's point. On a twisted prism a side row is a twisted quad: the
two diagonals differ by 27 mm³. The top corner's point slides down the
helical edge, which is not in the plane of the row's long triangle
(corner, far top corner, far lower vertex), so the whole 20 mm triangle
tilts by 0.9 of the offset: 4 mm³ a wall at r 0.5. A cone's fan at an
apex does the same along the cone's whole slant. Measured over the scan,
the distance of a replaced corner from its triangle's plane is at most
1.5e-11 of the offset on every solid but three (the twisted and the
twisted tapered prism, 0.14–0.80; the apex revolve, 0.33–1.08), so the
rule is exact: a new corner more than 1e-9 of the largest offset off its
triangle's plane is refused, and the bands (bevelled edge, face) at that
vertex are cut as §7.6 cuts a wide band, even with no vertex inside. Two
allowances, both §7.6's approximations: a tenth of the offset after pass
1 slid or merged a run vertex, or after pass 2 collapsed a swallowed
vertex (the ring's cut bands tilt by 0.006 of the offset there); and a
corner replaced by a vertex of the solid itself (a cut point) is exact.
What cannot be cut is `Unsupported(at, "a bevel along a face whose
triangles at this vertex are not in one plane …")`.

| twisted prism (square 20, 1 rad), `top` | r 0.5 | 1 | 1.5 | 2 | 2.5 | 3 |
|---|---|---|---|---|---|---|
| fillet removed / r², before | −44.4 | −10.7 | 0.38 | 5.8 | 9.0 | 11.0 |
| fillet removed / r², now | 18.6 | 17.4 | 16.1 | 15.3 | 17.5 | 18.1 |
| chamfer removed / r², now | 41.8 | 40.0 | 38.3 | 41.7 | 42.3 | 41.8 |

From r 3.5 the wide bevel already cut these bands (18.1–18.8, §7.6).
Every vertex of a side face in the result lies on the input's side
triangles to 1e-9, at every radius (a test). The fillet's dip at
1.5–2: between its stations the strip bridges the row's concave diagonal
fold, so a profile point lies outside the input by up to 0.026 (r 2) and
0.075 (r 3); the chamfer, flat, does not.

Refused now, wrong before (main's results, measured): on the apex revolve
`fillet(0.5, start)` moved the apex corner of the cone's triangles off
their planes by up to 1.08 of the offset, leaving them up to 0.27 off the
input surface (old faces, not the fillet's); the twisted prism's
`fillet(0.5)` of every
edge added 21.6 mm³, a single top edge 3.3 mm³; `bevel.slivers`'s
twisted `fillet(1.5)` of every edge removed 14.7 mm³ where the tapered
prism's removes 101. The twisted prism's side edges (twisted faces on
both sides: cuts on both sides of one edge, refused by §7.6's pass 2),
a top edge's terminal end on the next twisted wall, and the apex
revolve's caps stay `Unsupported`. §7.5's hand-built ridge is refused too:
its gap point, the mean of the curved face's and the flat face's points,
lifted the flat `top` face by 0.11 of the offset, and no single point is
in both faces' planes there.

**Strips of no width.** A bevelled edge whose faces meet again at both
its ends tapers to a tip at both (§7.4's choice (b), slide zero): its
strip has no width and it came back unbevelled, with only its name gone
(a helical side piece of the twisted prism removed exactly 0; a cylinder's
rim piece gave the input's 92 triangles back). Now
`Unsupported(at, "a bevelled edge that tapers to a tip at both ends …:
select the whole crease")`. This is the scan's largest change: every
single rim piece and every random subset holding an isolated rim piece,
10 160 results that came back as ok.

**Checked and kept: faces off the input surface.** A rebuilt old face may
leave the input surface legitimately. A concave fillet ending on an end
face fills the corner, and its end cap lies in the end face's plane
outside the old face (the join's cut-edge piece, 0.09 off). A chamfer
ending at a concave corner exposes a cap in the wall's plane inside the
old solid (the notch, 0.75 at d 1.5). About 280 scan results show this,
all on joins, the notch and the cross bar. So no such check was added.

**Fillet and chamfer disagree** (§7.5, "Still errors"). In this scan, on
`a4b6046`, the pairs where the fillet fits and the chamfer of the same
offset does not, or the reverse, are 2 (`bore | subset 12` and
`ring 4 | subset 22`, both a crossing result); now 0. The "about 80" of
§7.5 came from its scratch scan, before §7.6's wide bevels.

**The scan** (`cargo run --release -p shape-bevel --example scan --
out.txt`; 17 516 bevels):

| outcome | before (`a4b6046`) | now |
|---|---|---|
| ok | 16 304 | 6 012 |
| `TooLarge` | 446 | 168 |
| crossing triangles | 458 | 0 |
| `NotClosed` | 308 | 0 |
| `NotManifold` | – | 308 |
| `Unsupported`: tip at both ends | – | 10 812 |
| `Unsupported`: faces not in one plane | – | 216 |

By line: 10 160 ok, 376 crossing and 276 `TooLarge` became the tip
error (their selections hold a lone rim piece); 214 ok and 2 `TooLarge`
the tilt error (twisted 72, twisted tapered 116, apex revolve 28); 82
crossing became ok; 308 `NotClosed` became `NotManifold`. Of the 5 930
results ok before and after, 5 914 keep their bytes; the 16 that moved are
the twisted and twisted tapered prisms' `name top` and `name bottom`,
fixed above. Nothing that fitted correctly fails. `TooLarge` left: notch
44, cross bar 38, thin box 36, join 32, tube 6, counterbore 4, apex
revolve 4, ring 2, cross bore 2. The scan took 58 s before the lone-piece
refusals and 28 s after (x86-64, release, one thread).

**Previews** (`cargo run --release -p shape-bevel --example phase3 --
docs/previews/phase3`; earlier images byte for byte the same):
`twisted_top` (`fillet(1.5, top)`: volume 7 556.877, quads 60/64, worst
fold 59.5°), `twisted_top_corner` (a close-up of `chamfer(1, top)` at a
top corner, 16/16), `bore_bottom_chamfer` (the bore turned over,
`chamfer(1.5, bottom)`: 3 385.624, 60/60). `twisted` (the twisted prism
filleted all round) and `inline_ridge` are removed: those results are
refused now.

**Not done.**
- The twisted prism's side edges, a terminal end on a twisted face and
  the apex revolve's caps are refused, not built. Building them needs
  cuts on both sides of one edge and an end face cut along the profile.
- §7.6's own limits (item 6): not attempted.
- The post's sliver is a boolean's output crossing itself; the bevel now
  welds it, but the boolean should not leave it (for `shape-bool`).
- Tips at both ends are refused, not slid: Blender's slide by the offset
  would tilt a faceted wall, as §7.4 found.

### 7.8 Edges that taper to a tip at both ends (stream X, 2026-11-10)

**Sources.** Nothing new from Blender: the two published design notes of
§7.1 only (Trickey's Bevel page and the Bevel V2 issue #98674). Neither
covers a bevel whose both ends are tips inside a longer crease, faces cut
along their folds, or checks on a result; every choice below is ours.
Blender's source and the Mesh Bevel node PR were not opened.

**Reproduced.** The scan of §7.6 on `b57f2d7` gives §7.7's numbers
exactly (6 012 ok, 168 `TooLarge`, 308 `NotManifold`, 10 812 tip
refusals, 216 tilt refusals).

**What the 10 812 tip refusals select** (item 1). None comes from a
selection an author would write: 5 908 are the scan's single edges and
4 904 its random subsets; no `all` and no name. By the geometry of the
refused edge (each end has exactly one other real edge, unselected),
over the 5 908 single edges:

| class | single edges | subsets holding one |
|---|---|---|
| a lone piece of a curved crease (both neighbours turn away): a rim of a cylinder, tube, cone, dome, revolve, bore or hole; a straight side of a rounded rectangle, slot or D-slot meeting arcs tangentially | 4 044 (3 608 curved and flat face, 436 two curved faces: the twisted prisms, apex revolve, post, hole) | 5 248 |
| a piece of a rim a boolean split, one neighbour in line | 1 636 (bores, holes, counterbore, cross bore) | 1 200 |
| a rim chord in several pieces, both neighbours in line | 204 (counterbore 180, cross bore, blind hole and bore) | 364 |
| a short straight edge between two tips (a boolean's cut edge in pieces) | 24 (join, notch, cross bar) | 168 |

(A subset counts once per class it holds.)

**Real programs** (item 2). Every `fillet`, `chamfer` and `soften` in
`eval/phase6/**` and `golden/programs/` (28 programs) was built on
`a4b6046`, on `b57f2d7` and here. Stream W refuses one: run A's toaster,
`op 1 fillet "housing"` (`fillet(14, top)` on a loft), with the tilt
refusal, not the tip one; it was a result before W. No program hits the
tip refusal. Here the toaster builds, and the other 27 give the same
bytes as on `b57f2d7`. The guides' samples fillet whole names (`top`,
`side`) on plain solids; the binding's tests build them.

**A lone piece tapers to a tip at both ends** (item 3, `tips.rs`). The
tips stay at the edge's ends (slide zero, §7.4), so nothing at the
vertices moves. The width grows inside the edge: on each face the offset
line leaves each tip at 45° to the edge and runs at the offset between,
so the strip has a profile at a station one offset in from each end, and
fans from each tip to the nearer station; a piece shorter than twice the
largest offset has one station, in the middle, at half its length. Each
station's two points lie in the plane of the edge's own triangle on that
face, square to the edge, and are added to that triangle's outline, so no
face leaves its plane. Where a point falls outside its triangle the
triangle's planar face is rebuilt from its outline (§7.4); where the face
is that one triangle, the attempt is `TooLarge`. Blender's slide by the
offset would tilt a faceted wall (§7.4); this does not slide at all.

Checked: a cylinder's rim piece, `chamfer(1.5)`, removes exactly
`(L/2)³/3` (two tetrahedra; L the piece's length) to 1e-9; a rounded
rectangle's straight top edge, `chamfer(1.5)`, removes exactly
`d²/2 (L − 2d) + d³/3`; a middle piece of the twisted prism's helical
edge builds with every old vertex and centroid on the input to 1e-9.

**Where the taper does not fit**, the error is `BevelError::LonePiece(a,
b, names)`: "the edge a–b is a piece of a longer crease whose neighbours
are not bevelled, and its bevel, tapering to a tip at both ends, does not
fit: select the whole crease (named …)". It is given where the bevel
is `TooLarge` or the tilt refusal and the selection without its lone
pieces fits (or has nothing else), so a failure caused by other edges
keeps its own error. In the scan: 46, on a counterbore's floor rim
beside a wall triangle the boolean left as a sliver (its point falls
outside the triangle and the wall there is not planar; 0.1 fits, 0.5
does not), and on random subsets at 1.5 (ring revolves, notch, cross bar,
twisted prisms, apex revolve) where a piece's taper meets another bevel.

**Faces not in one plane, cut on their folds** (item 4). The cut bands
of §7.6 need a band to end on an unbevelled real edge, so on a smooth
faceted wall whose rows are twisted (a loft between sections of
different corner radii, a twisted prism, a cone's fan at an apex) they
give up, at any radius; the toaster's loft failed at r 0.2. Where every
earlier attempt fails and the classic layout would tilt a face, the
layout is tried once more taking every **fold** of a face (two of its
triangles at the vertex not in one plane, normals beyond 1 − 1e-12) as a
real edge. The gap then has a point on each fold, where the offset line
crosses it, and §7.4's Full Vertex Bevel joins them; a face bounded by a
fold is cut straight across, on the offset line, in its own plane (a
profile around the vertex there scalloped every row and pushed the
corner patch through the wall at r 14). Every replaced corner then lies
on its triangle's edges and the tilt check passes at 1e-9. Measured:
the toaster's `fillet(14, top)` removes 31 567 mm³ (a square-edged
estimate over its rim, 31 925), its old faces on the input to 4e-14;
the twisted prism's every edge, `fillet(1.5)`, removes 108.5 mm³ of
7 593.2 (the tapered prism 101); `side` at 0.5, the apex revolve's
`start` at 0.5 and §7.5's ridge build with every old face in its plane.

Of the 216 tilt refusals, 194 are results; of the 186 left (164 are
subsets that held a lone piece and fail the same way on their other
edges),
the fold layout is `TooLarge` at the twisted prism's top corners and the
apex revolve's end-cap corners: at 1.5 the offset line passes beyond the
corner triangles' folds there. Two are `all` on the apex revolve at 1.5.

**The final check, cheaper** (item 5, `check.rs`). The input passed the
exact check, so a result triangle whose corners are, in order, those of
an input triangle (kept) cannot cross another kept one: the result is
canonical, so its vertex ids are its positions, and two input triangles
meeting at a position without sharing that vertex would have touched and
been refused. Only pairs with a triangle the bevel made or moved are
tested; the crossing pairs, and the first one that names the error, are
the full check's. The broad phase is our own (not `shape-bool`'s
`box_pairs`): a tree of boxes split at the median along the widest axis,
walked in pairs of nodes, a pair of subtrees holding only kept triangles
skipped. Measured, the check alone (x86-64, release, best of 15):

| result | triangles | full check | now |
|---|---|---|---|
| box, twelve edges, `fillet(3)`, 32 seg | 13 068 | 42 ms | 16–18 ms |
| same, 8 seg | 972 | 1.2 ms | 1.3 ms |
| cylinder 64, rims, 8 seg | 2 300 | 3.1 ms | 4.4 ms |
| cylinder 64, one rim piece | 270 | 1.6–1.8 ms | 1.3–1.5 ms |

§7.7 put nearly all the cost in the broad phase: at 32 segments
`box_pairs` takes 39 of the 42 ms, the exact tests 3.6. A uniform grid is
slow on a bevel's long thin strip triangles (40 mm by 0.15); the tree
splits them apart. On the cylinder, almost every triangle is moved (each
wall triangle has a rim corner) and its fan caps' slivers overlap, so
there is nothing to skip. The scan's 17 516 outcomes and hashes are
byte for byte those of the full check.

**`TooLarge` left** (item 6; the 168 of §7.7, all at 1.5). By group:
- thin box (36): its sides are 2.5 tall under two 1.5 bevels;
- tube (6): the top ring is 3 wide, two 1.5 rims meet exactly (refused
  since §7.2);
- counterbore (4): the ledge is 2 wide; cross bore (2): 2.5 above the
  bore; cross bar (38): the bar stands 2 above the top, under a 1.5
  bevel and a 1.5 concave seam bevel;
- join (32): fits to 1.4; at 1.5, after the straight-run retry, the
  big box's top between the small box's foot (x 7) and its own edge
  (x 10) is 3 wide under two 1.5 bevels, which meet exactly;
- notch (44): the boolean leaves a separate coplanar surface (one
  triangle, legs 0.8 and 1.0) in the corner of the notch's wall, so the
  wall has a real seam edge 1 mm from the corner and the top and vertical
  edges are split there. Any bevel over about 0.8 of those edges is
  `TooLarge`; the straight-run retry cannot dissolve a vertex on a seam.
  This is the boolean's output, for `shape-bool`; not changed here.
- ring 2 (2, a subset) and apex revolve (4, single edges): not analysed.
No bevel bug was found among them.

**The scan** (`cargo run --release -p shape-bevel --example scan --
out.txt`; 17 516 bevels):

| outcome | `a4b6046` | before (`b57f2d7`) | now |
|---|---|---|---|
| ok | 16 304 | 6 012 | 16 564 |
| `TooLarge` | 446 | 168 | 412 |
| crossing triangles | 458 | 0 | 0 |
| `NotClosed` | 308 | 0 | 0 |
| `NotManifold` | – | 308 | 308 |
| `Unsupported`: tip at both ends | – | 10 812 | 0 |
| `LonePiece` | – | – | 46 |
| `Unsupported`: faces not in one plane | – | 216 | 186 |

By line, from `b57f2d7`: 10 358 tip refusals became results, 46
`LonePiece`, 244 `TooLarge` and 164 the tilt refusal (each of those 408
fails the same way with its lone pieces taken out); 194 tilt refusals
became results (twisted 56, twisted tapered 116, apex revolve 22). The
6 012 results ok before keep their bytes, and no other line changed.
`TooLarge` rose from 168 to 412 only by those 244. The scan takes 65 s
(x86-64, release, one thread).

**Previews** (`cargo run --release -p shape-bevel --example phase3 --
docs/previews/phase3`; earlier images not regenerated: the renderer has
changed since, and every one of them would move without a bevel change):
`lone_rim_piece` (a close-up of one rim piece of a cylinder r 5,
`fillet(1.5)`: volume 776.414, quads 0/8, its fans), `lone_straight_edge`
(`chamfer(1.5)` on a rounded rectangle's straight top edge: 3 491.660,
2/4), `loft_top_folds` (the toaster's housing, `fillet(14, top)`:
6 839 258.220, 3 744/3 980, worst fold 159.4°, in the fold patches at the
rows' junctions), `twisted_all` (`fillet(1.5)` every edge: 7 484.682,
984/1 248, worst fold 108.3°).

**Not done.**
- The taper's shape is fixed (45°, one offset in): no width or slide
  control for tips. A piece whose station point falls outside a
  non-planar triangle is `LonePiece`; moving the station to where the
  triangle is deep enough is not tried.
- The fold layout's corner patches are ADJ patches with straight sides on
  the folds; at the twisted prism's top corners and the apex revolve's
  end-cap corners at 1.5 they do not fit, and those stay the tilt
  refusal.
- The notch's coplanar seam sliver is the boolean's (for `shape-bool`).
- The check costs as much as before where nearly every triangle moves
  and caps are fans of slivers (the 64-segment cylinder's rims, +1.3 ms).

### 7.9 The fillet of a curved loft's top (stream AC, 2026-11-10)

**Sources.** Nothing new from Blender: the two published design notes of
§7.1 only (Trickey's Bevel page and the Bevel V2 issue #98674). The V2
notes' "inset into the adjacent geometry regarded as one big face", then
"intersect the outside edges of the bevel with all the existing edges"
(§7.6) is the idea; how the inset's corners are found on a wall that is
not one plane, the walks across it and the band's removal are ours.
Blender's source and the Mesh Bevel node PR were not opened.

**Why it was refused** (item 1). `eval/phase6/Z/repro/bevel_curved_loft_top.json`
is a spline loft with `steps: 8`, so the rows next to the top are
55/8 = 6.875 mm high (7.05 along the wall). At the refused vertex
(−64, −100.886, 175), on the straight long side just before the corner's
arc, the wall's gap holds three triangles whose normals differ in the
fourth decimal: the rows are twisted quads, because the sections' corner
radii differ. In order:
- the classic layout would move the corner of each to the gap point and
  tilt them (`Tilted`, §7.7);
- the straight-run retry changes nothing; the wide bevel (§7.6) finds
  the next ring of the wall inside the band but ends each cut band on an
  unbevelled real edge, and a closed rim has none (`ends_of` ≠ 1);
- the fold layout (§7.8) takes both folds at the vertex as real edges and
  slides a point down each to where the offset line crosses it, 10/7.05
  of the way along a spoke 7.05 deep: past its far end, so the face
  between the folds cannot be cut and the attempt is `TooLarge`;
- so the tilt refusal is given.

Run A's toaster (§7.8) is a loft of three sections with one row a span
(61 and 114 mm), and the tapered extrude is one row: the folds' far
ends lie beyond the offset there. Here radii up to the first row's
depth built (2, 5, 6.5 and 7, by the earlier layouts) and radii past
it (10, 14, 20) were refused. The smallest
reproduction is a straight loft of two rounded rectangles (20 × 20 r 4
to 16 × 16 r 3, 10 high, 4 rows of 2.5) with `top` bevelled 3 deep;
the same loft of one section twice (straight walls of 4 rows) was
`TooLarge` for the same reason (test
`a_wall_whose_rows_are_shallower_than_the_bevel_is_cut_across_them`).

**What was built** (`crates/shape-bevel/src/rows.rs`). A last attempt,
after every other (classic, straight run, wide, folds) has failed, so no
result they give changes. It applies where each selected edge has one
side on a **wall** (a surface whose triangles are not one plane) and the
other on a surface that is one plane, and where each end's gap on the
wall is bounded by two bevelled edges with no real edge between:
1. **The gap points**, on the wall. At each such vertex `v`, the plane
   through `v` square to the run's chord there (previous to next run
   vertex) is walked down the wall's triangles for `d / sin(α/2)`, `d`
   the offset and `α` the wall's angle at `v` summed over its triangles
   (π where the run is straight on the wall, so the length is `d`).
   The point lies on the segment the plane cuts from one triangle, so in
   that triangle's plane, and is added to it (the triangle split in
   three, or the edge split where the walk runs along an edge).
2. **The cut.** For each bevelled edge, the plane through its two gap
   points and square to its wall triangle is walked across the wall from
   one to the other; every edge it crosses is split where it crosses
   (`orient3d` decides the sides, a vertex on the plane is taken as it
   is). A walk that would leave the wall, reach a bevelled vertex or
   another gap point, or cross an earlier cut gives up.
3. **The band**, every wall triangle reached from the bevelled edges
   without crossing a cut, is not rebuilt; a band that reaches the
   wall's other edges gives up. The strip gets a profile at each cut
   point, at its share of the cut's length, its other end on the
   straight offset line of the planar face between that face's two gap
   points (§7.6's stations). The classic layout then runs on the cut
   wall with those gap points.

What is left of the wall is its old triangles, or pieces of them cut
along the line in their own planes. A cut that gives up, or a layout on
the cut wall that is `TooLarge`, is `TooLarge` at the bevelled vertex
nearest; any other failure keeps the earlier refusal. An old edge whose
end is inside the band loses its name with its triangles.

**Measurements** (`fillet` with 0 segments, the chord rule; all results
closed, uncrossed by the final exact check, every old triangle's corners
and centroid on the input's surface to 4e-14, the scan's `ok`):

| program | before | now, removed mm³ | prism of the top section | ratio |
|---|---|---|---|---|
| spline loft, `fillet(10, top)` (the repro) | tilt refusal | 15 619.122 | 15 999.600 | 0.976 |
| same, `chamfer(10, top)` | tilt refusal | 35 267.577 | 36 086.837 | 0.977 |
| `fillet(14, top)` | tilt refusal | 30 235.006 | 30 861.538 | 0.980 |
| `chamfer(14, top)` | tilt refusal | 68 563.928 | 69 908.254 | 0.981 |
| `fillet(20, top)` | tilt refusal | 61 258.257 | 61 966.811 | 0.989 |
| `chamfer(20, top)` | tilt refusal | 138 240.171 | 140 153.747 | 0.986 |
| `fillet(2, top)`, `chamfer(2, top)` | 646.615, 1 433.624 | the same (earlier layouts) | | |
| `fillet(5, top)`, `chamfer(5, top)` | 4 069.243, 8 903.847 | the same (earlier layouts) | | |
| straight loft of the same sections, `fillet(10, top)`; `chamfer` | tilt refusal | 15 694.855; 35 456.221 | | |
| spline loft, `fillet(10, bottom)`; `chamfer` | tilt refusal | 17 333.971; 39 193.687 | | |
| circle r 70 to two rounded rectangles (spline, 8 rows), `fillet(10, top)`; `chamfer` | tilt refusal | 15 389.277; 34 719.338 | | |
| same loft, `fillet(10, bottom)`; `chamfer` | 9 188.824; 20 121.763 | the same (classic layout) | | |

The prism is `extrude` of the top section (128 × 264 r 24), 175 high.
The loft's wall leans in at the top: its normal there has z 0.217, so
the rim's inner angle is 102.5°, and a profile on offsets `d` along both
faces (the superellipse in the faces' own frame) removes sin 102.5° =
0.976 of the square edge's section, as measured. The repro's quads are
3 072/3 072 (worst fold 5.6°). With the smallest reproduction's straight
walls the result removes exactly what the same loft with one row does
(143.784 for the fillet, 297.905 for the chamfer, to 1e-12); the twisted
loft removes 113.412 and 235.052. A wall 4 high under a 5 mm bevel is
`TooLarge` at the rim (the test). Run through `shape measure`, the
repro's program gives a closed housing of 6 396 triangles, volume
7 287 213.633. The whole bevel takes 2.4 s (x86-64, release): the
earlier attempts that fail first take 2.2 s (the wide bevel 1.1 s), as
they did to refuse it before; this pass 0.25 s.

UVs (stream Y, §21.2): the smallest reproduction, textured, keeps its
UVs through the cut (`tests/uvs.rs`): geometry equal to the bare bevel,
no UV triangle turned over, new triangles not stretched.

**The neighbours** (item 3), in short: built are radius 2, 5, 14 and 20,
`chamfer`, the straight loft, `bottom`, and the circle-to-rounded-rectangle
loft (table above). Stream X's leftovers stay refused, the same as on
`5417fac`:
- the twisted prism's top corners (`fillet` and `chamfer` of single top
  and bottom edges at 1.5, and 80 lines of random subsets): a bevel that ends at
  a corner, its end face a twisted wall. This pass needs each run end's
  wall gap bounded by two bevelled edges; an end on a real edge needs an
  end face cut along the profile, not built. `top` and `bottom` whole
  build (since §7.7);
- the apex revolve's end-cap corners (`all` at 1.5, and subsets): the
  edge between the cone and the cylinder wall has a curved wall on both
  sides, and cuts on both sides of one edge are not built (§7.6's limit).

**The scan** (`cargo run --release -p shape-bevel --example scan --
out.txt`). It gains one solid, `loft` (a spline loft of rounded
rectangles 20 × 16 r 4, 22 × 18 r 5 and 16 × 12 r 3 at 0, 4 and 8, 4 rows
a span, so 1 mm rows under the 1.5 offset), added last so the earlier
solids' random subsets are unchanged. Before is `5417fac` with the same
scan file:

| outcome | before, 45 solids | now, 45 solids | before, with `loft` | now, with `loft` |
|---|---|---|---|---|
| ok | 16 556 | 16 556 | 16 642 | 16 648 |
| `TooLarge` | 416 | 416 | 426 | 426 |
| `NotManifold` | 308 | 308 | 308 | 308 |
| `LonePiece` | 46 | 46 | 226 | 226 |
| `Unsupported`: faces not in one plane | 186 | 186 | 346 | 340 |
| total | 17 512 | 17 512 | 17 948 | 17 948 |

The 17 512 lines of the 45 solids are byte for byte those of `5417fac`,
hashes included: no line moved. On `loft`, six tilt refusals became
results (`all`, `name top` and `name bottom`, fillet and chamfer at 1.5).
The loft's other refusals are its single rim edges and random subsets
(`LonePiece` 180, tilt 154, `TooLarge` 10): runs ending inside a smooth
rim, whose ends taper to tips (§7.8). This pass does not take them.

**Golden.** New line `bevel.rows`: the repro's `fillet(10, top)` and the
smallest reproduction's fillet and chamfer. No other line moved: no
golden case reached the new pass.

**Previews** (`cargo run --release -p shape-bevel --example phase3 --
docs/previews/phase3`; earlier images not regenerated, as in §7.8):
`loft_rows_top` (the repro: volume 7 287 213.633, quads 3 072/3 072,
worst fold 5.6°) and `loft_rows_top_corner` (a close-up of one top
corner).

**Not done.**
- Runs that end on a wall: at a single edge's end, or at a corner on a
  real edge of the wall (the twisted prism's top corners), the gap is not
  bounded by two bevelled edges. Its gap point would slide down the real
  edge's chain and the band would end on it; not built.
- An edge with a curved wall on both sides (cuts on both sides).
- The gap point's depth is a walk's length, `d / sin(α/2)`, in the plane
  square to the run's chord: exact on a straight wall, a choice on a
  curved one (the fillet's width on the wall varies with the wall's
  curvature across the band).
- The earlier attempts' cost: on the repro they take 2.2 s before this
  pass runs, as they did before to refuse it.

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
- *Since stream J (§5.4, item 3): `fuse` and repair's fuse step
  untangle such a surface by `self_union`, keeping the space it winds
  round at least once. Booleans refuse it as `SelfIntersecting`. The
  measurements below are from before.*
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

## 12. The CLI's mesh tools and the MCP server (stream H, 2026-11-09)

Stream H makes the kernel usable by any agent now, before the merge: the
`shape` CLI gains the mesh tools, and a new crate, `shape-mcp`, serves
the same commands over MCP.

### 12.1 Decisions

- **One library, two front ends.** `shape-cli`'s commands became the
  `shape_cli` library, one module per subcommand (`build`, `measure`,
  `preview`, `export`, `import`, `repair`, `decimate`, `unwrap`,
  `process`), each a desc struct with a zero value and a function that
  returns a JSON report or a `Failure` value. The `shape` binary parses
  arguments into those descs; `shape-mcp` parses tool arguments into the
  same descs. File access goes through a `Files` trait: plain files for the
  CLI, the confined workspace for the server, memory for the golden case.
  So the server cannot drift from the CLI, and every result it returns is
  the kernel's.
- **Inputs by extension.** `.glb`, `.gltf`, `.obj` and `.fbx` are meshes
  (`shape_tools::import`; glTF buffers and images and OBJ material
  libraries are read beside the file, through the same `Files`, so the
  server confines them too); anything else is a program. `measure`,
  `preview` and `export` take either. A mesh's parts are placed by their
  transforms (a skinned part stays in its bind pose); `repair`,
  `decimate` and `process` join them first (`shape_tools::join`, which
  refuses a skin), and `unwrap` takes a single skinned part as it is.
- **Outputs.** The mesh tools write one solid: GLB as `glb_plain` (one
  node, one mesh, UVs when unwrapped), or OBJ or STL. Face names do not
  survive a file: a plain GLB has none, and re-import names faces by part.
  `export` and `import -o` write a mesh file's parts placed through `glb`,
  which keeps skins. `decimate --lods N` writes `<stem>.lod1.<ext>` ….
- **Reports.** Keys in a fixed order; numbers printed by serde_json
  (shortest round trip). A kernel report (`MeshMeasures::to_json`) is
  embedded as the kernel wrote it, never parsed and printed again. The
  mesh reports use `shape-tools`' snake_case; program measures keep
  `shape_model::program`'s camelCase, as before.
- **`process`** is the guide's chain on one file: import → join → repair
  → decimate to `budget` (skipped at or under it) → unwrap → measure
  (with the UVs and the file's images) → export. Repair and unwrap take
  their zero values unless set; the settings that worked on the
  acceptance meshes stay in `guides/meshtools.md`, not in the tool.
- **The protocol layer is written here, not an SDK.** The official Rust
  SDK, `rmcp` 3.9.0 (2026-11-05, Apache-2.0, MSRV 1.88), is permissive
  and pure Rust, but it is built on tokio and brings `tokio-util`,
  `futures`, `chrono`, `tracing`, `thiserror`, `pin-project-lite` and
  `indexmap` (and `schemars` for its macros): an async runtime for a
  loop that reads one line, answers it and reads the next. What MCP's
  stdio transport needs is that loop, JSON-RPC 2.0 framing and seven
  methods (`initialize`, `ping`, `tools/list`, `tools/call`,
  `resources/list`, `resources/read`, `resources/templates/list`); it is
  about 200 lines over serde_json, which the kernel already uses. Versions
  2025-11-25, 2025-06-18, 2025-03-26 and 2024-11-05 are answered as asked;
  any other gets the newest. Notifications are taken and not answered. A
  tool that fails answers `isError: true` with the kernel's message (an
  argument not in the schema, or of the wrong type, too); an unknown tool
  or method is a JSON-RPC error.
- **Tools** (`crates/shape-mcp/src/tools.rs`, each with a JSON schema,
  `additionalProperties: false`): `build`, `preview` (PNG as MCP image
  content, base64, plus its size as text; `out` saves it too), `measure`,
  `export`, `import`, `repair`, `decimate`, `unwrap`, `process` and
  `run_typescript`. Argument names are the kernel descs' fields in
  snake_case.
- **TypeScript: capture the recorded program, replay it natively.** The
  author's module default-exports `(og) => Shape`. The server writes an
  embedded runner (`crates/shape-mcp/ts/run.ts`) to a scratch directory
  and runs `bun run.ts <binding index.ts> <program.ts> <out.json>` in the
  workspace root; the runner loads the binding, calls the function and
  writes `shape.program()`. The server replays that program with
  `shape_model::program::run` and reports from the native kernel. Why
  this capture:
  - the program is the asset (`docs/design.md` §1), and the recorded
    program already replays to the same bytes (§8.2), so nothing new is
    trusted;
  - every number, image and file the server returns then comes from one
    place, the native kernel, as for the JSON tools; bun's process only
    decides which operations run;
  - a file, not stdout: the program's own `console.log` stays free, and
    comes back as `logs`.
  Rejected: reading GLB, PNG or measures from bun (a second path to the
  outputs, through wasm32 in JavaScriptCore, deterministic by the gate
  but not the server's own), and parsing stdout (collides with the
  program's logs). The program's JSON is returned and can be saved
  (`program_out`) for `build`, `preview` and `export`. bun missing, the
  binding not built, a timeout (`--ts-timeout`, 120 s by default), a
  program that throws or returns no Shape: each is an error value that
  says which. bun's failure output leads with its `error:` lines.
- **Safety.** `--root` is canonicalized at start. A path is resolved by
  canonicalizing its longest existing prefix (following links) and
  appending the rest; it must lie under the root, the rest may hold no
  `..`, and a dangling link is refused, so no write goes through one.
  Files read and inline programs are capped by `--max-bytes` (64 MiB);
  a request line over the cap plus 1 MiB is skipped and answered with an
  error. A TypeScript program is code: it runs with the server's
  permissions and is not confined (said in the tool's description and
  `guides/mcp.md`); `--no-typescript` turns the tool off.
- **Resources** are the guides, compiled in: `guides/program.md` (new:
  the program JSON, ids, frames, names, profiles and every operation),
  `guides/typescript.md` (new: the binding's API, about 1 300 tokens) and
  `guides/meshtools.md`. Both new guides' examples are built by the tests.
- **Golden.** `mcp.process.ball_defects` runs `shape_cli`'s `process` on
  the committed mesh from memory and hashes its report and GLB; native
  x86-64 and wasm32-wasip1 under wasmtime agree. No existing hash moved.

### 12.2 Dependencies

No new crate in the tree. `shape-cli` adds `serde_json` and `shape-mesh`
(both already in the workspace); `shape-mcp` depends on `shape-cli`,
`shape-tools` and `serde_json` (MIT OR Apache-2.0), with `shape-model` for
its tests; `shape-golden` adds `shape-cli`. `cargo deny check licenses
bans sources` passes. Tools, not dependencies: bun 1.4.4 (MIT) for
`run_typescript` and the binding step, wasm-bindgen-cli 0.2.130 (MIT OR
Apache-2.0).

### 12.3 Measurements (x86-64, release)

- `shape process` on `golden/meshes/ball_defects.obj` (a 1 980-triangle
  sphere with every 25th face flipped and a 1 mm floater; budget 1 000):
  35 ms. Repair turns 79 faces and drops the floater; the result is
  closed, 1 000 triangles, 8 charts, no UV overlap.
- `run_typescript` on `guides/typescript.md`'s bracket, through the
  release server: 80 ms end to end (bun start, wasm load, the program,
  native replay). The binary is 4.1 MB.

### 12.4 What is untested

- **No real MCP client was run against the server here.** The tests drive
  it over stdio in-process and through the binary (initialize, the tools,
  the resources, refusals, malformed lines); registration with Claude
  Code, Claude Desktop or others follows their documented config but was
  not exercised.
- Calls are synchronous: no progress notifications, and
  `notifications/cancelled` is ignored, so a long repair blocks the
  server until it ends. No `outputSchema` or `structuredContent`; reports
  are JSON text.
- Windows: path confinement (canonical `\\?\` paths) and bun importing an
  absolute Windows path are untested; the link test runs on Unix only.
- A path is checked before each read and write; a link swapped in by
  another process between the check and the I/O is not guarded against.
- The TypeScript program is not sandboxed (above).
- Through the server, FBX and `.gltf` with external buffers were not
  tried (the tests use OBJ and GLB); the CLI and server share the
  importer, which §9 tested on them.
- Mesh tool outputs lose face names other than the part's (plain GLB),
  as §9.8's chain does.

## 13. Phase 6: the authoring guide and its evaluation (stream I, 2026-11-09)

Stream I wrote `guides/shape.md`, rewrote `guides/typescript.md` against
the binding, and evaluated both: agents with only the two guides and a
runner rebuilt 20 objects from sand3d's datasets.

### 13.1 Method

- **Sources.** sand3d (Apache-2.0, `84c980a`), `docs/guidelines.md` (the
  eight assembly rules) and `docs/api.md`, read for rules only; no text
  copied. The datasets `Sandflow/sand3d-{cad,articraft,simready,automate,
  objaverse}` on Hugging Face (CC BY 4.0), read through their Parquet
  files (validation splits; one Objaverse object from the train split,
  the validation split has no articulated object). Attribution:
  `eval/phase6/README.md` and `references.json`.
- **Objects.** Four per dataset, 20 in all, 12 articulated (24 joints):
  CAD parts (4, rigid), Articraft (4: slides, a four-hinge chain, a mast
  with slides and a turn), SimReady (3 articulated, 1 rigid), AutoMate
  (4: spins, a slide plus a spin on one part), Objaverse (3 rigid,
  spectacles with 2 hinges). `f_score` above 0.85, programs under
  4 500 characters.
- **Reference.** The joint count from the dataset row; the bounding box
  from each object's OpenUSD file (every mesh's points through its
  translate, orient and scale ops; a script in the stream's scratch, not
  committed).
- **What an agent got.** The two guides (frozen per run), the runner
  (`eval/phase6/run.ts`: bun and the binding; it prints measures, writes
  a four-view preview and, with `--pose`, a posed one), a brief written
  here (a few sentences, the overall size per axis, the stated motions)
  and the dataset's render. Run `T` also got the sand3d program. Each
  agent was a subagent with the same model as the stream, told to read
  nothing else, to use at most 8 builds, and to report what in the
  guides was missing, wrong or misleading.
- **Measured per object:** built (the final program runs), closed parts,
  joints against the reference, the largest per-axis deviation of the
  bounding box, review iterations (builds in `runs.log`; failed builds in
  brackets), tokens, and the guide problems the agent reported. Tokens
  are the subagent totals the Agent tool reports (input and output
  together; it gives no split). A subagent that only runs `cat` on an
  empty file costs 62 668, the fixed context of the agent; the work on
  the object is the rest. Builds of throw-away probe programs into other
  directories, which some agents ran, are not counted.
- **Tools.** bun 1.4.4 was already installed (`/root/.bun/bin/bun`).
  wasm-bindgen-cli 0.2.130, cargo-deny and wasmtime-cli were installed
  with `cargo install --locked` (wasm-bindgen with `--version 0.2.130`),
  then `crates/shape-wasm/build.sh` built the binding.

### 13.2 The guide's size

Measured with the same tool as the runs: a subagent runs `cat` on the
two guides concatenated, and the empty-file subagent's count is
subtracted. For v1, `cat` twice minus `cat` once gave 5 898, within 2 %
of the subtraction (6 004).

| version | bytes | tokens | note |
|---|---|---|---|
| v1 (runs `A`, `T`) | 14 314 | 6 004 | the first draft |
| v2 (run `B`) | 18 824 | 7 524 | v1 plus the fixes from `A` |
| v3 (run `C`, committed) | 15 126 | 6 229 | v2 tightened, plus the fixes from `B` |

`typescript.md` is about 2 600 tokens of v3, `shape.md` about 3 600.

### 13.3 Results

All runs: 20 objects each (`C`: 5). "Errors" are builds that threw;
"first-run builds" are objects whose first build ran.

| | A: description, guide v1 | T: translation, guide v1 | B: description, guide v2 |
|---|---|---|---|
| built | 20 / 20 | 20 / 20 | 20 / 20 |
| every part closed | 20 / 20 | 20 / 20 | 20 / 20 (one part with 2 degenerate triangles) |
| joints as the reference | 19 / 20 (all 24 declared; 2 invented on a rigid lamp) | 20 / 20 | 20 / 20 |
| bounding box within 2 % per axis | 19 / 20 | 20 / 20 | 20 / 20 |
| review iterations, mean | 2.55 | 2.55 | 2.30 |
| failed builds, total | 9 | 11 | 3 |
| first-run builds | 15 / 20 | 12 / 20 | 19 / 20 |
| guide problems reported, mean | 4.85 | 5.20 | 3.70 |
| tokens a subagent, mean | 76 606 | 78 891 | 79 013 |
| tokens on the object (less 62 668) | 13 938 | 16 223 | 16 345 |
| tokens, total | 1 532 127 | 1 577 813 | 1 580 254 |

Success, as built with every part closed, is 100 % in every run: the
kernel never refused a finished program, and agents recovered from every
error within the 8 builds. What the guide changed is how they got there:
fewer failed builds (9 → 3), more first-run builds (15 → 19), fewer
guide problems (4.85 → 3.70 a run), the invented joints gone. Run `B`'s
tokens rose by 2 400 a run, of which 1 520 is the longer guide. The
translation baseline was not easier by these measures: it hit more
kernel errors (it reproduces sand3d's coplanar joins and quarter-torus
intersections faithfully) and reported more problems, mostly about
mapping sand3d's conventions (centred extrudes, fixed-axis Euler turns,
`d` as diameter, `station` offsets).

Run `C` (guide v3, the five objects with the most trouble in `A` and
`B`): mean iterations 3.6 (`A`) / 3.8 (`B`) / 2.6 (`C`), failed builds
5 / 3 / 2, guide problems 5.0 / 4.2 / 3.6, tokens 76 889 / 80 642 /
77 924, joints and sizes as before. Five objects are too few to call
`C` better than `B`; they show v3's cuts lost nothing.

| object | A: iterations (errors), problems | B | C |
|---|---|---|---|
| auto2_lever_knob | 6 (2), 5 | 7 (1), 5 | 3 (0), 4 |
| cad2_mounting_block | 2 (0), 5 | 4 (2), 4 | 1 (0), 3 |
| sim2_mouse_and_pad | 5 (2), 6 | 3 (0), 5 | 6 (2), 5 |
| auto4_framed_panel | 1 (0), 3 | 3 (0), 4 | 1 (0), 2 |
| auto1_curved_tubes | 4 (1), 6 | 2 (0), 3 | 2 (0), 4 |

What these numbers do not show: shape fidelity beyond the bounding box.
The briefs give the overall size, so the bounding box mostly measures
whether the agent kept to it; no F-score against the reference mesh was
computed. One brief was wrong (`art1`: it says the front panel slides
forward, the source slides both panels toward the centre); the agents
followed the brief.

### 13.4 Per object

#### run A

| object | built | closed parts | joints (ref) | size, mm (ref) | max size dev. | iterations (errors) | triangles | tokens | guide problems |
|---|---|---|---|---|---|---|---|---|---|
| cad1_bracket_block | yes | 1/1 | 0 (0) | 80 × 60 × 20 (80 × 60 × 20) | 0.0 % | 1 (0) | 500 | 72 854 | 4 |
| cad2_mounting_block | yes | 1/1 | 0 (0) | 99.5 × 80 × 109.5 (99.5 × 80 × 109.5) | 0.0 % | 2 (0) | 1062 | 73 175 | 5 |
| cad3_stepped_bracket | yes | 1/1 | 0 (0) | 76 × 53 × 30 (76 × 53 × 30) | 0.0 % | 2 (1) | 808 | 77 136 | 5 |
| cad4_curved_plate | yes | 1/1 | 0 (0) | 304.8 × 914.4 × 17.53 (304.8 × 914.4 × 17.53) | 0.0 % | 1 (0) | 428 | 72 983 | 4 |
| art1_sunroof_cassette | yes | 5/5 | 2 (2) | 1260 × 840 × 48 (1260 × 840 × 48) | 0.0 % | 1 (0) | 596 | 75 200 | 4 |
| art2_telescoping_slide | yes | 5/5 | 2 (2) | 1100 × 240 × 300 (1100 × 240 × 300) | 0.0 % | 4 (0) | 504 | 82 298 | 6 |
| art3_inspection_arm | yes | 20/20 | 4 (4) | 2604.5 × 300 × 487.5 (2606 × 300 × 488) | 0.1 % | 2 (0) | 1334 | 76 452 | 4 |
| art4_telescoping_mast | yes | 4/4 | 3 (3) | 280 × 220 × 1546 (280 × 220 × 1546) | 0.0 % | 2 (0) | 1934 | 78 138 | 6 |
| sim1_closet | yes | 3/3 | 2 (2) | 622.2 × 914.6 × 2023 (622.2 × 914.6 × 2023) | 0.0 % | 1 (0) | 64 | 73 607 | 4 |
| sim2_mouse_and_pad | yes | 4/4 | 1 (1) | 254 × 191 × 41 (254 × 191 × 41) | 0.0 % | 5 (2) | 1630 | 79 023 | 6 |
| sim3_toaster | yes | 9/9 | 3 (3) | 155 × 309 × 179 (155.11 × 306.55 × 179) | 0.8 % | 4 (1) | 5338 | 79 465 | 5 |
| sim4_bollard | yes | 1/1 | 0 (0) | 314 × 314 × 960 (314 × 314 × 960) | 0.0 % | 1 (0) | 3840 | 72 076 | 4 |
| auto1_curved_tubes | yes | 3/3 | 1 (1) | 702 × 494.95 × 1658.4 (702.17 × 495.47 × 1658.35) | 0.1 % | 4 (1) | 10368 | 78 897 | 6 |
| auto2_lever_knob | yes | 9/9 | 2 (2) | 50.02 × 57.76 × 98.94 (50.54 × 57.76 × 98.94) | 1.0 % | 6 (2) | 482 | 80 565 | 5 |
| auto3_hinged_straps | yes | 2/2 | 1 (1) | 914.4 × 5.72 × 76.2 (914.4 × 5.72 × 76.2) | 0.0 % | 2 (1) | 3336 | 75 174 | 4 |
| auto4_framed_panel | yes | 2/2 | 1 (1) | 1016 × 35.56 × 685.8 (1016 × 35.56 × 685.8) | 0.0 % | 1 (0) | 68 | 72 784 | 3 |
| obj1_octagonal_cup | yes | 1/1 | 0 (0) | 193 × 352.2 × 137 (193 × 352.2 × 137) | 0.0 % | 2 (0) | 152 | 74 417 | 3 |
| obj2_djembe | yes | 69/69 | 0 (0) | 408 × 408 × 600 (407.91 × 407.91 × 600) | 0.0 % | 4 (1) | 12924 | 78 514 | 6 |
| obj3_lamp | yes | 4/4 | 2 (0) | 595 × 223 × 598.5 (595 × 223 × 598.5) | 0.0 % | 3 (0) | 126 | 77 688 | 6 |
| obj4_spectacles | yes | 9/9 | 2 (2) | 140 × 11.3 × 26.5 (140 × 12.48 × 26.77) | 9.5 % | 3 (0) | 4334 | 81 681 | 7 |


#### run T

| object | built | closed parts | joints (ref) | size, mm (ref) | max size dev. | iterations (errors) | triangles | tokens | guide problems |
|---|---|---|---|---|---|---|---|---|---|
| cad1_bracket_block | yes | 1/1 | 0 (0) | 80 × 60 × 20 (80 × 60 × 20) | 0.0 % | 1 (0) | 572 | 72 262 | 4 |
| cad2_mounting_block | yes | 1/1 | 0 (0) | 99.5 × 80 × 109.5 (99.5 × 80 × 109.5) | 0.0 % | 2 (0) | 3778 | 76 362 | 6 |
| cad3_stepped_bracket | yes | 1/1 | 0 (0) | 76 × 53 × 30 (76 × 53 × 30) | 0.0 % | 2 (1) | 830 | 74 958 | 4 |
| cad4_curved_plate | yes | 1/1 | 0 (0) | 304.8 × 914.4 × 17.53 (304.8 × 914.4 × 17.53) | 0.0 % | 3 (0) | 88 | 76 441 | 4 |
| art1_sunroof_cassette | yes | 16/16 | 2 (2) | 1260 × 840 × 48 (1260 × 840 × 48) | 0.0 % | 2 (0) | 518 | 77 684 | 6 |
| art2_telescoping_slide | yes | 8/8 | 2 (2) | 1100 × 240 × 300 (1100 × 240 × 300) | 0.0 % | 2 (1) | 894 | 78 375 | 4 |
| art3_inspection_arm | yes | 29/29 | 4 (4) | 2606 × 300 × 488 (2606 × 300 × 488) | 0.0 % | 3 (0) | 5998 | 80 756 | 5 |
| art4_telescoping_mast | yes | 4/4 | 3 (3) | 280 × 220 × 1546 (280 × 220 × 1546) | 0.0 % | 3 (0) | 1558 | 83 397 | 5 |
| sim1_closet | yes | 3/3 | 2 (2) | 622.2 × 914.6 × 2023 (622.2 × 914.6 × 2023) | 0.0 % | 1 (0) | 98 | 75 825 | 3 |
| sim2_mouse_and_pad | yes | 4/4 | 1 (1) | 254 × 191 × 41 (254 × 191 × 41) | 0.0 % | 3 (0) | 1140 | 78 229 | 6 |
| sim3_toaster | yes | 33/33 | 3 (3) | 154 × 306 × 179 (155.11 × 306.55 × 179) | 0.7 % | 4 (0) | 19880 | 87 440 | 7 |
| sim4_bollard | yes | 2/2 | 0 (0) | 314 × 314 × 960 (314 × 314 × 960) | 0.0 % | 1 (0) | 2176 | 72 331 | 4 |
| auto1_curved_tubes | yes | 6/6 | 1 (1) | 702.17 × 495.47 × 1657.2 (702.17 × 495.47 × 1658.35) | 0.1 % | 4 (3) | 15384 | 90 220 | 10 |
| auto2_lever_knob | yes | 2/2 | 2 (2) | 50.54 × 57.76 × 98.94 (50.54 × 57.76 × 98.94) | 0.0 % | 3 (1) | 388 | 82 169 | 5 |
| auto3_hinged_straps | yes | 2/2 | 1 (1) | 914.4 × 5.72 × 76.2 (914.4 × 5.72 × 76.2) | 0.0 % | 3 (1) | 3296 | 79 150 | 5 |
| auto4_framed_panel | yes | 2/2 | 1 (1) | 1016 × 35.56 × 685.8 (1016 × 35.56 × 685.8) | 0.0 % | 1 (0) | 68 | 73 698 | 4 |
| obj1_octagonal_cup | yes | 2/2 | 0 (0) | 193 × 352.2 × 137 (193 × 352.2 × 137) | 0.0 % | 4 (2) | 326 | 80 683 | 4 |
| obj2_djembe | yes | 7/7 | 0 (0) | 407.91 × 407.91 × 600 (407.91 × 407.91 × 600) | 0.0 % | 2 (0) | 7224 | 76 038 | 6 |
| obj3_lamp | yes | 4/4 | 0 (0) | 595 × 223 × 598.5 (595 × 223 × 598.5) | 0.0 % | 4 (1) | 614 | 81 379 | 5 |
| obj4_spectacles | yes | 11/11 | 2 (2) | 140 × 12.4 × 26.5 (140 × 12.48 × 26.77) | 1.0 % | 3 (1) | 996 | 80 416 | 7 |


#### run B

| object | built | closed parts | joints (ref) | size, mm (ref) | max size dev. | iterations (errors) | triangles | tokens | guide problems |
|---|---|---|---|---|---|---|---|---|---|
| cad1_bracket_block | yes | 1/1 | 0 (0) | 80 × 60 × 20 (80 × 60 × 20) | 0.0 % | 1 (0) | 518 | 75 644 | 4 |
| cad2_mounting_block | yes | 1/1 | 0 (0) | 99.5 × 80 × 109.5 (99.5 × 80 × 109.5) | 0.0 % | 4 (2) | 1062 | 78 389 | 4 |
| cad3_stepped_bracket | yes | 1/1 | 0 (0) | 76 × 53 × 30 (76 × 53 × 30) | 0.0 % | 1 (0) | 808 | 75 583 | 3 |
| cad4_curved_plate | yes | 1/1 | 0 (0) | 304.8 × 914.4 × 17.53 (304.8 × 914.4 × 17.53) | 0.0 % | 1 (0) | 284 | 76 322 | 3 |
| art1_sunroof_cassette | yes | 5/5 | 2 (2) | 1260 × 840 × 48 (1260 × 840 × 48) | 0.0 % | 2 (0) | 884 | 78 987 | 4 |
| art2_telescoping_slide | yes | 4/4 | 2 (2) | 1100 × 240 × 300 (1100 × 240 × 300) | 0.0 % | 2 (0) | 640 | 79 921 | 4 |
| art3_inspection_arm | yes | 19/19 | 4 (4) | 2606 × 300 × 487.5 (2606 × 300 × 488) | 0.1 % | 2 (0) | 1578 | 81 377 | 4 |
| art4_telescoping_mast | yes | 4/4 | 3 (3) | 280 × 220 × 1546 (280 × 220 × 1546) | 0.0 % | 2 (0) | 1170 | 79 157 | 3 |
| sim1_closet | yes | 3/3 | 2 (2) | 622.2 × 914.6 × 2023 (622.2 × 914.6 × 2023) | 0.0 % | 1 (0) | 64 | 76 800 | 2 |
| sim2_mouse_and_pad | yes | 4/4 | 1 (1) | 254 × 191 × 41 (254 × 191 × 41) | 0.0 % | 3 (0) | 3694 | 82 222 | 5 |
| sim3_toaster | yes | 8/8 | 3 (3) | 155 × 306.5 × 179 (155.11 × 306.55 × 179) | 0.1 % | 3 (0) | 6654 | 82 462 | 5 |
| sim4_bollard | yes | 1/1 | 0 (0) | 314 × 314 × 960 (314 × 314 × 960) | 0.0 % | 1 (0) | 4992 | 75 068 | 2 |
| auto1_curved_tubes | yes | 3/3 | 1 (1) | 702 × 494.68 × 1658.4 (702.17 × 495.47 × 1658.35) | 0.2 % | 2 (0) | 10368 | 80 103 | 3 |
| auto2_lever_knob | yes | 2/2 | 2 (2) | 50.02 × 57.76 × 98.94 (50.54 × 57.76 × 98.94) | 1.0 % | 7 (1) | 716 | 84 747 | 5 |
| auto3_hinged_straps | yes | 2/2 | 1 (1) | 914.4 × 5.72 × 76.2 (914.4 × 5.72 × 76.2) | 0.0 % | 2 (0) | 4340 | 78 397 | 4 |
| auto4_framed_panel | yes | 2/2 | 1 (1) | 1016 × 35.56 × 685.8 (1016 × 35.56 × 685.8) | 0.0 % | 3 (0) | 68 | 77 751 | 4 |
| obj1_octagonal_cup | yes | 1/1 | 0 (0) | 193 × 352.2 × 137 (193 × 352.2 × 137) | 0.0 % | 3 (0) | 152 | 78 407 | 3 |
| obj2_djembe | yes | 5/5 | 0 (0) | 407.91 × 407.91 × 599.99 (407.91 × 407.91 × 600) | 0.0 % | 2 (0) | 7700 | 78 153 | 3 |
| obj3_lamp | yes | 3/3 | 0 (0) | 595 × 223 × 598.5 (595 × 223 × 598.5) | 0.0 % | 1 (0) | 272 | 77 377 | 4 |
| obj4_spectacles | yes | 7/7 | 2 (2) | 140 × 12.5 × 26.77 (140 × 12.48 × 26.77) | 0.2 % | 3 (0) | 1662 | 83 387 | 5 |


The guide-problem counts are the items each agent listed under "guide
problems", counted by hand from its report; agents also listed brief
ambiguities there, which no guide change can remove.

### 13.5 Failure modes, and what the guide changed

Counted over objects (an object counts once per mode). `A` and `T` used
v1; `B` v2. Guide problems fixed in v2 and v3 are in `guides/shape.md`
and `guides/typescript.md`; kernel causes are in §13.6.

| failure mode | A | T | B | cause | fix |
|---|---|---|---|---|---|
| frame axes not stated (face frames, `og.frame` with `up`, `ring`, `grid`, `bbox`, turn sign): guessed, probed with `console.log`, or placed by world coordinates | 12 | 11 | 3 | guide | the facing rule, `ring`/`grid`/`bbox` frames, `up` → +y (typescript.md) |
| a later `at` silently replaced a part's offset `on` (parts off by half their size, hinged at their centre): one or more lost builds each | 4 | 2 | 0 | guide (the API's semantics) | the rule in bold, the group pattern, a joined detail gets `on` only, a symptom row (shape.md §4, §8) |
| operation details not stated: `hole` depth and counterbore/countersink format, `pocket` frame, `shell` open, `poly`/`polygon` radius, `taper` meaning, `sweep` path, loft interpolation, `mirror` plane, `tube`/`torus` radii | 11 | 14 | 4 | guide | the generator table and edit notes (typescript.md) |
| a join of faces that only touch (coplanar, or flat on round) left the result open or threw | 3 | 5 | 0 | kernel; the guide said "overlap or share a face" | "overlap 0.5 mm" and a symptom row; the kernel gap stays (§13.6) |
| errors do not name the call: builds spent bisecting | 5 | 4 | 0 | kernel | "build after each part" (shape.md §8); the gap stays |
| the guide asked for measures the runner did not print (`nonManifoldEdges`, `degenerateTriangles`); several poses after one `--pose` ignored | 5 | 6 | 0 | the eval runner | the runner prints them and takes several poses |
| the preview's views not described | 5 | 7 | 0 | guide | view order and elevation stated |
| no way to view a side, zoom or show one part | 2 | 2 | 7 | the eval runner | `--azimuth`, `--elevation`, `--width` added after the runs |
| joints invented for motions the brief did not state | 1 | 0 | 0 | guide | §1.3: only stated motions |
| an axis sign guessed wrong for a case the guide's examples did not cover | 1 | 1 | 1 | guide | the right-hand rule spelled out, flap and lid examples (v2, v3) |
| a `const` read before its declaration (part functions below `return`) | 1 | 0 | 0 | guide | §2: declare shared numbers first |
| bends and arcs: no arc in a profile, no bend in a sweep path, no part torus; points computed with `Math.sin` | 2 | 3 | 2 | kernel; guide silent | v3: bends from `ring` origins; arcs by cutting a large circle; the gaps stay |
| segment counts: `segments` on `cylinder`, `sphere` or `torus` silently ignored; small spheres and tori cost hundreds to thousands of triangles | 2 | 4 | 1 | kernel | only `og.circle` takes `segments` (stated); the gap stays |
| loft looked twisted or banded wrongly; a curved loft came out open | 2 | 2 | 1 (+1 in `C`) | kernel | v3 states the loft is straight unless `steps` > 1; the gaps stay |

Summed over the modes a guide or runner change can address (all rows
but the touching joins, the view flags, bends, segments and lofts): 45
object-hits in `A`, 45 in `T`, 8 in `B`.

### 13.6 Kernel gaps for the integrator

Found by the agents; each reproduced here with a short program unless
marked. None was fixed in this stream.

1. **Booleans on operands that only touch leave the result open.** Two
   lofts stacked on a shared face join to `closed: false` with no error
   (reproduced); a box face tangent to a tube's outside throws "the
   result is open (26 unmatched edges)" (reproduced); a rim coplanar with
   a shelled wall gives 81 or 115 unmatched edges, a box through a
   torus's ring of vertices 268, two elbows sharing a ring face 128
   (reported). The hint "an operand most likely passes through itself"
   points the author away from the cause. Boxes sharing a face join
   cleanly, so it is the general coplanar and tangent case that fails.
   **Done, but for one case** (stream L, §5.4 item 5): the lofts, the
   tangent strap, the cup rim, the box through a torus's ring of
   vertices and untilted elbows join closed. Elbows whose ring faces are
   only nearly coplanar (tori turned about their axis) are still refused,
   now as "the result is open … where the operands touch along coplanar
   or tangent faces", naming the faces.
2. **`join` can return an open solid without an error**, against
   `docs/design.md` §6 ("never a best effort silently").
   **Done** (stream L, §5.4 item 5): the result check counted directed
   edges, which an edge used twice each way passes. A result is now
   closed, or non-manifold only along edges where the operands touch
   (two faces of each, reported as `BoolReport::touching_edges`), or
   refused.
3. **A curved loft (`steps` > 1) with a small end section folds through
   itself**: open, 2 non-manifold edges, no error (reported in runs `A`
   and `C`, not reproduced with the sections tried here). **Done
   (§15.2)**: a fold is refused, naming the sections; the open result
   was most likely the cap fill of §15.1, now fixed.
4. **Loft resampling ignores corners**: sections are resampled evenly by
   length from their largest-x point, so polygon corners fall between
   samples and a hexagon loft looks twisted (reported; §10.3 already
   notes the missing corner correspondence). **Done (§15.1).**
5. **A `cut` whose tool lies wholly inside the target changes nothing,
   silently** (an enclosed void; reported).
   **Done** (stream L, §5.4 item 5): the void is kept, as the correct
   result, and reported (`BoolReport::voids`); `fuse` no longer loses it
   in a later boolean. The binding and the CLI do not show the report
   yet.
6. **Unknown desc fields are ignored**, so `cylinder({ segments })`,
   `sphere({ segments })` silently do nothing (reproduced). The program
   parser could refuse fields it does not know, as the MCP tools do.
   **Done** (stream M, §14.1): every operation, profile, section, view,
   frame, `mimic` and path refuses a field it does not take, naming it
   and the fields taken (checked again in §16 for the new ops).
7. **Errors from a live session do not name the operation or call**;
   replayed programs say `op N`, the binding does not.
   **Done** (stream M, §14.1): `ModelError::At` names the index, the
   verb and the part in both, and the binding appends the author's call
   site.
8. **No segment control** on `cylinder`, `cone`, `sphere`, `dome`,
   `torus` and `revolve`: an 8 mm sphere is 812 triangles, a drum hoop
   torus 3 712.
   **Done** (stream M, §14.1): `segments` a turn on those and on `wire`.
9. **Missing from the program and the binding** though the generators
   exist: `wire` (bent rods with bend radii), `body`, `knurl`, the gear
   solid, the dome's height, the loft's `curve` choice, `scale`; and in
   the binding, `og.translate`, `og.rotate`, `og.offset` (the JSON forms
   work). Missing in the kernel: an arc segment in `poly`, a part torus,
   an across-flats size for `polygon`, a non-uniform profile scale.
   **Done** (stream M, §14.1): every generator named is in the program
   and the binding; the kernel gained `PolyPoint::bulge`, `TorusDesc`'s
   `angle`, `Polygon::flats`, and the program `Profile::scale`. Checked
   again in §16.1: all four are reachable from the program and the
   binding.
10. **No interference measure**: nothing reports parts passing through
    each other, at rest or posed; measures are at rest only. **Done
    (§15.3)**; the CLI, MCP and binding are stream M's to wire.
11. **Bevels after a join**: `fillet(edges: "top")` takes every
    operand's top edges and throws "the bevel is larger than the faces
    allow" (reproduced); `soften` on a loft's top edge throws at any
    radius tried (reported).
12. **Joining overlapping tapered extrudes leaves 2 degenerate
    triangles** (reported). **Done** (stream O, §15.4): exactly
    collinear output triangles are flipped away, in booleans and when a
    part is moved. A needle with a non-zero exact area whose f64 cross
    product rounds to zero is still counted by `measure` (the bands'
    intersection has one).
13. **A through hole on a boss over a bottom pocket** threw "the axis from
    [≈0, ≈0, 0] does not meet the solid", naming a point that is not the
    frame given (reported, not reproduced). **Done** (stream O, §15.4):
    reproduced from `eval/phase6/B/cad2_mounting_block` with the boss hole
    made through (`depth` 0). The refusal was right: the pocket's frame is
    the bottom face's centroid (≈0 by rounding, the point named), and its
    axis runs inside the 6 mm bore, so it meets no face. The error now
    names the direction and the axis's clearance to the nearest edge
    (2.97, the bore's polygon). `entry` also decides hits by `orient3d`
    now, so an axis through a shared edge meets the solid; the floating
    point test missed a turned box's face diagonal.
14. **The preview** ignores `opacity` and shades smoothly across a loft's
    facets.
15. **`shape-mcp` does not serve `guides/shape.md`** as a resource (it
    serves `program.md`, `typescript.md`, `meshtools.md`); an agent on
    MCP does not see the authoring guide.
    **Done** (stream R, §16): served as `og-shape://guides/shape.md`, and
    the server's instructions and `guides/mcp.md` point to it first.

### 13.7 What is untested

- One agent per object and run: no repeats, so differences of one or two
  builds between runs are within noise. Run `C` covers five objects.
- The agents were the same model as the stream; other models were not
  tried. Agents were told to read nothing else, which was not enforced.
- Shape fidelity was judged by bounding box and by the agents' own look
  at previews, not by a distance to the reference mesh.
- Agents ran the binding through `eval/phase6/run.ts`, not the MCP
  server or the `shape` CLI.
- Token counts are totals from the Agent tool, with no input and output
  split; the fixed context (62 668) is the same for every agent.

## 14. The authoring surface (stream M, 2026-11-09)

Stream M closes items 6 to 9 of §13.6: every generator that exists is
reachable from the program JSON and the binding (and so the CLI and
MCP), round generators take a segment count, unknown fields are refused,
and every error says where it happened.

### 14.1 Decisions

- **Added fields, zero values keep today.** `shape-2d`: `Polygon::flats`
  (across flats; with `radius` too it is `ProfileError::Conflict`),
  `PolyPoint::bulge` (the edge to the next point is an arc whose middle
  stands that far off the chord, positive out of the outline whichever
  way the points run; a corner radius next to an arc is
  `ProfileError::ArcCorner`). The non-uniform scale already existed
  (`Profile::scale`); it was only missing from the program.
  `shape-gen`: `segments` on `CylinderDesc`, `SphereDesc`, `DomeDesc`,
  `Revolve` and `Wire`; `angle`, `segments`, `tube_segments` on
  `TorusDesc`; `ConeDesc` and `TubeDesc` for the bare `cone` and `tube`.
  Every zero path is the old code path, so no golden hash moved (checked
  natively and as wasm32-wasip1). The only edit outside these crates'
  own files that the new fields forced is `..Default::default()` in
  struct literals, one of them in a `shape-bool` test.
- **`segments` is a count a full turn.** A part turn (a revolve, a part
  torus, a wire's bend) gets its share, rounded up, at least 1; a sphere
  gets half as many pole to pole, a dome a quarter. 3 to 1 024, else
  `GenError::Invalid`. It is the same unit everywhere, so an author sets
  one number per part.
- **No chord-tolerance override.** Considered and left out. The cost the
  agents hit (an 8 mm sphere at 812 triangles, a hoop torus at 3 712) is
  the chord rule's floor of 24 a turn, which a looser tolerance does not
  lift; a tolerance would also be a second knob that interacts with the
  24 and 64 clamps, and `docs/design.md` §4 fixes tolerances in
  millimetres whatever the unit. A count is direct and visible in the
  program.
- **Program ops.** `wire`, `body` (views: a number, `[[y, v], …]`, or
  `{points, smooth}`), `knurl`, `gear` (the gear solid; the profile keeps
  the name `gear` as a profile), the dome's `height`, the loft's
  `curve`, turned solids' `axis` (`"+Y"`, `"+Z"`, in the spelling joint
  axes use), the part torus, and `scale`: a part's solid along its own
  axes about its own origin, positive factors only (a mirror is
  `mirror`), placement, names and joints kept, a group refused. The
  binding adds the same, plus `og.translate`, `og.rotate`, `og.offset`
  and `og.scale` as profile helpers (desc objects, the JSON forms).
- **Unknown fields are refused** at every level: an operation, a
  profile, a `poly` point, a loft section, a body view, a frame, a
  `mimic`, an `along` path. The error is `ModelError::UnknownField`:
  where, the field, and the fields taken. `null` stays the same as
  absent. Descs the binding reads itself (`og.model`, `og.frame`,
  `preview`) are checked in TypeScript the same way. No program in
  `eval/`, `golden/` or `guides/` used a stray field (the 65 eval
  TypeScript programs of runs A, T, B and C and every recorded JSON
  program were rebuilt to check).
- **Errors are located.** `Session::apply_value` wraps any failure in
  `ModelError::At { index, op, reader, part, error }`: the index in a
  replayed program, or the index the op would be recorded at in a live
  session; the part is the node the op worked on (`id`, `target`, `host`,
  `child`, `to`, `ids`), else the `name` it made. Shown as `op 3 fillet
  "lid": …`, and for a reader `face "lid" (a reader, before op 3): …`.
  Field errors became `ModelError::BadField` and no longer repeat the
  op.
- **The binding's call site** comes from the stack, read in the `catch`
  around the wasm call, while the author's call is still on it: the
  first frame outside `index.ts`, appended as `(at file:line:col)`.
  Nothing is captured on success, so it costs nothing until a call
  fails. It works in bun (JavaScriptCore) and in V8's format; a bundled
  or minified binding would point into the bundle.
- **Found and fixed:** a generator whose `on` frame was refused had
  already added the part, so the model changed on a failed operation
  and the recorded program no longer replayed to it. `on` is now read
  and checked before the part is added.

### 14.2 Golden

`crates/shape-golden/src/surface.rs`: one program that uses every new
op and field, replayed; `surface.program.glb` and
`surface.program.measure`. Every part must be closed with no degenerate
triangle, or the case fails. Native x86-64 and wasm32-wasip1 agree; no
earlier line moved.

**For stream N:** a loft whose top section is a rotated rect
(`{"rotate": {"profile": {"rect": {"width": 3, "height": 2}}, "angle":
0.3}}` at z 8, over circles r 3 and a circle scaled 0.5 in x) is open
with 10 non-manifold edges and 1 degenerate triangle, with `curve`
`straight`, `catmullRom` or `spline`. The same rect unrotated, or a
circle, is closed. It looks like the cap against the resampled ring
(§13.6 item 3 may be the same fault); the golden program uses the
unrotated rect.

### 14.3 The guide's size

Measured as in §13.2: a subagent `cat`s the two guides concatenated, less
one that `cat`s an empty file. The empty-file baseline is now 61 547
tokens (it was 62 668 in §13).

| version | bytes | tokens | note |
|---|---|---|---|
| v3 (§13) | 15 126 | 6 229 | |
| v4, run `M` | 15 828 | 6 542 | the new surface, with cuts elsewhere |
| v4, committed | 16 020 | 6 638 | plus run `M`'s fixes: which generators take `segments`, `axis` in the table, where a `+Z` torus sits |

So 6.6 % over v3. `program.md` grew too (it is not part of the measured
pair).

### 14.4 Re-runs (run `M`)

Five objects whose earlier agents hit these gaps: bent tubes built from
sweeps, `Math.sin` and quarter tori (`auto1`), torus hoops and their
triangle cost (`obj2`), bent frames (`obj4`), and the most failed builds
and errors that did not say where (`auto2`, `sim2`). Method as §13.1:
one subagent each, the same model, the guides as committed before the
fixes in the last row of §14.3, the brief and render, at most 8 builds,
told to read nothing else.

| object | A: it. (err.), problems, tokens | B | C | M | M: triangles, size dev. |
|---|---|---|---|---|---|
| auto1_curved_tubes | 4 (1), 6, 78 897 | 2 (0), 3, 80 103 | 2 (0), 4 | 3 (1), 6, 84 298 | 28 632, 0.0 % |
| auto2_lever_knob | 6 (2), 5, 80 565 | 7 (1), 5, 84 747 | 3 (0), 4 | 2 (0), 6, 77 401 | 498, 0.0 % |
| sim2_mouse_and_pad | 5 (2), 6, 79 023 | 3 (0), 5, 82 222 | 6 (2), 5 | 5 (0), 8, 92 784 | 8 238, 0.1 % |
| obj2_djembe | 4 (1), 6, 78 514 | 2 (0), 3, 78 153 | — | 5 (0), 6, 83 572 | 14 268, 0.0 % |
| obj4_spectacles | 3 (0), 7, 81 681 | 3 (0), 5, 83 387 | — | 7 (5), 6, 90 490 | 3 324, 0.0 % |
| mean / total | 4.4 (6), 6.0, 79 736 | 3.4 (1), 4.2, 81 722 | | 4.4 (6), 6.4, 85 709 | |

Every `M` object built, every part closed, joints as the reference
(5 of 5), every size within 0.1 % (A's spectacles were 9.5 % off).

What changed:
- **The new surface was used where the gaps were.** Bent tubes and frames
  are `wire`s (auto1, obj2, obj4; no `sweep` and no `Math.sin` in any `M`
  program, where B's auto1 and obj1 used `Math.sin`); the mouse is a
  `body`; the drum hoop a `torus` with `axis: "+Z"`; the knob's
  non-regular hexagon an `og.scale`d polygon; `segments` on round parts.
- **Errors said where.** Of the 6 failed builds, every one was fixed in
  the next build or the one after, and the agents said the line and op
  pointed at the failing call. auto1's one error and one of obj4's were
  `segments` on `wire`, which the guide's "round generators take
  `segments`" implied; `wire` now takes it. obj4's other four were a
  join of solids sharing a plane (§13.6 item 1, unchanged: "the result is
  open … passes through itself" still misleads) and a `poly` corner
  radius too large.
- **Not better by the counts.** Iterations and failed builds match run
  `A`, above `B`; guide problems are up (6.4 a run). Most of the reported
  problems are about the new generators' details (`body`'s y origin, end
  caps and overshoot; a `+Z` torus's height; which generators take
  `segments`; `axis` missing from the table), plus older ones (polygon
  orientation, continuous joints and `range`, dark previews). Tokens are
  up 6 000 a run on `A` and 4 000 on `B`, although v4 is 538 tokens
  longer than `A`'s guide and 982 shorter than `B`'s, and the fixed
  context is 1 121 smaller: the rest is more builds and image reads
  (sim2, obj4). With one agent per object,
  differences of a build or two are noise (§13.7).
- **Triangles went up, not down**, on the bent tubes (28 632 against B's
  10 368): the wires carry full 24-to-64 rings and bends, and the agent
  could not lower them (`wire` had no `segments` then). The djembe has
  more parts (16 wire strips). Segment control was used on the mouse and
  spectacles.

### 14.5 What is untested

- The binding's call site was checked under bun only, not node or a
  browser, and not through a bundler.
- Run `M` used the guides before the last fixes of §14.3; those fixes
  (and `segments` on `wire`) were not re-run with agents.
- aarch64 and Windows: CI only, as before.
- The MCP server gets the new ops through `build` and `run_typescript`
  unchanged; it was exercised by its existing tests, not with the new
  ops.
## 15. Loft quality and an interference measure (stream N, 2026-11-09)

Items 3, 4, 10 and 12 of §13.6. Code: `shape-gen/src/loft.rs`, the cap
fill in `shape-2d/src/ops.rs` (`triangulate`), `shape-tools/src/
interference.rs`, `shape-model/src/checked.rs` and `Model::interference`,
`program::report_json`. Golden cases `loft.corners`, `loft.folds` and
`interference.washing_machine` (`shape-golden/src/interference.rs`);
every earlier hash is unchanged, natively and under wasmtime. Examples
that print the measurements below: `cargo run --release -p shape-tools
--example interference` (phase 1) and `cargo run --release -p
shape-model --example report -- <program.json> [joint=value,…]…`.

### 15.1 Loft corners (item 4)

- **Which sections.** The new resampling applies to a contour slot (the
  outline, or one hole through every section) when some section's
  contour has a sharp point (`Contour::sharp`): polygons, `rect` with
  radius 0, `poly` corners. Circles, ellipses and rounded rectangles are
  `Contour::smooth` and keep today's rule (evenly by length from the
  largest-x point). Every existing golden loft uses circles and rounded
  rectangles (`gen2.loft.holes`, `gen2.loft.spline`, `solid.loft`), so no
  `Loft` field was needed and no hash moved (checked by the gate). The
  unit tests with sharp squares take the new path and still pass (the
  square frame's 3 000 mm³ is exact either way).
- **Start.** Section 0 starts at its corner of largest x, then smallest
  y (today's rule, among corners). Each later section starts at its
  corner nearest the previous start by angle about its centroid: largest
  cosine, compared as `dot·|b| > dot_b·|a|` without dividing; ties to the
  earlier corner. A section with no corner in a cornered slot (a square
  to a circle) starts at its point nearest by angle.
- **Shares.** The runs between corners share the samples by length, at
  least one each, the rest by largest remainder (ties to the earlier
  run). When every section has as many corners, one share serves all
  (each run's length over its section's perimeter, summed), so corner `i`
  runs to corner `i` and a hexagon to a turned hexagon twists by the turn
  only. Otherwise each section shares its own. The count is raised to the
  most corners any section has.
- **Holes** are slots like the outline: matched by centre as before
  (§10.1), then resampled by the same rule.
- Checked: a hexagon r 10 to r 5 over 10 mm at 32 samples (32 does not
  divide by 6) is the exact frustum, `h/3 (A₀ + √(A₀A₁) + A₁)`, within
  1e-9; with even resampling five corners were cut.

**The cap fill dropped lofts open.** Resampling a polygon puts points
along its straight sides that are not exactly collinear in f64. The
cap's constrained Delaunay triangulation (`shape-2d` `triangulate`)
then has sliver triangles outside the region whose rounded centroid
falls inside, and kept them: a contour edge used twice, the solid open
and non-manifold. This happened with today's rule as well (a hexagon
marked smooth, r 10 to r 5):

| samples | 12 | 16 | 24 | 32 | 48 |
|---|---|---|---|---|---|
| boundary edges | 2 | 2 | 10 | 10 | 22 |
| non-manifold edges | 4 | 4 | 14 | 12 | 24 |
| degenerate triangles | 0 | 0 | 2 | 4 | 4 |

(a hexagon to a circle: 1, 5, 13 boundary edges at 16, 32, 48 samples).
`triangulate` now decides a triangle on a contour edge by the exact side
of that edge (`orient2d`; the region's side is known from the contour's
orientation, the outline's inside and a hole's outside), carries that
decision breadth first to neighbours across edges that are not contour
edges, and uses the centroid only for a triangle no contour edge
reaches. Every case above is closed, with no degenerate triangle. No
existing hash moved, so no earlier output had such a sliver. This is
the most likely cause of the eval's "curved loft came out open, 2
non-manifold edges" (§13.5, item 3): the binding's loft has no `curve`,
and polygon sections with `samples` hit it with or without `steps`.

### 15.2 Curved lofts (item 3)

The eval's program was not kept (`runs.log` has the failed build, not
its source); the mouse shell of `sim2` (five ellipses) does not fold with
any `steps` from 2 to 12, on Catmull-Rom or the spline. Two folds were
reproduced with other sections:

- **z overshoot (Catmull-Rom, uneven heights).** Circles at z 0, 1, 20,
  21, `steps` 4: z ran from −0.6875 to 21.6875. The tangent at section 1
  is `(20 − 0)/2` over a span of 1, so the curve leaves the end sections
  and comes back: a lip below the bottom cap and above the top. It is
  closed and does not cross itself (`self_crossings` finds nothing), so
  no check catches it. **Prevented**: when the Catmull-Rom z of the rings
  would not rise strictly, z runs straight between sections (as the
  spline's already does); x and y keep the curve, and rings stay flat.
  Where z rises strictly the output is bit for bit the old one.
- **Radial overshoot.** A natural spline through circles r 10, 10, 8,
  0.5 at z 0, 10, 11, 30: 1 958 crossing triangle pairs, closed, no
  error. Catmull-Rom through a neck, r 20, 1, 60 at z 0, 10, 20 with
  `steps` 6: the radius runs below 0 before the neck and the wall
  passes through the axis. **Detected**: `shape_model::loft` (which the
  program's `loft` op now calls) runs `shape_bool::selfcheck::
  self_crossings` on the result and refuses a fold with
  `GenError::Invalid`: "loft: the surface passes through itself between
  sections 0 and 1 (z 0 to 10; N crossing triangle pairs): the curve
  overshoots there. Add a section between them, bring the sizes closer,
  or set steps to 1 (straight)". The sections named are the lowest span
  holding a crossing triangle. Prevention by monotone interpolation was
  not chosen: it would change every curved loft's hash, and a monotone
  curve per coordinate does not stop a ring from passing through the
  axis when the sections themselves do.
- `shape-gen` cannot call `shape-bool` (which depends on it), so
  `shape_gen::paths::loft` itself still builds a folded surface; the
  model layer and the program refuse it. Moving `selfcheck` below
  `shape-gen` (into `shape-mesh`) would let the generator refuse it too;
  that is stream L's file, so it is a suggestion for L.
- Cost: the check on the binocular shell spline loft (2 428 triangles)
  is 3.1 ms on top of 2.4 ms to build it. All 65 eval programs
  (`eval/phase6/{A,B,C,T}`) still replay without error.

### 15.3 Interference (item 10)

- **What is reported.** `shape_tools::interference(&Scene)`: for every
  pair of parts, triangle pairs that **cross** (each triangle has corners
  strictly on both sides of the other's plane, by `orient3d`) and that
  **touch** (meet otherwise: faces lying on each other, an edge or a
  corner on a face; `selfcheck::tris_meet`), the box of what met, and
  **inside**: the part held wholly inside a closed other, touching from
  inside allowed, when no triangles cross. Pairs that neither meet nor
  nest are left out. No threshold: a pin seated in its hole touches it,
  a door through its frame crosses it; the agent judges.
- **Inside** is the parity of a ray from a corner of the inner part to a
  point beyond the outer one's box, every test by `orient3d`. A ray
  through an edge or a corner, or ending in a triangle's plane, is not
  counted on: the next of eight fixed directions is tried. A corner lying
  on the outer surface (exactly, by `orient3d` and `orient2d`) is
  skipped for the next, at most 16. With none left the answer is "not
  inside". Only boxes that nest are tested.
- **Pairs come from one box sweep** over every part's triangles
  (`selfcheck::box_pairs`), keeping pairs of different parts.
- **Pose.** The scene's joint values (followers from their leaders, the
  export's `check`). Parts that the pose moves by the very same motion
  (bit for bit: a door's ring, glass and handle) are compared where they
  were built, since their relative placement is unchanged: posed, a
  rotation's rounding broke the exact contact of the ring and the glass,
  which went from 7 246 touching pairs to 401 crossing ones with the door
  open. Pairs moving differently are compared posed.
- **Model.** `Model::interference(pose)`: the joints named in `pose` at
  their values (checked as `Model::pose` checks them), every other joint
  at rest; bounds in the model's unit; the model is not changed.
  `program::report_json(model, poses)` is `measures_json` with an added
  field `interference`: a list, the rest first, then each pose, each
  `{"pose": {joint: value}, "pairs": [{"a", "b", "crossings",
  "touching", "inside" (a name or null), "boundsMin", "boundsMax"}]}`.
  `measures_json` itself is unchanged (`model.washing_machine.measure`
  hashes it). The CLI, the MCP server and the binding are not touched:
  stream M wires `report_json` (or `Model::interference`) into them.

The washing machine (`golden/programs/washing_machine.json`, 7 parts,
4 014 triangles; x86-64, release, one thread):

| pose | pairs | crossing pairs (crossings) | time |
|---|---|---|---|
| rest | 5 | cabinet/drawer (8), cabinet/dial (178) | 29 ms |
| recorded: door 1.61 rad, drawer 120 mm | 4 | cabinet/drawer (67), cabinet/dial (178) | 22 ms |
| open: door 2.50 rad, drawer 200 mm | 4 | cabinet/drawer (65), cabinet/dial (178) | 24 ms |

The rest's other pairs touch: cabinet/ring, ring/glass, ring/handle. The
dial is sunk into the cabinet's face and the drawer runs through it:
both are how the program builds them, and the report says so.

Phase 1's ten sand3d rebuilds (as their programs pose them):

| program | parts | triangles | time | pairs meeting or nested | crossing | inside |
|---|---|---|---|---|---|---|
| xyz_cartesian_stage | 29 | 348 | 1.4 ms | 39 | 19 | 0 |
| floor_task_lamp | 18 | 3 228 | 16.3 ms | 26 | 20 | 0 |
| double_leaf_garden_gate | 37 | 928 | 3.6 ms | 49 | 17 | 0 |
| compact_apartment_refrigerator | 34 | 1 472 | 10.5 ms | 43 | 22 | 0 |
| skateboard | 11 | 1 332 | 3.1 ms | 10 | 8 | 0 |
| compact_theatre_binocular | 30 | 4 528 | 17.5 ms | 50 | 43 | 2 |
| compact_pickup_tailgate | 43 | 1 492 | 5.4 ms | 61 | 49 | 0 |
| compact_trackpoint_keyboard | 59 | 13 596 | 53.8 ms | 33 | 24 | 1 |
| bevel_to_spur_transfer | 30 | 11 240 | 63.4 ms | 40 | 26 | 0 |
| gate | 6 | 296 | 0.8 ms | 3 | 0 | 0 |

sand3d's assembly rule makes parts overlap to join, so crossings at rest
are the norm there. Over the 65 eval programs the costliest was
`T/sim3_toaster` (33 parts, 19 880 triangles): 379 ms; the next 93 ms.
The sweep lists large triangles against every other, which is what
grows on such models.

### 15.4 Joining tapered extrudes (item 12, for stream L)

Reproduced from `eval/phase6/B/auto2_lever_knob/program.json`: the
knob is eight tapered hexagonal extrudes, each sunk 0.5 mm into the one
below and joined in turn. Replayed here the knob has 1 degenerate
triangle (the eval's measure said 2); it appears at op 18, the join of
`band_7` into the body, and joining `band_6` and `band_7` alone gives it
too (66 triangles, closed), on `main` as on this branch. Neither input
has a degenerate triangle. The triangle lies on `band_6`'s top cap
(z = 62.046), which `band_7`'s tapered side crosses at x = 8.30072170574391:
its corners have y = 4.558071733838249, 4.792423911279415 and
4.792423911279416, so one point of the cut is computed twice, 1 ulp
apart, and the sliver between the copies has zero area. The generator's
vertices are exact (the taper scales the hexagon's corners), so the
cause is in `shape-bool`'s intersection points; it is left to L.
Reproduce: ops 15 and 17 of that program, then `join` 0 with 1.

**Cause and fix (stream O).** The two copies are not one point reached
twice. Point 28 is `band_6`'s top diagonal (edge 2–9) through
`band_7`'s side; point 29 is `band_7`'s corner edge (21–23) through
`band_6`'s top. Each crossing is strictly inside, by `orient3d`, so they
are two exact points; in rationals they lie 9·10⁻¹⁶ apart (about an ulp
in x and in y), so even correctly rounded positions differ. The cause is
in the inputs: `band_6`'s top corner, 13.2848 scaled by its taper, and
`band_7`'s bottom corner, 9.8192, differ by an ulp, so the corner edge
misses the diagonal by a rounding error. Making them one point would take
a tolerance. Instead the zero-area triangle is removed exactly
(`shape_bool::sliver`): a triangle whose three corners are distinct
positions on one line (`orient2d` zero in each axis plane) has its middle
corner strictly between the others, so the triangle across its long edge
is split there into two that tile it exactly, and the sliver is dropped.
Every edge stays balanced. A half that lands on a needle facing the other
way cancels with it. It runs on every boolean's output and in the model's
`placed` (a move): the knob's `at` moved it by −1.8·10⁻¹⁵ in x, which put
another 1-ulp needle onto a line. The knob now measures closed with no
degenerate triangle, as built and as moved.

Left: every band corner of the knob still has such a pair of points an
ulp apart, joined by needles of non-zero exact area. A later rounding can
put one on a line (then flipped away when the part moves), and one whose
f64 cross product rounds to zero is counted by `measure` though its exact
area is not zero: the bands' intersection has one, 17 mm long.

### 15.5 What is untested

- Corners: sections whose corner counts differ share samples per
  section, so corners of one land wherever the other's samples fall
  (a square to an octagon lines up only roughly). Holes that swap places
  are still matched by centre only. No `Loft` field selects the old
  resampling for cornered sections.
- Folds: only the model layer refuses them. A curve that overshoots
  without touching itself (a bulge past a section, like the z lip) is a
  valid surface: it is prevented for z only, and built as asked in x and
  y. The check costs about as much as building the loft.
- Interference: `inside` needs the outer part closed; open parts are
  only tested for meeting triangles. A part nested in another that also
  crosses a third is reported pair by pair. Parts moved by different
  joints that are exactly flush when posed (a slide by a non-dyadic
  distance) can read as crossing by rounding; only identical motions
  are compared as built.
- Every new hash is checked natively (x86-64) and as wasm32 under
  wasmtime here; aarch64 and Windows run in CI only.

## 16. The rest of the authoring surface (stream R, 2026-11-10)

Stream R makes everything `docs/design.md` §2 promises an author
reachable from the program JSON, the binding, the CLI and MCP, and
described in `guides/`, so that the IDL translated at the merge has
nothing missing. Checked against `origin/main` at a4b6046.

### 16.1 What was missing, and what was already there

| item | at a4b6046 | now |
|---|---|---|
| `superellipse` profile | kernel only | program, binding |
| `spline` profile | none (only `Path::spline_through`, open) | `shape_2d::spline` (closed Catmull-Rom), program, binding |
| path with three-point and tangent arcs | kernel `Path` only | `{"path": [...]}` profile, binding `og.path` |
| `stroke`, `hull`, 2D `mirror` | kernel only | program, binding |
| `text` | kernel only (font bytes) | program and binding by font name; CLI and MCP read the file |
| `extrude` `until` | none | program, binding |
| helix as a path | kernel only (`paths::helix` points) | `{"helix": …}` as a `sweep` or `wire` path |
| curved `sweep` and `wire` paths | points only | paths of pieces, `shape_gen::path3` |
| `merge`, `delete` | `shape_edit::merge` only; no delete | `Model::merge`, `Model::delete`, program, binding |
| `BoolReport` | kernel only | answered by `cut`/`join`/`intersect`, kept by the model, listed by `shape build` and MCP `build` |
| `check_operands` | kernel only, opt-in | `check` on `cut`/`join`/`intersect` |
| §13.6 item 9 kernel items | done by stream M | checked: `bulge`, part torus (`angle`), `flats`, `scale` all in the program and binding |
| §13.6 item 15 | open | `guides/shape.md` served |

### 16.2 Decisions

- **One path form, two dimensions.** A path is a list: a start point,
  then pieces from where the last ended — a point (a line to it),
  `{"arc": {"through", "to"}}`, `{"tangentArc": {"to"}}`,
  `{"spline": {"points"}}`. The same list is a profile's `path` and a
  stroke's `path` (`[x, y]`), and a `sweep` or `wire` path
  (`[x, y, z]`); a plain list of points, as before, is a path of lines,
  so every existing program reads the same. Quadratic and cubic
  Béziers stay kernel-only (`Path::quadratic_to`, `cubic_to`): sand3d's
  vocabulary has arcs and splines, and three-point arcs and splines
  cover what an author writes by hand.
- **The one curve rule in 3D.** `shape_gen::path3` flattens a 3D arc by
  `shape_2d::curve::segments` of its radius and sweep, and a spline's
  spans as `bezier_segments` sizes a 2D one. Arc centres come from the
  circumcentre formula and the end point is written exactly, so the
  path reaches the point given.
- **Refused, not bent.** A three-point arc on one line (decided by
  `orient2d` in 2D, a zero cross product in 3D), a tangent arc with no
  piece before it, a tangent arc whose end lies straight behind it, and
  a piece that does not move are errors that name the piece
  (`path[3].arc`). The 2D `Path` builder turns the first two into lines;
  the program does not reach that silent case.
- **A helix is a whole path**, `{"helix": {"radius", "pitch",
  "turns"}}` about +z from `(radius, 0, 0)`, placed with `on`. A helix as
  a piece in a list would need an axis relative to the current point,
  which an author cannot read off the program; a whole path is what a
  spring or a coil needs.
- **Wire bends between pieces only.** Points inside a flattened curve
  are already round; `fillet` bends only the corners between pieces
  (`Flat::corner`), and `fillets` (one radius a point) is refused with
  curves, because the points it would index are the flattened ones.
- **`until` is up to a face's plane.** `until: {part, face}` reads the
  face's frame (refused if not flat), moves it into the extrusion's
  space, and finds where each profile point's line meets the plane. A
  plane parallel to the profile (its normal exactly ±z there) gives a
  plain extrusion of that height, so the solid is exactly what `height`
  gives. A tilted plane extrudes past its highest meeting point and
  `split`s along the plane, keeping the profile's side; the cut faces
  and edges are tagged `top` as well as the split's own names, so
  `face(…, "top")` and `fillet(edges: "top")` work. Refused: `height`,
  `twist`, `taper` or `bothSides` with it (their meaning depends on one
  height), a face parallel to the extrusion's direction, and a plane at
  or below any profile point.
- **Fonts by name, read by the caller.** A `text` profile names a font
  (`"font": "fonts/a.ttf"`); the program records the name, never the
  bytes. `Session::font` and `program::run_with` take the bytes;
  `program::fonts_named` lists the names a program uses. `shape` and MCP
  read each through the command's `Files`, relative to the program file
  (the current directory, or the workspace root, for an inline
  program), so MCP's confinement applies: a font outside the workspace
  is refused as any path is. The binding takes the bytes in
  `og.model({ fonts })` and `og.replay(program, { fonts })`. The same
  program and the same font bytes give the same output; a different
  file under the same name is a different input. A file that is not a
  font is now `ProfileError::Font` (it was `Empty`).
- **Segment counts the profile cannot take are refused.** `segments`
  1 or 2 on `circle` and `ellipse`, and 1 to 3 on `superellipse`, used
  to fall back to the chord rule silently in the kernel; the program
  refuses them. The kernel constructors are unchanged.
- **`merge` and `delete`.** `merge(target, tool)` appends the tool's
  solid in the target's space with no boolean (`shape_edit::merge`);
  names stay as given, the tool is used up. `delete(id)` removes the
  node and everything under it; joints whose child goes, go with it.
  Refused, changing nothing: a joint that would hang a node that stays
  on a node that goes, and a joint that follows (`mimic`) one that goes.
  A deleted node answers `ModelError::Deleted` and its name is free.
- **Boolean reports.** `Model::combine` runs
  `shape_bool::boolean_report` and returns the `BoolReport`;
  `cut`/`join`/`intersect` now return it too (they returned `()`).
  The program answers `{voids, touchingEdges, touching}`, the binding
  returns that object, and the model keeps every report
  (`Model::booleans`), which `shape build` and MCP `build` list as
  `booleans`. Only these three ops are listed: the booleans inside
  `hole`, `pocket`, `seat`, `shell` and `split` are not.
- **`check` is opt-in, per boolean.** `check: true` runs
  `shape_bool::check_operands` before any work. It stays off by default
  for the reason of §5.4 item 3 (the kernel's own outputs cross
  themselves by an ulp and still boolean closed). A program has no op
  that imports a mesh, so imported meshes reach a boolean only after a
  mesh tool; for those, `measure --intersections` (CLI) and
  `intersections` (MCP `measure`), and `repair`'s fuse, were already
  there. `check` serves operands an author builds that may fold (a
  wire whose path crosses itself, a sweep that turns too tight).

### 16.3 Golden

`crates/shape-golden/src/rest.rs`: one program using every new profile,
path form, `until` (parallel and tilted), `merge`, `delete`, and a
checked cut that leaves a void; `rest.program.glb`,
`rest.program.measure` and `rest.program.booleans` (the reports). Every
part must be closed with no degenerate triangle, and the cut must leave
exactly one void, or the case fails. `text` is built from a font made by
`crates/shape-golden/src/font.rs`: three glyphs (`O` with a hole, `D`
with quadratic curves, `I`) in the seven tables ttf-parser needs, written
by hand as test data; no font ships with the kernel. No existing golden
line moved.

### 16.4 Measurements (x86-64, debug)

- A wire spring of radius 4, pitch 2, 3 turns, diameter 1: 1 196
  triangles, closed, one component; its ends lean with the helix, so it
  reaches 6.498 (not 6.5) at the top.
- `until` on a 4 × 4 post under a roof tilted by 0.2: the post's volume
  is 160 to 1e-9 (the prism under z = 10 + 0.2·x), the top at 10.4.
- A three-point half circle of radius 5 closed by its diameter: within
  2 % of π·12.5 and under it (chords).
- Guides: `guides/typescript.md` 8 453 bytes (was 7 417), `program.md`
  grew by the new profiles, paths and ops. Token counts were not
  re-measured (§14.3's method needs subagent runs).

### 16.5 What is untested

- `text` with a real font in the gate: only the hand-made font. A system
  font (Crimson Pro) was rendered by hand through `shape preview` and
  looked right; kerning and shaping stay absent (§8.5).
- The `until` split on faces nearly parallel to the profile: a plane
  tilted by less than about 1e-12 is not "parallel" and goes through
  `split`, whose box then reaches far; not measured.
- A tangent arc or spline whose flattened path doubles back on itself
  in a `wire` (bends sharper than the wire's radius) is left to
  `wire`'s own checks; no `wire` corner check runs inside a curve.
- No agent run (§13.1) with the new surface.
- aarch64 and Windows: CI only.

## 17. UVs in the data model (stream S, 2026-11-10)

Before this, `Solid` had no texture coordinates: generators and bevels
made none, imported UVs and textures were measured and then dropped by
repair and decimation, and the mesh-tool chain always unwrapped afresh
(§9.8).

### 17.1 The decision

- **`Solid::uvs: Option<Vec<[DVec2; 3]>>`**: one UV per triangle
  corner, f64, rounded to f32 only on output. A seam is two corners of
  one vertex with different UVs, so welding by position (canonical
  order, §6) never has to split or merge anything for UVs.
- **Convention:** (0, 0) at the bottom left of the image, v up (OBJ's,
  FBX's, the unwrap's). glTF's v runs down: import writes `1 − v`, and
  both GLB writers write `1 − v` back. In f64 both flips are exact for
  the f32 values a file holds in [0, 1], so a file's UVs survive import
  and export bit for bit; UVs computed in f64 on the way (interpolated,
  averaged) come back within f32's rounding (3–7·10⁻⁸ in UV units on the
  files below).
- **Optional, and invisible when absent:**
  - `canonicalize` rotates a triangle's UVs with it, checks them for NaN,
    and orders triangles on the same corners, surface and names by their
    UVs, as a last tie-break;
  - `hash_into` adds `"uvs"` and the values only when present.
  
  With `None`, canonical order and every hash are as before: no line of
  `golden/hashes.txt` moved.
- **Keeping it consistent:** `push_triangle` gives a new triangle (0, 0)
  at each corner when the solid has UVs; tools that add triangles call
  `push_triangle_uv`. `reverse_triangle` turns a triangle with its UVs.
  `append` of a solid without UVs to one with them (or the other way)
  gives the bare side's triangles (0, 0): zero-area UV triangles, which
  `measure` counts.
- **The texture:** an imported part keeps its first material's
  base-colour texture as the file's bytes, never decoded:
  `ImportedPart::map: Option<Texture>` (`Texture`: name, MIME type,
  bytes; three.js's `map`). Read from glTF `baseColorTexture` (the plain
  source, `EXT_texture_webp` or `KHR_texture_basisu`), OBJ `map_Kd`, FBX
  a material's `DiffuseColor` texture's `Video`. On export,
  `Material::map` (`glb`) and `PlainGlb::map` (`glb_plain`) embed it in
  the GLB's binary chunk with one sampler (linear, repeat). Only PNG and
  JPEG are written (glTF's core formats); another format, or a map on a
  part without UVs, is `ExportError::BadTexture` /
  `PlainGlbError::BadTexture`.
- **Said, not dropped:** the import's `skipped` list names what it reads
  past: a part where only some primitives (OBJ: faces; FBX: corners) have
  UVs gets none and says so; a base-colour texture on a second UV set,
  a `KHR_texture_transform` (not applied), and a material's normal,
  occlusion, emissive and metallic-roughness textures (only the base
  colour is kept).

Rejected:

- **UVs per vertex, vertices split at seams.** The usual GPU layout, but
  canonical order welds by position: split vertices would weld back,
  and every tool that compares positions (weld, the decimator's link
  condition, booleans' welding) would have to learn that two vertices
  at one point are one. Per corner keeps the solid's topology the
  geometry's. The writers split per (position, surface, UV) on output,
  as they already did for normals.
- **Per-vertex UVs plus a seam table ("wedges", Hoppe 1999).** Smaller
  in memory, but every operation must keep two structures in step; per
  corner is one array indexed like `triangles`.
- **UVs beside the solid** (the old `ImportedPart::uvs` and `Uvs`): lost
  by any operation that reorders triangles, which is every operation.
  `unwrap` still returns `Uvs` (charts, scale) and now also sets
  `Solid::uvs`.
- **f32 inside.** Interpolation (booleans, decimation midpoints) would
  round twice; f64 inside, f32 out, as positions.
- **Several UV sets.** Only the first is read; a texture on the second is
  reported, not mapped. A `Vec` of sets can replace the `Option` later
  without changing a hash that has none.
- **UVs always on generator output.** Every generator golden hash, and
  every GLB built from them, would move. Generators give UVs on request
  (§17.3).

### 17.2 Through the tools

| tool | what happens to UVs |
|---|---|
| `weld`, `remove_degenerate`, `remove_floaters` | a kept triangle keeps its corners' UVs; welding moves positions, never UVs |
| `orient` | a turned triangle turns its UVs (`reverse_triangle`) |
| `close_holes` | each rim vertex takes its UV in the rim triangle of the rim edge leaving it, a fan's centre their mean; a rim whose vertices have different UVs on their two rim triangles crosses a seam, and `RepairReport::uv_seam_fills` counts those fills |
| `fuse`, booleans (`shape_bool`) | every output triangle lies in one input triangle (`origins`), and takes its UVs by barycentric interpolation there, so a cut face keeps its mapping exactly; faces from an operand without UVs, when the other has them, get (0, 0) and `BoolReport::faces_without_uvs` counts them |
| `apply_transform`, `join` | carried; a mirroring transform turns triangles with their UVs; `join` zero-fills parts without UVs (above) |
| `decimate`, `lods`, `collapse_short_edges` | a UV seam (an edge whose two triangles give either end different UVs) is a feature line, like a surface border: a seam vertex slides only along its seam. A collapse gives each corner of the removed vertex the UV the kept vertex has on the same side, matched through the edge's own two triangles; a midpoint collapse gives both ends' corners the mean. Refused, as a turned-over face is: a collapse whose corners cannot be matched (one end with two UVs and no seam to say which applies), or that turns a UV triangle over or flattens it (`orient2d`) |
| `unwrap` | replaces the solid's UVs |
| `measure` | coverage, overlap, flipped, outside: from the solid's UVs, whatever made them (the `Measure::uvs` field is gone) |
| `glb`, `glb_plain` | write TEXCOORD_0 (v flipped) and the map |
| `fillet`, `chamfer`, `soften` | refuse a solid with UVs (`BevelError::HasUvs`), §17.4 |

The seam lines are belt and braces: with them turned off, the matching
alone already keeps the sphere test's UV error small (it refuses every
collapse across a seam), but a seam vertex could then move off its
seam's line within the surface. With both, the seams stay where they
were.

The CLI: `repair`, `decimate` and `export` of a mesh write its UVs and
its texture (the one all textured parts share). `process` keeps a mesh's
own UVs and texture and skips the unwrap (`"unwrap": null`); `--unwrap`
(MCP: `fresh_uvs`) unwraps anyway, and then writes no texture (it was
painted for the old UVs). The import report gains `"map"` on a part
that has one, so reports of files without textures are unchanged.

### 17.3 Generators

`shape_gen::solids::textured(&Generator)` builds extrude, revolve, box,
cylinder, cone, tube, sphere, torus and dome with UVs, in millimetres
on the surface (a texture with a repeat sampler tiles once per mm; scale
it by `1/size` for a texture of `size` mm):

- caps (`top`, `bottom`, `start`, `end`, a dome's flat base): planar,
  the profile's own coordinates (an extrude's twisted top: its own x, y);
- an extrude's walls: u is the arc length along the profile's contour
  from its first point, v the height above the bottom;
- a turned solid's walls: u is radius × angle (the arc length round the
  axis at that vertex's radius), v the arc length along the profile;
- each contour's closing segment ends at the full length, so a closed
  wall has one seam, at its first point, and a full turn one at angle 0;
- every face's UVs run counter-clockwise as the face does from outside:
  a surface whose UV triangles mostly run clockwise (counted exactly by
  `orient2d`) has u negated.

The same positions, triangles and names as the generator alone; the
test checks that, the orientation of every UV triangle with area, the
box's top equal to its plan, and the cylinder's wall 2π·r round and its
height up.

Why a separate entry and not a `uvs` field on each desc: the descs are
built as struct literals in about 25 files across the workspace, many
of them other streams'. A field would have meant editing all of them.
One entry adds nothing to any desc.

Not done: sweep, loft, body, wire, thread, gear and knurl (each needs
its own rule; sweep and loft would take the extrude's, along the path).
The model's program surface (stream R) does not ask for UVs yet.

### 17.4 Bevels: refused, for now

Plan phase 3 asks for UVs on bevel faces by mean-value interpolation
from the boundary. Stream W is rewriting `shape-bevel`'s layout, so the
change here is three guards: `fillet`, `chamfer` and `soften` refuse a
solid with UVs (`BevelError::HasUvs`), never returning it without them.
The way in, once W lands, is in the assembly step of `bevel.rs`, where
each output triangle is either an old face's (`unit` `None`, with its
source triangle) or a new face's:
- an old face's corners take UVs by barycentric interpolation in their
  source triangle's plane, as booleans do (its corners only slide within
  that face);
- a new face's vertices take the mean-value coordinates (Floater 2003)
  of their unit's boundary loop, flattened onto the unit's mean plane,
  with the boundary's UVs from the old faces next to it. A unit whose
  boundary crosses a UV seam should be split along it, or report it.

### 17.5 Acceptance (`bench/meshtools`, `uv-check`; x86-64, release)

Outside textured GLBs (Khronos glTF-Sample-Assets, fetched, not
committed): `import → join → repair (weld 1 µm, floaters 5 %, holes ≤ 64
edges) → decimate to half → glb_plain` with the file's base-colour
texture, read back by the `gltf` crate and by our importer, then loaded
by three.js r160's `GLTFLoader` in headless Chromium.

UV error: for every output corner, the UV the input surface has at the
nearest point (interpolated in the input triangle there; on a seam or an
edge, the nearest triangle whose UV is closest) against the corner's
own, in UV units and in texels of the texture
(`crates/shape-tools/tests/support/uverror.rs`).

| file | triangles | repair UV error | decimate to half: UV error max / p99 / mean (texels) | surface moved | GLB round trip (corner by corner) | texture |
|---|---|---|---|---|---|---|
| Duck (512² PNG) | 4 212 → 2 106 | 0 | 0.33 / 0.062 / 0.003 | 2.45 mm (of ~3.3 m) | 4·10⁻⁸ | same bytes back; three.js: `map` 512×512, `uv` 1 224 |
| BoxTextured (256² PNG) | 12 → 8 | 0 | 0 / 0 / 0 | 0 | 0 | same; three.js reads it |
| Avocado (2048² PNG) | 682 → 718 (1 hole filled) → 358 | 0 | 6.5 / 1.0 / 0.055 | 0.16 mm | 3·10⁻⁸ | same; three.js reads it |
| DamagedHelmet (2048² JPEG) | 15 452 → 15 796 (69 holes filled, 59 across a seam, 12 left) → 7 898 | 0 | 404 / 2.2 / 0.33 | 6.9 mm | 7·10⁻⁸ | same; three.js reads it |

- **Repair** changes no UV of a triangle it keeps (error 0 against the
  import on every file).
- **Decimation:** the mean error is a third of a texel or less, p99 at
  most 2.2 texels. The helmet's maximum (404 texels) is where the surface
  itself moved 6.9 mm: the nearest input point lies on another part of
  the model, in another chart. Duck and Avocado have no such spot.
- **Hole fills** add what the measure shows: on Avocado one fill raises
  UV overlap from 0 to 6.9 % of texels (its fan covers part of a chart);
  on the helmet, 59 of 69 fills cross a seam and are reported.
- **Flipped UV triangles** (clockwise against their face): half of
  Duck's and the helmet's, as the files have them (their charts are
  mirrored); decimation keeps the share (Duck 2 075 of 4 212 → 1 050 of
  2 106). Fill triangles add their own (Avocado 1 → 37).
- The helmet's UVs lie in v ∈ [1, 2] in the file (−1..0 here, after the
  flip): it relies on the repeat sampler. `measure` reports coverage only
  in the unit square, so 0 there; `outside` counts every triangle.
- **Seen:** three.js renders of the full and the half model side by
  side, `docs/previews/uv/duck-full-half.png` and
  `damaged-helmet-full-half.png`. The duck is the same. The helmet's
  decals and panels sit where they did; its visor and metal look
  different because only the base colour is kept (its emissive,
  metallic-roughness, normal and occlusion maps are reported as skipped).
- Time (helmet): repair 142 ms, decimation 52 ms.

Golden: `uvs.generators` (each `textured` generator), `uvs.chain` (a
textured sphere as a soup with a hole, repaired and decimated),
`uvs.boolean` (a textured block cut by a pin without UVs), `uvs.glb`
(scene and plain GLB with a PNG map, and the scene read back). Tests:
`crates/shape-mesh` (UVs follow corners through canonical order),
`shape-gen` (`textured`), `crates/shape-tools/tests/uvs.rs` (GLB round
trip with the `gltf` crate, decimation, repair, a cut, OBJ `map_Kd`),
`crates/shape-bevel/tests/uvs.rs` (refusal), and the FBX fixture now has
a `DiffuseColor` texture.

### 17.6 Gaps and what is untested

- Tools not audited for UVs, which may drop them silently: `shape-edit`'s
  `dent` and `split` (they rebuild solids), `shape-skin`'s refinement,
  and `shape-model`'s placement. Generators feed them no UVs today
  (`textured` is opt-in and the program surface does not ask for it),
  so nothing reaches them yet; each should carry or refuse before it
  does.
- Hole fills take rim UVs as they are: a fill across a seam stretches
  (reported), and a fill inside one chart can overlap the chart's other
  triangles (measured, not reported by repair).
- One texture per joined solid: `shape process` and `glb_plain` write
  one material; parts with different maps lose theirs on the plain GLB
  (`Mesh::map` is `None` then). `glb` of an imported mesh keeps each
  part's.
- No UV-aware quadric: decimation keeps UVs by matching and refusing,
  not by adding UV error to the cost (Hoppe 1999's attribute quadrics).
  The UV error above is what that leaves.
- Generators beyond extrude, revolve and the primitives; bevels (§17.4).
- The three.js check ran by hand (`GLTFLoader.loadAsync` in Chromium,
  `uv` and `map` present, renders compared by eye), not in CI.
- FBX textures: checked on the hand-written fixture only; the three
  real FBX files tried have no textures.
- Determinism of the new hashes: native x86-64 and wasm32 under wasmtime
  here; aarch64 and Windows in CI only.

## 18. The boolean cases still refused (stream T, 2026-11-10)

The open items of §5.4 and §13.6 for `shape-bool`, each refused with an
error value on a4b6046. Every side is still decided by `orient2d` and
`orient3d`; nothing below adds a distance test to a decision, except the
snap of item 1, which only moves a tool's vertex onto a target vertex
already within 2⁻⁴⁰ of the operands' size.

### 18.1 Turned elbows (§13.6 item 1, §5.4 item 5)

**Cause.** The two elbows are tori turned half a facet about their axis,
each cut by a box whose face is the world plane z = 0. A part's solid
lives in its own frame, and the model hands the boolean its tool in the
target's frame through the world (`placed` twice), so the tool's ring
face comes back a few ulps off the target's. Measured on the join's
operands (dumped from `golden/programs/touching_elbows_turned.json`):
134 of the tool's vertices sit exactly on a target vertex, 126 within
2⁻⁴⁹ to 2⁻⁴⁴ (≈ 6·10⁻¹⁴ mm at this 1 000 mm scale). The ring faces cross
at 10⁻¹⁶ angles: both passes stay open (8 unmatched edges), the
intersection and the cut close.

**Fix: the design's snap, as a third pass** (`csg_points_traced`,
`snap_onto`). `docs/design.md` §6 already says coordinates within a weld
distance of the target's are snapped onto them before a boolean; nothing
did it. It is done now, only where both passes leave the result open or
not manifold: each tool vertex within 2⁻⁴⁰ of the operands' box diagonal
of a target vertex, and not on one, moves onto the nearest (ties: lowest
index), and both passes run again. The snapped result is taken only if it
has fewer unmatched edges, or as many and fewer edges not used once each
way. `CsgOutput::snapped` says so. Every result that was closed is built
as before.

**Found on the way: an empty union.** On solid (unbored) turned elbows,
the second pass grouped A's ring face with its outer surface into one
region, dropped it whole, did the same to B, and returned no triangles:
zero unmatched edges, so it beat the open first pass, and `boolean`
answered `Empty`. A union of two closed solids is never empty; such a
result now ranks below any open one, and the snap then closes it (volume
twice one elbow's, to 10⁻¹⁵).

**Results.** The program joins closed, 9 752 triangles, volume
16 954 637.804 37 mm³, against 16 954 637.804 37 for the unturned
elbows (§5.4 item 5). Tests: `elbows_turned_about_their_axis_join_closed`
(`crates/shape-bool/tests/touching.rs`: the kernel-level case, refused
with 67 unmatched edges without the snap) and the golden program test,
now `the_cup_rim_and_the_elbows_join_closed`.

### 18.2 The untangled folded sphere against the centred bar (§5.4 item 3)

Re-measured first: on a4b6046 all three booleans of the untangled
sphere (984 triangles) with the centred 10 × 10 × 60 bar come out
closed, by the second pass, with V(∪) + V(∩) = V(A) + V(B) and
V(−) = V(A) − V(∩) to 10⁻¹² (8 307.82, 1 727.50, 2 307.82 mm³). It was
already closed before stream O merged (checked at a382068's first
parent), so stream L's second pass fixed it, not O's sliver flips. The
test that said the centred bar comes out open now asserts that it closes
and keeps the identities, centred and moved.

### 18.3 A hierarchical winding number (§5.4 item 3)

`self_union` classified each region by the winding number of every other
triangle at its largest piece's centre: one direct sum per region, so a
noisy operand with thousands of regions cost seconds.

`crates/shape-bool/src/winding.rs` builds a hierarchy of triangle
clusters (median splits along the longest axis of the centroids, eight
triangles a leaf, ties by index). A cluster far from the point counts by
the kernel's expansion about its centre `c`, to first order:
`K(c)·N + Σᵢⱼ ∂ⱼKᵢ(c) Mᵢⱼ`, with `K(x) = (x − p)/|x − p|³`, `N` the
cluster's vector area and `Mᵢⱼ = Σ vᵢ (g − c)ⱼ` (exact: the integrand is
linear over each triangle). Its error is at most `4 S / ρ⁴`, where
`S = ∫|x − c|² dA` and `ρ` is the point's clearance from the cluster's
ball; `|∂²K| ≤ 6/|x|⁴` (the sup, at x̂ = u = v), taken as 8. A
first-order bound (`2 r A / ρ³`) was tried first: on a noisy surface it
opened about half the triangles per query. Near triangles are summed
term by term with the direct sum's own arithmetic.

**Decisions are the direct sum's.** Each query has thresholds (0.5 for a
probe; 0 and 1 for a self-union's centre with its own triangle left
out). The tree's value is used only where it clears every threshold by
more than its bound, plus 16 ε·π per far triangle (each far term is well
conditioned) and 4·n·ε of the terms' sizes (either sum's rounding, in any
order). Clusters are tried coarse first (θ = 1/64 rad per cluster), then
1/512, then 1/4 096; where none clears, the direct sum decides. Since
near terms are bit for bit the direct sum's, the two sums differ by at
most that bound, so the decision, and the output, cannot differ. The
unit test checks `|tree − direct| ≤ bound` at every θ on a noisy,
self-crossing ball (every centroid, a grid of probes); it fails with the
second-order term's sign flipped or the bound 10× too small. On the
bench's synthetic suite (untangled) no query fell through to the direct
sum. Counted with an earlier cascade (1/32, 1/256, 1/2 048): on the noisy
sphere, box, torus and blob nearly every query was decided at the
coarsest level; on the mug, whose thin walls put sheets close together,
839 of 3 473 were, and the rest at the next.

The two-operand classification (`arrange`) uses the same tree once a
mesh has 256 triangles and 16 regions query it.

### 18.4 A countersink's ring of coincident edges (§5.4 item 4)

Tested with the tools unturned: a countersink cone whose narrow end is
the bore's own 24-gon at the same angles, so its ring of vertices lies on
the bore's vertex columns and its ring edges in the bore's wall planes
(the hole tool itself still runs the cone on to 0.9 r).

**Found: a NaN.** With the bore cut first, its wall pieces are bent by
the first cut's rounding (a corner at z = 3.6·10⁻¹⁵ for 0). A ring edge
lies in the true wall plane, so `orient3d` sees it cross a bent piece
strictly, while the f64 plane distances `dp`, `dq` round to equal values:
`dp / (dp − dq)` is 0/0, and the result was refused as "a position that
is not finite". Where that fraction falls outside [0, 1], it is now
taken from `orient3d`'s values instead, whose signs are exact and
opposite, so the point lies strictly on the edge. Every other point is
computed as before. The fix also let the turned elbows' first pass
finish with the right volume but 11 edges used four times (ulp-wide
cracks between the ring faces); the snap is therefore also tried for a
closed result with such edges, and taken when cleaner (above).

Measured on a plate 40 × 40 × 10, both cut orders: through bores of
r 2 (mouth 4) and 3.3 (mouth 6.6), a blind one 6 deep: all closed, the
two orders' volumes equal to 10⁻¹⁴. Test
`a_countersink_ending_on_the_bore_closes_in_either_order`
(`crates/shape-bool/tests/edge_on_edge.rs`); golden
`touching.countersink.ring`.

### 18.5 Why the kernel's own outputs cross themselves (§5.4 item 3)

**Where the rounding comes from.** An edge of one operand passes within
rounding of an edge of the other. In exact arithmetic the two surfaces
then meet along a segment about 10⁻¹⁶ long: edge e crosses triangle T at
P₁, edge f (of T's operand) crosses triangle S (e's neighbour) at P₂, and
P₁P₂ is the exact intersection of T and S. The arrangement keeps both
points and the needles between them, as it must. Rounding P₁ and P₂ to
f64 moves each by up to half an ulp, which can turn a needle through its
neighbours: the pairs `crossing_pairs` reports are those. The countersink
rim: the cone's generators lie at the bore's column angles, but their
plan positions (`0.9 r·(c, s)`, `R·(c, s)`, plus the hole's offset) round
independently, so a generator misses its column by an ulp, and each rim
point is made twice: column through cone (z = 7.999999999999999) and
generator through wall (z = 7.999999999999998, x one ulp off). With
`slope = tan(π/4)` exactly 1 the rounding happens to agree and the rim is
clean; `trig::tan` gives 0.9999999999999999, and the rim has 4 crossing
pairs (bore first) or 3 (cone first).

**Can the edit tools emit crossing-free rims?** The countersink's, yes:
turning the cone half a segment about the axis puts its generators
between the bore's columns, and the rim has no crossing pair in either
cut order (measured on the hole of `edit.holes.pocket`). That moves
`edit.holes.pocket`, and only it, so it is **not done**: it needs the
orchestrator's go-ahead.

**Can the check then be on by default? Not cheaply.** With
`check_operands` called first in `boolean`, four golden lines move:
`selfunion.check` (its inputs cross themselves by design; it is refused
up front instead of after the result), and three chains of the kernel's
own booleans whose intermediate solids cross themselves:
`edit.holes.pocket` (the countersink rim, 4 pairs), `smallgaps.knob.program`
(6 pairs: the band corners of §15.4, two points an ulp apart joined by
needles) and `touching.program.elbows` (68 pairs: the torus's ring of
vertices 8·10⁻¹⁴ off the box face). With the countersink turned, the
last two remain. The cure is snap rounding of the arrangement (rounding
points to f64 and then splitting every needle that rounding made cross,
until none does), or a tolerance that makes the two points one, which
the design avoids. Neither is cheap: the check stays opt-in. The bench's
`--crossings` count (below) measures how often outputs cross.

### 18.6 Degenerate triangles in `measure` (§13.6 item 12)

**Rule: a triangle is degenerate when its three corners lie exactly on
one line** (`orient2d` zero in each axis plane). That is the rule of the
mesh tools' `measure`, `drop_slivers` and the self-crossing check;
`Solid::measure` (the program's measures) counted a triangle whose f64
cross product is zero instead. A needle of non-zero exact area is a real
triangle: the booleans keep it (removing it would open the surface), and
it is not counted. Its f64 area and normal may still round to zero, so it
adds nothing to the area and needs no normal of its own when shaded
smoothly. The knob bands' intersection (§15.4) measured 1 degenerate
triangle and now 0; no golden line moved. Test
`a_needle_of_non_zero_exact_area_is_not_degenerate` (`shape-mesh`).
This is a two-line change in `crates/shape-mesh/src/measure.rs`, outside
this stream's crates. One test outside them relied on the old rule:
`shape-bevel`'s `a_hole_s_rims_wider_than_their_pieces` asserted that the
holed plate has a degenerate triangle to show its rims come in needles;
it now asserts that directly (a triangle whose f64 cross product is
zero).

### 18.7 Benchmark (`bench/booleans`, x86-64, release, one thread)

Before (a4b6046) and after, run back to back on one machine, engines
og-geom, og-shape-checked and og-shape-untangled, every suite (Thingi10K
downloaded by `fetch-thingi10k.sh`):

| suite | engine | before: ok / refused / wrong, mean ms | after |
|---|---|---|---|
| sand3d (17) | og-geom | 17/0/0, 2.9 | 17/0/0, 2.9 |
| | og-shape-checked | 17/0/0, 4.2 | 17/0/0, 5.0 |
| | og-shape-untangled | 16/1/0, 5.3 | 16/1/0, 5.7 |
| Thingi10K (40) | og-geom | 40/0/0, 111.3 | 40/0/0, 116.2 |
| | og-shape-checked | 38/2/0, 221.7 | 38/2/0, 224.8 |
| | og-shape-untangled | 40/0/0, 272.3 | 40/0/0, 271.2 |
| synthetic (40) | og-geom | 19/0/21, 92.5 | 19/0/21, 92.7 |
| | og-shape-checked | 24/16/0, 329.9 | 24/16/0, 332.3 |
| | og-shape-untangled | 29/11/0, 926.6 | 29/11/0, **415.9** |
| touching (7) | og-geom | 4/0/3, 0.5 | 4/0/3, 0.5 |
| | og-shape-checked | 7/0/0, 22.1 | 7/0/0, 22.8 |
| | og-shape-untangled | 7/0/0, 28.1 | 7/0/0, 28.1 |
| scaling (5) | og-geom | 5/0/0, 180.2 | 5/0/0, 172.1 |
| | og-shape-checked | 5/0/0, 377.6 | 5/0/0, 365.7 |
| | og-shape-untangled | 5/0/0, 517.4 | 5/0/0, 524.0 |

All 921 output hashes are the same before and after. No verdict moved:
the bench's meshes are f32, so the cases this stream closes (operands a
few f64 ulps apart, a countersink ring cut from a bored f64 plate) do not
occur in it; they are in the tests and the golden. Other differences are
run-to-run noise (an earlier run of the same before-binary gave 5.1 and
5.8 ms on sand3d).

Untangling (the untangled engine's operand time, three operations):

| synthetic noise | before ms | after ms | |
|---|---|---|---|
| sphere (15 924) | 4 683 | 597 | 7.8× |
| rounded box (17 584) | 5 181 | 568 | 9.1× |
| torus (9 456) | 1 674 | 325 | 5.1× |
| blob (8 236) | 1 583 | 314 | 5.0× |
| mug (14 220) | 4 696 | 898 | 5.2× |

Operands with few regions (floaters, soup, two overlapping copies) use
the direct sum as before and take the same time, within noise. What
remains of a noisy operand's time is the pair tests, the cut and the
regions (about 100 ms of the mug's self-union), and the mug's
classification at θ = 1/512.

`--crossings` (outside the timing): of the ok cases' f64 outputs, those
that cross themselves within one shell are sand3d 0 of 17 and 16,
Thingi10K 0 of 38 and 40, scaling 0 of 5, synthetic 1 of 24 and 29
(`rounded.box.degenerate`, 2 pairs: its input's vertices are rounded
onto grid corners), and touching 1 of 7 (`torus.ring.on.face`: 28 pairs
checked, 88 untangled; the torus's ring of vertices a rounding error off
the box face, §18.5).

### 18.8 What is untested

- The snap moves only tool vertices onto target vertices. Faces a few
  ulps apart whose vertices do not correspond (one ring face triangulated
  differently from the other, or a vertex near the other's edge rather
  than its vertex) are not snapped, and may still be refused.
- The snapped pass is tried only on results that are open or not
  manifold; a closed result with ulp-wide gaps between faces that should
  be one is kept as built.
- The winding tree's remainder constant (6, taken as 8) was found by
  sampling, not proved here.
- The edit tools' countersink turn (§18.5) is measured on the plates
  above only, and not merged.
- Every new hash is checked natively (x86-64) and as wasm32 under
  wasmtime by the gate here; aarch64 and Windows run in CI only.

## 19. Validation with real consumers (stream U, 2026-11-10)

Stream U ran what §8.5, §12.4, §13.7 and §14.5 left untested against
the consumers the kernel will meet: node, a bundler and a browser for
the binding; three.js with a renderer for the GLBs; a real MCP client for
the server; a mesh of a million triangles; and the phase 6 objects
through the server. Code: `crates/shape-wasm/ts/tools/` (consumers,
render, scale), `crates/shape-mcp/` (progress, cancellation, paths, the
SDK client in `ts/client/`), `eval/phase6/mcp_run.ts` and `U/`.

### 19.1 The binding outside bun

`node tools/check_consumers.ts` hashes the washing machine's GLB and
preview PNG (`washer_open()`, as `binding.test.ts`) with WebCrypto in
each consumer and compares them with `golden/hashes.txt`:

| consumer | GLB | PNG | build + GLB + PNG |
|---|---|---|---|
| node 22.23.4, `src/index.ts` imported as it is | golden | golden | 300–490 ms |
| bun 1.4.4, the same | golden | golden | 450 ms |
| esbuild 0.28.2 bundle (ESM, platform node), run by node and by bun | golden | golden | 250–500 ms |
| esbuild bundle (platform browser) in headless Chromium 153 (Playwright), wasm fetched | golden | golden | 350–490 ms |

What broke, and what changed:
- **node refused the binding**: its type stripping does not take
  TypeScript-only syntax, and `Part`, `Frame` and `Shape` used
  parameter properties (`constructor(readonly id: number)`). They are
  plain fields now; nothing else in the binding needed a transform. The
  gate's binding step runs the node check when node ≥ 22.18 is
  installed, and CI installs node 22.23.4 on the wasm job.
- **A bundle runs the example's script block.** `examples/washing_
  machine.ts` ends with `if (import.meta.main) { … }`; in a bundle every
  module shares the bundle's `import.meta`, so importing the example ran
  its file writing. The checks bundle with `import.meta.main` defined
  false. An author who bundles a module with such a block meets the same.
- **In a browser bundle, errors lose their call site.** The binding
  finds the author's line as the first stack frame outside its own file;
  bundled, the binding and the author's code are one file, so no frame
  qualifies and the message ends without `(at …)` (seen in Chromium:
  `op 0 cylinder "rod": cylinder has no field "segmentz"; …`). The op,
  index and part are still named. Not fixed: it needs source maps, or a
  marker the bundler keeps.
- `load()` with no argument reads the wasm with `node:fs`; a page passes
  the bytes (`load(await (await fetch(url)).arrayBuffer())`), which
  works. The browser bundle stubs `node:` imports.
- Vite was not tried; esbuild is the bundler Vite uses for dependencies,
  but Vite's own module graph and wasm handling were not exercised.

### 19.2 three.js with pixels

`node tools/check_render.ts` loads the GLBs with three.js 0.186.1's
GLTFLoader in headless Chromium 153, renders with WebGLRenderer (WebGL 2
on SwiftShader, a CPU implementation), and measures; the screenshots are
in `docs/previews/validate/` (washer at rest, open as exported, moved in
three.js to the door's 143° and the drawer's 200 mm limit; the figure at
rest, `bend` and `wave`). Images are not compared.

- **Scene graph** (washer): `washing_machine` → `cabinet` (mesh) →
  joint nodes `drum_spin`, `door_hinge`, `drawer_slide`, `dial_turn`,
  each with its `s3d_joint` extras, holding `drum`; `door` (`ring`,
  `glass`, `handle`); `drawer`; `dial`.
- **Joint motion**: the open file re-posed to rest in three.js matches
  the rest file to 1.2e-16 m, and the rest file posed open matches the
  open file to 1.9e-16 m; at 143° the door reaches x = 0.670 m; the
  drawer slides 0.200000 m.
- **Skin** (`cargo run -p shape-skin --example figure_poses` writes the
  figure's GLB and the kernel's `pose` of every vertex): 14 581 GLB
  vertices (13 077 in the kernel, split by normals), 26 150 triangles,
  10 bones. The bones are posed by the kernel's turns (rest rotation,
  then each turn about the bone's rest axes, Z up → Y up). For rest,
  `bend` and `wave`: every GLB vertex has a kernel vertex at the same
  rest position; three.js's skinning (`SkinnedMesh.getVertexPosition`,
  the shader's linear blend done on the CPU) is within 1.5e-5 mm of the
  kernel's, f32 rounding; vertices that share a rest position stay
  together (0 mm apart), so the skin does not tear at the normal splits.
  The largest edge stretch is ×1.009 at rest (rounding of short edges),
  ×2.04 in `bend` and ×4.24 in `wave`: a measurement for the author, no
  verdict here.
- The GPU's skinned positions were not read back; the screenshots show
  them, and they look as the kernel's previews do.

### 19.3 The MCP server with real clients

- **The official TypeScript SDK** (`@modelcontextprotocol/sdk` 1.32.2,
  `Client` over `StdioClientTransport`) drives the release binary
  (`crates/shape-mcp/ts/client/client.ts`, all 34 checks pass):
  initialize, ping, the ten tools' schemas, the four guides read back
  byte for byte, `build` of a JSON program with stream M's ops (`wire`
  with fillets and segments, `gear` with a bore, `knurl`, a `+Z` torus
  with segments, a hexagon across flats, a poly with a bulge under
  `scale`, `body`, the `scale` op), `preview` (PNG image content, 320 ×
  160, and `out`), `measure`, `export` to GLB, OBJ and STL,
  `run_typescript` with the new ops in the binding (`wire`, `torus`,
  `og.scale`, `og.polygon` flats), `import` of a binary FBX (centimetres,
  two materials, embedded PNG) and of a `.gltf` whose buffer and image
  are files beside it, `repair`, `decimate` with LODs, `unwrap`,
  `process` of the FBX, progress and cancellation (§19.4), and refusals
  (an unknown argument, a path outside the root) as tool errors. Call
  times on the small inputs: 0–66 ms each.
- **The MCP Inspector's CLI** (`@modelcontextprotocol/inspector` 0.17.2,
  `--cli`): `tools/list` (10), `resources/list` (4), `resources/read`,
  `tools/call measure` and `process` with `--tool-arg budget=1000` (the
  inspector types the argument by the schema) all answer.
- **Claude Code** 2.1.322: `claude mcp add --scope local og-shape --
  shape-mcp --root …` then `claude mcp list` reports "✓ Connected" (the
  `--scope project` form waits for approval in an interactive session,
  as `guides/mcp.md` implies). Headless agents used it for run `U`
  (§19.7).
- **`guides/shape.md` is served** as `og-shape://guides/shape.md`, listed
  first, and the server's instructions say to read it first (§13.6
  item 15). `guides/mcp.md` lists it.
- **No `structuredContent` or `outputSchema`.** The SDK client validates
  `structuredContent` against a tool's `outputSchema` when the tool
  declares one, and a result without it, or not matching it, becomes a
  client-side error (`client/index.js`, `callTool`). The reports are
  written by the kernel and differ by input (a program's measures are
  camelCase, a mesh's snake_case, with nulls for skipped steps); a schema
  for each would be a second description of every report, able to
  drift, and its failure mode turns a correct result into an error. An
  agent reads the text content either way, and the specification asks
  for the same JSON as text beside `structuredContent`. So the reports
  stay JSON text; a typed client can parse them.
- **Interference is not reachable from the server**, the CLI or the
  binding: §15.3's `program::report_json` is used only by the golden
  case. §13.6 item 10 said the front ends were stream M's to wire; they
  are not wired on main.

### 19.4 Long calls: progress and cancellation

The kernel stays single-threaded. The server reads its input on one
thread of its own (lines into a channel; `std::thread::spawn` is allowed
there by name, the only thread) and serves on the main thread:

- Before each step of `process` (import, repair, decimate, unwrap,
  measure, export: `shape_cli::process::{STEPS, process_steps}`, a step
  hook added to `shape-cli`) and of `run_typescript` (run under bun;
  replay and measure), the server takes every line read so far:
  `notifications/cancelled` for this call stops it there, a `ping` is
  answered at once, anything else waits for the call to end. With
  `_meta.progressToken` it then sends `notifications/progress`
  (`progress` k of `total`, `message` the step).
- While bun runs, the server checks every 10 ms, and a cancellation kills
  bun.
- A request cancelled while it waits in the queue never runs. A
  cancelled request is not answered (the specification's SHOULD).
- A single kernel call is never interrupted: `repair` alone, or the
  decimate step of `process`, runs to its end (§19.6: 7 s and 14 s at a
  million triangles).

Tests (`crates/shape-mcp/tests/long_calls.rs`) gate the client's input
so that each is deterministic: lines after the gate are released when
the server sends its first progress notification, and the server's
output waits until the reader has passed them on. They check the six
progress notifications, a ping answered during a call, a queued request
dropped, a `process` stopped before its second step with nothing
written, and bun stopped (the program loops forever; the call ends in
well under the 60 s timeout). Through the SDK client, an `AbortSignal`
on `run_typescript` returns control and the next ping answers 205 ms
after the call started (cancelled 200 ms in).

### 19.5 Path confinement

A path was checked, then opened by name, so a directory or link swapped
in between was followed. Now (`crates/shape-mcp/src/workspace.rs`):
- a read opens the file, then resolves its path again and requires the
  real path under the root and, on Unix, the opened file and the file at
  the real path to be one (device and inode); the size cap is read from
  the opened file and enforced while reading;
- a write creates a new file beside the target (`create_new`, which
  never follows a link), confirms it the same way, writes, and renames it
  over the target: a link put at the target is replaced, not written
  through, and a swapped directory leaves no file outside (the new file
  is removed when the check fails).

The race is played in order by a test (`a_swap_after_the_check_is_
refused`, Unix): resolve, swap `sub/` for a link out, then read and write
are refused and the outside directory is unchanged; it fails on the old
code. Windows: std gives no stable file identity, so the opened file's
path is resolved again but not compared by identity. `windows_paths`
(`#[cfg(windows)]`, so CI's x86-64 Windows target runs it) checks that
the root is `\\?\…`, that relative, plain absolute, verbatim and
slash-separated paths inside resolve under it, that `..`, another
directory and another drive are refused, a write and read back, and a
file link out when the runner may create links. Here it was type-checked
and linted for `x86_64-pc-windows-msvc` but not linked or run (no MSVC
linker).

Left: `run_typescript` with a `path` checks the file, then hands the
path to bun, which opens it again.

### 19.6 Scale: a million triangles

The mesh: the XYZ RGB dragon as distributed in
`alecjacobson/common-3d-test-models` (`data/xyzrgb_dragon.obj`,
249 882 triangles, a decimated copy of the Stanford scan; Stanford's own
server failed TLS through this container's proxy). No public copy of a
million triangles was reachable, so `tools/scale.ts --densify` split each
triangle into four at its edge midpoints and moved each new vertex a
sixteenth of the way towards its opposite corners' midpoint (so the four
are not coplanar): 999 528 triangles, a 43 MiB OBJ, open (the source has
1 non-manifold edge and 1 duplicate triangle, so 5 and 4 after the
split). Read in millimetres (`--unit-mm 1`, about 202 × 134 × 91 mm).
Each step is a run of the release `shape` CLI (x86-64, 4 cores, one
used), peak memory the child's `VmHWM` sampled every 5 ms:

| step | input | time | peak memory |
|---|---|---|---|
| import (OBJ → GLB) | 1.0 M | 1.29 s | 246 MiB |
| repair (zero values) | 1.0 M | 6.82 s | 799 MiB |
| decimate `--target 100000` | 1.0 M → 100 k | 13.71 s | 600 MiB |
| unwrap | 100 k | 0.95 s | 61 MiB |
| measure | 100 k | 0.44 s | 50 MiB |
| export (STL) | 100 k | 0.04 s | 32 MiB |
| `process --budget 100000` (all of it, from the OBJ) | 1.0 M | 19.58 s | 799 MiB |
| measure (from the OBJ) | 1.0 M | 4.43 s | 404 MiB |
| unwrap | 1.0 M | 11.5 s | — (5 583 charts) |
| measure `--intersections` | 1.0 M | 4.6 s | — (91 pairs) |
| repair `--fuse` / `--intersections` | 1.0 M | 7.9 s / 7.3 s | — |

The decimated mesh: 100 000 triangles, error 0.095 mm, 31
self-intersecting pairs; the chain's output GLB has 2 113 charts.

What does not scale, or misleads at this size:
- **Self-intersections are over-counted on a mesh with seams.** On the
  unwrapped 100 k GLB the part reports 119 923 pairs where the mesh (its
  `whole`, welded) has 31: the count takes vertex ids as given, so the
  triangles either side of a UV seam, which share positions but not ids,
  "touch". Reproduction (any processed GLB):
  `shape process golden/meshes/ball_defects.obj --budget 1000 -o b.glb`,
  then `shape measure b.glb --intersections`: the part (closed) reports
  789 pairs, the whole 0. A fix of a few lines welds exact positions
  before counting (`shape-tools/src/measure.rs`), but it moves the
  golden line `meshtools.measure.json` (its raw mesh has split vertices
  too), so it was not applied: the orchestrator decides.
  **Applied at the merge** (`weld_exact` in `measure.rs`): the part of
  the reproduction now reports 0, as the whole does; repair's own report
  (`repair.rs`) counts the same way.
- **Repair's memory**: about 800 bytes a triangle at peak (799 MiB for
  1 M); a 10 M scan would need about 8 GiB. Decimation, at about 30 µs a
  collapse (450 000 of them), dominates `process`.
- **Unwrap of the full mesh** takes 11.5 s for 5 583 charts; the chain
  unwraps after decimating, as `guides/meshtools.md` says.
- **The server's limits**: the default `--max-bytes` (64 MiB) admits
  this OBJ (43 MiB); about 1.5 M triangles of OBJ text exceed it. A
  `process` call of 20 s outlives no SDK default (60 s), but at about
  3 M triangles it would, unless the client resets its timeout on
  progress (`resetTimeoutOnProgress`), which §19.4 now makes possible.
- Everything is one thread; nothing here is parallel, by the rules.

### 19.7 Phase 6 objects through the MCP server (run `U`)

Run `M`'s five objects again (`eval/phase6/mcp_run.ts`): one headless
Claude Code agent each (`claude -p`, model `claude-opus-5-5`, the same as
the stream), whose only tools were the og-shape server's and Read, with
the workspace a fresh directory holding `brief.md` and `reference.png`.
The agents read `guides/shape.md` and `guides/typescript.md` as MCP
resources (the guides as now on main, with stream M's last fixes), ran
programs with `run_typescript`, looked with `preview`, at most 8 builds.
Results in `eval/phase6/U/<object>/` (`program.ts`, `program.json`,
`preview.png` and `measure.json` from `run.ts` as in the other runs,
`runs.log`, the agent's `report.md`, `usage.json`).

| object | M: it. (err.), triangles, size dev. | U: it. (err.) | U: turns | U: output tokens | U: final context | U: triangles | U: size dev. | U: joints |
|---|---|---|---|---|---|---|---|---|
| auto1_curved_tubes | 3 (1), 28 632, 0.0 % | 1 (0) | 8 | 7 044 | 49 836 | 8 250 | 0.0 % | 1/1 |
| auto2_lever_knob | 2 (0), 498, 0.0 % | 3 (1) | 11 | 6 571 | 54 378 | 822 | 0.0 % | 2/2 |
| sim2_mouse_and_pad | 5 (0), 8 238, 0.1 % | 4 (0) | 13 | 7 731 | 57 899 | 7 886 | 0.3 % | 1/1 |
| obj2_djembe | 5 (0), 14 268, 0.0 % | 1 (0) | 8 | 3 015 | 48 637 | 6 008 | 0.0 % | 0/0 |
| obj4_spectacles | 7 (5), 3 324, 0.0 % | 1 (0) | 12 | 11 114 | 60 461 | 2 116 | 0.0 % | 2/2 |
| mean / total | 4.4 (6) | 2.0 (1) | 10.4 | 7 095 | 54 242 | | | |

Every `U` object built, every part closed, joints as the reference,
sizes within 0.3 %. Each agent previewed 1 to 4 times and called
`measure` never (`run_typescript` already returns the measures).

- **Fewer builds** (2.0 against 4.4) and one failed build in five
  objects (the knob: a join of stacked tapered bands sharing side planes,
  "the result is open (4 unmatched edges)", §13.6 item 1's message, fixed
  in the next build by narrowing each band so the sides cross). Three
  agents built once and only previewed after.
- **Fewer triangles** where segment control now reaches: the bent tubes
  8 250 (M 28 632), the djembe 6 008 (M 14 268).
- **Problems reported**: 4.0 an agent (M 6.4). Repeated themes: how
  `body` reads its `plan`, `top` and their y values, and that a smooth
  `body` overshoots like a curved loft; where a torus's `on` origin sits
  with `axis: "+Z"`; how much straight length a `wire` fillet uses and
  how to hollow a wire; that joint ids of used-up parts stay; how to look
  at a pose without writing it into the program (`run_typescript`
  always writes `program_out`, and the preview tool takes no pose); two
  joints on one child insert a hidden part (`knob:knob_slide`); a
  loft of straight sections renders smooth; stacked overlapping pieces
  must not share side planes (the 0.5 mm overlap rule is not enough).
  The full lists are in each `report.md`.
- **Tokens are not comparable with `M`.** `M`'s are the Agent tool's
  totals for a subagent (fixed context 62 668); `U`'s come from Claude
  Code's result: output tokens (thinking included) and the last turn's
  context (input, cache read and written), which holds the CLI's own
  system prompt, the server's tool schemas (loaded through Claude Code's
  tool search), the two guides and the work. Input summed over turns,
  mostly cache reads, was 206 306 to 426 316 an agent.
- One agent per object, as before: differences of a build or two are
  noise (§13.7).

### 19.8 What is untested

- Windows: `windows_paths` runs only in CI; bun importing an absolute
  Windows path is still untried (the wasm and binding steps run on Linux only).
- macOS and aarch64: CI only, as before. node in the gate runs only on
  x86-64 Linux.
- Firefox and WebKit: only Chromium is installed here. Vite and other
  bundlers: only esbuild.
- The GPU's skinned vertices were not read back (three.js's CPU path of
  the same formula was measured).
- Claude Desktop and other MCP clients; Claude Code was run headless
  (`-p`), not interactively. The MCP server over HTTP: it speaks stdio
  only.
- A scan of a million triangles or more as captured: the mesh of §19.6
  is a 250 k scan split four ways.
- Cancelling a single long kernel call (a `repair` of a million
  triangles) is not possible by design; it ends, then the cancellation
  is seen.
- Run `U`'s agents could read any file with Read; told not to, which
  was not enforced (one read its own `program.json`).

## 20. Preview fidelity and joints through copies (stream V)

Closes the gaps of §8.5 and §13.6 item 14: what an agent looks at and
measures is the model as posed.

### 20.1 Decisions

- **Transparency (item 1).** Opaque parts are drawn first, into the
  depth buffer, as before. Then every triangle of a part whose
  `opacity` lies strictly between 0 and 1 is drawn in one sorted list:
  the key is the sum of the three corners' depths (the projection's f64
  `d · forward`, the same expression on every host), farthest first;
  equal keys go to the lower part index, then the lower triangle index.
  `f64::total_cmp` on finite values is the plain order, so the side of
  every comparison is decided exactly on values every host computes
  alike. A transparent pixel is `opacity × lit colour + (1 − opacity) ×
  the pixel`, in sRGB bytes, tested against the opaque depth and never
  writing it, so glass in front of glass blends both, and edges behind
  glass still show. Sorting by triangle (painter's order) is wrong where
  transparent triangles cross each other; no part of the washing machine
  does. `opacity` 0 draws nothing and casts nothing. The shadow is as dark
  as the most opaque caster at the pixel (the maximum, not a product, so
  it does not depend on how many triangles overlap there): an opaque
  part's shadow is the old colour exactly.
- **Crease shading (item 2).** A rule by angle, not by the generator's
  knowledge: generators already split real edges by surface id, but a
  loft's or extrude's walls are one surface whatever their facets, and
  carrying a "curved" flag through booleans and bevels would touch every
  crate. Per triangle corner, the normal is the area-weighted sum of the
  normals of the triangles around its vertex that share the triangle's
  surface and whose unit normals lie within 35° of its own (dot product
  ≥ cos 35°, from `trig`), normalized. 35° keeps a 12-sided turn (30°
  between facets) and the generators' 24 to 64 segments smooth, and
  shows a hexagon's (60°) and an octagon's (45°) facets. Where every
  neighbour is within the angle the sum is the old one, in the same
  order, so smooth surfaces shade bit for bit as before
  (`model.joints.preview` did not move). The GLB's normals are not
  changed: three.js and other renderers do their own shading.
- **Golden hashes (item 3).** Moved: `tools.preview`, `tools.preview.png`,
  `model.washing_machine.png`. No other line moved. New:
  `tools.preview.facets` (a hexagonal loft, a 48-sided cylinder and a
  pane at opacity 0.4, three views), `model.joints.obj`,
  `model.joints.stl`, `model.joints.copies`,
  `model.washing_machine.posed`.
- **Joints through copies (item 4).** `copy` and `mirror` copy every joint
  whose child is in the copied tree, in declaration order, so a chain
  composes as the original's and a leader comes before its followers.
  A copied joint takes a free suffix (`hinge.001`, as nodes do), hangs on
  the parent's copy when the parent was copied and on the same parent
  otherwise, and a follower follows its leader's copy when there is one.
  A joint whose child is outside the tree is not copied, even if it hangs
  on a copied part: copy the group that holds both. `copy` keeps the
  value (the copy is posed as the original). For `mirror`, with `S` the
  reflection: the pivot's origin is `S o`, its x and y are `S x`, `S y`,
  and its z their cross product, `−S z`, so the frame stays right-handed.
  A slide is a plain direction, `S T(a d) S = T(S a, d)`: its axis is `S a`
  (a default axis, the pivot's z, is written out, since the new pivot's z
  is negated). A turn's axis is a pseudovector, `S R(a, θ) S = R(−S a,
  θ)`: its axis is `−S a`. With these, ranges, values and mimic
  multipliers are kept unchanged and every motion is exactly the image:
  a follower whose leader was not mirrored still follows it as the
  image. The other convention, `S a` with the range and value negated,
  is the same motion, but a mirrored door would open by negative values
  and a follower outside the copy would need its multiplier negated;
  this one keeps "the same value poses the image". Tested by posing the
  original and the image at three values each and comparing every
  posed vertex with the original's mirrored (`tests/pose.rs`).
- **Reads off rest (item 5): refused.** sand3d reads the posed scene but
  refuses to declare joints off rest. Here every frame the model takes or
  gives is in the world at rest (§8.2, "one world"): `at`, `joint`,
  `hole` and the rest take rest frames. A posed read would give a frame
  that those then take for a rest frame, silently putting parts in the
  wrong place; the read is refused instead (`ModelError::Posed`: the
  read, the part, the joint and its value). It applies to every read that
  gives a frame or a path from a node's geometry or placement: `face`,
  `bbox`, `edge`, `snap`, `placement`, `local`, `anchor`; only nodes that
  a non-zero joint value moves (the joint chain from the node up through
  its parents, a follower by its leader's value) are refused, so the
  cabinet reads as before while its door is open. `names` and `measure`
  are not frames and are not refused: `measure` says it is at rest, and
  the posed measure (item 9) is a separate call. `world_solid` (a Rust
  accessor for tests and tools) answers the rest, as documented.
- **OBJ and STL (item 6).** `Model::obj` and `Model::stl` write the pose
  when any joint has a value other than 0, as sand3d does, and the rest
  otherwise. The file's header says so (`…, Y up, posed`; STL's 80-byte
  header likewise) and `shape export` reports `"geometry": "posed"` or
  `"rest"`. A part no non-zero value moves is written with its rest
  positions bit for bit (its motion is skipped, not composed: a pivot
  composed with its inverse is not exactly the identity); a moved part
  is moved by the same composition the preview uses. `shape_tools::obj`
  and `stl` keep writing the rest (their golden hashes hold; the tools
  scene has a posed wheel), and `obj_at`, `stl_at` take a `Placement`.
  The GLB keeps rest meshes with the pose in its joint nodes, unchanged.
- **Joint seating (item 7): a measurement.** Frames still do not remember
  the part they were read on: `Frame` is the kernel's plain value in
  every API and JSON form, and a part field would change all of them.
  What that memory was for is measured instead. Each joint records, when
  declared, the parent's own space seen from the pivot (`on⁻¹ ∘ parent`).
  `Model::seating` reports per joint, at rest: `pivotToParent` and
  `pivotToChild`, the distance from the pivot's origin to the nearest
  point of the parent's parts (those under it but not under the child)
  and of the child's (`place::snap`'s brute force); and `parentMoved`
  and `parentTurned`, how far the parent's space has moved and turned off
  the pivot since (the turn from `‖R − I‖_F = 2√2 sin(θ/2)`, accurate
  near 0 where `acos` is not). Moving a child moves its pivots (§8.2), so
  only a parent moved without its child, or a pivot read on another
  part, shows. A copied joint keeps the original's record; a mirrored one
  starts a new record at the mirror. No threshold: a hinge's pivot lies on
  both surfaces, a drum's axle pivot in the air at its centre.
- **`shell(open)` (item 8): the part's own axes**, documented in
  `guides/program.md` and `guides/typescript.md`. The shell runs in the
  part's own space like every edit, and `open: "+z"` keeps naming the
  same face when the part is later turned with `at`, as a face name
  does. Accepting a world word as well would add a second spelling for
  one thing; a world direction is `face(part, "+X")` away. Tested by
  turning a box's own +z onto the world's +x and opening "+z".
- **Measures at a pose (item 9).** `Model::interference` already took a
  pose (§15.3). `Model::measure_posed` measures every part at the model's
  pose: a part no joint moves measures as at rest bit for bit; a moved
  part's counts, area and closure are the rest's, its bounds the pose's,
  and its volume may differ in the last bits. `program::posed_json` (the
  `measure` reader with `"posed": true`, `shape measure --posed`, the
  binding's `measure({ posed: true })`) gives `measures_json`'s fields at
  the pose, `pose`, `posed`, `joints` (the seating) and `interference`
  at the pose. Each pair now says `asBuilt`: true when both parts move by
  the very same motion and were compared where built, false when compared
  posed, where faces flush only under different joints can read as
  crossing by rounding (§15.5). That rounding is not removed: the
  crossings are reported, and `asBuilt: false` with a crossing box on the
  contact plane says it may be rounding. `measures_json` and `report_json`
  keep their output (`model.washing_machine.measure` and
  `interference.washing_machine` hold).

### 20.2 Measurements (x86-64, release)

- The washing machine (4 014 triangles): the CLI's `preview` (replay and
  four 320² views) 120–130 ms, as before within noise; `measure` 88–94
  ms, `measure --posed` 129–138 ms (the posed interference about 25 ms,
  the seating's nearest points the rest). PNG 19 612 bytes (18 650
  before: the blended glass).
- Its posed measure: door 1.606 rad, drawer 120 mm, dial 0.524 rad.
  Crossing pairs at that pose: cabinet/drawer 67, cabinet/dial 442 (178
  with the dial at rest: the turned dial's facets cross the seat's), both
  `asBuilt: false`; ring/glass and ring/handle touch, `asBuilt: true`.
  Seating: every `parentMoved` and `parentTurned` 0; the hinge's pivot
  45 mm off the cabinet's surface and on the door's, the drawer's 50 and
  15, the dial's 10 and 10, the drum's in its centre (220 and 201).
- Previews regenerated (`docs/previews/`): 42 of the 50 images were
  rendered again (the four example programs and the washing machine) and
  18 changed, each checked against a render from `origin/main` in a
  separate worktree: the change is this stream's for all but
  `phase1/bevel_to_spur_transfer.png`, which `main` already rendered
  differently from the committed file. `phase5/` was not regenerated (the
  mesh-tools bench's external inputs are not here).

### 20.3 Gaps and what is untested

- Transparent triangles that cross each other blend in triangle order,
  which is wrong across the crossing (no per-pixel sort). Transparent
  parts do not tint the shadow, only lighten it.
- The crease rule compares each neighbour around a vertex with the
  corner's own triangle, not across the edge between neighbours, so on a
  fan that turns in small steps the corners on either side of a 35°
  total can shade differently. The angle is fixed; `Preview` has no
  field for it.
- A joint whose child is outside the copied tree is not copied; a jointed
  child hanging on a copied part is left on the original.
- `Model::seating` reports the parent's drift and the pivot's distances,
  not the gap between the child and the parent (a mesh-to-mesh distance).
- The posed-read refusal covers the model API and the program; the MCP
  server reaches it through the CLI and the binding and was not run here.
- Run here: the native gate's lint, test and golden steps; the wasm32
  builds; the binding's tests under bun (wasm32-unknown-unknown, which
  compare the washing machine's GLB and PNG). Not run: the golden run as
  wasm32-wasip1 under wasmtime (not installable here), aarch64 and
  Windows (CI only).

## 21. UVs everywhere a program can reach (stream Y)

Stream S put UVs on `Solid` and carried them through import, export,
repair, decimation and booleans (§17). This stream closes §17.6's gaps
that a program meets: the program can ask for UVs, every generator and
edit carries them or refuses, bevels carry them, and a material names a
texture. UVs stay opt-in: with no `uvs` anywhere, every existing golden
line is as it was (checked: `golden/hashes.txt` gains three lines, none
moves).

### 21.1 Generators

`textured` covers every generator now. The rule is §17.3's (caps planar,
walls by arc length round and along, in the model's unit); a shared
helper (`RingUvs`, `shape_gen::solids`) measures a wall stitched between
rings in 3D: u the arc length round ring i from its point 0 (the closing
segment ends at the whole length), v the arc length along the line of
point j from ring 0. Measuring along each line, not by section height,
keeps v in millimetres on a sweep that bends or a loft that bulges.

| generator | walls | caps | seams |
|---|---|---|---|
| `sweep`, `thread` | `RingUvs` (u round each section, v along the path) | the section's own placed 2D coordinates (twisted and scaled as there) | along the contour's first point |
| `loft` | `RingUvs` | x, y | along each ring's first sample (outline and holes) |
| `wire` | `RingUvs`; a closed wire wraps (v ends at the whole length) | the ring's circle | along the ring's first point; across ring 0 when closed |
| `body` | `RingUvs`; an apex ring borrows its neighbour's u | the section's (x, z − centre) | along +x |
| `knurl` | the crest cylinder: u = r × angle, v = z | x, y | at +x |
| gear solid | as an extrude's (u along the outline, v = z) | x, y | at the outline's first point (a tooth's middle on +x), the bore's and hub's at +x |
| `wedge` | an extrude's | | |

Decided: the knurl does not use `RingUvs`. Its rows zigzag in radius
(the diamonds), so v measured along each line differs from line to line
by the teeth's ups and downs; the first version turned 1 in 20 of the
diamond triangles over in UV. The crest cylinder's map shears nothing.

`crates/shape-gen/tests/textured.rs` checks each: the same solid as the
generator alone, and every triangle with area counter-clockwise in UV;
a straight wire unrolls to its polygon's perimeter by its length.

### 21.2 Bevels

Plan phase 3 asked for new faces by mean-value interpolation from the
boundary (§7.2). Stream X was rewriting the layout at the same time, so
UVs are a pass over the result (`shape-bevel/src/uvs.rs`): the bevel runs
on the solid with its UVs taken off, and the pass matches the result to
the input. No line of the layout changed.

- **Old faces.** Each result triangle on an old face is matched to the
  input triangle with the same face names nearest its centroid (of those
  within 1e-7 of the part's size of the nearest, the largest: the first
  version took a boolean sliver and extended its map, which turned two
  triangles over near a through hole). Its corners take UVs by
  barycentric interpolation there (`Solid::uv_at`). A trimmed triangle
  lies inside its source, so this is the same map re-fitted; an
  untouched one keeps its UVs bit for bit (the test checks it).
- **New faces**, per connected patch: each triangle takes the chart (old
  surface) around the patch whose nearest triangle it faces most, and
  that triangle's map extended over its corners. On a chart whose UVs
  are affine in position (every flat face of a generator) that is the
  chart's map projected onto the new face. Mean-value interpolation of
  boundary values that are one affine map reproduces that map exactly
  (linear precision), so on flat charts the two agree; the projection is
  what is computed.
- Why not one mean-value patch per fillet: a fillet's boundary runs
  along two charts whose UVs are unrelated (a cap's x, y and a wall's
  arc length), so the boundary data jump where the charts meet, and the
  interior blends two maps into one that matches neither. Split by the
  face it faces, a fillet strip is continuous with both of its faces,
  with one seam along its middle; a corner patch splits in three.
- Measured (`crates/shape-bevel/tests/uvs.rs`): the edges between a new
  and an old face along which UVs are continuous: box fillet on `top`
  8 of 8, chamfer 8 of 8, `soften` of every edge 24 of 24, a cylinder's
  top fillet 96 of 96, a plate with two holes filleted on `top` and
  `hole` 188 of 200. No triangle turned over or flat in UV; the ratio of
  UV to surface area of new triangles stays within 0.05 to 2.
- On a curved chart (a bore, a cylinder's wall) each new triangle takes
  the tangent plane of the facet nearest it: the patch has small cracks
  in UV where that facet changes (sub-millimetre at these sizes), and
  the chart's own seam stays a seam. A first version gave each vertex
  one value from the facet nearest it; at a bore's mouth neighbouring
  vertices projected onto different facets and turned triangles over.

`BevelError::HasUvs` is gone.

### 21.3 The other tools

| tool | UVs |
|---|---|
| `split` | the half-space box is built `textured`: the cut face is planar in the plane's own x, y |
| `hole`, `pocket` | the tools are `textured` extrudes: a bore's wall u round, v along the axis; floors planar |
| `shell` | the inner solid takes the outer's UVs corner for corner |
| `seat` | the part's UVs; a part without UVs on a host with them is `EditError::NoUvs` |
| any cut (`split` below, `hole`, `pocket`, `shell`, `seat`, the model's `cut`) | the tool's u is negated first: a cut turns the tool's faces inside out, and its UVs would read mirrored from the cavity (the hole's bore did, and a fillet at its mouth projecting that chart turned over) |
| `dent` | a split edge's midpoint takes the mean of its ends' UVs in each triangle (a seam stays a seam) |
| `merge` (`shape_edit`) | `Solid::append`'s rule (zero-filled side); the model refuses merging a part with UVs and one without |
| `shape-skin` refinement | as `dent` |
| `drop_slivers` (placement) | **was wrong**: it renumbered triangles and left `uvs` indexed as before. Each piece now interpolates in its source triangle (the test's UVs are an affine function, kept exactly) |
| placement, `copy`, `scale` | carried; a scale stretches the texture with the part |
| `mirror` | the image's triangles turn over with their UVs: the texture reads mirrored, as a mirror image does, and `measure` counts every such triangle `flipped`. Negating u instead would read unmirrored but change the chart's meaning; the agent can tell from `flipped` |
| `delete` | nothing to carry |
| booleans | `facesWithoutUvs` in the program answer, `shape build`, MCP and the binding |

### 21.4 The program surface

- **Asking.** The `uvs` operation (no fields) makes every later part
  textured; it is refused once a part without UVs exists, so a model
  that asks never holds a part that silently lacks them. Each generator
  takes `uvs` (a boolean) to choose for itself. The binding's
  `og.model({ uvs: true })` sends the operation; `--uvs` (CLI) and
  `uvs` (MCP) replay a program as if it began with it.
- **Units.** UVs are in the model's unit on the surface (a millimetre
  model's are millimetres). A material's `mapSize` (default 1) is how
  many units the image spans: the model's build divides the UVs of the
  parts the material reaches by it, so the GLB's TEXCOORD_0 is in
  images and repeats with the sampler. `measure`'s `uv` reads the same
  divided UVs, so its coverage and overlap are of the image. Rejected:
  a scale on `uvs` (tools inside edits are built in units, and their UVs
  would not match the part's); KHR_texture_transform (three.js reads it,
  other tools may not, and measure would not see it).
- **Textures** follow fonts (§16): the program records the name only;
  the caller supplies bytes (`Session::texture`, `og.model({ textures })`)
  and the CLI and MCP read it relative to the program
  (`textures_named`). PNG and JPEG, by their signatures; a missing or
  other file is an error at the `material` operation, located like the
  rest.
- **Measures.** A part with UVs adds `uv` (area, coverage, overlap,
  overlappingPairs, flipped, zeroArea, outside); the model's
  `uvCoverage` (least over parts) and `uvOverlap` (most), null without
  UVs, so reports of models without UVs are unchanged.

### 21.5 Acceptance

`golden/programs/uv_plate.json` (recorded by
`crates/shape-wasm/ts/examples/uv_plate.ts`): an 80 × 50 × 6 mm plate
with two 8 mm through holes, its top edges filleted (r 1.5), a lofted
handle (rounded rectangle, ellipse, rounded rectangle; 4 steps a span)
joined on, and a 64² checker (`golden/programs/checker.png`, 8 squares a
side) spanning 20 mm.

- One part, closed, 3 144 triangles (the same count without UVs); the
  join reports `facesWithoutUvs` 0. `measure`: `flipped` 0, `zeroArea` 0,
  coverage 1 and overlap 1 (the UVs repeat the image 4 × 2.5 times
  across the plate, so every texel is covered several times), 2 942 of
  3 144 triangles outside the unit square (repeat, as intended).
- Time (x86-64, release): `shape build` 60 ms with UVs, 49 ms without.
- three.js r186 `GLTFLoader` in headless Chromium
  (`tools/check_uvs.ts`): one mesh, a UV on every one of its 2 554
  vertices, all finite, the map 64 × 64 with repeat wrapping, no page
  error. Screenshots: `docs/previews/uv/plate_checker.png` and
  `plate_checker_back.png`. Seen: squares square on the caps, the walls
  and the fillets, continuous over the fillets onto the top and the
  walls; the handle's squares shear where the loft's rings turn from the
  rectangle to the ellipse (the samples run corner to corner, §15.1), so
  its wall map is sheared, not flipped.
- In the edits test (`crates/shape-model/tests/uvs.rs`), booleans leave a
  few needles (two corners an ulp apart, area about 1e-15 mm²) whose two
  near corners take one UV: `measure` counts them `zeroArea` (2 after a
  counterbored hole, 1 after `seat`). They are the boolean's, not the
  UVs'.

Golden: `uvs.generators2` (every new generator textured), `uvs.edits`
(hole with a counterbore, pocket, fillet, chamfer, shell, dent and split
of a textured plate), `uvs.program` (the acceptance program's GLB and
measures).

### 21.6 Gaps and what is untested

- §17.6 item 2 and 3 (hole fills overlapping their chart reported by
  repair; one texture per joined solid stated in the report): not done.
- New faces on curved charts have small UV cracks where the nearest
  facet changes (§21.2); true mean-value interpolation per chart region
  would remove them. A patch whose old faces are all curved and cross
  their charts' seams keeps those seams; it is not refused.
- A bevel on an imported solid whose faces' names repeat across
  separate regions matches old faces by name and distance; a face that
  a wide bevel rebuilt from its outline across a UV seam of one planar
  face (not made by any generator) would take one side's map.
- `mapSize` scales UVs at export and in `measure`; the preview does not
  draw textures.
- The three.js check runs by hand (`node tools/check_uvs.ts`), not in
  the gate. Determinism of the new hashes: native x86-64 and wasm32
  under wasmtime here; aarch64 and Windows in CI only.

## 22. The evaluation on the finished surface (stream Z, 2026-11-10)

Whether an agent with only the guides and the tools still builds the 20
phase 6 objects after streams L to W changed the surface, and where the
guides fall short. Base `b57f2d7`. Results in `eval/phase6/Z/`.

### 22.1 Method

- **As run `U` (§19.7)**, on all 20 objects of §13.1 (12 articulated,
  24 joints): one headless Claude Code agent per object (`claude -p`,
  model `claude-opus-5-5`, the stream's), whose only tools were the
  og-shape MCP server's (`shape-mcp`, release, its root a fresh
  directory holding `brief.md` and `reference.png`), Read, and Claude
  Code's resource tools. The prompt is `U`'s, unchanged: read
  `og-shape://guides/shape.md` and `og-shape://guides/typescript.md` and
  nothing else but the brief and the render, at most 8 `run_typescript`
  calls, report iterations and guide problems. Driver:
  `eval/phase6/mcp_run.ts … --run=Z/Z1 --jobs=5` (it gained `--run`,
  `--jobs` and `errors.log`, each failed build's whole text).
- **Not enforced:** that agents read nothing else. Read was allowed (as
  in `U`); the transcripts show reads of `brief.md`, `reference.png`,
  the two resources and, in `Z1`, the agent's own `program.json`; no
  other file.
- **Measured per object:** built, closed parts, joints against the
  reference, the largest per-axis size deviation, builds (`run_typescript`
  calls) and failed builds with their text, turns, output tokens and the
  last turn's context (as §19.7: not comparable with runs `A`–`M`'s Agent
  tool totals), previews and posed checks (`preview`/`measure` calls
  with a pose), the bevel calls in the final program, the new surface
  the final program names, and the surface distance (§22.4).
- **`Z2`:** the objects of `Z1` with a failed build or more than three
  builds (sim2, sim3, auto1, obj4), with the guides and server of §22.3,
  same prompt. `Z2` ran with an earlier wording of two fixes (the sweep's
  lead-in "short"; `body`'s ends "where `plan` or the height is not 0
  are flat"); `Z2`'s agents showed both were wrong, and the committed
  guides say it as tested (§22.3).
- **One agent per object**: differences of a build or two are noise
  (§13.7).

### 22.2 Results

| | M (5) | U (5) | Z1 on U's 5 | Z1 (20) | Z1 on Z2's 4 | Z2 (4) |
|---|---|---|---|---|---|---|
| built, every part closed | 5/5 | 5/5 | 5/5 | 20/20 | 4/4 | 4/4 |
| joints as the reference | 5/5 | 5/5 | 5/5 | 20/20 (24 joints) | 4/4 | 4/4 |
| builds a object, mean | 4.4 | 2.0 | 2.8 | 2.0 | 3.75 | 3.75 |
| failed builds | 6 | 1 | 2 | 3 | 3 | 2 |
| first build ran | — | 5/5 | 4/5 | 19/20 | 3/4 | 3/4 |
| turns, mean | — | 10.4 | 11.8 | 10.05 | 13.25 | 15.0 |
| output tokens, mean | — | 7 095 | 7 984 | 5 982 | 11 549 | 13 502 |
| last context, mean | — | 54 242 | 59 176 | 54 297 | 65 306 | 76 517 |

Run `B` (§13.3; the Agent tool, guide v2) built 20/20 with 2.30
iterations and 3 failed builds. `Z1`: every object built with every part
closed and every joint, 2.0 builds an object, 3 failed builds, the first
build ran on 19. Sizes within 0.0 % on 18; auto2 1.0 % (its knob 50.02
for 50.54 wide), auto1 7.0 % (x 751 for 702: the agent's tube layout).
In `Z2` auto1 is within 0.0 % and obj4 is 11.3 % off in y (11.1 for
12.5 mm deep: the agent noted it and kept its arm fold).

The 12 articulated objects took 2.5 builds (all 3 failed builds were
theirs); the 8 rigid ones 1.25.

#### Z1, per object

| object | built | closed parts | joints (ref) | max size dev. | builds (failed) | turns | output tokens | final context | triangles | previews / posed checks | bevels in program | surface distance, mm (of diagonal) | least if mirrored or turned | new surface used |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cad1_bracket_block | yes | 1/1 | 0/0 | 0.0 % | 1 (0) | 8 | 3042 | 45431 | 518 | 1 / 0 | 1 | 8.129 (7.97 %) | 0.261 | — |
| cad2_mounting_block | yes | 1/1 | 0/0 | 0.0 % | 2 (0) | 11 | 4435 | 48315 | 1062 | 3 / 0 | 1 | 6.468 (3.84 %) | 0.961 | — |
| cad3_stepped_bracket | yes | 1/1 | 0/0 | 0.0 % | 1 (0) | 9 | 5684 | 47863 | 798 | 2 / 0 | 0 | 4.403 (4.52 %) | 3.006 | — |
| cad4_curved_plate | yes | 1/1 | 0/0 | 0.0 % | 2 (0) | 10 | 3506 | 46621 | 40 | 2 / 0 | 0 | 30.351 (3.15 %) | 0.718 | — |
| art1_sunroof_cassette | yes | 15/15 | 2/2 | 0.0 % | 3 (0) | 12 | 9333 | 69571 | 1122 | 3 / 0 | 1 | 1.843 (0.12 %) | 1.842 | opacity |
| art2_telescoping_slide | yes | 7/7 | 2/2 | 0.0 % | 1 (0) | 8 | 5267 | 50584 | 396 | 1 / 0 | 1 | 33.692 (2.89 %) | 5.311 | — |
| art3_inspection_arm | yes | 22/22 | 4/4 | 0.0 % | 2 (0) | 11 | 7775 | 70232 | 1468 | 2 / 0 | 0 | 19.501 (0.73 %) | 19.501 | — |
| art4_telescoping_mast | yes | 4/4 | 3/3 | 0.0 % | 1 (0) | 8 | 5162 | 50066 | 1172 | 1 / 0 | 0 | 30.783 (1.94 %) | 30.756 | — |
| sim1_closet | yes | 3/3 | 2/2 | 0.0 % | 3 (0) | 11 | 5681 | 52596 | 64 | 2 / 0 | 0 | 3.665 (0.16 %) | 3.662 | — |
| sim2_mouse_and_pad | yes | 4/4 | 1/1 | 0.0 % | 5 (0) | 20 | 12496 | 71155 | 4306 | 9 / 0 | 2 | 1.689 (0.53 %) | 1.519 | — |
| sim3_toaster | yes | 6/6 | 3/3 | 0.0 % | 4 (1) | 12 | 14440 | 71461 | 4346 | 2 / 0 | 4 | 7.305 (1.89 %) | 6.495 | merge |
| sim4_bollard | yes | 1/1 | 0/0 | 0.0 % | 1 (0) | 9 | 1957 | 44723 | 4096 | 2 / 0 | 0 | 7.917 (0.75 %) | 7.917 | — |
| auto1_curved_tubes | yes | 3/3 | 1/1 | 7.0 % | 3 (1) | 10 | 10234 | 56711 | 16800 | 1 / 0 | 0 | 85.001 (4.55 %) | 82.346 | arc |
| auto2_lever_knob | yes | 2/2 | 2/2 | 1.0 % | 2 (0) | 10 | 4711 | 51568 | 570 | 2 / 0 | 0 | 1.182 (0.94 %) | 1.182 | — |
| auto3_hinged_straps | yes | 2/2 | 1/1 | 0.0 % | 2 (0) | 9 | 5180 | 56335 | 5280 | 1 / 0 | 0 | 1.289 (0.14 %) | 1.289 | — |
| auto4_framed_panel | yes | 2/2 | 1/1 | 0.0 % | 1 (0) | 8 | 2423 | 45126 | 44 | 1 / 0 | 0 | 8.28 (0.68 %) | 0.184 | — |
| obj1_octagonal_cup | yes | 1/1 | 0/0 | 0.0 % | 1 (0) | 8 | 2243 | 44492 | 152 | 1 / 0 | 1 | 50.436 (11.89 %) | 3.135 | — |
| obj2_djembe | yes | 29/29 | 0/0 | 0.0 % | 1 (0) | 8 | 3452 | 54549 | 11036 | 1 / 0 | 1 | 11.028 (1.32 %) | 11.028 | — |
| obj3_lamp | yes | 3/3 | 0/0 | 0.0 % | 1 (0) | 8 | 3593 | 46636 | 184 | 1 / 0 | 0 | 172.843 (19.80 %) | 26.811 | — |
| obj4_spectacles | yes | 11/11 | 2/2 | 0.0 % | 3 (1) | 11 | 9026 | 61896 | 2972 | 2 / 0 | 0 | 1.825 (1.27 %) | 1.825 | arc, opacity |

#### Z2

| object | built | closed parts | joints (ref) | max size dev. | builds (failed) | turns | output tokens | final context | triangles | previews / posed checks | bevels in program | surface distance, mm (of diagonal) | least if mirrored or turned | new surface used |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| sim2_mouse_and_pad | yes | 4/4 | 1/1 | 0.1 % | 5 (1) | 18 | 10884 | 68421 | 9636 | 5 / 2 | 4 | 0.951 (0.30 %) | 0.777 | — |
| sim3_toaster | yes | 6/6 | 3/3 | 0.0 % | 7 (1) | 22 | 27184 | 116406 | 7550 | 4 / 5 | 4 | 6.089 (1.57 %) | 6.087 | merge |
| auto1_curved_tubes | yes | 3/3 | 1/1 | 0.0 % | 1 (0) | 9 | 5195 | 50109 | 17760 | 2 / 1 | 0 | 83.114 (4.45 %) | 80.353 | arc |
| obj4_spectacles | yes | 13/13 | 2/2 | 11.3 % | 2 (0) | 11 | 10743 | 71132 | 5364 | 2 / 1 | 1 | 2.025 (1.42 %) | 2.025 | arc, opacity |

"New surface used" is what the final program names of streams R and V:
`opacity`, `merge`, arc pieces in a `wire` or `sweep` path. No final
program used `superellipse`, `spline`, `og.path`, `stroke`, `hull`, the
`mirror` profile, `text`, `until`, `helix`, `delete`, `check`, `copy`
or `s.mirror`; the briefs do not call for most of them. sim1's and
sim3's `Z1` agents printed `s.measure({ posed: true })` with
`console.log` to check a pose (then took the pose out); no `Z1` agent
could pose through the tools. In `Z2`, with `pose` on `preview` and
`measure`, the four agents made 9 posed checks; sim3's found its lever
tab passing through the housing at full travel and a stem through its
slot, and fixed both: 3 of its 7 builds.

### 22.3 Failure classes, and the guide changes

Every failed build and refusal (5 in all):

| run, object, build | error | class |
|---|---|---|
| Z1 auto1, 1 | `op 0 sweep "lower_tube": path has no field "path"; it takes: helix`: `og.path([...])` given as a sweep's 3D path | guide gap: "`sweep` and `wire` take 3D paths" right after `og.path`'s description |
| Z1 sim3, 3 | `ReferenceError: Cannot access 'leverZ' before initialization` | agent error: §2 says to declare shared numbers before the first call |
| Z1 obj4, 2 | `program.ts: No such file or directory`: `run_typescript` with `path: "program.ts"`, never written | agent error; the tool's description now says the server writes no .ts file |
| Z2 sim3, 1 | `op 1 fillet "housing": … a bevel along a face whose triangles at this vertex are not in one plane (a twisted or converging face), where the bevel would tilt them is not supported` | kernel gap (§22.5, item 2) |
| Z2 sim2, 2 | `op 3 body "mouse_shell": body: at y = -57 only one view closes: a body closes to a point where both views close, at an end` | guide gap, made by `Z2`'s own wording ("flat where `plan` or the height is not 0"); corrected |

Refusals that were not builds: `preview` with elevation −0.8 and −0.9
(Z1 cad2), answered "the preview's elevation is not finite" (a kernel
message for a finite number out of range; `shape-mcp` now refuses it
first with the range).

**Bevels:** `Z1`'s final programs hold 14 bevel calls (fillets and
chamfers of `top`, `bottom`, `side` or every edge, radius 0.8 to 10),
none refused. `Z2`: one refusal, `s.fillet(h, { radius: 10, edges:
"top" })` on a `loft` (`steps: 8, curve: "spline"`) of four rounded
rectangles; the agent rounded the shoulder with more loft sections
instead.

**Guide problems reported** (list items in the reports): 3.7 an agent
in `Z1` (U: 4.0), 4.25 in `Z2`.
The repeated ones, and what changed (`guides/typescript.md` unless
named):

| problem (agents) | change |
|---|---|
| no way to look at a pose without writing it into the program (art1, art3, art4, sim1, sim3; also `U`) | `shape-mcp`: `preview`, `measure`, `export` take `pose`; `measure` takes `posed`. Guides say so (`shape.md` §8, `typescript.md`, `program.md`, `mcp.md`) |
| `og.path` for a 3D path (auto1) | 3D paths are the list itself |
| a swept tube 6 mm past each end (auto1) | the gap (§22.5, item 1) and the lead-in workaround |
| `body`: half or full width, flat ends, smooth overshoot (sim2 ×2) | full width; an end closes only where width and height reach 0; smooth can overshoot |
| `touching`, `crossings`, `asBuilt`, the box (art4, auto3, sim3 ×2, sim2, obj4) | each said in a line; `program.md` says `touching` |
| `console.log` output (sim1, sim3) | comes back as `logs` |
| a hollow or square bent tube (auto1, obj3) | `shape.md` §2: a `sweep` of a ring or `rect` |
| a cylinder along +X (art3, sim2) | `shape.md` §4: `og.frame({ normal: [1, 0, 0] })` |
| `og.frame` with `up`: where x goes (auto4, obj4) | x = y × z, with an example |
| flats of a polygon on the axes (auto2, obj1) | `og.rotate` by π / sides |
| `grid`'s order (art1); `snap` (sim3); `bulge` (cad4); `sweep`'s `up` (obj3) | one clause each |
| front along x: joint signs (art1, sim1) | `shape.md` §1: §6's signs turn with it |
| preview from below (cad2) | the range, in the schema and guide |

Not changed (one agent each, or a brief's question): which side a
brief's "front" is when its length runs along x; reading "spins" as
continuous; rest pose against a brief's words; the reference render's
camera; `merge` patterns; `mirror`'s `.001` names; loft faces all `side`.

**Guide size** (tokens as Claude Code's model counts them: a one-turn
`claude -p` with the file in the prompt less one without; bytes):

| guide | before | after |
|---|---|---|
| `shape.md` | 3 483 (8 932 B) | 3 602 (9 212 B) |
| `typescript.md` | 3 913 (8 882 B) | 4 483 (10 382 B) |
| `program.md` | 5 323 (11 661 B) | 5 391 (11 855 B) |
| `mcp.md` | 1 569 (3 941 B) | 1 708 (4 289 B) |

An agent of these runs reads `shape.md` and `typescript.md`: 7 396 →
8 085 tokens (+689). The guides' samples are built by
`crates/shape-wasm/ts/test/guides.test.ts` and `shape-mcp`'s
`guide_examples_build`, both green.

**`Z2` against `Z1`** on the same four objects: builds 3.75 and 3.75,
failed builds 3 and 2. auto1: 3 (1) → 1 (0), size 7.0 % → 0.0 %. obj4:
3 (1) → 2 (0). sim2: 5 (0) → 5 (1), the failure from `Z2`'s own
`body` wording. sim3: 4 (1) → 7 (1): one refusal (§22.5, item 2) and 3
builds spent fixing what its new posed checks found. Last context
65 306 → 76 517 a run, mostly sim3's 22 turns. Four objects, one agent
each: the change is in which errors occurred, not in the counts.

### 22.4 A shape measure that is not the agent's opinion

`eval/phase6/Z/distance.ts` (bun): a symmetric surface distance between
the program's mesh and the dataset's own mesh. `references.json` now
records each object's OpenUSD file (`usda_path`, `usda_sha256`,
`usda_bytes`, looked up in the Hugging Face dataset server);
`Z/fetch_references.ts` downloads them and checks the digest (the files
are CC BY 4.0 and not committed). The reader takes every `Mesh` through
its ancestors' translate, orient, scale and transform ops and gives
every reference's bounding box within 0.01 mm of `references.json`'s
sizes. 20 000 points are sampled on each surface, area-weighted, with a
fixed generator; each point's distance to the other surface is exact.
Reported in `distance.json`: each direction's mean, their average
(`mean`), the 95th percentile and the largest, as placed and with the
bounding boxes' centres together (`centred`; the briefs give sizes, not
origins), and the centred mean again with the built mesh mirrored in x,
in y or turned 180° about z (`centredMeanIf`). No threshold. Inner
pieces count on both sides; joints at rest.

- `Z1`: median centred mean 8.0 mm, 1.61 % of the reference's diagonal;
  `B` (the same 20, guide v2): 6.4 mm, 1.27 %. The least of the four
  placements: medians 3.1 (`Z1`) and 2.9 mm (`B`).
- Most of the large values are a side chosen differently, not a
  different shape: the cup's handle (50.4 → 3.1 mm mirrored in y), cad1
  (8.1 → 0.26 mirrored in x), cad4 (30.4 → 0.72), the lamp (172.8 →
  26.8 turned), the slide (33.7 → 5.3). The briefs give the size per
  axis and leave which side a feature is on to a render whose camera
  they do not state.
- What stays large is shape: auto1 (80 mm: the tubes' layout), art4
  (31), art3 (19.5), obj2 (11).
- `U`'s five: sim2 0.78, auto1 101.7, auto2 1.22, obj2 10.2, obj4
  1.95 mm; `Z1`'s 1.69, 85.0, 1.18, 11.0, 1.83; `Z2`'s sim2 0.95, auto1
  83.1, obj4 2.03, sim3 6.09 (`Z1` 7.31).

### 22.5 Kernel gaps, with reproductions

In `eval/phase6/Z/repro/` (README there gives expected and actual):

1. **A sweep's ends square to the chord, not the path** (`shape-gen`;
   `sweep_end_tangent.json`). A `sweep` or `wire` of radius 127 along an
   arc tangent to +z at its start reaches z −5.85 and x 282.24 for 0 and
   276.39: the end sections are square to the first and last chord, an
   overshoot of r·sin(half a segment). With a 20 mm straight piece at
   each end the bounds are exact; with 1 mm the overshoot stays (4.6
   mm), as the mitre reaches past it. auto1's `Z1` agent shrank its bend
   radius by 11.7 mm to fit the height.
2. **A fillet of a tapered rounded wall's top edge is refused**
   (`shape-bevel`, stream X's crate; `bevel_tapered_rounded_extrude.json`,
   `bevel_curved_loft_top.json`). `extrude` of a rounded `rect` with
   `taper: 0.1`, or a straight or spline loft of rounded rectangles of
   different sizes, then `fillet` of `edges: "top"`: "a bevel along a
   face whose triangles at this vertex are not in one plane". The same
   fillet on a tapered cylinder and on the untapered extrude builds. It
   is the refusal stream W introduced in place of a wrong result, met on
   an ordinary housing.
   **After the merge with stream X (§7.8):** the tapered extrude's
   fillet builds; the curved loft's (`bevel_curved_loft_top.json`) is
   still refused.

Not kernel gaps: the preview's "not finite" message (a misleading text,
now caught before the kernel), the build report's flattened groups
(documented), and posed interference boxes that seemed not to move (the
box spans both parts' meeting triangles; on a simple slide it follows
the pose).

### 22.6 What the orchestrator should know

- `shape-mcp` changed beyond a few lines: `pose` on three tools,
  `posed` on `measure`, an elevation check (about 60 lines with the
  test `a_pose_for_one_call`). It answers the most repeated problem of
  runs `U` and `Z1`.
- Item 2 of §22.5 is in stream X's crate; item 1 in `shape-gen`, which
  no stream holds now.

### 22.7 What is untested

- One agent per object and one model; no repeat runs, so no spread.
- `Z2` ran with two fix wordings later corrected (§22.1); the committed
  wording was checked by the reproductions and the sweep and `body`
  errors, not by another agent run.
- Agents' reading was not confined (Read was allowed, as in `U`).
- The distance compares at rest, with no alignment of rotation beyond
  the four placements; hidden inner pieces count; a different part
  count is not seen as such. The references are the sand3d exports of
  the datasets, not the source objects.
- Tokens: Claude Code's figures, as §19.7; not comparable with `A`–`M`.
- Gate run here: lint (fmt, clippy, cargo-deny), test, golden (native)
  and wasm (the wasm32 builds, the golden run as wasm32-wasip1 under
  wasmtime 50.0.1, the binding under bun and node), all green; no line
  of `golden/hashes.txt` moved. aarch64 and Windows: CI only.

## 23. Boolean outputs that cross themselves by an ulp (stream AA, 2026-11-10)

Base b57f2d7. Every side below is still decided by `orient3d` and
`orient2d`. Two choices use a distance, both the design's snap distance
(2⁻⁴⁰ of the operands' box diagonal, §18.1, `docs/design.md` §6): which
points the repair may merge or which needles it may split (each change is
then kept or undone by exact predicates), and which tool vertices the snap
moves.

### 23.1 The countersink turn (§18.5)

`hole` turns the countersink's cone half a segment (π/n) about the axis,
so its generators run between the bore's columns. Swept on an 80 × 50 × 10
plate (positions (0, −5), (−30, −15), (0, 0), (12.3, 4.56); diameters 3,
4, 5, 6.6; through and 6 deep; angles 60°, 82°, 90°; mouth twice the
diameter): 53 of 96 countersunk plates crossed themselves before (3 to 79
pairs), none after. Golden: `edit.holes.pocket` moved, no other line.
Test `a_countersink_rim_does_not_cross_itself` (`shape-edit`; it fails
unturned).

The plain hole: 0 of 32 crossed. The counterbore: 1 of 32, before and
after: diameter 6.6 with a 13.2 counterbore at (12.3, 4.56), through,
8 pairs. It was not diagnosed; the turn was not applied to the
counterbore, whose crossings did not appear elsewhere in the sweep, and
is left to the repair (§23.2).

### 23.2 Where the kernel's booleans leave crossings, and the repair

Every boolean the golden cases run was checked (`crossing_pairs` on its
result, operands checked too). Ten golden lines hold a boolean whose
output crosses itself where its operands do not: `bevel.sound` (the post,
4 pairs), `smallgaps.hole.diagonal` (2), `smallgaps.knob.program` (6 to
11 per join), `touching.countersink.ring` (34), `touching.lofts.cut`
(34), `touching.lofts.join` (8), `touching.program.elbows` (68),
`touching.program.rim` (20), `touching.torus.ring` (47), plus
`selfunion.check`, whose inputs cross by design. §18.5 counted only the
chains whose crossing output is a later boolean's operand.

Two shapes, read off the pairs:

- **Points a rounding error apart joined by needles** (all but one). One
  operand's edge passes an ulp off the other's edge or vertex, so two
  exact crossing points lie 10⁻¹⁶ to 10⁻¹³ apart; rounded, the needle
  between them turns over its neighbour. The post (§7.7): the plate's top
  diagonal x = y passes 3·10⁻¹⁵ from the pin's vertex column at 225°
  (whose cos and sin round apart), so the diagonal's crossing with the
  pin's bottom edge is made next to that vertex, and the needle between
  them folds over the plate's piece and through the pin's wall.
- **Needles along a long edge** (the countersink ring): a point exactly on
  a ring edge rounds off it, and the needle with the edge's two ends
  folds.

**The repair** (`crates/shape-bool/src/uncross.rs`,
`shape_bool::uncross_output`). Only triangles with a constructed corner (a
position that is no operand's vertex) can have moved, so only pairs with
one are tested: a grid over those triangles, every output triangle
queried against it. None found: the output is returned as built. Else:

1. each edge at a crossing pair's corners shorter than the snap distance,
   with a constructed end, shortest first, is collapsed: the constructed
   end moves onto the other (an operand's vertex never moves); the two
   triangles on the edge go;
2. each triangle at a pair still crossing whose corner lies within the
   snap distance of the opposite edge, between its ends, is a needle: the
   triangle across that edge is split at the corner and the needle
   dropped (`sliver::flip`'s move, for a corner an ulp off the line);
3. each change is kept only if no triangle at it is degenerate (exact)
   or doubled, every edge at it is used once each way, and the crossing
   pairs there, by `selfcheck::crossing`, become strictly fewer;
   otherwise it is undone.

Rounds repeat (at most 16) while a change is kept. Every kept collapse
removes a point and every kept split removes a crossing pair, so it ends.
A closed result stays closed (each change rewrites only edges at one
point, checked). An output it cannot improve is returned unchanged.

On every golden case above it leaves no crossing pair (checked with the
full `self_crossings`, not only the seam's), the result closed, the
volume equal to 10⁻¹³: the post 4 → 0 (one point merged),
`hole.diagonal` 2 → 0, the knob's joins 6 → 0, the countersink ring
34 → 0 (8 needles split), the lofts 34 → 0 and 8 → 0, the elbows'
intersection 68 → 0 (23 merged), the rim 20 → 0, the torus ring 47 → 0.
It runs in 0.3 to 17 ms on these. Tests: `crates/shape-bool/tests/
uncross.rs` (the post, the countersink ring, the torus ring, a clean
output returned equal); golden `touching.uncross`.

**This is not snap rounding.** Snap rounding in 3D (round every point to
the grid, then split every edge and face passing through a rounded
point's cell, until nothing crosses) has no bound on the number of rounds
that we could prove here, can blow up the triangle count (Fortune's
vertex rounding of a 3D subdivision; Devillers, Lazard and Lenhart's
rounding of 3D meshes), and its splits make two sheets meet along a seam
used four times, which a closed solid then has to resolve by a self-union
at the scale of rounding, where probes cannot classify. The repair above
is the step short of it: local, bounded (each kept change removes a point
or a pair), exact in what it keeps, and a no-op where nothing crosses.
What it does not do: split a face that a rounded point passed through
(no case needed it), or move an operand's vertex.

**On in `boolean` since the merge** (the owner's decision; the seven
lines below were blessed then, and are exactly the ones that moved).
As the stream left it: `named.rs`'s `UNCROSS` was `false`. Turned on,
these golden lines move: `smallgaps.knob.program` and
`touching.program.elbows` (allowed by the stream), and also
`smallgaps.hole.diagonal`, `touching.countersink.ring`,
`touching.lofts.cut`, `touching.lofts.join` and `touching.torus.ring`.
In each the output crossed itself before and does not after, with the
volume equal to 10⁻¹³ and names from the same input faces (the repair
keeps each triangle's source). `bevel.sound` and `touching.program.rim`
do not move, although a boolean inside each is repaired (not
investigated further). The stream's rule is to stop and report lines beyond the two,
so the switch is left for review; flipping it and blessing is the whole
change. No test failed with it on.

### 23.3 The post's points (§7.7) and the bevel's weld

The two points 10⁻¹⁵ apart come from the plate's top diagonal passing
3·10⁻¹⁵ off the pin's vertex at 225° (above). The exact arrangement must
keep both; rounding makes the fold. The repair merges the crossing into
the pin's vertex (`uncross_output`, `Uncrossed { before: 4, after: 0,
collapsed: 1 }`). With it on in `boolean`, `bevel.sound`'s hash is
unchanged, and the post's boolean output no longer crosses itself, so the
bevel's input weld is not needed for this case. It should stay: other
inputs (imported meshes, booleans the repair cannot improve) can still
reach the bevel crossing themselves. `shape-bevel` was not edited (stream X).

### 23.4 Faces a few ulps apart (§18.8)

Reproductions (two 32-gon prisms, 0 to 10 and 10 to 20, radius 10, joined,
cut and intersected):

| case | before | now |
|---|---|---|
| tool turned and turned back (vertices ulps off, corresponding) | closed | closed |
| the same, tool caps fanned from another vertex | closed | closed |
| both tilted, caps fanned differently (one ring, two triangulations, not planar by ulps) | refused, naming `top`/`bottom`, `side` | the same |
| tool cap with a vertex at each edge's midpoint, untilted | closed (4 crossing pairs) | the same |
| the same, tilted | join: not closed, 10 touching edges, 3 crossing pairs | the same |
| tool's bottom 2 ulps above the target's top | **union lost the tool, closed, no error** | joined, one shell |
| the same, tilted | the same wrong union | joined, one shell |
| tool's bottom 2 ulps below | closed | closed |

**Found: a wrong union with no error.** A region that meets the other
surface nowhere was classified by probes a millionth of the operands' size
in front of and behind its largest piece; across a gap of two ulps the
front probe lands inside the other solid, and the region is taken to lie
on its surface and dropped. Such a region (none of its triangles crossed,
touched or lying in one plane with the other's) is now classified
exactly: the signed crossings, by `orient3d`, of a segment from its
largest piece's first corner to a point outside both boxes, along one of
six fixed directions (one through an edge, a vertex, or ending in a
plane, is tried along the next). The winding sum at the corner would not
do: within 10⁻¹³ of a face a term's sign is lost to rounding.

**A closed result in several shells** is now built again with the snap
(§18.1), and taken if closed, manifold and in fewer shells: the stacked
prisms join as one shell, as the touching ones do. Test
`slabs_stacked_two_ulps_apart_join_whole` (it fails with either change
undone); golden `touching.gap.slabs`. No existing golden line moved.

**Not closed:** the tilted ring triangulated two ways (still
`OpenTouching`, naming the faces), and the tilted midpoints (a join that
reports touching edges where there should be none). Both are two
surfaces with one boundary that are not exactly one plane. The smaller
step: treat two regions with the same boundary loop, edge for edge, as one
surface when every vertex of each lies within the snap distance of the
other (`coincident_regions` asks for exact coplanarity now). That is a
distance deciding a classification, so it is left for review. A tool
vertex near a target face or edge but not a vertex (a pocket a few ulps
short of a face, leaving a membrane) is not snapped; that needs the
target's triangle split at the vertex first, and was not attempted.

### 23.5 The winding tree's bound (§18.3, §18.8)

Proved: with `y = x − p`, `u = y/|y|`, a unit `h` and `c = u·h`,
`∂²K[h, h] = (u (9c² − 3) − 6c (h − c u)) / |y|⁴`; its length squared
times `|y|⁸` is `45c⁴ − 18c² + 9`, at most 36 for `c²` in [0, 1]. So
`|∂²K[h, h]| ≤ 6|h|²/|y|⁴`, with equality along `u`. The remainder of
the first-order expansion, in integral form along the segment from the
cluster's centre (inside the cluster's ball, at least `ρ` from `p`), is
at most `½ · 6/ρ⁴ · ∫|x − c|² dA = 3 S/ρ⁴`. The code used `4 S/ρ⁴`
(6 taken as 8); it uses `3 S/ρ⁴` now. Decisions are the direct sum's, so
no output changed; the unit test of §18.3 passes. (Checked numerically
too: 20 000 random `y`, `h`, finite differences match the formula, and
the largest `|∂²K[h, h]| |y|⁴` was 5.9991.)

### 23.6 `check_operands` on by default

Measured with `check_operands(target, tool)` called first in
`boolean_report`:

| | golden lines that move |
|---|---|
| §18.5 (before this stream) | 4: `selfunion.check`, `edit.holes.pocket`, `smallgaps.knob.program`, `touching.program.elbows` |
| countersink turned (now) | 3: `selfunion.check`, `smallgaps.knob.program`, `touching.program.elbows` |
| countersink turned and the repair on | 1: `selfunion.check` (its inputs cross by design: refused up front instead of after) |

Not turned on. Its cost is §5.2's (35–55 % of a boolean).

### 23.7 Benchmark (`bench/booleans`, x86-64, release, one thread)

Engines og-geom, og-shape-checked, og-shape-untangled, every suite,
`--crossings`; three runs back to back: before (b57f2d7), now, and now
with `--uncross` (the repair inside the timing). Mean ms of ok cases:

| suite | engine | before | now | now, `--uncross` |
|---|---|---|---|---|
| sand3d (17) | og-geom | 3.6 | 3.2 | 3.3 |
| | og-shape-checked | 8.0 | 8.0 | 9.8 |
| | og-shape-untangled | 8.4 | 8.6 | 10.2 |
| Thingi10K (40) | og-geom | 137.2 | 140.1 | 132.4 |
| | og-shape-checked | 402.1 | 403.0 | 528.4 |
| | og-shape-untangled | 473.2 | 484.2 | 604.3 |
| synthetic (40) | og-geom | 120.5 | 116.8 | 118.4 |
| | og-shape-checked | 507.0 | 506.6 | 533.7 |
| | og-shape-untangled | 615.8 | 604.9 | 616.0 |
| touching (7) | og-geom | 0.7 | 0.7 | 0.7 |
| | og-shape-checked | 35.4 | 37.4 | 42.8 |
| | og-shape-untangled | 42.9 | 44.3 | 46.2 |
| scaling (5) | og-geom | 210.1 | 183.7 | 211.1 |
| | og-shape-checked | 482.1 | 461.0 | 480.1 |
| | og-shape-untangled | 665.0 | 673.1 | 645.9 |

Verdicts are the same in all three runs (§18.7's table). All 921 output
hashes are the same before and now. With `--uncross`, 913 are the same;
the 8 that move are the two cases whose outputs crossed:
`rounded.box.degenerate` (subtract, both engines) and
`torus.ring.on.face` (all three operations, both engines). `--crossings`:
outputs crossing themselves within one shell, before and now synthetic 1
of 24 and 29, touching 1 of 7 (as §18.7); with `--uncross` 0 in every
suite. The timings here ran on a shared 4-core container and are noisier
than §18.7's (og-geom, unchanged code, moved from 210 to 184 ms on
scaling between two runs). The repair's cost is its detection: up to
about 30 % on Thingi10K (403 → 528 ms) and 20 % on sand3d, where nothing
crosses. Most of it is welding the output and looking every output point
up among the operands' positions to find the constructed ones; the
arrangement knows them already (ids past the operands' points), and
passing that through would remove both. Not done here.

### 23.8 What is untested

- The repair is tested on the kernel's own outputs only (golden and the
  bench). On an operand that already crosses itself it leaves those pairs
  (only pairs with a constructed corner are its business).
- The counterbore case of §23.1 is found, not repaired in a test.
- The two refusals and the touching join of §23.4.
- Every new hash is checked natively (x86-64) and as wasm32 under
  wasmtime by the gate here; aarch64 and Windows run in CI only.

## 24. Sweep and wire ends square to the path (stream AB)

Closes §22.5 item 1. Checked against 5417fac.

### 24.1 What was wrong

`paths::sweep` takes its frames from `rmf`, whose tangent at an end is
the first or last chord; `wire` places its end rings square to the first
and last run. Along a flattened curve each chord turns half a segment
from the curve's tangent at its end, so an end section tilts by that
much and overshoots by r·sin(half a segment): the reproduction (radius
127 along a quarter arc of radius 276.39) reached z −5.85 and x 282.24
for 0 and 276.39.

### 24.2 Decisions

- **The flattening knows the tangent; the path carries it.**
  `path3::Flat::tangents` holds the unit tangent at the start and the end
  where a curve piece ends the path, zero where a line does. Exact, no
  transcendental call: an arc's is `axis × (end − centre)` (its start's
  `e2`); a tangent arc's the same with its own axis; a spline's the
  direction of the Bézier's first and last control legs (`c1 − p1`,
  `p2 − c2`); the helix's its derivative
  (`paths::helix_tangents`, one `sin_cos` of the whole angle, the same
  call that places its last point).
- **Sweep and wire take them; zero is the chord.** `paths::Sweep` and
  `wire::Wire` gain `tangents: [DVec3; 2]`, zero by default, so a path of
  points and a path whose ends are lines go through exactly the same
  arithmetic as before (`rmf` is `rmf_ends` with zeros). A given tangent
  replaces the end's chord in the frames (`rmf_ends`): the end section is
  square to it, the cap's normal is it. `wire` places its end ring as a
  mitre's (its run's frame moved onto the plane square to the tangent,
  through the end point), as every ring inside a curve already is.
  Refused: a tangent that is not finite, or at 90° or more from its
  chord (a flattening never gives one; the kernel struct could).
  `closed` wires ignore them.
- **Inside, unchanged.** A sweep's inner section is square to the
  direction from the point before to the point after, a wire's to the
  bisector of its two runs; on a curve flattened with even steps both
  are the curve's own tangent at that point. Where a line meets an arc
  (a lead-in) the mitre is as before; no defect was shown there.
- **A plain point has no tangent.** The program passes `Flat::tangents`
  through for `sweep` and `wire` (`shape-model`'s `program.rs`, two
  lines); a list of points gives zeros. `guides/program.md` and
  `guides/typescript.md` say so in a clause; the lead-in workaround
  (`typescript.md`'s `sweep` row, §22.3) is removed.

### 24.3 Other users of the same frames

- `thread` sweeps a triangle along `paths::helix`'s points with the
  chord at each end: the same tilt. It now passes the helix's tangents.
  Its z bounds barely move (for radius 4, pitch 0.7, length 5: −0.31440
  to 5.46848 before, −0.31484 to 5.46656 now; the triangle's half width
  dominates), but the end faces now lie square to the helix.
- `sweep::sweep` (three.js-style, `frames::frenet_frames`) and
  `extrude` along an `ExtrudePath` take each sample's tangent from the
  curve's own `sample`: no chord, no overshoot. Unchanged.
- `loft` has no path. Unchanged.

### 24.4 Golden

Every golden line of a point path or a straight-ended path keeps its
bytes. Moved, each a sweep or wire ending on a curve piece or a helix,
with the angle between each end cap's normal and the path's exact
tangent (measured from the built caps, old → new):

| line | what ends on a curve | old | new |
|---|---|---|---|
| `rest.program.glb`, `rest.program.measure` | `rail` (sweep, ends in a spline) | start 0° (a line), end 5.52° | 0°, 6e-14° |
| | `spring` (wire, helix radius 3, 2.5 turns) | 18.64°, 18.64° | 2e-15°, 5e-15° |
| | `crook` (wire, ends in an arc) | start 0° (a line), end 7.50° | 0°, 0° |
| `solid.thread`, `uvs.generators2` | `thread` (the only change in either: with the thread's tangents set to zero both keep their bytes) | 53.5° chord to tangent at both ends | 8e-14° |

Every moved part stays closed with no self-intersection (`shape measure
--intersections` on the exported parts: 0 each). New:
`gen2.sweep.ends` — the reproduction with its `through` exact (a sweep
and a wire; the case fails unless z starts at 0 and x ends at 276.39 to
1e-9), a sweep ending in a spline and a wire along a helix.
`crates/shape-gen/tests/sweep_ends.rs` checks the same bounds; with the
end tangents ignored it fails (z −5.87).

The reproduction as committed (`eval/phase6/Z/repro/sweep_end_tangent.json`,
its `through` rounded to 0.01) now measures z from −0.0200 and x to
276.4100: exactly 127 · sin of the angle by which that rounded arc's own
tangent leans from +z (1.576e-4 rad), so the caps sit on the arc's
tangent.

### 24.5 Found, not changed

- **`paths::helix` has one turn's segments in all.** It asks
  `segments(radius, 2π · turns)`, and `shape_2d::curve::segments` clamps
  the angle to one turn, so a helix of any number of turns gets the
  segment count of one (24 for these radii): `thread`'s golden helix
  (7.1 turns) has 107° a segment, `rest`'s spring 37.5°. With the ends
  on the tangent this no longer shows at the ends, but the helix itself
  is coarse (§16.4's spring reaching 6.498 for 6.5 is the same). Fixing
  it moves every line with a helix (`solid.thread`, `uvs.generators2`,
  `rest.program.*`, `gen2.sweep.ends`); not this stream's to decide.
  **Fixed at the merge** (the owner's decision): one turn's count times
  the turns; those lines and `solid.sweep.helix` moved.
- **A tangent arc after a curve leaves along the last chord**, not the
  curve's tangent (`flatten`'s `heading`): a `tangentArc` after an `arc`
  or `spline` kinks by half a segment. Interior, so not changed here.

### 24.6 What is untested

- A spline's and a tangent arc's end tangent are checked through the
  golden case and the cap angles above, not by a dedicated test.
- Gate run here: lint (fmt, clippy, cargo-deny), test, golden (native)
  and wasm (the wasm32 builds, the golden run as wasm32-wasip1 under
  wasmtime 50.0.1, the binding under bun and node), all green. aarch64
  and Windows: CI only.

## 25. The counterbore, and the repair measured (stream AD, 2026-11-10)

Base 5417fac, where `boolean` repairs every result
(`shape_bool::uncross_output`, §23.2). Every side is still decided by
`orient3d` and `orient2d`.

### 25.1 The counterbore (§23.1)

**Re-measured with the repair on: it no longer crosses itself.** The
plate of §23.1 (80 × 50 × 10, a 6.6 hole with a 13.2 counterbore 4 deep at
(12.3, 4.56), through) has 8 crossing pairs as built and none repaired
(one point merged, `Uncrossed { before: 8, after: 0, collapsed: 1 }`).

**Cause.** The bore is cut first, then the counterbore. The counterbore's
floor (its tool's bottom cap, at z = 6) has an edge along a diameter, from
the corner at one of the bore's column angles (both tools have the same
segment count and the same angles; the corner is at twice the bore's
radius, on the same ray). The bore's column at that angle lies on the
edge in exact arithmetic and an ulp off it in f64. The column's crossing
with the floor rounds to z = 0.6 · 10 = 6.000000000000001, against 6.0
for the floor edge's crossing with the bore wall next to it, so a needle
between the two points (1.2·10⁻⁴ long in x, an ulp in z) folds over the
floor and the wall: 8 pairs. Same kind of coincidence as the
countersink's (§23.1), with a floor edge in place of a cone generator.

**Not fixed in `shape-edit`.** Turning the counterbore half a segment, as
the countersink is, clears the counterbores (3 → 0 of 414 below, repair
off), but the countersink's cone, turned the same half segment, then has
its generators on the counterbore's columns: 440 of 828 counterbores with
a countersink crossed themselves (up to 156 pairs; all repaired). The
counterbore is left unturned and the repair handles it; no golden line
moved.

**The sweep, extended** (plate 80 × 50 × 10; positions (0, −5),
(−30, −15), (0, 0), (12.3, 4.56), (7.77, −3.21), (−21.4, 11.9),
(23.3, −11.7), (0.1, 0.1); diameters 2.5, 3, 4, 5, 6.6 and 8.4 (the last two
not at |x| > 25); through, 6 and 8.5 deep; counterbore twice the
diameter, 2, 4 and 5.5 deep (shallower than the hole); countersink twice
the diameter at 60°, 82°, 90°; both together, counterbore 4 deep with a
countersink 2.5 or 3 times the diameter at the same three angles):

| holes | repair off | repair on |
|---|---|---|
| plain (138) | 0 | 0 |
| counterbore (414) | 3: (12.3, 4.56) 6.6 through, 4 deep (8 pairs); (−21.4, 11.9) 6.6, 8.5 deep, 4 and 5.5 deep (4 each) | 0 |
| countersink (414) | 0 | 0 |
| counterbore and countersink (828) | 0 | 0 |

Every result closed. Test `a_counterbore_floor_does_not_cross_itself`
(`shape-edit`, the three counterbores; it fails with the repair off).

### 25.2 The bench with the repair on

`bench/booleans` now repairs the og-shape engines' outputs as `boolean`
does (inside the timing); `--no-uncross` leaves them as built, and
`--hashes` with `--crossings` writes each boolean's crossing pairs after
its hash. Engines og-geom, og-shape-checked, og-shape-untangled, every
suite, `--crossings`; the two runs back to back, one binary (x86-64,
release, one thread; a shared 4-core container). Mean ms of ok cases:

| suite | engine | repair off | repair on |
|---|---|---|---|
| sand3d (17) | og-geom | 3.3 | 3.4 |
| | og-shape-checked | 7.6 | 10.3 |
| | og-shape-untangled | 8.2 | 9.7 |
| Thingi10K (40) | og-geom | 127.2 | 130.3 |
| | og-shape-checked | 377.9 | 506.0 |
| | og-shape-untangled | 446.8 | 580.8 |
| synthetic (40) | og-geom | 106.7 | 107.5 |
| | og-shape-checked | 485.4 | 521.2 |
| | og-shape-untangled | 568.7 | 613.1 |
| touching (7) | og-geom | 0.7 | 0.6 |
| | og-shape-checked | 32.7 | 37.8 |
| | og-shape-untangled | 40.6 | 47.0 |
| scaling (5) | og-geom | 208.2 | 198.7 |
| | og-shape-checked | 483.0 | 516.4 |
| | og-shape-untangled | 639.2 | 655.4 |

**Verdicts:** the same case by case (§18.7's counts). **Hashes:** 8 of
921 move, each from an output that crossed itself to one that does not:
`rounded.box.degenerate` subtract (2 → 0 pairs, both og-shape engines) and
`torus.ring.on.face` union, intersect and subtract (checked 12, 8, 8 → 0;
untangled 31, 28, 29 → 0). No unmoved output crosses itself.
**`--crossings`:** ok outputs crossing themselves, repair off: synthetic
1 of 24 and 29, touching 1 of 7, every other suite 0; on: 0 everywhere.
**Speed:** the repair's detection costs up to a third on Thingi10K, where
nothing crosses (378 → 506 ms checked), as §23.7 found; og-geom, whose
code did not change, moved by 5 % between the runs (noise).

### 25.3 Why `bevel.sound` and `touching.program.rim` did not move (§23.2)

Both reach the repair and are repaired; neither hash can see it.

- **`bevel.sound`:** the post's join (pin on plate) is repaired, 4 pairs
  → 0, one point merged: 56 points and 108 triangles against 57 and 110
  as built. The case hashes the fillet of that post, and the bevel welds
  an input that crosses itself first (§7.7), collapsing the same edge
  shorter than 10⁻⁹ of the offset. The fillet's bytes are the same
  either way.
- **`touching.program.rim`:** the cup's rim join is repaired in the
  cup's own frame, 20 pairs → 0, two points merged (350 triangles against
  354). The case hashes the program's GLB: the part is placed in the
  world (turned 22.5°, then canonicalized and its slivers dropped,
  `shape-model`'s `placed`), 330 triangles either way, and rounded to
  f32, where the two differ no more. The repair is reached where it
  should be.
- **Found:** the world-placed cup crosses itself by 5 pairs with the
  repair on or off: on its top face (z = 137), two points 2·10⁻¹⁴ apart
  whose needle the turn's rounding folds. No boolean makes these (the
  boolean's result in the cup's frame has none), so the boolean's repair
  cannot reach them; `placed` is `shape-model`'s and was not changed.

### 25.4 Faces a few ulps apart, tested (§23.4, §23.8)

Stream AA's reproductions were not kept; these are rebuilt
(`crates/shape-bool/tests/ulps_apart.rs`): two 32-gon prisms of radius 10,
0 to 10 and 10 to 20, built by hand so each cap's fan is chosen, tilted
0.3 rad about x and 0.2 about y (the shared ring's corners turn to the same
f64 points in both).

| case | join | cut | intersect |
|---|---|---|---|
| tilted, the tool's bottom fanned from corner 5 | refused: `OpenTouching`, 8 edges, `side`, `top` / `bottom`, `side` | refused: `NotManifold`, 1 edge | empty |
| the tool's bottom with a vertex at each edge's midpoint, flat | closed, touching, 0 touching edges, 0 pairs | the same | empty |
| the same, tilted | refused: `OpenTouching`, 42 edges, `side`, `top` / `bottom`, `side` | **closed, 259 crossing pairs** | empty |

Tests `a_tilted_ring_fanned_two_ways_is_refused` and
`a_ring_with_midpoints_joins_flat_and_is_refused_tilted`. With this tilt
the midpoints' join is refused rather than returned with touching edges
(§23.4's own tilt was not recorded).

**Found: a closed cut that crosses itself, returned with no error.** The
tilted midpoints' cut keeps the target's top cap (30 triangles) in 156
pieces, cut by the tool's 64-vertex fan lying within ulps of its plane;
each such crossing point is an edge's crossing with a plane it lies ulps
from, so it can fall anywhere along the edge, and the pieces fold over
one another (257 `top`/`top` pairs, 2 `side`/`top`). The volume is right
(3121.445, as untilted). The repair merges one point (266 → 259); its
moves are a snap distance long and cannot fix points misplaced by
millimetres. Not fixed here: the fix is §23.4's "one surface" step (two
regions with one boundary loop within the snap distance taken as one), or
refusing a result whose repair leaves crossing pairs at constructed
corners. The second costs nothing (the repair counts them) and would move
one golden line, `selfunion.check` (whose inputs cross; 26 pairs left),
with the error changing. Both are left for review.
**The second was tried at the merge and taken back.** With the refusal
on, `shell` of an ordinary cup failed (`eval/phase6/B/obj1_octagonal_cup`:
an octagonal `extrude` turned π/8, its bottom chamfered, `shell` open at
the top, 20 thick): the cut of the inner solid leaves 44 crossing pairs
after the repair, needles at rounding scale where the inner solid's
corner edges pass within an ulp of the top cap's diagonals. The cup
built before, closed and of the right shape, and an agent's program
depends on it; refusing it is worse for an author than the needles. So
a result the repair leaves crossing is still returned, with no error
and no report: a known gap against `docs/design.md` §6. Routes: the
"one surface" step above; a repair move for a corner on a cap's
diagonal; or the count carried in `BoolReport` as a measurement.
**Corrected by stream AF (§27.1):** the cup's 44 pairs were not needles
at rounding scale. The inner solid's chamfer, offset by more than its
width, folds over and runs out through the walls: a lip 1.7 high,
750 mm³. `shell` now untangles the inner solid first.

### 25.5 `check_operands` on by default, with the repair on

Measured with `check_operands(target, tool)` called first in
`boolean_report` (not kept): **1 golden line moves**, `selfunion.check`
(two of its booleans refused up front instead of after; its inputs cross
by design). No operand of any other golden boolean crosses itself, the
kernel's own intermediate solids included (§23.6's three chains are
clean now). Not turned on; its cost is §5.2's.

### 25.6 What is untested

- The counterbore is swept on one plate, top face only, axis along z.
- The tilted cut of §25.4 is found, not refused or fixed.
- Every new hash is checked natively (x86-64) and as wasm32 under
  wasmtime by the gate here; aarch64 and Windows run in CI only.

## 26. The built part that crossed itself (stream AE)

Base 514c386. Every side is still decided by `orient3d` and `orient2d`;
the one distance used is the boolean's snap distance (2⁻⁴⁰ of the box
diagonal, §18.1, §23.2), and only to decide where to look.

### 26.1 The mouse's shell

`eval/phase6/Z/Z1/sim2_mouse_and_pad`'s `mouse_shell` (a `body` placed at
(10, 15, 4), cut by a box and by an extruded ellipse) was returned with 8
crossing pairs. Replayed op by op (the part's world solid after each op):
`body` 0, `box` 0, the first `cut` (the wheel slot) **8**, every later op
8. The same three ops with every frame moved back by (10, 15, 4), so the
part is built at the world's origin: the cut gives 0 pairs (2 904
triangles). So the cut is clean in the part's own frame. The **move into
the world** (`shape-model`'s `placed`) folds it, not `body`, the boolean,
or the bevel. The later `fillet` is on another part.

**The pairs.** All lie in the slot's end wall, the plane y = 35, which
the move keeps exact. Two cut points on the shell's top edge, at x ≈
11.888 and z ≈ 36.320, lie 3.6·10⁻¹⁵ apart (one ulp in x; in z the move
rounded them equal). The needles between them, two triangles with an
edge 3.6·10⁻¹⁵ long, turn over their neighbours: a sliver of the top
surface lying in the wall's plane (6.3 long, 6·10⁻⁵ wide) and the wall's
triangle to the slot's corner (13.75, 35, 25.5). This is rounding scale,
not real geometry. The move also merged one other pair of points
(`canonicalize`, 1 454 → 1 453 vertices).

**The smallest program** is the shell's `body`, the slot's `box` and one
`cut`: `golden/programs/placed_slot.json`.

### 26.2 `placed`, and the cup of §25.3

A motion (`at`, an `on` frame, a turn) rounds every position. Two points
a rounding error apart cannot keep their relative position through it,
so a needle between them can turn over. `placed` already dropped needles
rounded exactly onto one line (`drop_slivers`, §15.4: each is flipped
away, exactly). Now it also repairs the near ones:

- **Where it looks.** Only where the moved solid has a triangle that
  rounding can turn over: an edge shorter than the snap distance, or a
  corner within it of the opposite (longest) edge's line. A fold of
  triangles that share an edge or a vertex needs one of the two. On such
  a solid its crossing pairs are counted (`crossing_pairs`, exact).
- **The repair** is the boolean's (`shape_bool::uncross_output`, §23.2),
  through its public signature, with every point free to move (the box's
  corners stand in for the operands: they set the snap distance). Points
  closer than the snap distance are merged, needles are split away, and
  each change is kept only if the solid stays closed and crosses itself
  strictly less. The kept triangles keep their source's surface, names
  and UVs (interpolated at a moved corner). A named edge keeps its names
  on what it became (or on both pieces of a split). A skinned vertex
  keeps its weights. A solid it cannot improve is returned as moved.
- **Also routed through it:** `mirror`'s image (a reflection rounds
  in the same way) and the unit scale on the way to the scene (`cm`,
  `m`).

`shape-bool` was not edited.

| part | before | after |
|---|---|---|
| `mouse_shell` (Z1 `sim2_mouse_and_pad`, and `placed_slot`) | 3 086 triangles, 8 pairs | 3 084, 0; volume 161 530.313364027 and area equal to every digit printed |
| §25.3's cup (`touching_rim.json`, turned 22.5°) | 330, 5 pairs (its top face) | 328, 0; volume equal to 9·10⁻¹¹ mm³ of 152 031, area equal |
| `uvs.rs`'s mirror image `b` (a seated plate mirrored about x = 30) | 216, 8 pairs (found here) | 212, 0; the same UV area |

The face names are the same sets, minus the needles. On the mouse, the
`wheel_slot` seam loses two tagged edges and the two odd-degree seam
vertices at the fold, so it is more of a loop than before. Two odd ones
remain at y = 55, from the boolean's seam naming. They do not cross.

Over every committed program (`eval/phase6/**`, `golden/programs/`; 580
built parts), parts crossing themselves go from 7 to 2. Fixed besides
the above: `T/art2_telescoping_slide`'s `outer_channel` (3 pairs) and
`U/auto1_curved_tubes`'s `middle_tube` (6), the same kind. **Left:** the
octagonal cup's `cup` in `A/` and `B/obj1_octagonal_cup` (44 pairs). That
is the boolean's leftover crossings after its repair, inside `shell`
(§25.4), which stream AF owns. Building all of them took 34.9 s before
and 34.2 s after (release; the check runs only on thin solids).

### 26.3 Golden

- `touching.program.rim` moved. This is the cup of §25.3: before, it
  crossed itself by 5 pairs; now it does not. It has two needle triangles
  fewer, and the same volume (to 10⁻¹⁶ relative), area and names. No
  other line moved, `mirror`'s included.
- New: `model.placed.slot`, the reduced mouse shell's world solid, hashed
  in f64. A GLB rounds to f32, where such a change can hide (§25.3).

### 26.4 Tests

- `crates/shape-model/tests/built_parts.rs`:
  `every_part_of_the_evaluation_programs_is_closed_and_uncrossed` builds
  all 20 `eval/phase6/Z/Z1` programs (140 scene entries, 119 with a mesh)
  and asserts each closed with no crossing pair. Its list of known
  exceptions is empty. With the repair off it fails, naming
  `sim2_mouse_and_pad mouse_shell: 8 crossing pairs`. It takes 1.4 s in
  release and 12 s in debug (`check.sh test` runs debug), most of it
  building the programs.
- `a_part_moved_into_the_world_does_not_cross_itself`: the reduction,
  built in its own frame and moved (0 and 0), and the cup.
- `uvs.rs`'s `every_edit_carries_uvs`: its mirror check counted the
  image triangle for triangle. It now allows the repaired needles (at
  most 8 fewer, never more), and it asserts that the image does not
  cross itself.

### 26.5 What is untested

- Two sheets of one solid a rounding error apart, not sharing a vertex,
  could be folded by a move without any thin triangle. The trigger would
  not look there. No case is known.
- The repair is checked through UVs and names on the cases above, not
  through a skinned part that crosses itself (none is known).
- The Z1 test covers the evaluation's 20 programs only. §26.2's count
  over every program was run once, by hand.
- Gate run here: lint (fmt, clippy, cargo-deny), test, golden (native),
  and the wasm32 builds (`shape-core`, `shape-wasm`, `shape-golden` for
  wasip1). wasmtime and wasm-bindgen were not available, so the wasm32
  golden run and the binding's tests were not run. The two new hashes
  are unchecked as wasm32, and aarch64 and Windows run in CI only.

## 27. Boolean results the repair leaves crossing (stream AF, 2026-11-10)

Base 514c386. Every side is still decided by `orient3d` and `orient2d`;
the one new use of a distance (the snap distance, below) chooses a
candidate result, which exact checks then keep or drop.

### 27.1 The cup: a folded inner solid, not needles

`eval/phase6/B/obj1_octagonal_cup` (an octagon of radius 104.45 turned
π/8, 137 high, its bottom chamfered 10, `shell` 20 thick open at the top)
left 44 crossing pairs after the repair (§25.4). **Read off the pairs,
the cause is not the top cap's diagonals.** The pairs are triangles of
the inner solid's bottom cap (z = 20, every corner the tool's own)
against its walls: the shell's inner solid crosses itself. `shell` moves each vertex by the
least-squares fit of the planes it touches (`offset`); the chamfer is 10
wide and the wall 20 thick, so the chamfer's offset turns over: the inner
bottom ring lands at z = 20 and radius 84.65 (a corner), the ring above it
at z = 18.28 and radius 82.8, and the inner bottom cap runs out through
the walls. `crossing_pairs` on the inner solid: 26. The cut keeps the
fold, a lip about 1.7 high and 1.85 wide around the inner bottom: the
cup's volume was 2 017 648.2, and is 2 016 896.4 with the fold gone (the
lip's section, about 1.6 mm², times the inner perimeter, 507). The
repair could not help: the crossings are millimetres long.

**Fixed in `shell`.** When the inner solid crosses itself (exact,
`crossing_pairs`), its outside is cut instead: `self_union`, the space
it winds round at least once. A fold turned inside out winds −1 or 0
there, so the fold drops and the inner solid is the prism 20 in from
every face. That result is taken if the cut is closed (no touching
edges: an inner solid meeting the outer along an edge would leave a
wall of no thickness) and crosses itself in no pair. Otherwise the folded
inner solid is cut as before, and the one with fewer crossing pairs
returned, so nothing that built before is refused. Test
`a_chamfer_narrower_than_the_wall_does_not_fold_the_hollow` (`shape-edit`;
44 pairs without the fix; the volume is the outer's less the octagonal
prism of apothem 76.5 from z = 20, to 10⁻⁹).

**The sweep** (`cargo run --release -p shape-edit --example
shell_sweep`): extrusions 80 high of regular polygons of 3 to 12 sides,
radius 50, turned 0, π/8, 0.3 and π/n; rounded rectangles 80 × 50 with
radius 5 and 15, 60 × 60 with radius 29; circles of radius 40 (the chord
rule, 24 and 64 segments). Bottoms: plain, tapered ±0.2, chamfered 2, 10
and 25, filleted 5 and 15 (`shape-bevel`). Thickness 1, 3, 10, 20, 30.
Open `top`, `bottom`, `+x`, or closed. 7 220 shells (the bevel refuses
7 of the bottoms); 1 410 open `+x` where no flat face faces +x
(`NotFlat`, the same before and after).

| | before | after |
|---|---|---|
| built | 4 827 | 4 920 |
| built, crossing themselves | 950 | 76 |
| refused | 983 | 890 |

Of the 950 that crossed, 886 are clean and 64 still cross (60 with the
same pairs, 4 with fewer). 93 refused before are built (81 clean, 12
crossing). None that built before is refused, and none comes out open
that did not (a square prism tapered 0.2, 30 thick, open at the top, is
non-manifold before and after). Crossing pairs over the sweep: 59 356
before, 24 496 after. `shell_report`'s count equals `crossing_pairs` on
every result. Two results that did not cross change volume: a triangle
turned π/8, chamfered 25, 20 and 30 thick, open at the bottom (205 681.0
→ 187 258.9 and 205 152.3): its inner solid crossed itself away from the
outer surface and its outside is cut now; a triangle of apothem 25
hollowed 20 or 30 thick turns its inner solid inside out, so neither
volume is a wall of that thickness.

Left crossing (76): rounded rectangles, mostly closed (`open: []`, 35 of
37), where rounded corners narrower than the wall fold the inner solid
and `self_union` of it comes out open; triangles and squares chamfered,
filleted or tapered (32), a hexagon (1), an octagon and a nonagon opened
at +x (6); 61 of the 76 are 20 or 30 thick. Refused (890; 885 diagnosed,
as before, as the shell's inner solid passing through itself): the cut of
a folded inner solid comes out open, and its outside could not be cut
closed either; 680 open the bottom under a chamfer or fillet, 162 open
+x. Each is an error value, as before.

### 27.2 Faces a few ulps apart as one surface (§23.4, §25.4)

The tilted midpoints' cut (§25.4) comes from the second pass: the first
is open (4 edges), the second closed with 266 crossing pairs, 259 after
the repair. Its top cap is cut by the tool's 64-corner fan, which lies
within ulps of it without being one plane, so every crossing point is an
edge's crossing with a plane it lies ulps from.

**Built: `shape_bool::csg_points_repaired`.** `csg_points_traced`, then
the repair (`uncross_output`); where the operands touch (coplanar or
touching pairs) and that result is open, not manifold, or still crosses
itself, both passes run once more with **each pair of triangles, one of
each operand, whose every corner lies within the snap distance of the
other's plane, without the two being exactly one plane (`orient3d`),
left uncut** (`Csg::near_coplanar`). Regions do not join such a triangle
to one of its own mesh that is not one (`Csg::flat`), so a cap left uncut
is a region of its own and its walls another; without that, the tilted
fanned join came back closed with the tool's half lost (its bottom, the
region's largest piece, has the target on one side, and the whole region
was dropped). Each result, repaired, is taken only if it is closed and
manifold (exact welding) and crosses itself in fewer pairs (exact,
`selfcheck::crossing`); an open or non-manifold result only by one that
crosses itself in none. `boolean` and the bench's og-shape engines call
it; the repair runs once per boolean as before.

The reproductions of §25.4 (`crates/shape-bool/tests/ulps_apart.rs`):

| case | join | cut | intersect |
|---|---|---|---|
| tilted, the tool's bottom fanned from corner 5 | was refused (`OpenTouching`); now closed, volume of both, 0 pairs | was refused (`NotManifold`); now closed, the target's volume, 0 pairs | empty |
| the tool's bottom with midpoints, flat | closed (unchanged) | closed (unchanged) | empty |
| the same, tilted | refused, `OpenTouching` (unchanged) | was closed with 259 pairs; now closed, the target's volume, 0 pairs | empty |

The tilted midpoints' join stays open: the tool's side walls have a
corner at each midpoint, an ulp off the target's ring edge, and nothing
splits the target's wall there. That needs the target's triangle split at
the tool's vertex (§23.4's last paragraph), not attempted.

**Bench** (`bench/booleans`, every suite and engine, `--crossings`,
Thingi10K's 40 meshes): verdicts the same case by case, all 921 output
hashes the same as at the base, no ok output crosses itself in either
run. No bench case reaches the new pass with a result it improves.
Mean ms of ok cases, the base against now (one binary each, run after
each other on a shared 4-core container with other work running, so
noisy): sand3d 10.0 → 9.4 (checked), 10.2 → 9.3 (untangled); Thingi10K
504.5 → 492.8, 577.5 → 574.3; synthetic 506.5 → 516.1, 599.8 → 607.0;
touching 39.3 → 37.2, 45.6 → 47.0; scaling 500.3 → 495.7, 636.3 → 652.0.
The new pass costs nothing where nothing touches and nothing crosses.

### 27.3 The count, and the refusal

Booleans that end with crossing pairs after the repair (`BoolReport::
crossing_pairs`), counted over:

| where | before | after |
|---|---|---|
| every committed program (`eval/phase6/**`, `golden/programs/`; 104 of 106 build, before and after) | 2 (the cup in runs A and B, 44 pairs each) | 0 |
| the golden cases | 1, `selfunion.check` (26; its inputs cross by design) | the same |
| `bench/booleans` | 0 | 0 |
| `shell` sweep (results crossing themselves) | 950 | 76 |
| `hole` (1 794), `pocket` (384), `seat` (192), `dent` (64) sweeps (`edit_sweep`) | 0 | 0 (every output byte for byte the same) |

**The refusal is not turned on**: 76 swept shells still cross
themselves (27.1), and refusing them would refuse shells that build
today. The count is carried instead: `BoolReport::crossing_pairs`
(`crossingPairs` in a boolean's answer, `shape build`, the MCP `build`
tool and the binding's `BoolReport`), and `shape_edit::EditReport` from
`hole_report`, `pocket_report`, `seat_report`, `shell_report` and
`dent_report`. A boolean counts pairs with a corner it made (the
repair's own count); pairs an operand brought crossing are not counted
(`check_operands` finds those): a closed shell's void is the inner solid
whole, so a folded inner solid's pairs never reach a constructed corner,
and `shell_report` counts the result's own pairs when its inner solid
crossed itself; `dent_report` counts the result's own (no boolean).
Golden: `rest.program.booleans` moved (it hashes the report's debug
text); no other line.

### 27.4 The standing test

`crates/shape-model/tests/phase6_b.rs`: every `eval/phase6/B/*/program.json`
builds natively, every part closed, without non-manifold edges, and none
crossing itself. With the shell fix undone it fails on the cup.

### 27.5 What is untested

- The cases left in 27.1 (folded inner solids `self_union` cannot
  untangle, or whose cut is open); `shell` builds an inner solid by vertex
  offsets, and a proper offset of a face that shrinks past nothing is not
  built.
- The "one surface" pass on operands that meet only partly within the
  snap distance (a tool's face overhanging the target's): its regions are
  then split by the walls' crossings, not tested beyond the prisms.
- The program answer of `shell`, `hole`, `pocket`, `seat` and `dent`
  did not show their count; **added at the merge** (`crossingPairs`, as
  a boolean's).
- Every new hash is checked natively (x86-64) and as wasm32 under
  wasmtime by the gate here; aarch64 and Windows run in CI only.

## 28. Real generated meshes (stream AG, 2026-11-10)

Base 514c386. Until now the mesh tools were tried on synthetic defects,
Thingi10K and the Khronos and three.js samples (§9, §17, §19.6); plan
phase 5 asked for real provider outputs to re-check the decision. No
provider key is configured here, so these are published outputs.
Validation first: numbers, no verdicts; the images are for a person.

### 28.1 The files

54 meshes from eight generators, listed in `bench/generated/manifest.tsv`
(URL, zip member, SHA-256, licence, generator, category) and fetched by
`bun bench/generated/fetch.ts DIR`, which reads single members out of
the published zips by HTTP range requests (208 MB for all 54) and checks
each hash. None is committed.

- **Hi3DBench** (3DTopia, `huggingface.co/datasets/3DTopia/Hi3DBench`,
  MIT): outputs of image-to-3D models on 3DGen-Bench's image prompts,
  generated by the benchmark's authors. The same six prompts for every
  generator: a house (5), four mugs (30), a cabinet (120), a teddy bear
  (300), a horse (420), an armchair (480). Hunyuan3D-2, TRELLIS, SPAR3D
  (GLB), TripoSR, InstantMesh, CRM (OBJ; CRM's with MTL and PNG), 6
  each; Unique3D (GLB, 16 MB each), the bear and the chair only.
- **Meshy** (`tfnn/MESHY.AI_800_GLB_3D-Assets_Categorised_and_Labelled`,
  MIT as published; curated from what Meshy's users generated and shared;
  Meshy's terms put free-tier outputs under CC BY 4.0, which also allows
  this use): 16 textured GLBs: aircraft, boat, two buildings, a cat, two
  trees, two people, a robot, a chair, a chest, a sword, two vehicles, a
  burger.

Skipped: 3DGen-Bench's own object sets (MIT, but gated behind a login,
so the prompts' names too); Tencent's and Stability's sample outputs
(licences with use restrictions); TripoSR heads (CC0, PLY only, which
the importer does not read); repositories without a licence
(`Zizun1/InstantMesh_Output`, `kaclad/hunyuan3d-outputs`); Tripo's
SimReady set (USD). Categories (per file in the manifest): 14
furniture, 10 characters, 9 props, 8 buildings, 7 creatures, 4 vehicles,
2 foliage.

### 28.2 What comes in

`shape import` and `shape measure --intersections` on each file, and
the file's own bytes read by `run.ts` (glTF JSON, OBJ and MTL text) for
what the importer does not report: maps, vertex colours, compression,
UV islands (triangles joined through corners with the same position and
UV).

| generator | files | format | triangles (median, range) | closed | pieces (median, max) | non-manifold edges (files) | degenerate (files) | self-intersecting (files; median pairs) | longest side (median, m) |
|---|---|---|---|---|---|---|---|---|---|
| Hunyuan3D-2 | 6 | glb | 150k (150k–150k) | 6 | 1, 4 | 0 | 0 | 4; 2 | 1.99 |
| TRELLIS | 6 | glb | 34k (9.7k–51k) | 2 | 3, 15 | 4 | 0 | 3; 11 | 1.00 |
| SPAR3D | 6 | glb | 42k (25k–70k) | 6 | 1, 5 | 0 | 0 | 0; 0 | 0.97 |
| TripoSR | 6 | obj | 159k (69k–253k) | 0 | 5, 120 | 0 | 0 | 1; 1 | 0.96 |
| InstantMesh | 6 | obj | 95k (36k–180k) | 6 | 32, 91 | 0 | 0 | 0; 0 | 1.92 |
| CRM | 6 | obj | 51k (15k–78k) | 4 | 23, 38 | 0 | 0 | 0; 0 | 1.85 |
| Unique3D | 2 | glb | 819k (619k–819k) | 1 | 4, 4 | 1 | 0 | 2; 3688 | 1.32 |
| Meshy | 16 | glb | 37k (15k–50k) | 15 | 1, 57 | 0 | 0 | 2; 531 | 1.20 |

### What comes in: appearance

| generator | UVs (files) | UV islands (median) | UV coverage (mean) | UV overlapping pairs (files) | textures (median per file; sizes) | decoded with mips (median MiB) | maps: base / MR / normal / emissive / occlusion | vertex colours | compression |
|---|---|---|---|---|---|---|---|---|---|
| Hunyuan3D-2 | 6 | 2277 | 0.68 | 4 | 1; 1024 | 5.3 | 6 / 0 / 0 / 0 / 0 | 0 | 0 |
| TRELLIS | 6 | 775 | 0.65 | 6 | 1; 1024 | 5.3 | 6 / 0 / 0 / 0 / 0 | 0 | 0 |
| SPAR3D | 6 | 505 | 0.30 | 6 | 2; 1024 | 10.7 | 6 / 0 / 6 / 0 / 0 | 0 | 0 |
| TripoSR | 0 | – | – | 0 | 0; – | 0.0 | 0 / 0 / 0 / 0 / 0 | 6 | 0 |
| InstantMesh | 0 | – | – | 0 | 0; – | 0.0 | 0 / 0 / 0 / 0 / 0 | 6 | 0 |
| CRM | 6 | 425 | 0.64 | 3 | 1; 1024 | 5.3 | 6 / 0 / 0 / 0 / 0 | 0 | 0 |
| Unique3D | 0 | – | – | 0 | 0; – | 0.0 | 0 / 0 / 0 / 0 / 0 | 2 | 0 |
| Meshy | 16 | 101 | 0.60 | 6 | 2; 2048, 4096 | 42.7 | 16 / 15 / 0 / 0 / 0 | 0 | 0 |

| generator | UVs (files) | UV islands (median) | UV coverage (mean) | UV overlapping pairs (files) | textures (median per file; sizes) | decoded with mips (median MiB) | maps: base / MR / normal / emissive / occlusion | vertex colours | compression |
|---|---|---|---|---|---|---|---|---|---|
| Hunyuan3D-2 | 6 | 2277 | 0.68 | 4 | 1; 1024 | 5.3 | 6 / 0 / 0 / 0 / 0 | 0 | 0 |
| TRELLIS | 6 | 775 | 0.65 | 6 | 1; 1024 | 5.3 | 6 / 0 / 0 / 0 / 0 | 0 | 0 |
| SPAR3D | 6 | 505 | 0.30 | 6 | 2; 1024 | 10.7 | 6 / 0 / 6 / 0 / 0 | 0 | 0 |
| TripoSR | 0 | – | – | 0 | 0; – | 0.0 | 0 / 0 / 0 / 0 / 0 | 6 | 0 |
| InstantMesh | 0 | – | – | 0 | 0; – | 0.0 | 0 / 0 / 0 / 0 / 0 | 6 | 0 |
| CRM | 6 | 425 | 0.64 | 3 | 1; 1024 | 5.3 | 6 / 0 / 0 / 0 / 0 | 0 | 0 |
| Unique3D | 0 | – | – | 0 | 0; – | 0.0 | 0 / 0 / 0 / 0 / 0 | 2 | 0 |
| Meshy | 16 | 101 | 0.60 | 6 | 2; 2048, 4096 | 42.7 | 16 / 15 / 0 / 0 / 0 | 0 | 0 |

- **Nothing was refused.** No file uses Draco or meshopt; every GLB is
  one node, one mesh, one primitive, one material. What the importer
  reads past, it now says (§28.5): 21 files' metallic-roughness (15
  Meshy) or normal maps (6 SPAR3D), and 14 files' vertex colours
  (TripoSR, InstantMesh, Unique3D), which were dropped silently before.
- **Scale and pivot.** Every generator normalises: the longest side is
  0.75–2.00 m whatever the object (a mug group 2.0 m, a sword 1.2 m, a
  house 1.0–2.0 m). All 54 have the origin at the box's centre: the
  base is 0.46–0.63 of the height below it (`pivot_from_base`).
- **Up and front.** The GLBs are Y up, as glTF says; CRM's OBJs too.
  TripoSR's OBJs are Z up (`--z-up` stands them up). InstantMesh's are
  in a frame that is neither: Y-up reading lays them on their side and
  `--z-up` turns them upside down; the OBJ options cannot express it.
  Which way the object faces differs too: Hunyuan3D and TRELLIS face +Z,
  CRM and SPAR3D away from it.
- **Material factors.** Hunyuan3D's and TRELLIS's GLBs leave
  `metallicFactor` unset (glTF's default 1.0: a metal) with roughness
  0.90 and 1; SPAR3D sets its own per file (metallic 0–0.15, roughness
  0.18–0.96); Meshy multiplies its metallic-roughness map by 1 and 1.
- **UVs.** Every textured generator writes an xatlas-style atlas: 100
  (Meshy) to 2 277 (Hunyuan3D) islands, up to 17 274 (SPAR3D's
  cabinet); 30–80 % of UV triangles mirrored (all but Meshy's robot);
  a few overlapping pairs in 25 of 40 files (at most 0.01 % of
  texels).
- **Topology.** 40 of 54 closed. TripoSR's are all open (63–261
  rims); four TRELLIS files have 3–41 non-manifold edges; Unique3D's
  chair crosses itself in 3 688 pairs and its bear in 206, Meshy's robot
  in 531; 31 files have floaters (TripoSR's house is in 120 pieces,
  InstantMesh's up to 91).

### 28.3 The game chain

`bun bench/generated/run.ts DIR OUT`: per file `shape process --budget
B` for B in 2 000, 10 000, 50 000, with the guide's repair (weld 1 µm,
floaters 5 %, holes up to 64 edges, fuse) and `--intersections`;
keeping the file's UVs and texture (`keep`, the default) and with
`--unwrap` (`fresh`); then `shape decimate --ratio 0.5 --lods 3` of the
`keep` 50k output; each run's wall time and peak memory (the child's
VmHWM, every 5 ms). x86-64, release, one thread, the fixes of §28.5 in.
`bun bench/generated/table.ts OUT/results.jsonl` prints these tables.

| mode | budget | ran | within budget | closed | one piece | error (median mm; median % of diagonal; max %) | UV overlapping pairs (files) | UV flipped (files) | coverage (mean) | base colour kept | maps dropped (files) | texture out (median MiB) | time (median, max s) | peak memory (median, max MiB) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| keep | 2.0k | 54/54 | 41 | 50 | 38 | 17.8; 1.12; 16.13 | 34 | 41 | 0.56 | 40 | 21 | 5.3 | 0.9, 145.6 | 88, 1373 |
| keep | 10k | 54/54 | 53 | 50 | 38 | 3.2; 0.16; 16.13 | 26 | 40 | 0.56 | 40 | 21 | 5.3 | 0.9, 145.5 | 88, 1372 |
| keep | 50k | 54/54 | 54 | 50 | 38 | 0.7; 0.03; 0.13 | 25 | 40 | 0.56 | 40 | 21 | 5.3 | 0.9, 149.6 | 88, 1372 |
| fresh | 2.0k | 54/54 | 54 | 50 | 38 | 7.9; 0.39; 1.16 | 0 | 1 | 0.50 | 0 | 21 | 0.0 | 0.9, 145.4 | 88, 1372 |
| fresh | 10k | 54/54 | 54 | 50 | 38 | 2.4; 0.12; 0.33 | 0 | 0 | 0.49 | 0 | 21 | 0.0 | 0.9, 150.4 | 88, 1372 |
| fresh | 50k | 54/54 | 54 | 50 | 38 | 0.7; 0.03; 0.09 | 0 | 0 | 0.49 | 0 | 21 | 0.0 | 1.0, 149.1 | 88, 1372 |

Within budget and closed, of 54: keep 40 (2k), 49 (10k), 50 (50k);
fresh 50 at every budget. The error is the decimation's largest
surface move. "UV flipped" counts files with any mirrored UV triangle,
as the files have them. "Base colour kept" counts outputs carrying a
texture: the 40 textured files keep it; the 14 vertex-coloured ones
have none to keep, and `fresh` drops every texture (it was painted for
the old UVs).

Per generator, within budget / closed:

| generator | 2k | 10k | 50k | fresh 2k | holes left after repair (files) | fuse refused (files) | floaters dropped (files) |
|---|---|---|---|---|---|---|---|
| Hunyuan3D-2 | 1 / 6 | 6 / 6 | 6 / 6 | 6 / 6 | 0 | 0 | 0 |
| TRELLIS | 3 / 2 | 6 / 2 | 6 / 2 | 6 / 2 | 3 | 2 | 3 |
| SPAR3D | 4 / 6 | 5 / 6 | 6 / 6 | 6 / 6 | 0 | 0 | 1 |
| TripoSR | 6 / 6 | 6 / 6 | 6 / 6 | 6 / 6 | 0 | 0 | 4 |
| InstantMesh | 6 / 6 | 6 / 6 | 6 / 6 | 6 / 6 | 0 | 0 | 6 |
| CRM | 4 / 6 | 6 / 6 | 6 / 6 | 6 / 6 | 0 | 0 | 6 |
| Unique3D | 2 / 2 | 2 / 2 | 2 / 2 | 2 / 2 | 0 | 0 | 1 |
| Meshy | 15 / 16 | 16 / 16 | 16 / 16 | 16 / 16 | 0 | 0 | 5 |

**Failures by cause** (35 of 324 runs; every run returned a report, none
an error):

- *Over budget, keeping the UVs* (14 runs, 13 files at 2k and
  SPAR3D's cabinet at 10k too): decimation stops where every remaining
  collapse would cross a UV seam the wrong way. The floor follows the
  island count, about three triangles an island: Hunyuan3D's house 2 277
  islands → 7 228 triangles; SPAR3D's cabinet 17 274 → 21 378. Reaching
  the budget there moves the surface far: the largest error at keep 2k
  is 16 % of the diagonal (the cabinet), 1.1 % median. `fresh` reaches
  every budget with 0.39 % median, 1.16 % worst.
- *Open, TRELLIS* (21 runs, 4 files): non-manifold edges (3–41) and
  holes next to them that a 64-edge fill does not close (2–10 left).
  The fuse step refuses an open shell, so the result stays open at
  every budget.

**LODs** (`decimate --ratio 0.5 --lods 3` of the keep 50k output; with
§28.5's fix each level halves the one before):

| generator | levels made | each level ≤ the one before | all closed | last level triangles (median) | last level error (median % of diagonal) |
|---|---|---|---|---|---|
| Hunyuan3D-2 | 6 | 6 | 6 | 6.3k | 1.06 |
| TRELLIS | 6 | 6 | 2 | 4.2k | 1.44 |
| SPAR3D | 6 | 6 | 6 | 5.2k | 1.19 |
| TripoSR | 6 | 6 | 6 | 6.3k | 0.76 |
| InstantMesh | 6 | 6 | 6 | 6.3k | 0.90 |
| CRM | 6 | 6 | 6 | 6.3k | 0.84 |
| Unique3D | 2 | 2 | 2 | 6.3k | 0.64 |
| Meshy | 16 | 16 | 16 | 4.6k | 0.57 |

**Time and memory.** Median 0.9 s and 88 MiB a run. Hunyuan3D (150k)
5.1 s, TripoSR 3.3 s, Meshy 0.6 s. Unique3D (619k–819k triangles) 28 s
for the bear and 146 s for the chair, 1.4 GiB: repair without fuse
takes 7.7 s on the chair, the fuse step 133 s (it untangles a
self-crossing shell of 619k triangles), decimation to 10k 6.5 s. All
324 runs: 28 minutes.

**Through the chain:**
- closed 40 → 50 of 54: TripoSR's cracks weld (80 vertices on the
  mugs) and its holes fill (5–155); every file but TRELLIS's four ends
  closed;
- pieces: 31 files in more than one piece before, 16 after (floaters
  dropped on 26). Unique3D's go *up*, 1 → 23 and 4 → 53: the fuse's
  untangling leaves the pockets the surface folds round as shells of
  their own (the largest 74 and 1 012 triangles);
- crossing pairs: 12 files before, 21 after keep 10k (13 fresh): the
  decimation folds thin parts through each other (TRELLIS's house 1 924
  pairs, Unique3D's chair 526);
- UV overlap after keep 10k stays at most 0.04 % of texels except two
  files whose holes were filled across UV seams: TRELLIS's cabinet (1
  fill, 55 %) and Meshy's robot (31 fills, 25 across a seam, 51 %;
  0.006 % without the fills). The fill gives each rim vertex its UV in
  one chart, so a fan across charts spans the atlas;
- texture memory out: the file's base colour, as it is: 5.3 MiB median
  (1024² with mips), up to 85 MiB (Meshy's 4096²).

### 28.4 Booleans (plan phase 5's re-check)

`cargo run --release` in `bench/generated` (its own workspace): each
file's keep 10k output, cut by a box over its top middle (0.4 of each
size) and by a cylinder along z off its centre, and joined with the next
file of the same generator moved to overlap a third of it.

| generator | boolean | outcomes |
|---|---|---|
| CRM | cut box | closed 6 |
| CRM | cut cylinder | closed 6 |
| CRM | join next | closed 4, refused: operand crosses itself 1 |
| Hunyuan3D-2 | cut box | closed 3, refused: operand crosses itself 2, tool missed 1 |
| Hunyuan3D-2 | cut cylinder | closed 6 |
| Hunyuan3D-2 | join next | closed 4, refused: operand crosses itself 1 |
| InstantMesh | cut box | closed 6 |
| InstantMesh | cut cylinder | closed 6 |
| InstantMesh | join next | closed 5 |
| Meshy | cut box | closed 13, refused: operand crosses itself 1, tool missed 2 |
| Meshy | cut cylinder | closed 15, refused: operand crosses itself 1 |
| Meshy | join next | closed 15 |
| SPAR3D | cut box | closed 6 |
| SPAR3D | cut cylinder | closed 6 |
| SPAR3D | join next | closed 5 |
| TRELLIS | cut box | closed 2, refused: operand not closed 4 |
| TRELLIS | cut cylinder | closed 2, refused: operand not closed 4 |
| TRELLIS | join next | refused: operand not closed 5 |
| TripoSR | cut box | closed 6 |
| TripoSR | cut cylinder | closed 6 |
| TripoSR | join next | closed 5 |
| Unique3D | cut box | closed 2 |
| Unique3D | cut cylinder | closed 2 |
| Unique3D | join next | closed 1 |

154 booleans: 132 closed, 3 cuts missed the mesh (the box over a gap),
19 refused with the cause: 13 an open operand (TRELLIS's four), 6 an
operand that crosses itself (decimated outputs of Hunyuan3D, CRM and
Meshy). None returned an open or crossing
result. Median 43 ms (box), 62 ms (cylinder), 108 ms (join); up to 9.5 s
on Unique3D. §5.3's finding holds: on these meshes the boolean is not
the problem; closing the input and not folding it in decimation are.

### 28.5 What was fixed

Each with a test that fails without it; no golden line moved.

1. **The fuse step made every edge a UV seam** (`repair.rs`). The
   self-union interpolates each output corner's UV in its source
   triangle, and the rounding differs from triangle to triangle round a
   vertex: Hunyuan3D's mugs had 6 237 seam edges before fusing and
   31 611 after; decimation keeping the UVs then stopped at 31 304
   triangles for a budget of 2 000, and Meshy's barn (one piece, untangled)
   at 17 810. Now a corner on an input vertex takes that vertex's own UV
   (the nearest of its UVs, so seams keep their sides): 6 237 again, and
   both reach 2 000. The interpolation itself is in `shape-bool` (another
   stream's crate); a point on an input edge still interpolates there.
2. **`process --unwrap` decimated under the old seams** (`process.rs`):
   the file's UVs are dropped before decimation when new ones are asked
   for. Before: the same floor as keep.
3. **`--unwrap` and `--posed` took a value** (`args.rs`): `process m.glb
   --unwrap -o out.glb` wrote nothing; `measure --posed --intersections`
   counted none.
4. **`decimate --lods N` gave N identical levels** (`decimate.rs` in the
   CLI, and the MCP tool): the desc was repeated, and `shape_tools::lods`
   takes a ratio of the input. Level k now keeps ratio^k, or the target
   and then half the level before.
5. **Vertex colours dropped silently** (`import.rs`): glTF `COLOR_0` and
   OBJ `v x y z r g b` are now in `skipped`.
6. **The preview drew UV seams as edges** (`preview.rs` in the CLI): a
   file's vertices split at seams were open edges to the rasterizer, a
   line and a crease along every seam. The parts are welded by exact
   position first.

`crates/shape-wasm/ts/tools/render_glb.ts` renders mesh files side by
side in three.js with every map the file has (GLTFLoader, OBJLoader with
its MTL and vertex colours).

### 28.6 Seen

`docs/previews/generated/`, 12 files, each 600 × 400: top row `shape
preview`, bottom row three.js, each as input, keep 10k, keep 2k. The
bear (Hunyuan3D) and the barn (Meshy) look the same at 10k and keep their
texture at 2k; the bear's input renders darker in three.js because its
material is a metal by glTF's default (§28.2) and the output's is not.
SPAR3D's cabinet: its 2k is the over-budget 21k. TRELLIS's bear: 10
holes left, not visible at this size. TripoSR's horse and InstantMesh's:
lying down (their up axis), their vertex colours gone after the chain.
Unique3D's chair: white after, tinted before. Meshy's robot: its 31
fills do not show at this size; the measure shows them (51 % overlap).
CRM's mugs, Meshy's car, palm tree and Hunyuan3D's house are the
ordinary cases.

### 28.7 What a game still lacks (ranked by files affected, of 54)

| gap | files | numbers | reproduction |
|---|---|---|---|
| The pivot and scale are the generator's: no tool sets a base pivot or a real size | 54 | origin at the box centre in all 54 (base 0.46–0.63 of the height below); longest side 0.75–2.00 m whatever the object | `shape measure` of any file: `pivot_from_base` |
| The preview draws no textures or vertex colours | 54 | 40 textured, 14 vertex-coloured; the CPU preview is grey | `shape preview` of any file |
| Material factors are not carried: the output is metallic 0, roughness 1 | 36 of 38 GLB | Hunyuan3D and TRELLIS (12) are metals by glTF's default and stop being; SPAR3D's (5) own values; Meshy's (15) with their map dropped | `shape process hunyuan-300.glb -o o.glb`, then the two `materials` |
| No texture budget tool (resize, compress) | 16 over 16 MiB | in: 40 files over 4 MiB with mips, 22 over 8, 16 over 16, 3 over 64 (Meshy 4096²); out at keep 10k: the same base colour, 16 over 16 MiB | `table.ts`'s per-file column |
| Maps other than base colour are dropped | 21 | 15 metallic-roughness (Meshy), 6 normal (SPAR3D: every file of it) | `shape import spar3d-30.glb`: `skipped` |
| Keeping UVs, the budget is not reached: no atlas repacking or chart merging | 13 at 2k, 1 at 10k | floor ≈ 3 triangles an island; 7 228 for Hunyuan3D's house at 2k | `shape process hunyuan-5.glb --budget 2000` |
| No baking: fresh UVs reach any budget but lose the texture | 40 textured | fresh: 50 of 54 within budget and closed at 2k, no texture | `shape process … --unwrap` |
| Decimation folds thin parts through each other | 21 at keep 10k | 12 before; booleans then refuse 6 | `shape process trellis-5.glb --budget 10000 --intersections` (1 924 pairs) |
| Vertex colours are not read, and the output has no colour at all | 14 | TripoSR, InstantMesh, Unique3D: their only colour | `shape import triposr-30.obj`: `skipped` |
| Up axis not readable for one generator; front differs | 12 | TripoSR Z up (`--z-up` works); InstantMesh neither Y nor Z up | `shape preview instantmesh-420.obj --z-up` |
| Hole fills across seams span the atlas | 2 | 51–55 % of texels overlapped (0.006 % without the fills) | `shape repair robot_mech1.glb --weld 0.001 --floaters 0.05 --holes 64` |
| Non-manifold edges and the holes beside them are not repaired | 4 | TRELLIS: 3–41 non-manifold edges, 2–10 holes left; open at every budget; 13 booleans refused | `shape repair trellis-5.glb --weld 0.001 --holes 64 --fuse`: `fused.refused` |
| The fuse step is slow on large self-crossing meshes | 1 | 133 s, 1.4 GiB on 619k triangles | `shape repair unique3d-480.glb --fuse` |

### 28.8 For the orchestrator

- **A fix that moves a golden line, not applied.** The CLI's and MCP's
  repair report (`shape-cli/src/report.rs`, `repair`) leaves out
  `RepairReport::uv_seam_fills`, though `guides/meshtools.md` step 6
  tells authors to read it. Adding it (one line) moves
  `mcp.process.ball_defects` (the MCP `process` report is hashed; its
  value would be 0). **Applied at the merge**, with the owner's leave. Reproduction: `shape repair robot_mech1.glb --weld
  0.001 --floaters 0.05 --holes 64` reports 31 fills and no
  `uv_seam_fills`; the library says 25. Decide whether the line may
  move.

- The fuse fix restores the file's UV on vertices the union keeps;
  `shape_bool`'s interpolation (`named.rs`, `interpolate`) still rounds
  on input edges and cut points, which only the boolean's owner can
  change (an exact corner and a symmetric edge parameter would do).
- Material factors and maps beyond base colour, a pivot and scale
  tool, texture resizing, chart merging and baking are each an API
  change (design.md §2 lists atlas and baking as later); the table
  above is the evidence for their order.
- Not tried: provider APIs (no key), Rodin, Tripo's own API outputs,
  Stable Fast 3D (its outputs' terms), FBX or Draco outputs (none of
  these generators write them), skinned generated characters (none
  here are rigged).

## 29. Materials, maps and vertex colours through the chain (stream AH)

Base 64af70c. §28.7 found four gaps in how a mesh's look goes through
the tools, by files affected of 54: material factors not carried (36 of
38 GLB: the output was always metallic 0, roughness 1), maps other than
base colour dropped (21: 15 metallic-roughness, 6 normal), vertex
colours not read (14: TripoSR, InstantMesh, Unique3D, whose only colour
they are), and no texture budget (16 files over 16 MiB of texture with
mips, Meshy's 4096² up to 85 MiB). This stream closes the first three
and adds the fourth. Numbers, no verdicts.

### 29.1 The decisions

- **`Solid::colors: Option<Vec<[[f64; 4]; 3]>>`**: linear RGBA per
  triangle corner, exactly parallel to `Solid::uvs` (§17.1), for the
  same reasons: a hard colour border is two corners of one vertex with
  different colours, so welding by position never splits or merges for
  colour. Absent unless the input has colours; `canonicalize` rotates
  them with their triangle and breaks the last ties by them, after the
  UVs; `hash_into` adds them only when present. No golden line of a
  solid without colours moved. A triangle added without saying is opaque
  white (`shape_mesh::WHITE`), which is what glTF draws for a primitive
  without `COLOR_0`; `append` gives the side without colours white.
- **Read:** glTF `COLOR_0` as float or normalised unsigned byte or short,
  RGB (alpha 1) or RGBA, linear as glTF defines it; a primitive without
  it is white. OBJ `v x y z r g b` as sRGB (as three.js's OBJLoader and
  the generators that write it mean it), made linear; a file whose
  components exceed 1 is read as 0..=255; a part takes colours when any
  of its vertices has one, and the others are white (reported). FBX's
  `LayerElementColor` is listed in `skipped`, not read.
- **The material of an imported part** is a `shape_tools::Material`
  (`ImportedPart::material`, which was the material's name;
  `ImportedPart::map` became `material.map`): the export's struct,
  three.js's names, extended with what glTF has and it lacked:
  `emissive_intensity` (`KHR_materials_emissive_strength`),
  `transparent` (`BLEND` or `OPAQUE` explicitly), `alpha_test` (`MASK`
  and its cutoff), `double_sided`, `metalness_map` (glTF's
  metallic-roughness texture; three.js sets it as both its metalness
  and roughness map), `normal_map` and `normal_scale`, `emissive_map`,
  `ao_map` and `ao_map_intensity`. Each new field's zero value writes
  what was written before, so no golden GLB of a program moved.
- **Colour space.** `Material::color` and `emissive` were sRGB (three.js
  `color`) and written to glTF linearised by `trig::pow`. A file's glTF
  factors are linear; converting them to sRGB and back would not return
  the file's bits. `Material::color_space` (`ColorSpace::Srgb`, the
  default, or `Linear`) says which: an import sets `Linear`, and a
  file's factors go out bit for bit (the round-trip test compares them
  with `==`). MTL's `Kd`/`Ke` and FBX's colours are sRGB, as three.js's
  loaders read them.
- **glTF's defaults as the file means them.** An unset `metallicFactor`
  is 1 by the specification, and every viewer draws such a material as
  metal; so does the import. `ImportedPart::defaults` lists each factor
  the file left unset (`metallicFactor`, `roughnessFactor`,
  `baseColorFactor`), and the import report shows them, so an agent can
  see that Hunyuan3D's bear is metal by default, not by intent, and
  decide. A primitive with no material takes glTF's default material
  (also metal).
- **Maps.** All five glTF maps are read on `texCoord` 0, kept as their
  files' bytes, and written back with the solid's UVs; a map on another
  UV set is listed in `skipped`, as is a sampler that does not repeat
  (Meshy's robot mirrors; one sampler, linear and repeating, is
  written). OBJ: `map_Kd`, `map_Ke` and `norm` (the PBR extension's
  normal map); `bump`/`map_Bump` are height maps, and `map_Pr`/`map_Pm`
  separate grey maps glTF cannot take without combining them: listed,
  with every other unread MTL statement (`Ks`, `Ns`, `Ka`, `illum`…).
  FBX: the textures linked to `DiffuseColor`, `NormalMap` and
  `EmissiveColor`; others listed.
- **Where the material goes.** The CLI joins a file's parts into one
  solid and writes one material (a plain GLB). Parts whose materials
  agree (names aside) give theirs; otherwise three.js's default with the
  base-colour map every textured part shares, as before, and the
  report's `material.dropped` says so. `--unwrap` (and `shape unwrap`)
  drop every map, each named in `dropped`; the factors and colours stay.
  OBJ and STL hold positions only: the output's `not_written` lists the
  UVs, colours and material left out.
- **The texture budget** (`shape_tools::downsize`, `--texture-max PX`):
  decode, box filter, PNG. Rejected: KTX2/Basis (an encoder is a large
  dependency or port, and out of scope here); resampling with a wider
  kernel (a box filter's weights are exact rationals, so the integer
  arithmetic is exact and target-independent; a Lanczos kernel needs
  `sin`).
- **No new crate.** The PNG decoder inflates with `miniz_oxide` (already
  used); the JPEG decoder is written here from the specification (ITU
  T.81: Huffman tables per F.2.2.3, the coefficient extension F.2.2.1,
  block order A.2): baseline and extended sequential Huffman, 8-bit,
  one or three components, any sampling, restart intervals, JFIF and
  Adobe colour transforms. Progressive and arithmetic-coded JPEG are
  refused with a reason (none of the 43 JPEGs in the bench is
  progressive).

### 29.2 Determinism of the image code

- The inverse DCT is separable and integer: an 8×8 basis table
  `c(u)/2·cos((2x+1)uπ/16)` computed once by `trig::cos` and rounded to
  integers × 2¹³, rows kept with 3 extra bits, columns rounded to
  bytes, all in `i64`. Chroma at half resolution is filled by the
  triangle filter (¾ the nearer sample, ¼ the next), in integers.
  YCbCr to RGB in 16-bit fixed point.
- The box filter weighs each input texel by its exact overlap with the
  output texel, as integers (input and output sizes as the units), in
  two passes with one rounding each. Colour channels are averaged in
  linear light: a 256-entry sRGB-to-linear table × 2²⁰, computed once
  by `trig::pow` and rounded; the result goes back to the sRGB byte
  whose table entry is nearest (a binary search). Alpha and data maps
  are averaged as stored (× 2¹² for the rounding). A normal map's
  averaged vectors are not renormalised: shorter normals where the map
  varies inside a texel, which a viewer renormalises.
- PNG out through the existing encoder (filter Up, deflate level 9).
- Golden `meshtools.look.texture` hashes the fixture JPEG's decoded
  texels and the PNG bytes of four downsizings; the gate compares it on
  every target.

### 29.3 Through the tools

| tool | vertex colours | material and maps |
|---|---|---|
| `weld`, `remove_degenerate`, `remove_floaters`, `orient` | a kept triangle keeps its corners' colours; a turned triangle turns them | per part, untouched |
| `close_holes` | each rim vertex takes its colour in the rim triangle leaving it, a fan's centre their mean (as UVs, §17.2) | – |
| `fuse` (self-union) | interpolated in the source triangle by `shape_bool`; a corner on an input vertex gets that vertex's own colour back (the nearest of its colours), as the UV fix of §28.5 does, so no border appears where the file had none | – |
| booleans (`shape_bool`) | interpolated barycentrically in each output triangle's source triangle, so a cut face keeps its colours; the faces of an operand without colours are white and `BoolReport::faces_without_colors` counts them (its `Debug` shows the field only when it is not 0, so no report of a solid without colours reads differently) | – |
| `apply_transform`, `join` | carried; a mirror turns them with the triangle; a part without colours joined to one with them is white | the CLI writes the parts' common material, or reports why not |
| `decimate`, `lods`, `collapse_short_edges` | a corner's UV and colour are matched together through the edge's triangles (§17.2's matching, on both); a hard colour border is a feature line as a UV seam is; a smooth colour costs nothing extra (a penalty was tried and dropped, §29.6) | kept (the UVs are) |
| `unwrap` | kept | every map dropped, each named in `material.dropped` |
| `glb`, `glb_plain` | `COLOR_0` as float RGBA, one vertex per (position, surface, UV, colour) | every factor and map |
| `obj`, `stl` | not written (`not_written`) | not written |
| `measure` | `colors.borders`: edges where a corner's colour differs between the two triangles | with `Measure::materials`: each material's maps and sizes, memory per kind |

Tests (`crates/shape-tools/tests/colors.rs`): an affine colour on a
sphere comes out of decimation to half within 10⁻¹² of the colour of
each corner's position; two hemispheres keep one colour per triangle
and their border on the input's border vertices (it fails with the
feature line taken out); a soup with a hole repaired keeps every rim
corner's colour; a coloured block cut by a pin without colours keeps
the affine colour on its own faces to 10⁻¹² and counts the pin's faces;
a material with every field and map, and colours, through `glb` and
`glb_plain` and back compares equal; glTF's defaults are listed; an
FBX material's colours; the JPEG fixture against libjpeg.

### 29.4 Normal maps under repair and decimation (SPAR3D)

A normal map is in tangent space. None of the 54 files carries
`TANGENT`, so a viewer derives the frame itself: three.js, from the
screen-space derivatives of position and UV per pixel (per triangle, in
effect: dP/du and dP/dv), around the interpolated vertex normal; glTF
asks for MikkTSpace, which also starts from the UVs and normals. The
chain changes both inputs: decimation moves vertices and UVs, and every
output's normals are recomputed (area-weighted, smooth within a surface)
where the file had its own. Mirrored UV charts (30–80 % of triangles,
§28.2) flip the frame's handedness; decimation keeps each UV triangle's
winding (`orient2d`), so it should not flip one.

Measured, not assumed: `bench/generated/src/bin/normals.rs` takes each
output triangle's centre, finds the nearest point of the input surface,
and compares the shading normal a viewer computes there on each mesh
(per-triangle frame from dP/du, dP/dv made orthogonal to the
interpolated normal; the map's texel at each mesh's own UV; glTF's +y
up the texture). Also the angle between the interpolated vertex normals
alone, and the share of points whose UV winding differs. SPAR3D's six
files (normal map 1024², the guide's repair, keeping UVs):

| file (triangles) | repaired: shading median / p95 (°) | keep 50k | keep 10k | keep 2k | handedness differs (10k; 2k) |
|---|---|---|---|---|---|
| 5 (42k) | 0.92 / 7.2 | 0.92 / 7.2 | 2.75 / 15.8 | 8.1 / 35.5 | 0.01 %; 0.70 % |
| 30 (42k) | 1.35 / 10.4 | 1.35 / 10.4 | 5.26 / 22.1 | 6.1 / 30.7 (7.4k) | 0.28 %; 1.04 % |
| 120 (70k) | 1.44 / 16.8 | 2.46 / 19.3 | 6.09 / 31.6 (21k) | 6.09 / 31.6 (21k) | 0.22 %; 0.22 % |
| 300 (35k) | 1.16 / 6.7 | 1.16 / 6.7 | 3.28 / 13.8 | 10.9 / 36.1 | 0.03 %; 1.95 % |
| 420 (25k) | 1.61 / 10.9 | 1.61 / 10.9 | 3.58 / 16.9 | 12.2 / 45.7 | 0 %; 3.05 % |
| 480 (25k) | 1.05 / 9.5 | 1.05 / 9.5 | 2.58 / 14.7 | 9.7 / 42.1 | 0 %; 1.65 % |

(In parentheses: where the budget is not reached, §28.3's UV-island
floor.) The input against itself gives 0 everywhere, so the measure
adds nothing of its own.

- **Repair alone** moves the shading normal a median 0.9–1.6°, p95
  7–17°, with no vertex moved (the median distance to the input is 0):
  it is the recomputed vertex normals. The median angle between the two
  vertex normals alone is within 0.11° of the median shading angle on
  every file (0.93 against 0.92° on file 5, 1.42 against 1.35° on 30),
  so the tangent frame adds almost nothing; the normals do. Writing the
  file's normals back would remove it, and needs normals on the solid
  (not done here: §29.8).
- **Decimation** to 10k: median 2.6–6.1°, p95 14–32°; to 2k, 6–12° and
  31–46°. The maxima (84–170° on every row) were not traced to their
  places; a nearest point on another part of the model (a thin part,
  or another chart) gives such angles.
- **Handedness**: none differs after repair; 0–0.28 % of points at 10k
  and 0.2–3 % at 2k. Decimation refuses a collapse that turns a UV
  triangle over, so these are points whose nearest input point lies in
  a chart of the other winding; the measure does not separate the
  cases further.
- What it means for an agent: a normal-mapped mesh keeps its relief at
  10k to within a few degrees; below that, or for a hard-surface map,
  bake again (§28.7, no baking here).

### 29.5 The decoder against a reference

`bench/generated/src/bin/decode.rs` decodes every texture in the 54
files (61 images: 18 PNG, 43 baseline JPEG, 1024² to 4096²) and writes
the texels; PIL 12.3 (libjpeg) decodes the same files:

- PNG: all 18 identical, texel for texel.
- JPEG: every texel within 4 levels of libjpeg's (median of the per-image
  maxima 3), mean difference 0.06 levels (worst image 0.14). The
  difference is rounding in the inverse DCT and the chroma filter
  (libjpeg alternates its rounding bias; this one does not). Before the
  triangle filter, with chroma upsampled by nearest sample, it was up to
  76 levels at colour edges (mean 0.9).
- Time (x86-64, release): median 33 ms at 1024², 115 ms at 2048², 518
  ms at 4096².

### 29.6 The bench, before and after

`bench/generated/look.ts` runs `shape process FILE --budget 10000`
(the guide's repair, keeping UVs) and reads the output GLB's material,
maps and `COLOR_0` against the input's (glTF's defaults applied, or the
MTL's); `look-table.ts` prints the table. Before: the binary at 64af70c.
Budgeted: this one with `--texture-max 1024`. x86-64, release, one run
each.

| generator | files | factors kept (before → after) | maps kept | vertex colours kept | maps in (median MiB with mips) | out | out, `--texture-max 1024` | time (median s; before → after → budgeted) |
|---|---|---|---|---|---|---|---|---|
| Hunyuan3D-2 | 6 | 0 → 6 | 6 → 6 of 6 | – | 5.3 | 5.3 | 5.3 | 6.0 → 4.7 → 4.9 |
| TRELLIS | 6 | 0 → 6 | 6 → 6 of 6 | – | 5.3 | 5.3 | 5.3 | 1.1 → 0.9 → 0.9 |
| SPAR3D | 6 | 0 → 6 | 0 → 6 of 6 | – | 10.7 | 10.7 | 10.7 | 0.9 → 0.7 → 0.8 |
| TripoSR | 6 | 6 → 6 | – | 0 → 6 of 6 | 0 | 0 | 0 | 3.7 → 3.5 → 4.4 |
| InstantMesh | 6 | 6 → 6 | – | 0 → 6 of 6 | 0 | 0 | 0 | 3.6 → 3.4 → 5.3 |
| CRM | 6 | 6 → 6 | 6 → 6 of 6 | – | 5.3 | 5.3 | 5.3 | 1.1 → 1.1 → 1.8 |
| Unique3D | 2 | 0 → 2 | – | 0 → 2 of 2 | 0 | 0 | 0 | 92 → 115 → 96 |
| Meshy | 16 | 0 → 16 | 1 → 16 of 16 | – | 42.7 | 42.7 | 10.7 | 0.7 → 0.7 → 2.2 |
| all | 54 | 18 → 54 | 19 → 40 of 40 | 0 → 14 of 14 | 5.3 | 5.3 | 5.3 | 1.1 → 1.1 → 2.2 |

- **Factors**: all 54 outputs now have the input's base colour,
  metallic, roughness, emissive, alpha mode and double-sidedness. Before,
  only the 18 whose input material is the default (TripoSR, InstantMesh,
  CRM's `Kd 1 1 1`) matched, by accident: Hunyuan3D lost its unset
  (metal) `metallicFactor` and its roughness, TRELLIS and Meshy their
  metallic 1, SPAR3D its own values, Unique3D and Meshy's robot their
  `doubleSided`.
- **Maps**: 40 of 40 textured files keep every map (19 before, the ones
  with a base colour only). **Colours**: 14 of 14 (none before).
- **Texture budget** at 1024 (per kind, over the 54 files):

  | kind | maps | resized | MiB with mips in → out (total) | file MiB in → out (total) | not decoded |
  |---|---|---|---|---|---|
  | base colour | 40 | 16 | 661 → 213 | 30.4 → 46.9 | 0 |
  | metallic-roughness | 15 | 15 | 512 → 80 | 5.2 → 7.1 | 0 |
  | normal | 6 | 0 | 32 → 32 | 0.8 → 0.8 | 0 |

  Files over 16 MiB of texture with mips: 16 → 0 (the largest 171 →
  10.7 MiB, Meshy's 4096² pair). The PNGs are larger on disk than the
  JPEGs they replace (+54 % for base colour): the budget is GPU memory,
  and PNG is the only format written. It costs Meshy's files 1.7–3.9 s
  (decode, filter, deflate at level 9), median 2.2 s against 0.7.
- **Time** otherwise unchanged at the median. The before column ran
  beside other work for its first half; the Unique3D chair's 92 → 115 s
  is the fuse step carrying colours on 618k triangles.

`run.ts` again with this binary (all 54, three budgets, keep and fresh):
every row of §28.3's tables is the same except where the Unique3D chair
enters it. Six files without vertex colours, and TripoSR's and
InstantMesh's, checked one by one: the same decimation report and the
same OBJ bytes as before. The chair (619k triangles, crossing itself in
3 688 pairs, §28.2) now stops at 10 294 triangles for a budget of 2 000
or 10 000, keep and fresh alike (was within both), and its reported
error is 215 mm (2.0 mm before). Its fused surface has 8 699 colour
borders: the self-union joins sheets of different colour where they
cross, each such edge is a feature line, and decimation keeps them as it
keeps UV seams (§28.3's island floor). The bear (one of the 2) has 237
and still reaches every budget. Rows that moved: within budget at keep
2k 41 → 40 and keep 10k 53 → 52, fresh 2k and 10k 54 → 53; Unique3D 2 / 2 → 1 / 2 at 2k, 10k
and fresh 2k; the medians of error at 2k and 10k (17.8 → 18.4, 3.2 →
3.5 mm), the maxima of fresh (1.16 → 10.5 %); the times, taken with
other work running, are not compared.

Two things measured and changed on the way:

- **No colour-smear penalty.** A first version penalised removing a
  vertex whose colour differs from its ring, as a bone border is. On
  three vertex-coloured files at 10k (`bench/generated/src/bin/colors.rs`:
  each output corner's colour against the input's at the nearest
  point) it raised the surface error (InstantMesh 30: 3.47 → 4.46 mm;
  300: 4.01 → 4.84; TripoSR 30: 1.44 → 1.68) and the colour error too
  (p95 0.015 → 0.021, 0.042 → 0.047, 0.010 → 0.014 in 0..=1), since a
  surface moved further samples its colour further away. Dropped.
  Without it these files decimate to the same bytes as before colours
  existed, with colour p95 0.010–0.042 and p99 0.025–0.07.
- **Rounding at cut vertices.** The fuse step interpolates a cut-line
  vertex once per source triangle around it; the roundings differ, and
  each difference was a colour border (20 on two overlapping boxes of
  one affine colour). Corners
  of one cut vertex closer than half an 8-bit step (1/512) now take
  their mean; the test fails without it.

### 29.7 Golden lines

- **Moved**: `mcp.process.ball_defects`, for new report fields only:
  `import.parts[].colors` and `look`, `material`, `textures`,
  `measures.texture.materials` and `by_kind`. Its GLB is the same bytes
  (checked against the 64af70c binary on the same input).
- **Kept**: `rest.program.booleans` hashes `{:?}` of the boolean
  reports; `BoolReport`'s `Debug` leaves `faces_without_colors` out when
  it is 0, so it did not move. No other line moved.
- **New**: `meshtools.look.import` (a coloured, textured sphere with a
  full material through `glb` and back; a hand-written glTF with unorm8
  RGBA and float RGB colours, two primitives, unset factors, emissive
  strength; an OBJ with colours and an MTL with maps), `meshtools.look.chain`
  (a coloured soup with a hole repaired, decimated, written with its
  material, measured; a coloured block cut by a pin and its report),
  `meshtools.look.texture` (the JPEG fixture decoded; it and a PNG
  shrunk as colour and data), `mcp.process.look` (`process` of a GLB
  with a material and colours, maps shrunk to 2 texels; then `--unwrap`
  to OBJ, with every map dropped and the not-written list).
- Run natively on x86-64 here. The wasm32 run and `cargo deny` could not
  run in this container (no wasmtime or cargo-deny installed, and
  installing them was not allowed); the gate's CI runs them. No crate
  was added to the kernel (`serde_json` only in the bench's own
  workspace).

### 29.8 Gaps, and for the orchestrator

- **Decide:** the Unique3D chair (1 of 54) no longer reaches 2k or 10k
  because its fused colour borders are kept (§29.6). Options: keep
  borders (as now; the colour stays sharp where the sheets cross), add
  a `Decimate` switch to cross colour borders (as `cross_bone_borders`),
  or treat a border as a feature only above some colour step. The
  number to beat is the chair's 2.0 mm at 10k without colours.
- **Normals of imported meshes** are recomputed: the main cause of a
  normal map's shading change after repair (§29.4). Carrying the file's
  normals needs a per-corner normal on the solid, like UVs, and a rule
  for decimation.
- Not read: FBX vertex colours (listed), a second UV set, MTL's separate
  `map_Pr`/`map_Pm` (glTF needs them combined), samplers other than
  repeat (listed; written as repeat), `KHR_texture_transform` (listed,
  not applied). Progressive JPEG is not decoded (none in the bench).
- The texture budget writes PNG only; KTX2/Basis would cut file size and
  GPU memory further and is a larger piece of work (an encoder).
- One material per plain GLB: parts with different materials get the
  default one (reported in `material.dropped`); `glb` of an imported
  file keeps each part's.
- Other crates that rebuild solids (`shape-edit`'s `dent` and `split`,
  `shape-skin`'s refinement, `shape-model`'s placement) are not audited
  for colours, as §17.6 says for UVs; nothing coloured reaches them
  today.
- Edits outside this stream's files, each the least that compiles or
  reports: `crates/shape-bool/src/{named,sliver}.rs` (colours through
  booleans, `faces_without_colors`), one `..Default::default()` in
  `crates/shape-model/src/program.rs`, `ImportedPart::map` →
  `material.map` in `crates/shape-model/tests/uvs.rs`, the golden
  crate's cases, a short note in `docs/design.md` §4, `guides/mcp.md`'s
  `process` row, and `crates/shape-mcp/tests/long_calls.rs` (the step
  count is `STEPS.len()`).

## 30. The preview draws textures and vertex colours (stream AI)

Base 64af70c. Closes the second row of §28.7: every one of the 54
generated meshes rendered grey in `shape preview` (40 textured, 14 with
vertex colours only), and a program's `map` (§21) showed no texture
either. The preview is how an agent without a GPU checks a look.

### 30.1 Decisions

- **Decoding.** Nothing decoded an image before: `measure` reads sizes
  from headers only (`image.rs`), so "the decoding it shares with
  measure" did not exist. A new module, `shape-tools/src/decode.rs`,
  decodes for the preview only; `image.rs` (stream AH's) is untouched.
  - PNG: written here on `miniz_oxide` (already a dependency): every
    colour type and depth, Adam7, palettes with `tRNS`; 16-bit samples
    round to 8 bits. Colour-space chunks are ignored: glTF says a
    base-colour image is sRGB.
  - JPEG: `jpeg-decoder` 0.3.2 (MIT OR Apache-2.0, no dependencies),
    built with `default-features = false` (no rayon) and
    `platform_independent`, which removes its SSSE3, NEON and wasm-SIMD
    paths and forbids its `unsafe`. What remains is the stb_image integer
    IDCT, integer upsampling and integer YCbCr conversion; the few f32
    operations left are on small integers (`row / 2.0`, its fraction × 3,
    −0.25) and are exact. Every one of the 40 textured files is PNG
    (Hunyuan3D, TRELLIS, CRM) or baseline 4:2:0 JPEG (Meshy, SPAR3D); the
    golden case `tools.preview.textured` decodes a baseline 4:2:0 JPEG
    (`golden/programs/label.jpg`, 1.6 KB, made for it), so the gate proves
    the bytes on x86-64, aarch64 and wasm32.
  - WebP and KTX2 are refused with the reason; so is an image over 2^26
    pixels (8192²). The part is then drawn in its plain colour and the
    reason goes to `Views::undrawn` (the CLI's and MCP's `undrawn`).
- **Colour.** The old rule is kept bit for bit for a part with neither a
  drawable map nor vertex colours: its sRGB `color` × the Lambert light.
  A part with a map or `Solid::colors` is coloured per pixel: texel ×
  `color` × vertex colour, all in linear light, then back to sRGB, then
  the same light. The light is applied in sRGB for both, so a white map
  and white vertex colours give exactly the untextured pixel (a unit
  test compares the bytes): textured and plain parts of one scene shade
  alike. Shading in linear light for every part would move every
  existing preview line; it is not done.
- **Sampling** (`sampler.rs`).
  - sRGB → linear by a 256-entry table (the sRGB curve from `trig::pow`,
    × 65535, rounded); linear → sRGB byte by an exact search of that
    table (mip levels) or `trig::pow` (the pixel).
  - Mip chain: level k + 1 is level k halved, sizes rounded down and at
    least 1, each texel the mean of a 2 × 2 block in linear light
    (alpha as is), rounded half up, stored as sRGB bytes; an odd last
    row or column is dropped. To 1 × 1. 4096² takes 85 MiB with its
    chain (the bytes `measure` already reports).
  - Level: ρ is the footprint in level-0 texels, the longer of the UV
    steps one pixel right and one pixel down (each scaled by the image's
    width and height), computed per pixel from the barycentrics one
    pixel over. The level is the nearest to log₂ ρ: the largest k with
    4^k ≤ 2ρ² (no logarithm). Bilinear within that level, weights
    `floor(256 × fraction)`, integer sums; repeat wrap; v = 0 at the
    image's bottom row. No trilinear blend: the level steps are visible
    as a slight change of sharpness on a receding floor, not as noise.
    A 256² one-texel checker on 40 pixels comes out as its linear mean,
    sRGB 188, at every pixel (unit test).
  - `mapSize` is already in the UVs a program's scene carries (export
    divides them, §21). Wrap is repeat: `Material` has no wrap field,
    export writes one repeat sampler, and the importer does not carry
    samplers (Meshy's robot asks for mirrored repeat; it shows repeat).
- **Alpha.** As export writes it: a part with `opacity` 1 is opaque
  (glTF `OPAQUE`) whatever its map's alpha; a part below 1 blends with
  alpha = `opacity` × map alpha × vertex-colour alpha, in the existing
  back-to-front order (§20). Imported files' `alphaMode` is not carried
  (stream AH's), so imported maps draw opaque.
- **Vertex colours.** Drawn from `Solid::colors` (linear RGBA per
  triangle corner), interpolated perspective-correctly and multiplied
  in. Stream AH adds that field at the same time; it was not in the
  base, so this branch adds it with the same name, type and meaning and
  nothing else: no reader, nothing carries it, and `canonicalize`
  leaves it as it is (AH's). The importer still skips colours here.
- **Material factors (item 3): not drawn.** `metalness`, `roughness` and
  `emissive` leave the picture as it was. The tools scene's wheel
  (`tools.preview`, `tools.preview.png`) has `metalness` 1, `roughness`
  0.25 and `emissive` [0.1, 0, 0]: any term for them moves those two
  lines, which this stream must keep. A bounded specular term from the
  one light would also not tell a metal from a plastic in a 256-pixel
  turntable without an environment; emissive is the one with a plain
  meaning (add its colour). Left for the orchestrator (§30.5).
- **Perspective** (`Preview::fov`). The hand-off needs a field of view;
  the orthographic view stays the default and keeps its bytes. With a
  vertical field of view in (0, π/3], the eye stands where the bounding
  sphere's silhouette fills 92 % of the smaller side, as the
  orthographic scale does: tan of the half-angle t = 0.92 tan(fov/2) ×
  min(w, h)/h, distance = r √(1 + t²) / t ≥ 2.13 r. The ground square's
  corners are 1.63 r from the centre, so nothing is behind the eye and no
  clipping is needed, except the shadow: a triangle pressed onto the
  ground along the light can land past the ground square, so in
  perspective it is clipped to it first (Sutherland–Hodgman, f64 `+ − ×
  /`). Depth, light, UVs and colours are interpolated with the
  barycentrics weighted by 1/depth; a unit test puts a map's v = ½
  boundary on the row the quad's 3D midpoint projects to (an affine
  interpolation puts it 2 rows off; the test fails then). Edges
  interpolate 1/depth.
- **Unlit** (`Preview::unlit`): light 1 for every pixel. With
  `no_edges` and `no_ground` it is a flat colour pass.
- **Cameras** (`views` → `Views { images, cameras, undrawn }`;
  `preview` is `views(..).images`). Per view: `position`, `target` (the
  box's centre), `up`, `fov` (or `None`), `ortho_height` (the scene
  units the image's height spans; 0 in perspective), `width`, `height`.
  An orthographic view's position is 3 radii out on its axis (any point
  on that line sees the same). The CLI and MCP give it in the scene's
  frame (kernel units, +Z up) and in the GLB's (`gltf`: metres, +Y up,
  kernel (x, z, −y) / 1000), the frame a texturing step renders the GLB
  in.
- **The UV view** (`uv_view`, `uvview.rs`). One image per material
  (parts with equal `Material`s together; parts without UVs left out),
  square, `size` pixels (default 512). It covers the whole UV tiles the
  UVs touch: from `uv_min` = (⌊min u⌋, ⌊min v⌋), `tiles` units each way.
  Pixel (x, y), rows top down, is u = uv_min.u + tiles (x + ½)/size,
  v = uv_min.v + tiles (size − y − ½)/size. Under the charts: the map
  sampled as the preview samples it, or a checker of 8 × 8 squares a
  tile; texels no triangle covers dimmed to 40 %. Lines: chart outlines
  only (an edge no other triangle of the part shares with the same two
  UVs: seams and open edges), dark, one pixel. Edges inside charts were
  drawn too at first, lightened; on Meshy's car (37 000 triangles at
  400²) they whitened the whole map, so they are gone. A tiling map's
  charts span many tiles (`uv_plate`'s: 21, from −2), all at one scale.
- **The hand-off.** `shape preview --each` writes one PNG a view
  (`out.0.png`, `out.1.png`, …) and reports each one's camera;
  `--unlit --no-edges --no-ground` makes the colour-only pass; `--uv`
  the layout. MCP `preview` takes `fov`, `unlit`, `each`, `uv`,
  `uv_size` and returns one image content per PNG; its report keeps the
  first image's `width`, `height`, `bytes`, `output` at the top, as
  before, and lists all under `images` with what each `shows`. The
  binding's `preview` takes `fov` and `unlit`. No image model is called.
- **The mesh preview carries the map.** `shape preview` of a mesh file
  gave its parts default materials; each part now keeps its imported
  `map` (and nothing else: factors are AH's).

### 30.2 Measurements (x86-64, release, one thread)

`shape preview FILE --views 3 --width 200`, all 54 files of §28, wall
time per file (decode, mip chain and three views):

| files | before (median, max s) | after (median, max s) |
|---|---|---|
| all 54 | 0.13, 2.27 | 0.29, 2.21 |
| the 40 textured | 0.12, 0.47 | 0.27, 0.71 |

The added time is mostly decoding and the mip chain: 0.03–0.18 s for a
1024² map (Hunyuan3D, TRELLIS, CRM, SPAR3D), 0.5–0.6 s for Meshy's
4096² JPEGs (the burger, the palm tree). No file failed and no map went undrawn. Memory: a 4096² map
with its chain is 85 MiB on top of the mesh.

### 30.3 Seen

`docs/previews/generated/<name>-colour.png`, 13 files, 600 × 400: top
row the old preview, bottom row this one, three views 200². The bear,
the barn, the police car (its "POLICE" lettering), the palm tree, the
cat, the robot, TRELLIS's bear, Hunyuan3D's house, SPAR3D's cabinet
(wood inside a grey shell), CRM's mugs (OBJ with MTL and PNG). The
vertex-coloured three (TripoSR's horse, InstantMesh's horse,
Unique3D's chair) are drawn by `Solid::colors`, filled by a scratch
reader that is not committed (OBJ `v x y z r g b` taken as sRGB, as
three.js's OBJLoader does; glTF `COLOR_0` linear), standing in for
AH's: every corner matched a coloured vertex by exact position. The
horses show their dapple and dark mane; Unique3D's chair is grey in its
colours too. `meshy-car_police-handoff.png`: `--each --views 4 --fov 0.7
--unlit --no-edges --no-ground`, the four views a texturing step gets.
`docs/previews/uv/plate_preview.png`: `golden/programs/uv_plate.json`
before and after, with its checker, and its UV view.

### 30.4 Golden

No line moved: the untextured orthographic path computes what it did,
in the same order (`model.washing_machine.png`, `tools.preview`,
`tools.preview.png`, `tools.preview.facets`, `model.joints.preview`
unchanged). No existing case has a drawable map or vertex colours in
its preview. New:
- `tools.preview.textured`: a can with the JPEG label tinted by its
  colour and a slab with the PNG checker at opacity 0.6, three
  perspective views and the cameras, then one orthographic unlit view;
- `tools.preview.colors`: a ball with vertex colours and alpha, four
  views;
- `tools.preview.uv`: the UV views of that scene (map, map, checker);
- `uvs.program.preview`: `uv_plate.json`'s preview with its checker and
  its UV view.

### 30.5 For the orchestrator

- **Emissive and a specular term** would move `tools.preview` and
  `tools.preview.png` (the wheel's `emissive` [0.1, 0, 0], `metalness`
  1); not done. Decide whether those lines may move for emissive alone.
- **`Solid::colors`** is added here with AH's name and type. At the
  merge AH's field wins; this branch adds no reader and no carrying
  (`canonicalize`, which the CLI's mesh preview runs, does not permute
  it here). If AH's `canonicalize` does not carry colours, the CLI's
  mesh preview will drop or misalign them: check after both land.
- **The decoder** is new code in `shape-tools` (`decode.rs`) and a new
  dependency (`jpeg-decoder`). If AH also decodes images (to resize or
  bake), one decoder should remain.
- Not done: other maps (normal, metallic-roughness, occlusion), wrap
  modes other than repeat, `KHR_texture_transform`, trilinear or
  anisotropic filtering, texture alpha on opaque parts (glTF `MASK`).

## 31. Pivot, size and up axis (stream AJ, 2026-11-10)

Base 64af70c. §28.7's first gap: all 54 generated meshes have the
origin at the box's centre and a size the generator chose (0.75–2 m
whatever the object), and 12 need more than the default reading to
stand up. `place` puts a mesh where a game wants it; `measure --axes`
gives the numbers for choosing.

### 31.1 Decisions

- **One tool, three optional steps, in a fixed order:** turn, size,
  pivot (`shape_tools::place`, `Place`, zero value changes nothing;
  `shape place`, MCP `place`, and `process`'s place options).
- **The turn is one of the 24 axis-aligned turns**, given as the input
  axis that is up and the one that is front (`Axis`: `+x` … `-z`). Up
  goes to +Z and front to −Y, the kernel's front (the side `preview`
  draws first, `docs/design.md` §4); the third axis is `up × front`, so
  the turn never mirrors. It moves and negates coordinates and rounds
  nothing (`AxisTurn`; the default OBJ reading through it gives the same
  bits as before, signed zeros included, so no golden line moved). One
  axis alone takes the smallest turn that brings it home (`up +y` alone
  is glTF's `front +z`; `up +z` alone is the identity). A further `turn`
  about up in degrees: a multiple of 90 joins the exact turn, any other
  angle goes through `trig::sin_cos`.
- **Sizes are uniform**: the longest side, the height (Z), width (X) or
  depth (Y) of the turned box to a length, or a factor. There is no way
  to stretch. Lengths are millimetres in the library, as every
  kernel length is; the CLI and MCP take them with a unit (`0.9m`,
  `90cm`, `900mm`) and refuse a bare number, so a height of 900 cannot
  be read as 900 m.
- **Pivot**: `base` (the centre of the bottom face of the box after the
  turn and scale, so "down" is the new −Z), `center`, or a point of the
  input in the coordinates `measure` reports. The base's z is
  subtracted from itself, so the output's lowest z is exactly 0.
- **Skins** (§11.5's rule: bones stay rigid): with the placement
  `A = T · s · R`, bone rest frames become `A · rest · S⁻¹` (axes turned,
  origin moved and scaled) and inverse bind matrices `S · IBM · A⁻¹`, so
  `rest · IBM` is unchanged and every vertex, in a bone's coordinates,
  is `s` times where it was. A skinned part is never refused.
- **Normals**: a solid stores none (export makes them from the
  positions), so they follow. UVs, surfaces and face names are untouched.
- **Several parts are placed as one model**: one box over all, the same
  motion for every part; the GLB keeps parts, skins and textures.
- **Import's axes**: `Import::up` and `front` name the file's own axes
  (OBJ and glTF); `z_up` stays as `up +z, front -y` and is refused next
  to them; FBX says its axes in the file and refuses them. For glTF the
  turn is applied after reading, to each part's transform (a skinned
  part's mesh and bones, its transform conjugated). `shape place` and
  `process` read the file plainly and pass the same turn to `place`
  (`Import::as_turn`), so the report has a before and an after; the
  report's `up` and `front` are the plain reading's kernel axes.
- **`process` places last**, after repair and decimation: placed first,
  `ball_defects.obj`'s floaters, dropped by the repair, left the output's
  box 760 mm left and 236 mm right of the origin for a 1 m height.
  Repair and decimation lengths therefore stay the file's.
- **`measure --axes` and `place`'s report, no verdict**
  (`AxisMeasures`): extents, `pivot_from_base`, the longest and
  shortest axis, the fraction of area facing each way (each triangle
  counted for the axis its normal leans along most) and where the mass
  sits in the box per axis (centre of volume when every part is closed
  with a positive volume, of area otherwise). The fields are added only
  when asked, so `measure`'s and `process`'s reports are as they were.
- **No pivot option on a program's export.** A program places its own
  parts in millimetres, Z up, and is exported in metres: its origin is
  where its author put it. Moving it at export would also move every
  joint's frame and bone, a second way of saying what the program says,
  in `shape-model` and the binding (not this stream's). Recorded in
  `guides/meshtools.md`.

### 31.2 The generators' frames

Found by previewing the cabinet (120), the horse (420) and the armchair
(480) of each generator in each of the six up axes, then the fronts:
the side the prompt image shows is the front.

| generator | up | front | as §28.2 says |
|---|---|---|---|
| Hunyuan3D-2, TRELLIS, Unique3D, Meshy | +y | +z | glTF's own |
| SPAR3D, CRM | +y | −z | face away from +Z |
| TripoSR (OBJ) | +z | +x | Z up (`--z-up` stands them up, side-on) |
| InstantMesh (OBJ) | −z | +x | neither: Y up lays them down, `--z-up` turns them over |

The prompt images show the horse from its side, so the horses face
front side-on in every frame; a `turn` of ±90 points the head forward.
Meshy's are users' text-to-3D files, one frame for all, but a few face
sideways (the chair, the cat).

### 31.3 The 54 placed

`bun bench/generated/place.ts DIR OUT`: per file `shape place` with the
generator's frame, `--pivot base` and the object's real size from a
table in the script (by prompt for Hi3DBench: a house 6 m tall, four
mugs 0.1 m, a cabinet 1.2 m, a sitting bear 0.35 m, a galloping horse
2 m, an armchair 1.2 m; by name for Meshy, e.g. a sword 1 m long, a
school bus 11 m long), then `shape measure --axes` of the GLB written.
x86-64, release.

| generator | files | frame (up, front) | longest side before (m) | origin above base before (of the height as read) | origin from base after, read back from the GLB (mm) | size asked = size after (files; worst relative error) |
|---|---|---|---|---|---|---|
| Hunyuan3D-2 | 6 | +y, +z | 1.94–2.00 | 0.50–0.51 | 0 | 6; 4e-8 |
| TRELLIS | 6 | +y, +z | 0.92–1.00 | 0.50–0.51 | 0 | 6; 4e-8 |
| SPAR3D | 6 | +y, −z | 0.94–1.03 | 0.48–0.53 | 0 | 6; 4e-8 |
| TripoSR | 6 | +z, +x | 0.75–1.02 | 0.48–0.56 | 0 | 6; 4e-8 |
| InstantMesh | 6 | −z, +x | 1.56–1.97 | 0.46–0.53 | 0 | 6; 4e-8 |
| CRM | 6 | +y, −z | 1.31–1.91 | 0.48–0.63 | 0 | 6; 4e-8 |
| Unique3D | 2 | +y, +z | 1.30–1.32 | 0.50–0.51 | 0 | 2; 4e-8 |
| Meshy | 16 | +y, +z | 1.00–1.20 | 0.50 | 0 | 16; 4e-8 |

All 54 stand on z = 0 with the origin at the base's centre, exactly
(the GLB's f32 keeps 0 and the symmetric x and y), at the size asked to
f32's rounding (4e-8). Sizes asked by the longest side give the other
extents the generator's proportions: Meshy's police car is 4.8 m long
and 3.1 m tall, its bus 11 m and 5.5 m (both chunky, toy-like
shapes); a game would ask those by height. Time: median 87 ms a file
(import, place, write), 2.7 s for Unique3D's 16 MB GLBs.

### 31.4 Would the numbers stand them up?

For each file read plainly (Y up), three simple readings of
`measure --axes`, each naming an up, against the up the generator's
frame gives:

| reading | right (of 54) | Hunyuan3D-2 | TRELLIS | SPAR3D | TripoSR | InstantMesh | CRM | Unique3D | Meshy |
|---|---|---|---|---|---|---|---|---|---|
| the mass sits low (up away from the side the mass is nearest, on the axis it is most off-centre) | 22 | 2/6 | 3/6 | 2/6 | 2/6 | 3/6 | 2/6 | 0/2 | 8/16 |
| most area faces down | 9 | 0/6 | 1/6 | 0/6 | 1/6 | 1/6 | 0/6 | 0/2 | 6/16 |
| the flattest side faces down (most area facing one way over its opposite) | 21 | 1/6 | 3/6 | 2/6 | 1/6 | 1/6 | 2/6 | 1/2 | 10/16 |
| as read (Y up) | 42 | 6/6 | 6/6 | 6/6 | 0/6 | 0/6 | 6/6 | 2/2 | 16/16 |

The test cases, TripoSR's and InstantMesh's horses and chairs:

| file | true up (as read) | mass low | most area down | flattest side down | mass in box (x, y, z) |
|---|---|---|---|---|---|
| triposr-420 (horse) | −y | −z | +x | −y | 0.47, 0.47, 0.60 |
| triposr-480 (chair) | −y | +x | +x | +x | 0.45, 0.53, 0.47 |
| instantmesh-420 (horse) | +y | −z | +z | +z | 0.56, 0.55, 0.60 |
| instantmesh-480 (chair) | +y | +y | −x | +z | 0.45, 0.42, 0.49 |

None of the readings beats the format's own axes (42) or the
generator's frame (54); on the four test cases each is right once at
most. The mass of these objects sits within 0.10 of the box's middle
on every axis, and a horse's or a chair's base is four small feet. So
the tool reports the numbers and does not choose: the agent knows the
generator, or previews and looks (§31.2 took 6 previews per
generator).

### 31.5 Seen

`docs/previews/generated/place-*.png`, 12 files, 600 × 400: top row the
file read plainly, bottom row placed, beside a post one decade of
metres tall (0.1, 1 or 10 m, the tallest not above the object), each
three views from the front round to the back
(`cargo run --release --bin place_previews` in `bench/generated`).
TripoSR's and InstantMesh's horses and chairs lie or hang upside down
before and stand after; SPAR3D's chair and CRM's cabinet turn from
showing their back to showing their front; Hunyuan3D's horse and
TRELLIS's cabinet are already upright and only gain a base and a size;
Meshy's sword, burger and police car, Unique3D's bear stand at their
sizes.

### 31.6 Golden

No existing line moved. New: `place.turn` (one axis, both, a refusal,
quarter turns, 30°), `place.pivot` (base, centre, a point after a
turn), `place.size` (each size, a factor, a zero length refused),
`place.skinned` (the skinned tube of §11.5 stood up, scaled to 1.7 m,
on its base: mesh, bones and inverse bind matrices), `place.import.axes`
(an OBJ in InstantMesh's frame, Z up by words and by switch, both
refused together, a GLB read Z up) and `place.command`, `place.process`
(the CLI's report and GLB on `golden/meshes/ball_defects.obj`).

### 31.7 Not done

- Choosing up for the author: §31.4 says why not.
- A pivot option on a program's export (§31.1).
- Front per object: the frames are per generator; an object the prompt
  shows from its side (the horses) needs a `turn` the agent picks.

## 32. Decimation that does not fold, and non-manifold repair (stream AK, 2026-11-10)

Base 64af70c. §28.7's gaps of the mesh chain, worked in this order: folding
in decimation, non-manifold repair, chart merging, fills across seams, the
fuse step's speed and pockets. Every number below is from
`bench/generated` (§28.1's 54 files, not committed), re-run before and
after on the same x86-64 host, release, one thread:
`bun bench/generated/compare.ts BEFORE/results.jsonl AFTER/results.jsonl`.
The after-run uses the guide's repair plus `--non-manifold`.

### 32.1 No folding

A collapse is refused when one of the triangles it rewrites would cross
another triangle (`shape_bool::selfcheck::crossing`, exact: no shared
corner and touching, or a shared corner or edge and overlapping beyond
it), or would be flat. Candidates come from a spatial hash over the live
triangles' boxes (cells twice a mean triangle across, rebuilt with larger
cells each time the count halves; one query per collapse, for the box of
the whole fan), so a collapse costs a few dozen exact tests, not a pass
over the mesh. On an input that already crosses itself, a collapse whose
fan crosses something is taken only when the fan crossed the same
partners before and no more pairs: the count never rises. Edges refused
for folding are tried again when the queue runs dry, until a round takes
none; elsewhere the order is as before, so an output where nothing would
have folded keeps its bytes (no golden line moved for it).

When the target is not reached, `DecimateReport::blocked` counts each
edge left under the first rule that refuses its cheapest collapse:
`fixed` (no end may move: corners, borders and seams it does not run
along), `error`, `link`, `turn`, `uv`, `fold`, and `not_retried`
(collapsible now, but the queue re-plans only edges at the vertex a
collapse kept; it was left so as not to change outputs). `folds_refused`
counts refused attempts.

Cost (`shape decimate --target 10000` on a repaired file, import and
measure included): Hunyuan3D's house (150k) 1.2 → 3.5 s; Unique3D's
bear (819k) 9.0 → 19.0 s; Meshy's barn (37k) 0.25 → 0.57 s. Per
`process` run, median 1.0 → 1.7 s; peak memory median 88 → 109 MiB,
largest 1 372 → 1 868 MiB (Unique3D).

### 32.2 Non-manifold edges (`Repair::non_manifold`)

TRELLIS's edges with more than two faces are almost all four-face edges
where two sheets meet (a marching-cubes pinch), plus a few three-face
fins. `split_non_manifold` pairs the faces of each such edge into
sheets: faces the surface already joins round an end of the edge through
manifold edges pair up (two sheets meeting at a dimple pair across the
empty wedge, not the solid one; pairing by the solid wedge first glued
those fans into one and left 21 of the house's 41 edges); the rest pair by
their exact order round the edge (`orient3d`), a face running b→a with
the next face running a→b, which bound one wedge of solid. At each end,
the faces round the vertex joined by manifold edges and these pairs fall
into fans; every fan but the first gets its own copy of the vertex.
Equal positions are welded back by `canonicalize` and by any file
round-trip, so each copy moves 1 % of its shortest edge towards the mean
of its fan's other corners, which stays inside the cone its own faces
span. (Moving against the fan's mean normal sent a dimple's copy across
the edge into the other sheet: 16 → 383 crossing pairs on the house.)
A fin's lone face, moved, can pass through the sheets round it:
`crossings_added` counts those pairs, and the fuse step untangles them
on a shell that ends closed. Pieces that come apart and are floaters are
dropped (`sheets_dropped`).

Holes over the fill limit (up to 512 edges) are filled when their
least-area fill turns no triangle against the fill's mean normal and
crosses nothing (exact). The cabinet's three holes (65, 74 and 97
edges, up to 26 cm on a 1 m cabinet) are neither: they stay, reported.

TRELLIS's four: the bear and the armchair end closed (19 and 10 edges
split; the bear's 10 holes then fill); the house keeps 3 of its 41 edges
(chains where neither pairing separates the fans at the inner vertices)
and the cabinet its 3 large holes, so both stay open, and the report says
so (`non_manifold.left`, `holes_left`).

### 32.3 Chart merging (`Decimate::merge_charts`, off by default)

Islands (triangles joined across edges whose two sides give both ends
the same UVs) join into one chart when every seam edge between two of
them gives one rigid move of the UV plane (the edge's two lengths agree
within the tolerance, and so do the moves) and the move puts the two
triangles at each such edge on opposite sides (no mirror). A seam inside
a chart is then not a feature line; a corner across it takes its new UV
through the two islands' moves. No texture is rewritten.

At 1/4096 (a quarter texel at 1024²), `--merge-charts` on the 12
Hunyuan3D and SPAR3D files, keep:

| file | islands → charts | 2k: triangles without → with |
|---|---|---|
| hunyuan-5 | 2 278 → 2 065 | 7 266 → 6 708 |
| hunyuan-120 | 1 049 → 991 | 3 526 → 3 364 |
| hunyuan-300 | 2 604 → 2 350 | 8 724 → 7 964 |
| hunyuan-420 | 2 224 → 2 067 | 6 352 → 5 894 |
| hunyuan-480 | 2 848 → 2 784 | 6 520 → 6 290 |
| spar3d-30 | 5 395 → 2 906 | 7 394 → 6 484 |
| spar3d-120 | 17 505 → 3 804 | 21 380 → 16 132 (10k too) |

The other five already met 2k. The floor drops 3–25 %; no file reaches
its budget. Hunyuan3D's neighbouring islands rarely agree rigidly (216
of 21 940 seam edges on the house), and on SPAR3D's cabinet half the
seams merge but the ends of those that remain are corners. At 1/1024 the
house goes to 6 330, the cabinet to 14 722. The surface error at 2k
rises (the house 84 → 115 mm) and UV overlap a little (0.05 → 0.16 ‰ of
texels). Files that would still need atlas repacking or baking to keep
their texture at the budget: 14 at 2k (5 Hunyuan3D, 4 TRELLIS (one,
the armchair, 2 004 for 2 000, held by folds), 2 SPAR3D, 2 CRM, Meshy's
robot), 1 at 10k (SPAR3D's cabinet).

### 32.4 Fills across seams

A fill whose rim crosses a UV seam takes one UV at every corner, its
first rim vertex's: it covers no area of the texture (one colour, the
rim's), where a fan across charts spanned the atlas. UV overlap after
keep 10k: Meshy's robot 51 % → 0.006 %, TRELLIS's cabinet 55 % → 0.002 %
of texels: their level without the fills. The alternative, a small chart
of its own in unused space, needs atlas packing.

### 32.5 The fuse step

Unique3D's chair (619k triangles in 3 shells, 4 102 crossing pairs in the
largest): untangling the largest shell takes 5 s, but its self-union is
48 shells, and `shape_bool::fuse` joined each to the 590k-triangle whole
with its own boolean: that was the 133 s. Now the repair untangles each
shell, counts the shells an untangling leaves besides its largest as
`pockets` (all positive slivers outside the main shell, mostly under
1 mm³, three of 2 752 to 8 090 triangles), drops those that are floaters
by the repair's `floaters`, joins by booleans only the shells that meet
another (exact) or whose box lies in another's, and joins a group's small
shells among themselves before one boolean with its largest. The chair's
fuse step: 133 → 24 s (untangling 7 s, one boolean 12.5 s). Nothing in
`shape-bool` changed: the winding tree was not the cost.

### 32.6 The bench, before and after

#### The chain, before and after

| mode | budget | within budget | closed | within budget and closed | output crosses itself (files; pairs, total) | time (median, max s) | peak memory (median, max MiB) |
|---|---|---|---|---|---|---|---|
| keep | 2.0k | 41 → 40 | 50 → 52 | 40 → 40 | 31; 2300 → 2; 10 | 1.0, 97.0 → 1.7, 52.9 | 88, 1372 → 109, 1867 |
| keep | 10k | 53 → 53 | 50 → 52 | 49 → 51 | 21; 2818 → 2; 28 | 1.0, 94.9 → 1.7, 56.2 | 88, 1372 → 109, 1868 |
| keep | 50k | 54 → 54 | 50 → 52 | 50 → 52 | 10; 1060 → 2; 72 | 0.9, 92.1 → 1.1, 52.7 | 88, 1372 → 109, 1867 |
| fresh | 2.0k | 54 → 54 | 50 → 52 | 50 → 52 | 20; 1532 → 2; 13 | 0.9, 92.8 → 1.7, 53.2 | 88, 1372 → 109, 1868 |
| fresh | 10k | 54 → 54 | 50 → 52 | 50 → 52 | 13; 1063 → 2; 22 | 1.0, 93.2 → 1.6, 53.6 | 88, 1372 → 109, 1868 |
| fresh | 50k | 54 → 54 | 50 → 52 | 50 → 52 | 8; 1053 → 2; 72 | 1.0, 96.3 → 1.4, 54.2 | 88, 1372 → 109, 1868 |

#### Crossing pairs, per file: the input, and the keep and fresh outputs at 10k (before → after)

| file | input | keep 10k | fresh 10k | keep 2k | collapses refused for folding (keep 10k) |
|---|---|---|---|---|---|
| hunyuan-5 | 2 | 93 → 0 | 4 → 0 | 125 → 0 | 70 |
| hunyuan-30 | 0 | 3 → 0 | 0 → 0 | 5 → 0 | 4 |
| hunyuan-120 | 2 | 55 → 0 | 5 → 0 | 35 → 0 | 73 |
| hunyuan-300 | 1 | 8 → 0 | 4 → 0 | 6 → 0 | 42 |
| hunyuan-420 | 0 | 11 → 0 | 6 → 0 | 6 → 0 | 36 |
| hunyuan-480 | 1 | 0 → 0 | 0 → 0 | 4 → 0 | 6 |
| trellis-5 | 16 | 1924 → 3 | 392 → 1 | 1162 → 3 | 949 |
| trellis-30 | 0 | 2 → 0 | 0 → 0 | 34 → 0 | 2 |
| trellis-120 | 11 | 48 → 25 | 24 → 21 | 139 → 7 | 48 |
| trellis-300 | 1 | 4 → 0 | 2 → 0 | 5 → 0 | 20 |
| trellis-420 | 0 | 0 → 0 | 0 → 0 | 3 → 0 | – |
| trellis-480 | 0 | 95 → 0 | 22 → 0 | 498 → 0 | 44 |
| spar3d-30 | 0 | 7 → 0 | 0 → 0 | 21 → 0 | 3 |
| spar3d-420 | 0 | 0 → 0 | 0 → 0 | 10 → 0 | 0 |
| spar3d-480 | 0 | 0 → 0 | 0 → 0 | 10 → 0 | 0 |
| triposr-5 | 0 | 7 → 0 | 7 → 0 | 65 → 0 | 23 |
| triposr-120 | 0 | 0 → 0 | 0 → 0 | 1 → 0 | 5 |
| triposr-300 | 1 | 1 → 0 | 1 → 0 | 0 → 0 | 5 |
| instantmesh-300 | 0 | 0 → 0 | 0 → 0 | 1 → 0 | 0 |
| crm-5 | 0 | 1 → 0 | 0 → 0 | 0 → 0 | 28 |
| crm-120 | 0 | 9 → 0 | 0 → 0 | 17 → 0 | 19 |
| crm-300 | 0 | 0 → 0 | 0 → 0 | 1 → 0 | 1 |
| crm-420 | 0 | 0 → 0 | 0 → 0 | 4 → 0 | 0 |
| crm-480 | 0 | 0 → 0 | 0 → 0 | 1 → 0 | 0 |
| unique3d-300 | 206 | 16 → 0 | 16 → 0 | 8 → 0 | 0 |
| unique3d-480 | 3688 | 526 → 0 | 526 → 0 | 122 → 0 | 781 |
| meshy-boat | 0 | 1 → 0 | 0 → 0 | 0 → 0 | 2 |
| meshy-barn | 0 | 0 → 0 | 0 → 0 | 1 → 0 | 0 |
| meshy-tree1 | 0 | 1 → 0 | 0 → 0 | 0 → 0 | 9 |
| meshy-palm_tree | 0 | 0 → 0 | 0 → 0 | 4 → 0 | 0 |
| meshy-army_man | 1 | 0 → 0 | 0 → 0 | 1 → 0 | 1 |
| meshy-businessman | 0 | 0 → 0 | 0 → 0 | 2 → 0 | 0 |
| meshy-robot_mech1 | 531 | 5 → 0 | 54 → 0 | 4 → 0 | 10 |
| meshy-car_police | 0 | 0 → 0 | 0 → 0 | 2 → 0 | 0 |
| meshy-burger3 | 0 | 1 → 0 | 0 → 0 | 3 → 0 | 1 |

#### The repair on the files that come in with non-manifold edges or holes left (keep 10k)

| file | non-manifold edges in | split; unpaired; left; crossings added | holes left (before → after) | large holes filled | fuse | closed (before → after) |
|---|---|---|---|---|---|---|
| trellis-5 | 41 | 38; 7; 3; 6 | 2 → 0 | 0 | refused → refused | no → no |
| trellis-120 | 3 | 3; 0; 0; 0 | 3 → 3 | 0 | refused → refused | no → no |
| trellis-300 | 19 | 19; 8; 0; 3 | 10 → 0 | 0 | ok → ok | no → yes |
| trellis-480 | 10 | 10; 0; 0; 0 | 0 → 0 | 0 | ok → ok | no → yes |
| unique3d-480 | 1 | 1; 0; 0; 0 | 0 → 0 | 0 | ok → ok | yes → yes |

#### Pieces and pockets (keep 10k)

| file | pieces in | pieces out (before → after) | pockets the untangling left | dropped as floaters | fuse time (keep 10k run, s; before → after) |
|---|---|---|---|---|---|
| unique3d-300 | 1 | 23 → 1 | 31 | 31 | 27.8 → 45.2 |
| unique3d-480 | 4 | 53 → 6 | 57 | 54 | 94.9 → 56.2 |
| meshy-robot_mech1 | 57 | 4 → 3 | 1 | 1 | 2.1 → 2.9 |

#### Keep runs over budget: where they stopped and why (after)

| file | budget | triangles | UV islands → charts | edges left: fixed / link / turn / uv / fold / not retried |
|---|---|---|---|---|
| hunyuan-5 | 2.0k | 7266 | 2278 → 2278 | 10130 / 49 / 44 / 528 / 20 / 128 |
| hunyuan-120 | 2.0k | 3526 | 1049 → 1049 | 4900 / 23 / 22 / 246 / 12 / 86 |
| hunyuan-300 | 2.0k | 8724 | 2604 → 2604 | 12427 / 44 / 36 / 470 / 3 / 106 |
| hunyuan-420 | 2.0k | 6352 | 2224 → 2224 | 9032 / 37 / 30 / 338 / 2 / 89 |
| hunyuan-480 | 2.0k | 6520 | 2848 → 2848 | 9270 / 47 / 27 / 318 / 0 / 118 |
| trellis-5 | 2.0k | 4342 | 1026 → 1026 | 5600 / 94 / 29 / 451 / 268 / 68 |
| trellis-120 | 2.0k | 3274 | 814 → 814 | 4563 / 17 / 25 / 262 / 38 / 50 |
| trellis-300 | 2.0k | 2620 | 768 → 768 | 3744 / 17 / 14 / 127 / 1 / 27 |
| trellis-480 | 2.0k | 2004 | 460 → 460 | 2598 / 15 / 18 / 247 / 95 / 33 |
| spar3d-30 | 2.0k | 7394 | 5395 → 5395 | 10826 / 13 / 15 / 204 / 2 / 31 |
| spar3d-120 | 2.0k | 21380 | 17505 → 17505 | 31643 / 9 / 20 / 367 / 0 / 31 |
| spar3d-120 | 10k | 21380 | 17505 → 17505 | 31643 / 9 / 20 / 367 / 0 / 31 |
| crm-5 | 2.0k | 5042 | 1708 → 1708 | 7197 / 29 / 18 / 249 / 3 / 67 |
| crm-120 | 2.0k | 3050 | 920 → 920 | 4327 / 26 / 12 / 170 / 9 / 31 |
| meshy-robot_mech1 | 2.0k | 3384 | 1086 → 1086 | 4858 / 22 / 38 / 130 / 8 / 20 |

`self_intersections` above counts the output's pairs; the inputs that
cross (TRELLIS's house 16, cabinet 11) keep a few: the house 3, the
cabinet 25, whose 64-edge fill crosses before decimation (51 after
repair). Unique3D's bear (819k) takes longer (27.8 → 45.2 s, decimation),
the chair less (94.9 → 56.2 s).

Booleans on the keep 10k outputs (`cargo run --release -- OUT`):

| | closed | refused: operand not closed | refused: operand crosses itself | tool missed | max time |
|---|---|---|---|---|---|
| before | 132 | 13 | 6 | 3 | 6.5 s |
| after | 143 | 7 | 0 | 4 | 0.35 s |

The 7 refused are TRELLIS's house and cabinet; the fourth miss is a box
over a gap in Unique3D's bear, which lost its 31 pockets.

### 32.7 Golden

`mcp.process.ball_defects` moved: its report gains `folds_refused`,
`blocked`, `uv_islands`, `uv_charts`, `non_manifold` (null),
`large_holes_filled`, `pockets`, `pockets_dropped` and
`pocket_triangles_dropped`; the written GLB is the same bytes. No other
line moved. New: `meshtools.decimate.fold`, `meshtools.decimate.charts`,
`meshtools.repair.non_manifold`, `meshtools.repair.large_hole`,
`meshtools.repair.seam_fill`, `meshtools.repair.pockets`.

### 32.8 What is not done

- TRELLIS's house (3 non-manifold edges in chains) and cabinet (3 holes
  that no flat clear fill closes) stay open.
- Keeping UVs, 14 files miss 2k: atlas repacking or baking.
- `not_retried` edges: re-planning every edge a collapse touches would
  move clean outputs; left for the owner to decide.
- Decimation is 2–3× slower; the hash is rebuilt on halving, and the
  exact tests are a third of the added time.
- Not run here: `cargo deny` and the wasm32 golden comparison
  (`wasmtime` is not installed and could not be).

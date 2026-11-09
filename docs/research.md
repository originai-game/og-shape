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

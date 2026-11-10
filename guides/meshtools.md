# Mesh tools: from an outside mesh to a usable asset

The kernel's mesh tools take a mesh it did not build (a scan, another
tool's GLB, an OBJ, an FBX) and make it fit for texturing, booleans and
the engine. They report measurements and never judge; this guide says
what to look for.

## The chain

```text
import → join → repair → decimate → place → unwrap → measure → preview → export
```

1. **Import** the file (`import`). Read what was skipped
   (`Imported::skipped`): points, lines, NURBS, morph targets and blend
   shapes are not read. A Draco- or meshopt-compressed glTF is refused:
   get an uncompressed one. A **skinned** mesh (bones and weights) is
   read whole, in its bind pose (`measure` says `skinned` and lists the
   bones), and every tool keeps the skin: weights follow the vertices
   through repair, decimation blends them, and `glb` writes it back.
   `glb_plain` alone refuses it (a plain GLB carries no skin). A
   skeleton under a uniformly scaled armature (centimetres)
   has the scale taken out; the import report (`skin_scales`) says so.
   UVs (the first set), vertex colours (glTF `COLOR_0`, OBJ
   `v x y z r g b`) and the part's material are kept too, and carried by
   every step below. The material (`ImportedPart::material`, the import
   report's `look`) has the factors as the file means them, by three.js's
   names, and its maps as the files' bytes: `map` (base colour),
   `metalness_map` (glTF's metallic-roughness), `normal_map`,
   `emissive_map`, `ao_map`. A glTF factor the file leaves unset takes
   glTF's default and is listed in `defaults`: an unset `metallicFactor`
   is 1, a metal, as every glTF viewer draws it (Hunyuan3D and TRELLIS
   leave it unset). The tools keep it so; set it yourself if the object
   is not metal. `skipped` names what is not kept (a map on a second UV
   set, an MTL's specular, a sampler that does not repeat).
   The axes: glTF is Y up and faces +Z by its specification, and OBJ is
   read the same way. When a file is in another frame, say which of its
   own axes is up and which way the object faces: `up` and `front`
   (`+x`, `-x`, `+y`, `-y`, `+z`, `-z`; give one and the other follows
   by the smallest turn). `z_up` is `up +z, front -y`. The turn is a
   permutation of coordinates with signs: nothing is rounded. FBX says
   its axes in the file and takes none of these.
2. **Join** the parts into one solid (`join`). Each part's faces keep its
   name, so a part can still be found after joining. Skinned parts share
   one skeleton, merged by bone name; bones of one name that disagree, a
   scaled or mirrored skinned part, or skinned and unskinned parts
   together are an error that says which.
3. **Repair** (`repair`). Settings that worked on the acceptance meshes
   (`docs/research.md` §9.5):
   - `weld: 0.001` (1 µm): closes the cracks a seam-split or soup export
     leaves. Keep `weld_all` off unless the mesh is a soup of separate
     sheets meant to be one.
   - `floaters.fraction: 0.05`: drops specks under 5 % of the largest
     piece. Look at `measure`'s `largest_pieces` first: eyes, teeth and
     buttons are pieces too.
   - `non_manifold: true`: splits edges with more than two faces (two
     sheets touching along an edge, a fin) into manifold sheets, moving
     each extra sheet's copy of a vertex 1 % of its shortest edge into
     its own fan. Read `non_manifold` in the report: `left` edges could
     not be split (the mesh stays open), `crossings_added` pairs the moves
     made cross (the fuse step untangles them on a closed shell).
   - `max_hole_edges: 64`: closes holes up to 64 edges round, and larger
     ones (up to 512) whose fill is flat and crosses nothing
     (`large_holes_filled`); the rest stay in `holes_left`. Fills are
     named `fill`; preview them. A fill whose rim crosses a UV seam
     takes one UV (`uv_seam_fills`): one colour, the rim's, instead of a
     fan across the texture.
   - `fuse: true` joins interpenetrating closed parts and untangles a
     shell that passes through itself; the report says when it could
     not (always on a skinned mesh: booleans carry no skin). Untangling
     can leave pockets the surface folded round as shells of their own:
     `pockets` counts them, and those that are floaters by `floaters`
     are dropped (`pockets_dropped`).
   - `min_edge: 0.001` collapses the slivers a union leaves.
4. **Decimate** to the budget (`decimate`, `target` or `ratio`), or make
   LODs (`lods`). The report's `error` is how far the surface moved, in
   mm. No collapse makes the surface pass through itself; where the
   budget cannot be met, `triangles` is above `target` and `blocked`
   counts the edges left by what stops each: `fixed` (corners, and
   seams or borders it does not run along), `uv` (the UVs cannot
   follow), `fold` (it would pass through itself), `link`, `turn`,
   `error`. Keeping a file's UVs, `merge_charts: 0.00025` (a quarter
   texel at 1024²) treats islands that agree across a seam up to a
   rigid move as one chart, which lowers the floor a little
   (`docs/research.md` §32). On a skinned mesh, the rows of vertices where weights change
   (joints) are kept by preference, so the mesh bends as before; the
   price is a little more surface error elsewhere at the same count
   (`docs/research.md` §11.5). `cross_bone_borders` turns this off for a
   mesh that will not be posed.
5. **Place** (`place`), for a mesh that goes into a game as it is:
   - **turn** it so its up is +Z and its front faces −Y (the side
     `preview` shows first): `up` and `front` as at import, then `turn`
     degrees about up (a multiple of 90 is exact);
   - **size** it uniformly, never stretched: `longest`, `height` (Z),
     `width` (X) or `depth` (Y) to a length with its unit (`0.9m`,
     `90cm`, `900mm`), or a `scale` factor;
   - **pivot**: `base` (the centre of the box's bottom face, after the
     turn) stands a prop or a character on z = 0; `center`; or a point of
     the input, in the millimetres `measure` reports.
   The report gives what was done and, before and after, the axis
   measures below. A skin moves with the mesh: bones turn and move, and
   a scale moves their origins, never their axes. UVs, vertex colours
   and the material are untouched. `process` takes the same options and
   places last, after repair and decimation, so the output stands on the
   base it reports.
6. **Unwrap** (`unwrap`) only when the mesh has no UVs, or you will paint
   a new texture: unwrapping replaces the file's UVs, and its maps no
   longer fit, so every one is dropped (the report's `material.dropped`
   names them); the factors and vertex colours stay. Raise `resolution`
   to the texture size you will bake at,
   so the padding is in its texels. `shape process` keeps a textured
   mesh's UVs unless given `--unwrap`.
7. **Measure** (`measure`) with the images and `intersections`; it
   measures the UVs the mesh carries. After repair, look at
   `uv_seam_fills` in its report: those hole fills stretch the texture
   across a seam. After decimation, `uv.overlapping_pairs` and
   `uv.flipped` should be what they were before (a file's own mirrored
   charts count as flipped).
8. **Preview** and look at it.
9. **Shrink the textures** to the game's budget (`downsize`; `shape
   process --texture-max PX`, MCP `texture_max`): every map whose longest
   side is over `PX` texels is box-filtered down (colour maps in linear
   light, data maps as stored) and written as PNG; the report gives each
   map's size and decoded bytes before and after. A PNG of a photo
   texture is larger on disk than the JPEG it came from; the GPU memory
   (`decoded_bytes_with_mips`) is what shrinks. A progressive JPEG is not
   decoded and is kept as it is (the report's `error` says so).
10. **Export** `glb_plain` for texturing (with `material` to keep the
   file's look), or `glb` for parts, joints and skins (a part's
   `material`). OBJ and STL hold the mesh only: the output's
   `not_written` lists the UVs, colours and material they leave out. To
   check a decimated skin, pose both meshes the same way
   (`shape-skin`'s `pose`) and compare them: the posed error should be no
   more than the rest error.

## Reading the measures

| measure | what it says | what to want |
|---|---|---|
| `closed` | every edge has two faces, wound opposite ways | `true` for anything a boolean, a 3D print or a texturing service takes |
| `boundary_loops` | holes left | 0, unless the object is a sheet |
| `non_manifold_edges`, `inconsistent_edges` | edges with three faces or more; faces that disagree | 0 |
| `self_intersections` | pairs of faces passing through each other | 0 for booleans; a few are harmless for rendering; decimation does not add any |
| `floaters`, `largest_pieces` | pieces besides the largest | only the ones the object should have |
| `degenerate_triangles`, `duplicate_triangles` | faces with no area; repeated faces | 0 |
| `shortest_edge` | in mm; below about 1e-5 mm, f32 export merges vertices | well above 1e-5 |
| `triangles` | the budget | at most the budget (a texturing tool's input limit is often about 40 000) |
| `skinned`, `skin.bones` | the mesh has a skin | know it before `glb_plain`, which refuses it |
| `pivot_from_base` | where the origin sits relative to the base's centre | 0 for props that stand on the ground (`place --pivot base`) |
| `uv.overlapping_pairs`, `uv.flipped`, `uv.outside` | UV faces over each other, mirrored, or outside the square | 0 for baking and painting |
| `uv.coverage` | how much of the square the charts fill | the higher the better; 0.1–0.8 on the acceptance meshes, 0.5 on average, lowest on thin parts |
| `texture.decoded_bytes_with_mips` | GPU memory the textures take | within the game's budget |
| `texture.materials`, `texture.by_kind` | each material's maps and sizes; memory per kind of map | a normal or metallic-roughness map rarely needs the base colour's size |
| `colors.borders` | edges where a vertex colour changes sharply (only on a mesh with colours) | what it was before decimation |

## Which way is up

`measure --axes` (MCP `axes`) and `place`'s report give, for the mesh
as it is: `size` (the box's extents), `pivot_from_base`, the `longest`
and `shortest` axis, `facing` (the fraction of the area whose normal
leans most along each of `+x` … `-z`) and `mass.in_box` (where the
centre of volume, or of area for an open mesh, sits in the box per
axis: 0 low, 0.5 middle, 1 high). They are numbers, not a guess. On
the 54 generated meshes (`docs/research.md` §31) no simple reading of
them stood the objects up more often than the files' own frame did:
"the mass sits low" was right for 22, "the flattest side faces down"
for 21, "most area faces down" for 9; reading every file as its format
says (Y up) was right for 42, and the generator's frame (below) for all
54. Know the generator's frame, or preview the mesh and look.

## A budget, for example a texturing tool's input

Texturing tools typically take a closed, joined, triangulated plain GLB
under a triangle budget, unwrapped: `closed`, `triangles` within the
budget, one part, and `glb_plain`'s output (one node, one mesh, one
triangle primitive with UVs). Every kernel solid is triangles, so
"triangulated" always holds.

## Generated meshes (image- and text-to-3D)

Measured on 54 outputs of eight generators (`docs/research.md` §28):
- Their scale and pivot are the generator's: 0.75–2 m whatever the
  object, the origin at the box's centre. `place` with `--pivot base`
  and the object's real height (or longest side) puts them right.
- Their frames, as `up` and `front`: Hunyuan3D-2, TRELLIS, Unique3D and
  Meshy glTF's own (`+y`, `+z`); SPAR3D and CRM `+y`, `-z`; TripoSR's
  OBJ `+z`, `+x`; InstantMesh's OBJ `-z`, `+x`. The front is the side
  the prompt image shows: a horse photographed from its side faces
  front side-on, and a `turn` of 90 or -90 points its head forward.
- Keeping the file's UVs, decimation stops at about three triangles a
  UV island (thousands on an atlas of 2 000 islands); `blocked.fixed`
  says so. `merge_charts` lowers it by 8–31 % on the atlases measured.
  Below that, `--unwrap` reaches the budget and drops the texture.
- TRELLIS's meshes have non-manifold edges: repair with `non_manifold`.
- A hole filled across UV seams takes one colour (`uv_seam_fills`).
- The material and every map are kept, and TripoSR's, InstantMesh's
  and Unique3D's vertex colours (their only colour). Hunyuan3D's and
  TRELLIS's files leave `metallicFactor` unset, so they are metal by
  glTF's rule (`defaults` in the import report); set it to 0 if that is
  not meant. Meshy's maps are 2048² and 4096² (up to 85 MiB with mips):
  `--texture-max 1024` brings each map to 5.3 MiB.
- A normal map is in tangent space: a viewer derives the tangents from
  the UVs and the normals the GLB carries, which the tools recompute
  (smooth within a surface). On SPAR3D's files the shading normal moves
  a median 1–1.6° after repair alone and 2.6–6° after decimation to
  10 000 triangles (`docs/research.md` §29.4); mirrored UV charts keep
  their handedness.
- Hard colour borders are kept by decimation as UV seams are. Fusing a
  vertex-coloured mesh that crosses itself joins sheets of different
  colour, and each meeting edge is a border: Unique3D's chair gets 8 699
  and stops at 10 294 triangles for a budget of 10 000. Compare
  `colors.borders` before and after repair.

## Models from a program

`place` is for meshes from elsewhere. A program places its own parts:
it is written in millimetres, Z up, front −Y, and exported in metres,
so its origin is wherever the author put it (build the model standing
on z = 0 and it stands there in the GLB). There is no pivot option on a
program's export: moving the origin there would move its joints' and
bones' frames too, which the program states itself.

## What repair cannot do

A single closed surface that folds through itself is untangled by
`fuse`. It keeps the space the surface winds round at least once (a
self-union), so `self_intersections` is 0 after it. An open surface that
folds through itself (one piece, `closed` false, `self_intersections`
above 0) cannot be untangled. Booleans refuse it. Close its holes first
(`max_hole_edges`), or get another mesh.

Untangling is slow on heavy noise: about 2 s for 30k triangles with
thousands of folds, 24 s for the fuse step on a 619k-triangle chair with
4 000 crossing pairs.

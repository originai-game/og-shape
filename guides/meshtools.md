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
   through repair, decimation blends them, and the commands write it
   back as a GLB with its bones and skin (parts whose skins hold
   different bones of one skeleton are joined on it). `process --bake`
   is not done on a skinned mesh: keep its UVs. A
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
   - `thicken: 1.5` (CLI `--thicken MM`): a piece still open after
     that (a leaf, a cape, a single-sided wall) gets a second sheet
     that far under it and a rim along its open edges, so it encloses a
     volume and can be cut, measured and printed. Faces keep their
     names; the second sheet adds `inner`, the rim `rim`. Read
     `thickened`: `pieces` done, `skipped` (an edge with more than two
     faces: use `non_manifold` too). A sheet curved tighter than the
     thickness folds its second sheet: check `intersections`.
   - `fuse: true` joins interpenetrating closed parts and untangles a
     shell that passes through itself; the report says when it could
     not (always on a skinned mesh: booleans carry no skin). Untangling
     can leave pockets the surface folded round as shells of their own:
     `pockets` counts them, and those that are floaters by `floaters`
     are dropped (`pockets_dropped`).
   - `min_edge: 0.001` collapses the slivers a union leaves.
4. **Decimate** to the budget (`decimate`, `target` or `ratio`), or make
   LODs: `lods: 3` (level k keeps `ratio`^k), or the levels' own shares
   of the input, `lod_ratios: [0.5, 0.2, 0.05]` (CLI `--lod-ratios
   0.5,0.2,0.05`), each level from the one before. A level may stop
   above what was asked: read each level's `decimate.triangles`, never
   the ratio. `process` takes `budget` or `ratio` (a share of what the
   repair leaves). The report's `error` is how far the surface moved, in
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
   mesh that will not be posed. On a mesh with vertex colours, a hard
   colour border is a feature line, as a UV seam is.
   `cross_color_borders` (`--cross-color-borders`) collapses across
   borders as anywhere: use it when the colours were sampled per vertex
   and a border marks no edge of the object (TripoSR, InstantMesh and
   Unique3D write such colours), and the budget is missed with
   `blocked.fixed` high or `colors.borders` in the thousands. A border
   then moves and blurs by up to an edge, so leave it off where a sharp
   colour line matters (a painted stripe, a decal).
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
   mesh's UVs unless given `--unwrap`, or `--bake PX`, which unwraps,
   packs and bakes the old maps into new ones (below, "Keep the UVs or
   bake").
7. **Measure** (`measure`) with the images and `intersections`; it
   measures the UVs the mesh carries. After repair, look at
   `uv_seam_fills` in its report: those hole fills stretch the texture
   across a seam. After decimation, `uv.overlapping_pairs` and
   `uv.flipped` should be what they were before (a file's own mirrored
   charts count as flipped).
8. **Preview** and look at it. It draws the base-colour map (PNG or
   JPEG), the normal map's detail in the light, vertex colours and the
   material's colour and emissive colour (not its metallic-roughness,
   emissive or occlusion maps, metalness or roughness); after a bake,
   look along chart borders for lines in the shading. Its `undrawn` lists a map it could not
   draw (WebP, KTX2, progressive JPEG, no UVs). `--uv` (MCP `uv`) shows
   the UV layout over the map: dark lines are chart outlines, dimmed
   texels are unused. For a texturing step or a turntable: `--each
   --fov 0.7` gives one PNG a view (`out.00.png` … `out.11.png`: the
   index padded to the last one's width, so the names sort) with its
   camera (position, target, up, field of view; in
   the scene's frame and the GLB's), `--unlit --no-edges --no-ground` the
   colour alone, and `--uv --uv-size PX` the layout to paint into (pixel
   (x, y) is u = uv_min[0] + tiles (x + 0.5) / size, v = uv_min[1] +
   tiles (size − y − 0.5) / size).
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

## Keep the UVs or bake

A textured or vertex-coloured mesh reaches a budget one of two ways
(`shape process`):

- **Keep the UVs** (the default). The maps stay as the file made them,
  byte for byte, and nothing is resampled. Decimation stops at about
  three triangles a UV island (`blocked.fixed` in the report): on the 54
  generated meshes, 14 miss 2 000 triangles this way (and Unique3D's
  chair by its colour borders) and one misses 10 000
  (`docs/research.md` §34). Where it is met, keeping looks closer to the
  file at small sizes: at 10 000 triangles the median colour error was
  1.6 keeping and 2.9 baked (0–255, 256² views), since a bake resamples. Keep the UVs when the budget is met:
  `decimate.triangles` at most `budget`.
- **Bake** (`--bake PX`, MCP `bake`) when it is not, or when the file's
  atlas is wasteful. The repaired mesh is the source. A copy without UVs
  or vertex colours is decimated (nothing holds it back, so any budget is
  reached), its charts are packed on a PX × PX texel grid at one texel
  density, and the source's base colour (times its vertex colours),
  metallic-roughness, emissive and occlusion maps and its shading normals
  are baked into new PNG maps in that layout. The GLB carries the baked
  maps, the material's factors and `TANGENT`. Bake at the texture size
  the game uses (`--texture-max` then has nothing to shrink). On the 54
  at 2 000 triangles, every file reached the budget baked (52 closed;
  the 2 open were open after repair), with a lower colour error than
  keeping on 33 of the 39 that keep reached it on. It costs 3–4 s at
  1024² and a base-colour and a normal map (10.7 MiB with mips). The source's
  vertex colours are in the base-colour map, so the output has none.

What to check in the report:

- `unwrap` (the atlas): `measures.coverage` (how much of the square the
  charts fill; 0.5–0.65 on the 54), `overlap` and `overlapping_pairs`
  (0), `density_texels_per_mm` (min, median, max: even when they are
  close).
- `bake.no_source`: covered texels that found no source point within
  `bake.distance` (2 % of the diagonal unless `--bake-distance` says);
  they are filled from their neighbours like the padding. Above a few
  hundred, raise the distance or look at the preview.
- `bake.nearest`: texels sampled at the nearest source point because the
  normal line found no source surface facing the same way;
  `bake.facing_away`: texels whose nearest crossing faced away (the
  inside of a thin wall) and were taken from farther;
  `bake.ambiguous`: two source sheets facing the same way on both sides
  of the low surface (parts pressed together), the nearer taken. Where
  these are high, compare the preview with the source's.
- `bake.largest_distance`: how far a source point was, in mm. Near
  `distance`, some texels reached for something far away.
- `bake.normals`: the shading normal's angle to the source's, mean and
  95th percentile, with the baked map (the bytes' rounding: about 0.2°)
  and without it (`*_without_map`: what the low mesh shows by its own
  normals). The difference is the detail the map carries.
- `bake.not_baked`: a map that could not be read (WebP, KTX2, a
  progressive JPEG) is not baked and is named here.
- `material.maps`: the baked maps (`baked_map.png`, …), `defaults` and
  the factors as the file had them.

The tangents are this kernel's (per-vertex sums of ∂P/∂u, not
MikkTSpace) and are written to the GLB as `TANGENT`: a viewer that reads
`TANGENT` shades the baked normal map as it was baked; one that derives
its own frame does not. What the bake cannot do, it reports and does not
guess: thin shells and touching sheets (choose by `facing_away` and
`ambiguous`), a source that crosses itself, mirrored source islands (read
correctly; the output has none), and seams between charts in the normal
map (each chart's frame is its own; a seam can show as a faint line in
glancing light).

`shape export program.json --uvs --atlas PX -o out.glb` (a program's
parts, not a mesh) packs every part's UV islands into one atlas at one
density and lets parts whose materials agree share one material; a part
whose material has a map keeps its own UVs (`own_uvs`).

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

## Colliders

A physics engine tests against convex shapes, not the mesh. `collide`
(`shape collide`, the MCP tool, `shape_tools::colliders`) covers each part
of a program or a mesh file with convex hulls and reports, per hull, its
vertices, triangles, volume, `concavity` (how deep the part's surface
lies under the hull, mm) and `outside` (how far the part pokes out of a
thinned hull, mm).

- **One hull** (the default) is right for anything that is convex or
  nearly so: a crate, a barrel, a rock. Its `concavity` says what it
  bridges: 0 for a convex part.
- **A part with a hollow something must enter** (a cup, a doorway, a
  chair's seat, an L) needs `hulls`: the part is cut where the hollow is
  deepest until every piece's concavity is within `concavity` or the
  count is reached. An octagonal cup with a handle, 205 across: 1 hull
  leaves a hollow 310 mm deep (4.3 times the cup's volume); 8 hulls
  leave 125 mm; 16 leave 41 mm (1.5 times) and follow the wall. Ask for the depth the game can ignore (a hand's width for a
  cup), not for 0.
- **`max_vertices`** (16 to 32 is usual) thins each hull, a vertex at a
  time where the error is largest. Read `outside`: a ball of radius 10
  at 14 vertices pokes out by about 2.
- Volumes: `volume` is the hulls' together (they overlap at the cuts);
  against `mesh_volume` (a closed mesh's own) it says how much air the
  colliders hold.
- A program's parts are covered where they stand at rest, each on its
  own, so a jointed part keeps its own colliders. A group has none.
- The cost grows with the mesh's points and the hulls asked: decimate a
  dense mesh first (a few thousand triangles are plenty for a collider).
- A flat sheet has no hull: give it a thickness.

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
  Below that, `--bake 1024` reaches the budget and keeps the look
  (`--unwrap` alone reaches it and drops the texture).
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
  colour, and each meeting edge is a border: Unique3D's chair keeps
  6 997 and stops at 8 078 triangles for a budget of 2 000 (194 mm of
  error); with `cross_color_borders` it reaches 2 000 at 7.4 mm, and
  10 000 at 2.0 mm (`docs/research.md` §33). Compare `colors.borders`
  before and after repair.

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

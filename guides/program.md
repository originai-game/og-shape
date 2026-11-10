# The program JSON

A model is a program: a list of operations the kernel replays to the same
bytes on every host. This is the form `shape build`, `shape measure`,
`shape preview`, `shape export` and the MCP tools `build`, `preview`,
`measure` and `export` take. The TypeScript binding records the same
program (`shape.program()`; `guides/typescript.md`). Write TypeScript
when a part sits on a face or box of another; write JSON for small or
generated models.

```json
{"name": "plate", "ops": [
  {"op": "box", "name": "plate", "width": 80, "depth": 40, "height": 6},
  {"op": "cylinder", "name": "bore", "radius": 3.3, "height": 20,
   "on": {"origin": [25, 0, -5], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "cut", "target": 0, "tool": 1},
  {"op": "fillet", "id": 0, "radius": 1, "edges": "top"},
  {"op": "material", "ids": [0], "name": "steel", "color": "#b4b7bb", "metalness": 1, "roughness": 0.35}
]}
```

A shelf on a leg built up to it, and a spring:

```json
{"name": "shelf", "ops": [
  {"op": "box", "name": "shelf", "width": 30, "depth": 20, "height": 2,
   "on": {"origin": [0, 0, 40], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}},
  {"op": "extrude", "name": "leg", "until": {"part": 0, "face": "bottom"},
   "profile": {"path": [[-3, -3], [3, -3], {"arc": {"through": [4, 0], "to": [3, 3]}}, [-3, 3]]}},
  {"op": "wire", "name": "spring", "diameter": 1, "path": {"helix": {"radius": 4, "pitch": 3, "turns": 4}},
   "on": {"origin": [20, 0, 0], "x": [1, 0, 0], "y": [0, 1, 0], "z": [0, 0, 1]}}
]}
```

## Rules

- **Units:** millimetres (`units` changes it, before the first part);
  angles in radians. Z is up; the front looks −Y.
- **Ids:** every operation that makes a part or a group (generators,
  `group`, `assembly`, `attach`, `copy`, `mirror`) gets the next id: 0, 1,
  2… in order. A boolean's tool is used up but keeps its id.
- **Frames:** `{"origin": [x, y, z], "x": […], "y": […], "z": […]}`,
  orthonormal and right-handed, in the world at rest. A generator's `on`
  puts the part's own origin at `origin`, its own +z along `z`. A
  generator's height runs along that +z.
- **Names:** generators name faces and edges: a box's faces `top`,
  `bottom`, `front`, `back`, `left`, `right` (all walls also `side`),
  its edges `top`, `bottom`, `side`; cylinders and tubes `top`, `bottom`,
  `side` (a tube's bore `inner`); extrusions `top`, `bottom`, `side`;
  sweeps `start`, `end`, `side`; revolves `side`, edges `rim`. A boolean
  names the cut edges `cut` and the tool's name, and keeps each operand's
  faces also as `<part>.<face>`. Fillets and chamfers name their faces
  `fillet`, `chamfer`. `edges` and `open` take a name or a list; every
  element carrying any of them is chosen. A name that matches nothing is
  an error listing the names that exist.
- **Fields:** an operation, profile, section or frame takes only the
  fields listed; any other is an error that names it and lists the
  ones taken. `null` is the same as leaving a field out.
- **Round generators** (`cylinder`, `cone`, `tube`, `sphere`, `dome`,
  `torus`, `revolve`, `wire`) take `segments` (a turn, 3 to 1024; 0 or
  missing sizes it by the radius, 24 to 64). Turned solids take `axis`: `"+Y"` (the default,
  about +y) or `"+Z"` (about +z, the profile's y up; a sphere, dome or
  torus then rests on z = 0).
- **Readers are not recorded.** `face`, `bbox`, `edge`, `grid`, `ring`
  and the other readers answer frames in a live session (the binding);
  a JSON program holds their answers as literal frames.

## Profiles (2D, for `extrude`, `revolve`, `sweep`, `loft`, `pocket`)

An object with one key: `{"rect": {"width", "height", "radius"?}}`,
`{"circle": {"radius", "segments"?}}`, `{"ellipse": {"rx", "ry"}}`,
`{"slot": {"length", "width"}}`, `{"polygon": {"sides", "radius" |
"flats"}}` (to the corners, or across the flats),
`{"poly": {"points": [[x, y] | {"x", "y", "radius"?, "bulge"?}]}}` (a
radius rounds that corner; a bulge bows the edge to the next point into
an arc that far, outward if positive), `{"gear": {"teeth", "module",
"pressureAngle"?}}`, `{"union" | "subtract" | "intersect": [p, q, …]}`
(the first operated on by the rest), `{"offset": {"profile",
"distance"}}`, `{"translate": {"profile", "x", "y"}}`, `{"rotate":
{"profile", "angle"}}`, `{"scale": {"profile", "x"?, "y"?}}` (0 keeps 1),
`{"superellipse": {"rx", "ry", "exponent", "segments"?}}` (2 an ellipse,
larger squarer), `{"spline": {"points": [[x, y], …]}}` (a closed smooth
curve through them), `{"path": [start, piece, …]}` (closed back to the
start), `{"stroke": {"path", "width", "join"? ("round", "miter",
"bevel"), "roundCaps"?, "closed"?}}` (the band round an open path),
`{"hull": [p, q, …]}` (the convex hull), `{"mirror": {"profile",
"axis"?}}` (`"y"`, the default: x becomes −x; `"x"`: y becomes −y),
`{"text": {"text", "font", "size", "spacing"?}}` (baseline on y = 0 from
x = 0; `size` the em).

**Paths.** A list: a start point, then pieces, each from where the last
ended: a point (a line to it), `{"arc": {"through", "to"}}` (a circle
through three points), `{"tangentArc": {"to"}}` (leaves along the last
piece), `{"spline": {"points"}}` (smooth through them). Curves get the
same segment rule as circles. Points are `[x, y]` in a profile's `path`
and `stroke`, `[x, y, z]` in `sweep` and `wire`, which also take
`{"helix": {"radius", "pitch", "turns"}}` (about +z from `[radius, 0,
0]`). Three arc points on one line, or a tangent arc first, are errors.

**Fonts.** No font ships. A `text` profile names a font file; `shape`
and the MCP tools read it relative to the program file (the workspace
root for an inline program, and never outside it). The binding takes
the bytes under that name (`guides/typescript.md`).

## Operations

`?` marks an optional field.

| op | fields | does |
|---|---|---|
| `units` | `unit`: `"mm"`, `"cm"`, `"m"` | the model's unit |
| `box` | `name`, `width` (x), `depth` (y), `height` (z), `on`? | centred in x and y, from z = 0 up |
| `cylinder` | `name`, `radius`, `height`, `radiusTop`?, `segments`?, `on`? | on z = 0 about z; `radiusTop` makes a frustum |
| `cone` | `name`, `radius`, `height`, `segments`?, `on`? | |
| `sphere` | `name`, `radius`, `axis`?, `segments`?, `on`? | |
| `dome` | `name`, `radius`, `height`?, `axis`?, `segments`?, `on`? | a half sphere, or a cap `height` high |
| `tube` | `name`, `outer`, `inner`, `height`, `segments`?, `on`? | radii |
| `torus` | `name`, `radius`, `tube`, `angle`?, `axis`?, `segments`?, `tubeSegments`?, `on`? | `angle`: a part ring with `start`, `end` caps |
| `wedge` | `name`, `width`, `height`, `depth`, `on`? | |
| `extrude` | `name`, `profile`, `height` or `until` (`{"part", "face"}`), `twist`?, `taper`?, `bothSides`?, `on`? | along +z; `until` runs up to the face's plane (a tilted face cuts the top, still `top`) |
| `revolve` | `name`, `profile`, `angle`?, `axis`?, `segments`?, `on`? | profile x is the radius (x ≥ 0) |
| `sweep` | `name`, `profile`, `path`, `up`?, `twist`?, `scaleEnd`?, `on`? | along a path (points, pieces or a helix) |
| `loft` | `name`, `sections`: `[{"profile", "z"}]`, `samples`?, `steps`?, `curve`? (`"auto"`, `"straight"`, `"catmullRom"`, `"spline"`), `on`? | through sections; corners kept; refused if it folds through itself |
| `thread` | `name`, `radius`, `pitch`, `length`, `depth`, `on`? | |
| `wire` | `name`, `path`, `diameter`, `fillet`?, `fillets`? (one a point; points-only paths), `closed`?, `segments`?, `on`? | a round rod; corners between pieces bent round `fillet`, else mitred |
| `body` | `name`, `plan`, `top`, `bottom`, `n`?, `on`? | along +y from views: a number, `[[y, v], …]`, or `{"points", "smooth"}` |
| `knurl` | `name`, `diameter`, `height`, `pitch`?, `straight`?, `on`? | a grip about +z |
| `gear` | `name`, `teeth`, `module`, `width`, `pressureAngle`?, `bore`?, `hub`?, `hubHeight`?, `on`? | a spur gear about +z; `bore`, `hub` diameters |
| `at` | `id`, `frame` | puts the part's own space on `frame` |
| `transform` | `id`, `frame` | moves it by a rigid motion of the world |
| `group` | `name`, `children` (ids) | |
| `assembly` | `name`, `children`, `anchors`? (`{name: frame}`) | |
| `attach` | `to`, `name`, `children` | |
| `copy` | `id`, `name` | |
| `mirror` | `id`, `plane` (a frame; mirrors across its xy), `name` | |
| `cut`, `join`, `intersect` | `target`, `tool`, `check`? | the tool is used up; answers `voids`, `touchingEdges`, `touching`; `check` first refuses an operand that passes through itself |
| `merge` | `target`, `tool` | the tool's solid added as it is, no boolean; the tool is used up |
| `delete` | `id` | removes it and all under it, and joints hanging what goes |
| `seat` | `host`, `part` | cuts `part`'s volume out of `host`; `part` stays |
| `scale` | `id`, `x`?, `y`?, `z`? | along its own axes about its own origin; 0 keeps 1 |
| `hole` | `id`, `diameter`, `at` (frames above the part, +z out of the material), `depth`? (0: through), `counterbore`? `[d, depth]`, `countersink`? `[d, full angle]` | drills along −z |
| `pocket` | `id`, `profile`, `depth`?, `at`? (frames) | |
| `split` | `id`, `frame`, `keep`? (`"above"`, `"below"`) | |
| `shell` | `id`, `thickness`, `open`? (face names) | hollows it |
| `dent` | `id`, `at` (points), `radius`, `depth` | |
| `fillet` | `id`, `radius`, `edges`?, `segments`?, `profile`? | rounds edges |
| `chamfer` | `id`, `distance`, `distance2`?, `angle`?, `edges`?, `segments`? | |
| `soften` | `id`, `radius`, `angle`? (30° by default), `edges`?, `segments`? | rounds every edge sharper than `angle` |
| `material` | `ids`, `name`?, `color`? (`"#rrggbb"` or `[r, g, b]`), `metalness`?, `roughness`?, `emissive`?, `opacity`? | glTF metallic-roughness |
| `joint` | `child`, `parent`, `name`, `kind`? (`revolute`, `continuous`, `prismatic`, `fixed`, `floating`), `on` (the pivot frame), `axis`? (`"+X"`… or `[x, y, z]`; `on`'s z by default), `range`? `[lo, hi]`, `mimic`? `{"joint", "multiplier"}` | revolute and prismatic need a range containing 0 |
| `pose` | `joint`, `value` | posed in export and preview only |

## What comes back

`build` reports every part (name, parent, joint, triangles). `measure`
reports per part and for the model: `triangles`, `vertices`,
`boundsMin`/`boundsMax`, `area`, `volume`, `closed`, `boundaryEdges`,
`nonManifoldEdges`, `degenerateTriangles`, `components`, and the joints.
`build` also lists every boolean with the voids it left and the edges
where its operands touch (`touchingEdges` > 0: not closed there).
Want `closed: true` and no non-manifold edges on every part. An error
names the operation's index, its verb and part (`op 3 fillet "lid": …`)
and what was wrong.

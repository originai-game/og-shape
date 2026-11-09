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
- **Readers are not recorded.** `face`, `bbox`, `edge`, `grid`, `ring`
  and the other readers answer frames in a live session (the binding);
  a JSON program holds their answers as literal frames.

## Profiles (2D, for `extrude`, `revolve`, `sweep`, `loft`, `pocket`)

An object with one key: `{"rect": {"width", "height", "radius"?}}`,
`{"circle": {"radius", "segments"?}}`, `{"ellipse": {"rx", "ry"}}`,
`{"slot": {"length", "width"}}`, `{"polygon": {"sides", "radius"}}`,
`{"poly": {"points": [[x, y] | {"x", "y", "radius"}]}}` (a radius rounds
that corner), `{"gear": {"teeth", "module", "pressureAngle"?}}`,
`{"union" | "subtract" | "intersect": [p, q, …]}` (the first operated on
by the rest), `{"offset": {"profile", "distance"}}`,
`{"translate": {"profile", "x", "y"}}`, `{"rotate": {"profile", "angle"}}`.

## Operations

`?` marks an optional field.

| op | fields | does |
|---|---|---|
| `units` | `unit`: `"mm"`, `"cm"`, `"m"` | the model's unit |
| `box` | `name`, `width` (x), `depth` (y), `height` (z), `on`? | centred in x and y, from z = 0 up |
| `cylinder` | `name`, `radius`, `height`, `radiusTop`?, `on`? | on z = 0 about z; `radiusTop` makes a frustum |
| `cone` | `name`, `radius`, `height`, `on`? | |
| `sphere`, `dome` | `name`, `radius`, `on`? | |
| `tube` | `name`, `outer`, `inner`, `height`, `on`? | radii |
| `torus` | `name`, `radius`, `tube`, `on`? | |
| `wedge` | `name`, `width`, `height`, `depth`, `on`? | |
| `extrude` | `name`, `profile`, `height`, `twist`?, `taper`?, `bothSides`?, `on`? | along +z |
| `revolve` | `name`, `profile`, `angle`?, `on`? | profile x is the radius (x ≥ 0), turned about +y |
| `sweep` | `name`, `profile`, `path` (points), `up`?, `twist`?, `scaleEnd`? | along a polyline |
| `loft` | `name`, `sections`: `[{"profile", "z"}]`, `samples`?, `steps`?, `on`? | through sections |
| `thread` | `name`, `radius`, `pitch`, `length`, `depth`, `on`? | |
| `at` | `id`, `frame` | puts the part's own space on `frame` |
| `transform` | `id`, `frame` | moves it by a rigid motion of the world |
| `group` | `name`, `children` (ids) | |
| `assembly` | `name`, `children`, `anchors`? (`{name: frame}`) | |
| `attach` | `to`, `name`, `children` | |
| `copy` | `id`, `name` | |
| `mirror` | `id`, `plane` (a frame; mirrors across its xy), `name` | |
| `cut`, `join`, `intersect` | `target`, `tool` | the tool is used up |
| `seat` | `host`, `part` | cuts `part`'s volume out of `host`; `part` stays |
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
Want `closed: true` and no non-manifold edges on every part. An error
names the operation (`op 3: …`) and what was wrong.

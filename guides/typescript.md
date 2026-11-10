# The TypeScript API (`og-shape` binding)

A program's default export takes the kernel `og` and returns the `Shape`
it built (`run_typescript` runs it; JSON form: `guides/program.md`; how
to model: `guides/shape.md`).

```ts
export default function (og) {
  const s = og.model({ name: "plate" });
  const plate = s.box({ name: "plate", width: 80, depth: 40, height: 6 });
  s.fillet(plate, { radius: 4, edges: "side" });
  s.hole(plate, { diameter: 6.6, at: s.grid({ nx: 2, px: 60, on: s.face(plate, "top") }) });
  s.material(plate, { color: "#b4b7bb", metalness: 1, roughness: 0.35 });
  return s;
}
```

Millimetres, radians (`og.deg(90)`), Z up, the front looks −Y. Points
are `[x, y, z]` arrays; every frame is in the world at rest. Desc objects
are zero-value; a field not listed here is refused. An error names the
op, its index, the part and your line.

## `og` and profiles

- `og.model({ name, units? })` → `Shape`; `og.deg(d)`; `og.world`.
- `og.frame({ origin?, normal?, up? })`: +z along `normal`, +y toward
  `up` (x = y × z: normal −Y and up +Z give x +X), else by the facing
  rule.
- Profiles are 2D, centred on their origin: `og.rect({ width, height,
  radius? })` (x by y), `og.circle({ radius, segments? })`,
  `og.ellipse({ rx, ry })`, `og.slot({ length, width })` (along x),
  `og.polygon({ sides, radius | flats })` (to the corners, or across
  the flats; a corner on +x: `og.rotate` by π / `sides` puts a flat
  there), `og.poly({ points: [[x, y] | { x, y, radius?, bulge? }] })`
  (`radius` rounds that corner; `bulge` bows the edge from this point
  to the next into an arc whose middle stands that many mm off the
  chord, outward if positive), `og.gear({ teeth, module })`, `og.union /
  subtract / intersect(a, b, …)` (subtract makes a hollow section),
  `og.translate({ profile, x?, y? })`, `og.rotate({ profile, angle })`,
  `og.offset({ profile, distance })`, `og.scale({ profile, x?, y? })`,
  `og.superellipse({ rx, ry, exponent })`, `og.spline({ points })`
  (closed, smooth), `og.path([start, …pieces])`, `og.stroke({ path,
  width, join?, roundCaps?, closed? })`, `og.hull(a, b, …)`,
  `og.mirror({ profile, axis? })` (x → −x; `axis: "x"`: y → −y),
  `og.text({ text, font, size, spacing? })`.
- A path is a start point, then pieces: a point (a line to it), `{ arc:
  { through, to } }`, `{ tangentArc: { to } }`, `{ spline: { points }
  }`. `og.path` makes a profile of one. `sweep` and `wire` take the
  list itself in 3D (`path: [[0, 0, 0], { arc: { through, to } }]`, not
  `og.path`), or `og.helix({ radius, pitch, turns })` (about +z from
  `[radius, 0, 0]`). Their ends sit square to the path: to a curve's
  tangent, or, on a plain point, to the segment that ends there.
- Fonts: `og.model({ name, fonts: { "fonts/a.ttf": bytes } })`; `text`
  names one. Name it by its path relative to the program, so `shape`
  replays it; `og.replay(program, { fonts })`.

## `Frame`

`{ origin, x, y, z }`; `f.offset(d)` along its +z; `f.move(x, y, z)` along
its own axes; `f.turn(rx, ry, rz)`, right hand, about its own X, then Y,
then Z; `f.compose(local)`.

**Facing rule** (face frames, `og.frame` without `up`): facing up or
down, x is world +X (y is +Y on top, −Y below); facing sideways, x is
horizontal (`front` +X, `right` +Y, `back` −X, `left` −Y) and y is +Z.

## `Shape`: one model, every method one kernel operation

Generators take `name`, `on?` and `uvs?` and return a `Part`.
`og.model({ name, uvs: true, textures: { "checker.png": bytes } })`
gives every part UVs and supplies the images materials name:
`s.material(part, { map: "checker.png", mapSize: 20 })`. `on` puts the
part's own origin and axes on the frame's; a profile's x, y are the
part's x, y.

`cylinder`, `cone`, `tube`, `sphere`, `dome`, `torus`, `revolve` and
`wire` take `segments?` a turn (3 to 1024; 0 sizes it, 24 to 64).

| generator | its own space |
|---|---|
| `box({ width, depth, height })` | x, y, z sizes; centred in x and y, from z = 0 up |
| `cylinder({ radius, height, radiusTop? })`, `cone({ radius, height })`, `tube({ outer, inner, height })` | radii; about +z from z = 0 |
| `extrude({ profile, height, twist?, taper?, bothSides? })` | along +z; the top is scaled by 1 − `taper` (negative widens); `bothSides` centres it; `until: { part, face }` instead of `height` runs up to that face's plane |
| `loft({ sections: [{ profile, z }], samples?, steps?, curve? })` | `z` in its own space; straight between sections; `steps` > 1 curves (`curve: "spline"` through every section), and can overshoot: a surface that passes through itself is refused, naming the sections. Polygon corners stay samples and run corner to corner |
| `sphere({ radius, axis? })` | centred |
| `revolve({ profile, angle?, axis? })`, `dome({ radius, height?, axis? })`, `torus({ radius, tube, angle?, tubeSegments?, axis? })` | about **+y**, or `axis: "+Z"` (profile y up; a sphere, dome or torus rests on z = 0, a torus's centre at z = `tube`); profile x is the radius (≥ 0); a part `angle` turns from +x toward −z; a dome `height` makes a cap |
| `sweep({ profile, path, up?, twist?, scaleEnd? })` | the profile (its y toward `up`, `[x, y, z]`) along a path; corners mitred, ends square to the path. A hollow bent pipe is a ring (`og.subtract` of two circles) swept |
| `wire({ path, diameter, fillet?, fillets?, closed? })` | a round rod along a path; corners between pieces bent round `fillet` (≥ diameter / 2), else mitred |
| `body({ plan, top, bottom, n? })` | along +y: `plan` the full width, `top` and `bottom` the z lines, `n` the squareness (2 round); each a number, `[[y, v], …]` or `{ points, smooth: true }` (smooth can overshoot its points, like a curved loft: add points). An end closes to a point only where `plan` and the height both reach 0; else it is flat |
| `gear({ teeth, module, width, bore?, hub?, hubHeight? })`, `knurl({ diameter, height, pitch?, straight? })` | about +z from z = 0; `bore`, `hub` diameters |
| `wedge({ width, height, depth })`, `thread({ radius, pitch, length, depth })` | from the origin into +x, +y, +z; about +z |

Placement (readers answer frames and are not recorded):
- `at(part, frame)` sets the placement, replacing `on`;
  `transform(part, frame)` moves it on top of it. Group placements
  compose.
- `face(part, name)`: a named face, or the one furthest along `"+X"`…;
  origin at its centroid, +z out of it.
- `bbox(parts, "+X" | "+X-Y" | "+X-Y+Z" …)`: the centre of that side,
  edge or corner of the whole box; +z along the first direction.
- `grid({ nx, ny?, px?, py?, stagger?, on? })`: centred on `on`, in its
  xy; x runs fastest, from −x, rows from −y. `ring({ n, radius, start?, angle?, on? })`: `n` frames on a circle
  in `on`'s xy from its +x (over `angle` if given, ends included), each
  x pointing out; `radius: 0` turns in place.
- `edge(part, name)` → a path; `along(path, { pitch?, count? })` →
  frames; `snap(parts, { points, direction? })` → a frame on the surface
  per point (the nearest spot, or where a ray along `direction` meets
  it), +z out toward the point; `names(part)`.

Structure: `group({ name, children })`, `assembly({ name, children,
anchors? })`, `attach(assembly, { name, children })`, `copy(part, name)`,
`mirror(part, { plane, name })` (`plane` a frame; across its xy). Both
bring the joints of what they copy (`hinge.001`…); a mirrored joint moves
as the image of the original, posed by the same values.

Edits (a tool must be a part; it is used up; the target keeps its name):
- `cut`, `join`, `intersect(target, tool, { check? })` return `{ voids,
  touchingEdges, touching, facesWithoutUvs, crossingPairs }`: voids left;
  edges where the operands only touch (> 0: not closed there);
  `touching`, faces lying on each other somewhere (not an error by
  itself); `crossingPairs`, pairs of the result's triangles that still
  cross (> 0: it passes through itself there). `check` refuses an operand that
  passes through itself. `merge(target, tool)` adds the tool as it is (no
  boolean); `delete(part)`. `seat(host, part)` cuts
  `part`'s volume out of `host` and keeps `part`. `scale(part, { x?, y?,
  z? })` along its own axes, about its own origin.
  `seat`, `hole`, `pocket`, `shell` and `dent` return `{ crossingPairs }`
  too: want 0.
- `hole(part, { diameter, at, depth?, counterbore?, countersink? })`:
  `at` one frame or a list, +z out of the material; `depth` 0 goes
  through; `counterbore: [d, depth]`, `countersink: [d, full angle]`.
- `pocket(part, { profile, depth?, at? })`: centred on each `at` frame,
  `depth` measured from it along −z. `shell(part, { thickness, open? })`:
  inward; `open` names the faces left open (none: a sealed hollow), or
  `"+z"`… for the face furthest along the part's own axis, as built.
  `split(part, { frame, keep? })`, `dent(part, { at, radius, depth })`.
- `fillet(part, { radius, edges?, segments? })`, `chamfer(part, {
  distance, distance2?, angle?, edges? })`, `soften(part, { radius,
  angle?, edges? })`; no `edges` takes every edge.

Names: faces `top`, `bottom`, `side` (extrude and loft walls are all
`side`; a box's also `front`, `back`, `left`, `right`; a tube's bore
`inner`), edges `top`, `bottom`, `side`. A boolean keeps operand faces as
`<part>.<face>` and names new edges `cut`; bevels name faces `fillet`,
`chamfer`.

Materials, joints, pose:
- `material(parts, { name?, color?, metalness?, roughness?, emissive?,
  opacity? })`: three.js names; colours `"#rrggbb"`.
- `joint(child, { name, parent, kind?, on, axis?, range?, mimic? })`:
  `parent` a part or group; `axis` a world direction (`on`'s +z by
  default); kinds `revolute` (default), `continuous`, `prismatic`,
  `fixed`, `floating`; revolute and prismatic need a `range` holding 0.
- `pose(joint, value)`: export, preview and `measure({ posed: true })`.
  It is recorded in the program: through MCP, pose with the `preview`
  and `measure` tools' `pose: { joint: value }` instead, which leaves the
  program at rest. Read frames before posing: `face`, `bbox`, `edge`,
  `snap` of a part a posed joint moves throw.

Output: `measure()` (per solid part, at rest; `{ posed: true }` at the
pose, with `joints`: each pivot's distance to its parent and child, and
`interference`: pairs `{ a, b, crossings, touching, inside, asBuilt,
boundsMin, boundsMax }`, the box round both parts' meeting triangles;
with `asBuilt: false` a flush contact can count crossings by rounding),
`glb()`, `obj()` and `stl()`
(at the pose if one is set),
`preview({ width?, views?, columns?, elevation?, azimuth?, noEdges?,
noGround? })` (PNG; views from −Y, then +X, +Y, −X, 25° up;
`elevation` over 0 and up to π/2, no view from below),
`program()`. `console.log` output comes back in `run_typescript`'s
`logs`.

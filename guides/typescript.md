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
  `up`, else by the facing rule.
- Profiles are 2D, centred on their origin: `og.rect({ width, height,
  radius? })` (x by y), `og.circle({ radius, segments? })`,
  `og.ellipse({ rx, ry })`, `og.slot({ length, width })` (along x),
  `og.polygon({ sides, radius | flats })` (to the corners, or across
  the flats; a corner on +x), `og.poly({ points: [[x, y] | { x, y,
  radius?, bulge? }] })` (`radius` rounds that corner; `bulge` bows the
  edge to the next point into an arc that far, outward if positive), `og.gear({ teeth, module })`, `og.union /
  subtract / intersect(a, b, …)` (subtract makes a hollow section),
  `og.translate({ profile, x?, y? })`, `og.rotate({ profile, angle })`,
  `og.offset({ profile, distance })`, `og.scale({ profile, x?, y? })`.

## `Frame`

`{ origin, x, y, z }`; `f.offset(d)` along its +z; `f.move(x, y, z)` along
its own axes; `f.turn(rx, ry, rz)`, right hand, about its own X, then Y,
then Z; `f.compose(local)`.

**Facing rule** (face frames, `og.frame` without `up`): facing up or
down, x is world +X (y is +Y on top, −Y below); facing sideways, x is
horizontal (`front` +X, `right` +Y, `back` −X, `left` −Y) and y is +Z.

## `Shape`: one model, every method one kernel operation

Generators take `name` and `on?` and return a `Part`. `on` puts the
part's own origin and axes on the frame's; a profile's x, y are the
part's x, y.

`cylinder`, `cone`, `tube`, `sphere`, `dome`, `torus`, `revolve` and
`wire` take `segments?` a turn (3 to 1024; 0 sizes it, 24 to 64).

| generator | its own space |
|---|---|
| `box({ width, depth, height })` | x, y, z sizes; centred in x and y, from z = 0 up |
| `cylinder({ radius, height, radiusTop? })`, `cone({ radius, height })`, `tube({ outer, inner, height })` | radii; about +z from z = 0 |
| `extrude({ profile, height, twist?, taper?, bothSides? })` | along +z; the top is scaled by 1 − `taper` (negative widens); `bothSides` centres it |
| `loft({ sections: [{ profile, z }], samples?, steps?, curve? })` | `z` in its own space; straight between sections; `steps` > 1 curves (`curve: "spline"` through every section), and can overshoot: a surface that passes through itself is refused, naming the sections. Polygon corners stay samples and run corner to corner |
| `sphere({ radius, axis? })` | centred |
| `revolve({ profile, angle?, axis? })`, `dome({ radius, height?, axis? })`, `torus({ radius, tube, angle?, tubeSegments?, axis? })` | about **+y**, or `axis: "+Z"` (profile y up; a sphere, dome or torus rests on z = 0, a torus's centre at z = `tube`); profile x is the radius (≥ 0); a part `angle` turns from +x toward −z; a dome `height` makes a cap |
| `sweep({ profile, path, up?, twist?, scaleEnd? })` | the profile (its y toward `up`) along a polyline; corners mitred |
| `wire({ path, diameter, fillet?, fillets?, closed? })` | a round rod along a polyline; corners bent round `fillet` (≥ diameter / 2), else mitred |
| `body({ plan, top, bottom, n? })` | along +y: `plan` the width, `top` and `bottom` the z lines, `n` the squareness (2 round); each a number, `[[y, v], …]` or `{ points, smooth: true }` |
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
  xy. `ring({ n, radius, start?, angle?, on? })`: `n` frames on a circle
  in `on`'s xy from its +x (over `angle` if given, ends included), each
  x pointing out; `radius: 0` turns in place.
- `edge(part, name)` → a path; `along(path, { pitch?, count? })`,
  `snap(parts, { points, direction? })` → frames; `names(part)`.

Structure: `group({ name, children })`, `assembly({ name, children,
anchors? })`, `attach(assembly, { name, children })`, `copy(part, name)`,
`mirror(part, { plane, name })` (`plane` a frame; across its xy).

Edits (a tool must be a part; it is used up; the target keeps its name):
- `cut`, `join`, `intersect(target, tool)`; `seat(host, part)` cuts
  `part`'s volume out of `host` and keeps `part`. `scale(part, { x?, y?,
  z? })` along its own axes, about its own origin.
- `hole(part, { diameter, at, depth?, counterbore?, countersink? })`:
  `at` one frame or a list, +z out of the material; `depth` 0 goes
  through; `counterbore: [d, depth]`, `countersink: [d, full angle]`.
- `pocket(part, { profile, depth?, at? })`: centred on each `at` frame,
  `depth` measured from it along −z. `shell(part, { thickness, open? })`:
  inward; `open` names the faces left open (none: a sealed hollow).
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
- `pose(joint, value)`: export and preview only.

Output: `measure()` (per solid part, at rest), `glb()`, `obj()`, `stl()`,
`preview({ width?, views?, columns?, elevation?, azimuth?, noEdges?,
noGround? })` (PNG; views from −Y, then +X, +Y, −X, 25° up),
`program()`.

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
are zero-value; a field not listed here is ignored, not refused. Errors
are thrown with the kernel's message, which does not say which call.

## `og` and profiles

- `og.model({ name, units? })` → `Shape`; `og.deg(d)`; `og.world`.
- `og.frame({ origin?, normal?, up? })`: +z along `normal`, +y toward
  `up`, else by the facing rule.
- Profiles are 2D and centred on their origin: `og.rect({ width, height,
  radius? })` (x by y), `og.circle({ radius, segments? })`,
  `og.ellipse({ rx, ry })`, `og.slot({ length, width })` (along x),
  `og.polygon({ sides, radius })` (radius to the corners, a corner on +x),
  `og.poly({ points: [[x, y] | { x, y, radius }] })` (points kept as
  given; a radius rounds that corner), `og.gear({ teeth, module })`,
  `og.union / subtract / intersect(a, b, …)` (subtract makes a hollow
  section). Anywhere a profile goes: `{ translate: { profile, x, y } }`,
  `{ rotate: { profile, angle } }`, `{ offset: { profile, distance } }`.

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

| generator | its own space |
|---|---|
| `box({ width, depth, height })` | x, y, z sizes; centred in x and y, from z = 0 up |
| `cylinder({ radius, height, radiusTop? })`, `cone({ radius, height })`, `tube({ outer, inner, height })` | radii; about +z from z = 0 |
| `extrude({ profile, height, twist?, taper?, bothSides? })` | along +z; the top is scaled by 1 − `taper` (negative widens); `bothSides` centres it |
| `loft({ sections: [{ profile, z }], samples?, steps? })` | `z` in its own space; straight between sections; `steps` > 1 curves, and can overshoot: a surface that passes through itself is refused, naming the sections. Polygon corners stay samples and run corner to corner |
| `sphere({ radius })` | centred |
| `revolve({ profile, angle? })`, `dome({ radius })`, `torus({ radius, tube })` | about **+y**; profile x is the radius (≥ 0); a part `angle` turns from +x toward −z |
| `sweep({ profile, path, up?, twist?, scaleEnd? })` | the profile (its y toward `up`) along a polyline of `[x, y, z]` in the part's space; corners are mitred |
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
  `part`'s volume out of `host` and keeps `part`.
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

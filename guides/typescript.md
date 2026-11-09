# The TypeScript API (`og-shape` binding)

The authoring language. A program is a module whose default export takes
the loaded kernel `og` and returns the `Shape` it built. The MCP tool
`run_typescript` runs it under bun, takes the program it recorded and
replays that natively; the JSON form is `guides/program.md`. How to plan
and review an object is `guides/shape.md`.

```ts
export default function (og) {
  const s = og.model({ name: "bracket" });
  const plate = s.box({ name: "plate", width: 80, depth: 40, height: 6 });
  s.fillet(plate, { radius: 4, edges: "side" });
  const top = s.face(plate, "top");
  const rib = s.box({ name: "rib", width: 60, depth: 6, height: 30, on: top.move(0, 14).offset(-1) });
  s.join(plate, rib);
  s.hole(plate, { diameter: 6.6, at: s.grid({ nx: 2, px: 60, on: top.move(0, -8) }) });
  s.material(plate, { name: "steel", color: "#b4b7bb", metalness: 1, roughness: 0.35 });
  return s;
}
```

Millimetres, radians (`og.deg(90)`), Z up, the front looks −Y. Points
and vectors are `[x, y, z]` arrays. Every frame is in the world at rest.
Desc objects are zero-value: leave out what you do not need; a field
that is not listed is ignored, not refused. Errors are thrown with the
kernel's message, which says what failed but not which call: build and
measure step by step.

## `og`

- `og.model({ name, units? })` → `Shape`; `units`: `"mm"`, `"cm"`, `"m"`.
- `og.frame({ origin?, normal?, up? })` → `Frame`: +z is `normal`; +y
  leans to `up` if given, else the facing rule below. `og.world` is the
  world frame. `og.deg(d)`.
- Profiles (2D, centred on their origin): `og.rect({ width, height,
  radius? })` (width along x, height along y), `og.circle({ radius,
  segments? })`, `og.ellipse({ rx, ry })`, `og.slot({ length, width })`
  (along x), `og.polygon({ sides, radius })` (radius to the corners, a
  corner on +x), `og.poly({ points: [[x, y] | { x, y, radius }] })` (a
  radius rounds that corner), `og.gear({ teeth, module, pressureAngle?
  })`, `og.union(a, b, …)`, `og.subtract(a, b, …)` (a with holes: a
  hollow section), `og.intersect(a, b, …)`. To move one:
  `{ translate: { profile, x, y } }`, `{ rotate: { profile, angle } }`,
  `{ offset: { profile, distance } }`.

## `Frame`

`{ origin, x, y, z }`, and `f.offset(d)` (along its +z), `f.move(x, y, z)`
(along its own axes), `f.turn(rx, ry, rz)` (right hand, about its own X,
then Y, then Z), `f.compose(local)`.

**Facing rule** (faces, `og.frame` without `up`): on a face looking up or
down, x is world +X (y is +Y on top, −Y underneath); on any other face x
is horizontal and y is world +Z. So on `front` x = +X, on `right` x = +Y.

## `Shape` (one model; every method one kernel operation)

Generators return a `Part`; each takes `name` and `on?`. `on` puts the
part's own origin on the frame and its own x, y, z on the frame's:

| generator | its own space |
|---|---|
| `box({ width, depth, height })` | x, y, z sizes; centred in x and y, from z = 0 up |
| `cylinder({ radius, height, radiusTop? })`, `cone({ radius, height })`, `tube({ outer, inner, height })` | about +z from z = 0; every size a radius; `radiusTop` makes a frustum |
| `extrude({ profile, height, twist?, taper?, bothSides? })` | profile x, y kept, along +z; `taper` 0.5 halves the top; `bothSides` centres it |
| `loft({ sections: [{ profile, z }], samples?, steps? })` | through the sections, straight between them; `steps` above 1 makes a smooth curve that can overshoot; `samples`: points a section |
| `sphere({ radius })` | centred |
| `revolve({ profile, angle? })`, `dome({ radius })`, `torus({ radius, tube })` (both radii) | about **+y**: profile x is the radius (x ≥ 0), profile y runs along +y; a part `angle` turns from +x toward −z |
| `sweep({ profile, path, up?, twist?, scaleEnd? })` | the profile centred on a polyline `path` of `[x, y, z]` points; sharp corners are mitred (more points make a bend) |
| `wedge({ width, height, depth })`, `thread({ radius, pitch, length, depth })` | from the origin into +x, +y, +z; about +z |

Placement (readers answer frames; they are not recorded):
- `at(part, frame)`: sets the part's placement, replacing its `on`;
  `transform(part, frame)` moves it by a rigid motion on top of
  its placement. Placements of nested groups compose.
- `face(part, name)`: a named face, or the face furthest along `"+X"`…;
  origin at its centroid, +z its outward normal, axes by the facing rule.
- `bbox(parts, "+X" | "+X-Y" | "+X-Y+Z" …)`: the centre of that side,
  edge or corner; +z along the first direction named.
- `local(part, frame)`, `placement(part)`, `anchor(assembly, name)`.
- `edge(part, name)` → a path; `along(path, { pitch?, count? })` → frames.
- `grid({ nx, ny?, px?, py?, stagger?, on? })`: centred on `on`, in its xy.
  `ring({ n, radius, start?, angle?, on? })`: on a circle in `on`'s xy,
  from its +x, each frame's x pointing out. `snap(parts, { points,
  direction? })` → frames.
- `names(part)` → `{ faces, edges }`: the names to address.

Structure: `group({ name, children })`, `assembly({ name, children,
anchors? })`, `attach(assembly, { name, children })`, `copy(part, name)`,
`mirror(part, { plane, name })` (`plane` a frame; mirrors across its xy).

Edits:
- `cut(target, tool)`, `join(target, tool)`, `intersect(target, tool)`:
  the tool must be a part and is used up; the result keeps the target's
  name. `seat(host, part)` cuts `part`'s volume out of `host` and keeps
  `part`.
- `hole(part, { diameter, at, depth?, counterbore?, countersink? })`:
  `at` one frame or a list, above the part with +z out of the material;
  drills along −z; `depth` 0 goes through; `counterbore: [d, depth]`,
  `countersink: [d, full angle]`.
- `pocket(part, { profile, depth?, at? })`: the profile centred on each
  `at` frame (one or a list), its x and y on the frame's, cut along −z. `split(part, { frame, keep? })`,
  `shell(part, { thickness, open? })` (`open`: the face names left open;
  none leaves a sealed hollow),
  `dent(part, { at, radius, depth })`.
- `fillet(part, { radius, edges?, segments?, profile? })`,
  `chamfer(part, { distance, distance2?, angle?, edges?, segments? })`,
  `soften(part, { radius, angle?, edges?, segments? })`; no `edges`
  takes every edge.

Names: generators name faces (`top`, `bottom`, `side`; a box also
`front` −Y, `back`, `left` −X, `right`; a tube's bore `inner`) and edges
(`top`, `bottom`, `side`). A boolean names the cut edges `cut` and keeps
operand faces as `<part>.<face>`; fillets and chamfers name their faces
`fillet`, `chamfer`.

Materials, joints, pose:
- `material(parts, { name?, color?, metalness?, roughness?, emissive?,
  opacity? })`: three.js `MeshStandardMaterial` names.
- `joint(child, { name, parent, kind?, on, axis?, range?, mimic? })`:
  `parent` a part or group; `axis` a world direction (`on`'s +z by
  default); kinds `revolute` (default), `continuous`, `prismatic`,
  `fixed`, `floating`; revolute and prismatic need a `range` containing 0
  (radians or mm).
- `pose(joint, value)`: export and preview only; measures stay at rest.

Output: `measure()` (per solid part, not per group), `glb()`, `obj()`,
`stl()`, `preview({ width?, views?, columns?, elevation?, azimuth?,
noEdges?, noGround? })` (PNG bytes; views go round from the front, −Y,
then +X, +Y, −X, 25° above the horizon), `program()` (the recorded
JSON). Under `run_typescript`, return the Shape and use the MCP tools on
the program instead.

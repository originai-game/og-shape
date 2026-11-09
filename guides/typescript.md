# The TypeScript API (`og-shape` binding)

The authoring language. A program is a module whose default export takes
the loaded kernel `og` and returns the `Shape` it built. The MCP tool
`run_typescript` runs it under bun, takes the program it recorded and
replays that natively; the JSON form is `guides/program.md`.

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

Millimetres, radians (`og.deg(90)`), Z up, the front looks −Y. Every
frame is in the world at rest. Desc objects are zero-value: leave out
what you do not need. Errors are thrown with the kernel's message.

## `og`

- `og.model({ name, units? })` → `Shape`; `units`: `"mm"`, `"cm"`, `"m"`.
- `og.frame({ origin?, normal?, up? })` → `Frame` at `origin` looking
  along `normal`; `og.world` is the world frame. `og.deg(d)`.
- Profiles (2D, plain objects): `og.rect({ width, height, radius? })`,
  `og.circle({ radius, segments? })`, `og.ellipse({ rx, ry })`,
  `og.slot({ length, width })`, `og.polygon({ sides, radius })`,
  `og.poly({ points: [[x, y] | { x, y, radius }] })`,
  `og.gear({ teeth, module, pressureAngle? })`,
  `og.union(a, b, …)`, `og.subtract(a, b, …)`, `og.intersect(a, b, …)`.

## `Frame`

`{ origin, x, y, z }`, and `f.offset(d)` (along its +z), `f.move(x, y, z)`
(along its own axes), `f.turn(rx, ry, rz)` (about its own X, then Y, then
Z), `f.compose(local)`. A generator's `on: frame` puts the part's own
origin there, its height along the frame's +z.

## `Shape` (one model; every method one kernel operation)

Generators return a `Part`; each takes `name` and `on?`:
- `box({ width, depth, height })`: centred in x and y, from z = 0 up.
- `cylinder({ radius, height, radiusTop? })`, `cone({ radius, height })`,
  `tube({ outer, inner, height })`, `sphere({ radius })`, `dome({ radius })`,
  `torus({ radius, tube })`, `wedge({ width, height, depth })`.
- `extrude({ profile, height, twist?, taper?, bothSides? })`,
  `revolve({ profile, angle? })` (profile x is the radius, about +y),
  `sweep({ profile, path, up?, twist?, scaleEnd? })`,
  `loft({ sections: [{ profile, z }], samples?, steps? })`,
  `thread({ radius, pitch, length, depth })`.

Placement (readers answer frames; they are not recorded):
- `at(part, frame)`, `transform(part, frame)`.
- `face(part, name)`: a named face, or the face furthest along `"+X"`…;
  origin at its centroid, +z its normal.
- `bbox(parts, "+X" | "+X-Y" | "+X-Y+Z" …)`, `local(part, frame)`,
  `placement(part)`, `anchor(assembly, name)`.
- `edge(part, name)` → a path; `along(path, { pitch?, count? })` → frames.
- `grid({ nx, ny?, px?, py?, stagger?, on? })`,
  `ring({ n, radius, start?, angle?, on? })`,
  `snap(parts, { points, direction? })` → frames.
- `names(part)` → `{ faces, edges }`: the names to address.

Structure: `group({ name, children })`, `assembly({ name, children,
anchors? })`, `attach(assembly, { name, children })`, `copy(part, name)`,
`mirror(part, { plane, name })`.

Edits:
- `cut(target, tool)`, `join(target, tool)`, `intersect(target, tool)`:
  the tool is used up. `seat(host, part)` cuts `part`'s volume out of
  `host` and keeps `part`.
- `hole(part, { diameter, at, depth?, counterbore?, countersink? })`: `at`
  frames above the part, +z out of the material.
- `pocket(part, { profile, depth?, at? })`, `split(part, { frame, keep? })`,
  `shell(part, { thickness, open? })`, `dent(part, { at, radius, depth })`.
- `fillet(part, { radius, edges?, segments?, profile? })`,
  `chamfer(part, { distance, distance2?, angle?, edges?, segments? })`,
  `soften(part, { radius, angle?, edges?, segments? })`.

Names: generators name faces (`top`, `bottom`, `side`; a box also
`front`, `back`, `left`, `right`) and edges (`top`, `bottom`, `side`). A
boolean names the cut edges `cut` and keeps operand faces as
`<part>.<face>`; fillets and chamfers name their faces `fillet`,
`chamfer`. Address faces and edges by name, never by coordinates.

Materials, joints, pose:
- `material(parts, { name?, color?, metalness?, roughness?, emissive?,
  opacity? })`: three.js `MeshStandardMaterial` names.
- `joint(child, { name, parent, kind?, on, axis?, range?, mimic? })`:
  kinds `revolute` (default), `continuous`, `prismatic`, `fixed`,
  `floating`; revolute and prismatic need a `range` containing 0.
- `pose(joint, value)`: export and preview only.

Output: `measure()`, `glb()`, `obj()`, `stl()`, `preview({ width?, views?,
columns?, elevation?, azimuth?, noEdges?, noGround? })` (PNG bytes),
`program()` (the recorded JSON). Under `run_typescript`, return the Shape
and use the MCP tools on the program instead.

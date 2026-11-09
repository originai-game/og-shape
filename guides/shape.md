# Modelling an object

How to turn an object you are asked for into a program the kernel builds.
The API is `guides/typescript.md`. The program is the asset: you change
the model by changing the program, never the mesh.

## 1. Plan: parts first, then details

Before writing code, write down, from the description and the reference:

1. **The size.** The overall width (x), depth (y) and height (z) in
   millimetres. Z is up, the front looks −Y. If only one size is given,
   estimate the others from the reference's proportions.
2. **The parts.** A part is a piece that is one material or moves on its
   own: a body, a lid, a door, a wheel, a knob. Name each one.
3. **What moves.** For each moving part: its parent, the kind of motion
   (turns on a hinge, slides, spins), the axis and the range.
4. **The main dimensions of each part**, about 5 to 15 numbers each. Every
   other number is derived from them.
5. **The details**: holes, fillets, chamfers, grooves. They come last,
   once the parts are the right size, in the right place, and move right.

Build in that order: masses → placement → joints → review → details →
review. A fillet on a part that is the wrong size is wasted work.

## 2. The shape of a program

```ts
export default function (og) {
  const s = og.model({ name: "toolbox" });
  const width = 300, depth = 160, height = 140; // the object's size, named once

  const box = body(width, depth, height);
  const hinge = og.frame({ origin: s.bbox(box, "+Y+Z").origin }); // back top edge
  const top = lid(width, depth);
  s.at(top, hinge);
  s.joint(top, { name: "lid_hinge", parent: box, on: hinge, axis: "-X", range: [0, og.deg(110)] });
  return s;

  function body(w, d, h, wall = 4) {
    const b = s.box({ name: "body", width: w, depth: d, height: h });
    s.shell(b, { thickness: wall, open: "top" });
    s.material(b, { color: "#c0392b", roughness: 0.6 });
    return b;
  }

  // Built about its own origin: the hinge line runs along x at y = 0.
  function lid(w, d, t = 6) {
    const panel = s.box({ name: "lid_panel", width: w, depth: d, height: t, on: og.world.move(0, -d / 2, 0) });
    s.fillet(panel, { radius: 2, edges: "top" });
    s.material(panel, { color: "#2c3e50", roughness: 0.5 });
    return s.group({ name: "lid", children: [panel] });
  }
}
```

- **The head lists the parts.** The default export names the object's
  size, calls one function per part, places each part once and declares
  the joints. It builds no geometry itself.
- **One function per distinct part.** It builds the part about its own
  origin and returns it. A part used twice is one function called twice,
  or one part copied.
- **Name each dimension once.** Main sizes are arguments with defaults;
  everything else is an expression of them. A number written twice drifts.
- **Say each shape with the verb that names it**: a turned profile is
  `revolve`, a hollow is `shell`, a bore is `hole`, a bent rod is
  `sweep`, a changing section is `loft`. A table of points is the last
  resort.

## 3. Names

- **Parts:** `snake_case`, unique in the model, saying what the part is
  (`lid`, `left_door`, `drum`), never `part1`.
- **Joints:** what moves on what, or the motion: `lid_hinge`,
  `drawer_slide`, `wheel_spin`.
- **Faces and edges** are reached by name, never by coordinates. A box
  has faces `top`, `bottom`, `front`, `back`, `left`, `right` (walls also
  `side`), edges `top`, `bottom`, `side`. After a boolean the operand's
  faces are also `<part>.<face>`, and the new edges are `cut`.
  `s.names(part)` lists what a part carries; a wrong name throws an error
  listing the names that exist.

## 4. Placement with frames

A frame is an origin and three axes. `on: frame` in a generator, or
`s.at(part, frame)`, puts the part's own origin on the frame's origin and
its own +z along the frame's +z. Both set the same placement, so a later
`at` replaces an earlier `on`. When a part's pivot or seat is not at its
generator's origin, build it inside a group (its pieces placed in the
group's space, which starts as the world) and `at` the group, as the lid
in §2.

Where each generator puts its own origin:

| generator | origin and axis |
|---|---|
| `box` | centred in x and y, bottom on z = 0 |
| `cylinder`, `cone`, `tube`, `extrude`, `loft` | about +z, bottom on z = 0 (`bothSides` centres an extrude) |
| `sphere` | centred on the origin |
| `revolve`, `dome`, `torus` | about **+y**, centred on the origin |
| `wedge` | its corner on the origin, in +x, +y, +z |

To stand a revolve, a dome or a torus upright (its axis along world +z),
place it on `og.frame({ origin, normal: [0, -1, 0], up: [0, 0, 1] })`.

Frames to place on:

- `s.face(part, "top")`: on the face's centroid, +z out of the face. A
  part built on it grows out of the surface. Move along the face with
  `.move(x, y)`, sink into it with `.offset(-d)`.
- `s.bbox(parts, "+X-Y+Z")`: a corner, edge or side of the bounding box;
  +z points out of the box.
- `og.world.move(x, y, z)` and `og.frame({ origin })`: plain world axes.

**Face and box frames are turned**: their +z is the face's outward
normal. Put a part *on* a face with them (a boss, a rib, a hole). To put a
part *at* a point without turning it, take only the origin:
`og.frame({ origin: s.bbox(body, "+Y+Z").origin })`.

Parts that touch must share the surface: a part sitting on another starts
where the other ends. Parts to be joined into one solid overlap a little
(0.5 mm) or share a face.

**Repeat with frames:** `s.grid`, `s.ring` and `s.along(edge)` give
frames; build one copy on each (`for (const f of s.grid({...}))`). The
other side of a pair is `s.mirror(part, { plane, name })`. Never compute
positions with `Math.sin` and `Math.cos`.

## 5. Join, group or joint

| the pieces | use | gives |
|---|---|---|
| fused for good, one material | `s.join(a, b)` | one solid; `b` is used up |
| separate, moving together | `s.group({ name, children })` | each keeps its mesh and material |
| moving against each other | `s.joint(child, …)` | the child moves on its parent |

`assembly` is the top-level group. A boolean's tool must be a part, not a
group. A part with no joint moves with the group it is in.

## 6. Joints: one frame per joint

1. Build the moving part (or group) about its own origin, with the pivot
   at that origin: a door's hinge line on its own z axis, a wheel's axle
   on its origin.
2. Take one frame `f` for the pivot, in the world, at rest.
3. `s.at(child, f)` and `s.joint(child, { name, parent, on: f, axis,
   range })`: the same `f` for both.

- **Build at rest**: every joint at 0, so the closed door is the door at
  0. A range must contain 0: `[0, og.deg(110)]` opens one way,
  `[og.deg(-110), 0]` the other.
- **The axis's sign decides the direction**: a positive value turns by
  the right hand about the axis. A door hinged on its left edge (seen
  from the front) opens outward about `"-Z"`; hinged on its right edge,
  about `"+Z"`. A drawer slides out along `"-Y"`.
- **Kinds:** `revolute` (a hinge, with a range), `continuous` (a wheel,
  a knob; no range), `prismatic` (a slide, range in mm), `fixed`.
- **What does not move has no joint.** Group it with its parent.
- **A chain** (arm segments) is each segment jointed on the previous one.
  Several joints on one child (slide and turn) compose in order.

## 7. Materials

When the object has colours, give every part `s.material(part, { color:
"#rrggbb", metalness?, roughness? })` once, before it is copied or
mirrored. A group's material reaches every part under it that has none.
`join` keeps the target's material: join only what is one colour.

## 8. Budgets

- **Triangles.** Take the budget from the task; without one, keep a prop
  to a few thousand and a large object under about 20 000. Curves cost
  most: a circle takes up to 64 segments a turn (fewer with `segments`),
  a fillet `segments` rows. `measure().triangles` is the count.
- **Numbers.** About 15 per part, 40 at most. More means positions
  computed by hand instead of read from frames.
- **Parts.** One part per material or motion. Details that move with a
  part and share its colour are joined into it.

## 9. Review: build → preview → measure → fix

Build after each step of §1, not once at the end.

1. **Build.** An error names the operation and what was wrong; a wrong
   name lists the names that exist.
2. **Measure.** For every part: `closed: true`, `components: 1` (unless
   the part is meant to be several pieces), `nonManifoldEdges: 0`,
   `degenerateTriangles: 0`. The model's `boundsMax − boundsMin` against
   the size planned in §1. `triangles` against the budget.
3. **Preview.** Look at every view against the reference: proportions,
   parts floating in the air or sunk into others, parts on the wrong
   side, missing parts. The first view looks at the front (from −Y).
4. **Pose each joint** near the ends of its range (`s.pose(name, value)`),
   preview, and check it moves the right way and does not pass through its
   parent. Then take the pose out: the program is at rest.
5. **Fix the program**, one cause at a time, and build again. Stop when
   every measure holds and the preview matches the reference.

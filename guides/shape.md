# Modelling an object

How to turn an object you are asked for into a program the kernel builds.
The API is `guides/typescript.md`. The program is the asset: you change
the model by changing the program, never the mesh.

## 1. Plan: parts first, then details

Before writing code, write down, from the description and the reference:

1. **The size.** The overall width (x), depth (y) and height (z) in
   millimetres. Z is up, the front looks −Y. Numbers in the description
   win over the picture; the picture says what the words leave out. When
   a size is given per axis, match those axes even if the object's own
   front then faces another way.
2. **The parts.** A part is a piece that is one material or moves on its
   own: a body, a lid, a door, a wheel, a knob. Name each one.
3. **What moves.** Only the motions the description states: for each, its
   parent, the kind (turns on a hinge, slides, spins), the axis and the
   range. A part that is not said to move has no joint.
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
  const width = 300, depth = 160, height = 140; // named once, before any part

  const box = body(width, depth, height);
  const hinge = og.frame({ origin: s.bbox(box, "+Y+Z").origin }); // back top edge
  const top = lid(width, depth);
  s.at(top, hinge);
  s.joint(top, { name: "lid_hinge", parent: box, on: hinge, axis: "-X", range: [0, og.deg(110)] });
  return s;

  function body(w, d, h, wall = 4) {
    const b = s.box({ name: "body", width: w, depth: d, height: h });
    s.shell(b, { thickness: wall, open: "top" });
    const latch = s.box({ name: "latch", width: 30, depth: 6, height: 12, on: s.face(b, "front").move(0, h / 2 - 10).offset(-1) });
    s.join(b, latch); // overlaps 1 mm: `offset(-1)` sinks it into the wall
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
  the joints. It builds no geometry itself. Declare every shared number
  before the first call: the part functions below `return` run when
  called and cannot read a `const` declared after the call.
- **One function per distinct part.** It builds the part about its own
  origin and returns it. A part used twice is one function called twice,
  or one part copied.
- **Name each dimension once.** Main sizes are arguments with defaults;
  everything else is an expression of them.
- **Say each shape with the verb that names it**: a turned profile is
  `revolve`, a hollow is `shell`, a bore is `hole`, a rod or bent tube is
  `sweep` along points, a changing section is `loft`, a draft is
  `extrude` with `taper`, a constant section along any axis is `extrude`
  on a turned frame. A table of points is the last resort.

## 3. Names

- **Parts:** `snake_case`, unique in the model, saying what the part is
  (`lid`, `left_door`, `drum`), never `part1`. A `join` keeps the
  target's name, so name the target after the whole.
- **Joints:** what moves on what, or the motion: `lid_hinge`,
  `drawer_slide`, `wheel_spin`.
- **Faces and edges** are reached by name, never by coordinates. A box
  has faces `top`, `bottom`, `front` (−Y), `back`, `left` (−X), `right`
  (walls also `side`), edges `top`, `bottom`, `side`. After a boolean
  the operand's faces are also `<part>.<face>`, and the new edges are
  `cut`. `s.names(part)` lists them; a wrong name throws an error listing
  the names that exist.

## 4. Placement with frames

`on: frame` in a generator, or `s.at(part, frame)`, puts the part's own
origin on the frame's origin and its own x, y, z along the frame's. Each
generator's own space is in `guides/typescript.md`; to stand a revolve, a
dome or a torus upright, place it on `og.frame({ origin, normal: [0, -1,
0], up: [0, 0, 1] })` (profile y becomes world +Z).

**`on` and `at` set the same placement: a later `at` replaces an `on`
without warning.** So:

- A part placed once: give it `on` (or build it at the origin and `at`
  it), never both.
- A part whose pivot or seat is not at its generator's origin (a door
  hinged on its edge, an arm turning at one end): build its pieces with
  `on` inside a group, whose space starts as the world, and `at` the
  group, as the lid in §2.
- A detail you will `join` (a tool must be a part, not a group): give it
  `on: frame.move(…)` and do not `at` it.

Frames to place on:

- `s.face(part, "top")`: on the face's centroid, +z out of the face. A
  part built on it grows out of the surface: a box's `height` runs out of
  the face, its `width` along the face frame's x. `.move(x, y)` slides
  along the face (axes in `guides/typescript.md`), `.offset(-d)` sinks
  into it.
- `s.bbox(parts, "+X-Y+Z")`: the centre of a side, edge or corner.
- `og.world.move(x, y, z)`, `og.frame({ origin })`: plain world axes.
- `f.turn(rx, ry, rz)`: tilt any frame, instead of computing a normal.

**Face and box frames are turned**: their +z is the outward normal. Put a
part *on* a face with them (a boss, a rib, a hole). To put a part *at* a
point without turning it, take only the origin: `og.frame({ origin:
s.bbox(body, "+Y+Z").origin })`.

**Pieces to join must overlap**, by about 0.5 mm or more. Faces that
only touch (coplanar faces, a flat face on a round one) can leave the
result open or throw `the result is open`. A part resting on another
without joining may share the face.

**Repeat with frames:** `s.grid`, `s.ring` and `s.along(edge)` give
frames; build one copy on each (`for (const f of s.ring({...}))`). The
other side of a pair is `s.mirror(part, { plane, name })`, with the
plane a frame whose +z is the mirror's normal. Never compute positions
with `Math.sin` and `Math.cos`: turn a frame instead.

## 5. Join, group or joint

| the pieces | use | gives |
|---|---|---|
| fused for good, one material | `s.join(a, b)` | one solid; `b` is used up |
| separate, moving together | `s.group({ name, children })` | each keeps its mesh and material |
| moving against each other | `s.joint(child, …)` | the child moves on its parent |

`assembly` is the top-level group. A part with no joint moves with the
group it is in. Fillet or chamfer each piece before joining it: after a
join, `edges: "top"` takes every operand's top edges.

## 6. Joints: one frame per joint

1. Build the moving part (or group) about its own origin, with the pivot
   at that origin: a door's hinge line on its own z axis, a wheel's axle
   on its origin.
2. Take one frame `f` for the pivot, in the world, at rest. For a turn,
   any point on the axis will do; for a slide, only the axis counts.
3. `s.at(child, f)` and `s.joint(child, { name, parent, on: f, axis,
   range })`: the same `f` for both.

- **Build at rest**: every joint at 0, the object as the description
  shows it (a closed door, folded arms). A range must contain 0:
  `[0, og.deg(110)]` opens one way, `[og.deg(-110), 0]` the other.
- **The axis is a world direction and its sign decides the direction**:
  with the right thumb along the axis, the fingers curl the way a positive
  value turns. A door hinged on its left edge (seen from the front) opens
  outward about `"-Z"`, on its right edge about `"+Z"`; a lid hinged at
  the back opens about `"-X"`; a drawer slides out along `"-Y"`.
- **Kinds:** `revolute` (a hinge, with a range), `continuous` (a wheel,
  a knob; no range; it may sweep through its parent), `prismatic` (a
  slide, range in mm), `fixed`.
- **A chain** (arm segments) is each segment jointed on the previous one.
  Several joints on one child (slide and turn) compose in order. `mirror`
  and `copy` do not carry joints: declare the copy's own, with the axis
  mirrored.

## 7. Materials

When the object has colours, give every part `s.material(part, { color:
"#rrggbb", metalness?, roughness? })` once, before it is copied or
mirrored. A group's material reaches every part under it that has none.
`join` keeps the target's material: join only what is one colour.

## 8. Budgets

- **Triangles.** Take the budget from the task; without one, keep a prop
  to a few thousand and a large object under about 20 000. Round things
  cost most: up to 64 segments a turn, and a small sphere or torus is
  several hundred triangles. Only `og.circle` takes `segments` (extrude
  it for a cylinder with fewer or more); fillets take `segments`.
- **Numbers.** About 15 per part, 40 at most. More means positions
  computed by hand instead of read from frames.
- **Parts.** One part per material or motion. Details that move with a
  part and share its colour are joined into it.

## 9. Review: build → preview → measure → fix

Build after each part, not once at the end: an error says what failed but
not which call, so the last change is the suspect.

1. **Measure.** For every part: `closed: true`, `components: 1` (unless
   the part is meant to be several pieces), `nonManifoldEdges: 0`,
   `degenerateTriangles: 0`. The model's `boundsMax − boundsMin` against
   the size planned in §1. `triangles` against the budget. Measures are
   per solid part, at rest.
2. **Preview.** The views go round from the front (−Y), then +X, +Y, −X,
   from above. Check every view against the reference: proportions,
   parts floating or sunk into others, parts on the wrong side, missing
   parts. For a feature too small to see, preview from its side
   (`azimuth`, `elevation`) or check it by its measures.
3. **Pose each joint** near the ends of its range, preview, and check it
   moves the right way and clears its parent. Then take the pose out.
4. **Fix the program**, one cause at a time, and build again. Stop when
   every measure holds and the preview matches the reference.

| you see | the cause, usually | fix |
|---|---|---|
| a part off by half its size, or centred on its pivot | `at` replaced the part's `on` | a group (§4) |
| a part turned on its side | `at` on a face or box frame | `og.frame({ origin: f.origin })` |
| `the result is open`, or `closed: false` after a join | faces that only touch | overlap 0.5 mm |
| `the bevel is larger than the faces allow` | radius above half the smallest face, or a fillet after a join | smaller radius, or fillet before joining |
| a part moves the wrong way | the axis's sign | flip the axis |
| a cut that changes nothing | the tool is wholly inside or outside | make it cross the surface |
| a polygon wider than planned | `radius` is to the corners | radius = flats ÷ 2 ÷ cos(180° ÷ sides) |

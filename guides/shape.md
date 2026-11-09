# Modelling an object

How to turn an object into a program (API: `guides/typescript.md`). The
program is the asset: change the program, never the mesh.

## 1. Plan: parts first, then details

Before writing code, write down from the description and the reference:

1. **The size**: width (x), depth (y), height (z) in millimetres, at
   rest. Z is up, the front looks −Y. The description's numbers win over
   the picture; the picture fills in what the words leave out. Match a
   size given per axis even if the object's own front then faces another
   way.
2. **The parts.** A part is one material or one motion: a body, a lid, a
   door, a wheel. Pieces the description names separately but that share
   a colour and never move apart are one part.
3. **What moves**: only the motions the description states, each with
   its parent, kind, axis and range. A part not said to move has no joint.
4. **Each part's main dimensions**, 5 to 15 numbers (40 at most).
5. **The details** (holes, fillets, chamfers) last, once the parts are
   the right size, in place, and move right.

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
    s.join(b, latch); // sunk 1 mm into the wall, so they overlap
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

- **The head lists the parts**: it names the size, calls one function
  per part, places each part once and declares the joints. Declare
  shared numbers before the first call: a function below `return` cannot
  read a `const` declared after the call.
- **One function per distinct part**, built about its own origin; each
  dimension named once, the rest derived.
- **Say each shape with its verb**: turned is `revolve`, hollow `shell`,
  a bore `hole`, a rod or bent tube `sweep` (a bend's points from
  `s.ring({ n, radius, angle })` origins), a changing section `loft`, a
  draft `extrude` with `taper`, a constant section along any axis
  `extrude` on a turned frame, a curved panel the `intersect` of a slab
  and a big cylinder or sphere. A table of points is the last resort.
  `Math.sin` and `Math.cos` may size things, never place them: turn a
  frame.

## 3. Names

Parts are `snake_case`, unique, saying what they are (`lid`, never
`part1`); a `join` keeps the target's name. Joints say what moves:
`lid_hinge`, `drawer_slide`. Reach faces and edges by name, never by
coordinates (`s.names(part)`; a joined detail's own: `"boss.top"`).

## 4. Placement with frames

`on: frame` in a generator, or `s.at(part, frame)`, puts the part's own
origin and axes on the frame's. A revolve, dome or torus stands upright
on `og.frame({ origin, normal: [0, -1, 0], up: [0, 0, 1] })`.

**A later `at` replaces an `on` without warning.** So:
- a part placed once gets `on`, or is built at the origin and `at`-ed;
- a part whose pivot is not at its generator's origin (a door hinged on
  its edge) is built with `on` inside a group, and the group is `at`-ed,
  as the lid in §2;
- a detail to `join` (a tool cannot be a group) gets `on: frame.move(…)`
  and no `at`.

Frames to place on:
- `s.face(part, "top")`: the face's centroid, +z out of it. A part built
  on it grows out of the surface: a box's `height` runs out of the face,
  its `width` along the face's x. `.move(x, y)` slides along the face,
  `.offset(-d)` sinks into it.
- `s.bbox(parts, "+X-Y+Z")`: the centre of a side, edge or corner of the
  whole box, joined details included.
- `og.world.move(x, y, z)`, `og.frame({ origin })`: world axes; `f.turn`
  tilts any frame.

Face and box frames are turned (+z outward): use them to put a part *on*
a face; to put it *at* a point unturned, use `og.frame({ origin:
f.origin })`.

**Pieces to join overlap** by 0.5 mm or more. Faces that only touch
(coplanar, or flat on round) can leave the result open. Parts that are
not joined may touch.

**Repeat with frames**: build one copy on each frame of `s.grid`,
`s.ring` or `s.along`. The other side of a pair is `s.mirror(part, {
plane, name })`, the plane a frame whose +z is the mirror's normal.

## 5. Join, group or joint

Fused for good and one material: `s.join(a, b)`. Separate but moving
together: `s.group`. Moving against each other: `s.joint`. A part with
no joint moves with its group. Fillet or chamfer each piece
before joining: after a join, `edges: "top"` takes every operand's.

## 6. Joints: one frame per joint

1. Build the moving part (or group) with its pivot at its own origin: a
   hinge line on its z axis, an axle through its origin.
2. Take one frame `f` for the pivot, at rest: any point on a turn's axis;
   for a slide only the axis counts.
3. `s.at(child, f)` and `s.joint(child, { name, parent, on: f, axis,
   range })`, the same `f` for both.

- **At rest** every joint is 0 and the object is as described (a closed
  door, folded arms). A range holds 0: `[0, og.deg(110)]` or
  `[og.deg(-110), 0]`.
- **The axis is a world direction; its sign sets the direction**: thumb
  along the axis, fingers curl the positive way. Seen from the front, a
  door hinged on its left edge opens outward about `"-Z"`, on its right
  edge about `"+Z"`; a lid hinged at the back opens about `"-X"`; a flap
  hung from a top front edge swings out about `"-X"`; a drawer slides
  out along `"-Y"`.
- **Kinds**: `revolute` (hinge), `continuous` (wheel, knob), `prismatic`
  (slide, mm), `fixed`.
- A **chain** is each segment jointed on the one before (groups may be
  parents). Several joints on one child compose in order. `mirror` and
  `copy` carry no joints: declare the copy's, axis mirrored.

## 7. Materials and budgets

- Give every part `s.material(part, { color: "#rrggbb", metalness?,
  roughness? })` once, before copying. A group's material reaches parts
  with none; `join` keeps the target's, so join only one colour.
- **Triangles**: the task's budget; without one, a few thousand for a
  prop and under about 20 000 for a large object. Round things cost most
  (up to 64 segments a turn; a small sphere or torus is several hundred);
  only `og.circle` and fillets take `segments`.

## 8. Review: build → preview → measure → fix

Build after each part: errors do not name the call.

1. **Measure** every part: `closed: true`, `components: 1` unless meant
   otherwise, no non-manifold edges or degenerate triangles; the bounds
   against §1's size; `triangles` against the budget. Measures are per
   solid, at rest, blind to parts passing through each other.
2. **Preview** (from −Y, then +X, +Y, −X, from above) against the
   reference: proportions, floating or sunk parts, wrong sides, missing
   parts. For a small or hidden feature, preview from its side
   (`azimuth`, `elevation`), alone, or check its measures.
3. **Pose each joint** near its range's ends: it moves the right way and
   clears its parent (a continuous joint may pass through). Take the
   pose out.
4. **Fix the program**, one cause at a time, until every measure holds
   and the preview matches the reference.

| you see | the cause, usually | fix |
|---|---|---|
| a part off by half its size, or centred on its pivot | `at` replaced its `on` | a group (§4) |
| a part turned on its side | `at` on a face or box frame | `og.frame({ origin: f.origin })` |
| `the result is open`, or `closed: false` after a join | faces that only touch | overlap 0.5 mm |
| `the bevel is larger than the faces allow` | radius over half the smallest face, or a fillet after a join | smaller, or before the join |
| a part moves the wrong way | the axis's sign | flip it |
| a cut that changes nothing | the tool is wholly inside or outside | make it cross the surface |
| a polygon too wide | `radius` is to the corners | radius = flats ÷ 2 ÷ cos(180° ÷ sides) |

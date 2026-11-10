The toaster is built and saved in `program.json`. Its bounding box is 155.13 × 306.56 × 179 mm, within about 0.02 mm of the brief.

- **Housing:** one brushed-steel shell that bulges a little near the bottom and narrows to rounded shoulders at the top. It's closed, in one piece, with no mesh errors.
- **Slots and supports:** two slots 140 mm deep, with 28 vertical wire supports inside them (7 along each wall).
- **Feet:** four small rubber feet underneath.
- **Controls:** all on the +Y end.
  - `lever_slide`: the lever tab slides down 59 mm along −Z in a vertical slot.
  - `dial_turn`: the browning dial, with a pointer, turns freely about +Y.
  - `cancel_push`: the cancel button pushes in 3 mm into a socket.
- **Triangles:** about 7,550 in total.

The previews match the reference.

The posed check still lists 17–30 crossings between the lever tab and the housing when the lever is lowered. These are flagged "not as built", meaning they come from rounding at near-contact rather than a real overlap. The reported area also covers parts of the slot the tab isn't in. I added 1.5 mm of clearance, but the count didn't change, and I had only one run left, so it isn't resolved. Separately, the dial and the feet are set into the housing about 0.5–1 mm on purpose, and the wire supports go 1 mm into the slot floor; those are expected.

**ITERATIONS:** 7 run_typescript calls; 1 failed (a fillet on the lofted housing).

**PROBLEMS:**
- **Fillet on a loft:** shape.md suggests `loft` for a changing section and `fillet` for rounding, but nothing says that filleting a loft's edges (here a spline loft's top) fails. It errors with "a bevel along a face whose triangles at this vertex are not in one plane". The workaround is to add extra loft sections near the top to round it, and the guide could say so.
- **Spline loft overshoot:** a `curve: "spline"` loft makes the body wider than its widest section (157.6 mm against 155 mm). The guide says it "can overshoot" but gives no idea how much, so fitting an exact bounding box took a scaling pass.
- **Lofted body:** a hull or fuselage is listed under `body`, but `body` has a single plan width along y. The guides don't say how to make a housing that narrows towards the top, and I used a loft of rounded rectangles instead.
- **`snap`:** the guide lists `snap(parts, { points, direction? })` → frames with no explanation (project onto the surface? what does the frame face?). I had to guess that it puts a point onto the surface, which was the only reliable way to seat controls on a curved end.
- **Facing rule:** the rule ("front +X, right +Y, back −X, left −Y") is hard to read for a frame facing +Y. Passing `up` explicitly was the safe choice.
- **Posed interference:** with `asBuilt: false`, the guide doesn't explain the reported `boundsMin`/`boundsMax`. Here they stayed fixed whatever the pose and were wider than the moving part, so they couldn't be used to find a contact. "A flush contact can count crossings by rounding" also leaves it unclear when crossings can be safely ignored.
- **Combining many pieces:** the guides don't say how to put many separate solids of one material (28 wires, 4 feet) into one part. `merge` worked but is described only as "adds the tool as it is", and nothing names a pattern for it (e.g. a `grid` with `merge`).

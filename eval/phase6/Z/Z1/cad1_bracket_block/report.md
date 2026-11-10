I built the bracket in one `run_typescript` call and saved it to `program.json`.

**Measurements:** it is one closed solid with 518 triangles, no holes in the mesh and no broken edges. The bounding box is 80 × 60 × 20 mm, as the brief asks.

**Shape:**
- **Base:** an L outline extruded 20 mm high, with the long leg along x at the front and the short leg along y at +X. The outer corners are rounded to 8 mm.
- **Chamfer:** 2 mm on the bottom edges.
- **Pocket:** 30 × 20 mm, 16 mm deep, sunk into the long leg right next to the corner. It runs across the full width of the leg, open on both sides, as in the reference image.
- **Hole:** 8 mm, through the end of the long leg.

**Check:** the preview matches the reference. The 2 mm chamfer is too small to confirm in the preview, and the volume is about 2% below my rough hand estimate.

- **ITERATIONS:** 1 `run_typescript` call, 0 failed.
- **PROBLEMS:**
  - The brief doesn't say whether the 30 × 20 pocket runs across the whole 20 mm leg (leaving it open on both sides) or stays enclosed. I had to read that from the reference image. To make sure the cut went fully through both sides, I made the pocket shape 24 mm wide instead of 20.
  - The guides don't say how `og.poly` corner `radius` should be used for an L-shaped outline, or whether the inner corner is affected. I left the inner corner sharp.
  - The `chamfer` call takes `edges: "bottom"` on an extrude, but the guides don't say whether that includes the inner corner edges of a non-convex outline. It worked, but I had to try it to find out.
  - Otherwise none.

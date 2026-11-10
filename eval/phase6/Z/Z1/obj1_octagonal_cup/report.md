I built the octagonal cup in one `run_typescript` call and saved it to `program.json`. Its bounding box is 193 × 352.2 × 137 mm, which matches the brief. `measure` shows it as one closed solid with no non-manifold edges and 152 triangles. In the preview it matches the reference: an eight-sided cup open at the top with a flat rim, chamfered bottom edges, and a flat bar handle sticking straight out of the +Y side just below the rim.

Some sizes are my own estimates, because the brief gives only the overall box:
- **Walls:** 12 mm thick. The rim in the reference looks somewhat thicker, maybe about 20 mm, but I kept it thinner because the brief says "thin-walled".
- **Bottom chamfer:** 10 mm.
- **Handle:** 40 mm wide and 12 mm thick, with its top 8 mm below the rim.

**ITERATIONS:** 1 `run_typescript` call, 0 failed.

**PROBLEMS:**
- `og.polygon` puts a corner on +x, so getting flats facing ±X and ±Y takes a 22.5° rotation. The guide notes the corner on +x but never says how to get flats on the axes; a one-line example would help.
- The guides don't say whether `chamfer` with `edges: "bottom"` on an extrude is safe to follow with `shell` (it was), or in what order the two should go.
- In the preview from +Y, the end of the handle shows as a rectangle on the wall, which looks like a hole at first glance. The guides don't warn about this; it's minor.

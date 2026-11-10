The plate is built and saved to `program.json`. From an angled view it matches the reference: full width at one end, a straight section, then a concave arc that narrows to the tip. The measurements check out: the bounding box is exactly 304.8 × 914.4 × 17.53 mm, and the part is a single closed solid with no non-manifold edges or degenerate triangles, at 40 triangles.

The brief only gives the overall size, so I estimated three dimensions from the picture: the tip is 125 mm wide, the curved side runs straight for 300 mm before the arc starts, and the arc is 50 mm deep.

- **ITERATIONS:** 2 `run_typescript` calls, 0 failed.
- **PROBLEMS:**
  - The guide doesn't clearly say what a `bulge` value measures. "Bows the edge to the next point into an arc that far" doesn't state the units, or whether the number is the arc's depth, an angle, or the tan(θ/4) value used in DXF files. I had to check the preview to confirm it's a depth in mm and that a negative value curves the edge inward.
  - The guide doesn't make clear that the bulge goes on the point where the curved edge starts (the arc runs from that point to the next one), not on the point where it ends.
  - The extrusion came out at only 40 triangles, and the guide doesn't say how a bulge arc is divided into segments or how to change that. I couldn't check whether the arc is smooth enough.

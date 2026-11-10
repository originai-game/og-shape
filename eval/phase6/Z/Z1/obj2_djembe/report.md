I built the djembe in one run, and `program.json` is saved.

The measured size is 407.91 × 407.91 × 600 mm, which matches the brief. Every part is closed, in one piece, with no non-manifold edges and no degenerate triangles. The whole model is about 11,000 triangles. In the preview it matches the reference: a flared wooden base narrowing to a neck, a goblet-shaped bowl with 24 black tension strips, a black band at the neck and another under the head, and a pale head with a black hoop on top. I only looked at it from the four standard turntable angles.

- **ITERATIONS:** 1 run_typescript call, 0 failed.
- **PROBLEMS:**
  - Neither guide gives a pattern for a part repeated round a turned body, such as strips following the bowl's curve. I worked out that a `wire` path in local x/z combined with `s.ring({ radius: 0 })` frames does the job. A one-line example would have saved the guesswork.
  - The guide doesn't say whether `ring` frames with `radius: 0` keep world +Z as their z axis (they do), which you need to know to place an upright path.
  - `torus` with `axis: "+Z"` "rests on z = 0, a torus's centre at z = `tube`" is correct, but it's easy to misread when sizing the hoop to a given top height. A note that the top sits at `on.z + 2·tube` would help.
  - Otherwise none.

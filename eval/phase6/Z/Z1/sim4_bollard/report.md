The bollard is built in one run and saved to `program.json`. It's a single revolved solid, 314 × 314 × 960 mm, which is exactly the size in the brief. I didn't make a separate measure call because the run reports the same measures: the mesh is closed, has one component, no non-manifold edges and 4,096 triangles. I previewed it from the front, the side and above, and it matches the reference: a flat round foot with a chamfered top, a short neck, a dark 244 mm-wide column, and a recessed top with a raised rim.

- **ITERATIONS:** 1 `run_typescript` call, 0 failed.
- **PROBLEMS:**
  - The guides don't say which way `og.poly` points must run for a `revolve` profile (clockwise or counter-clockwise), or whether the order matters. Mine worked, but the guides don't say it would.
  - The guides don't say whether a revolve profile needs a point on the axis (x = 0) to make a solid with no hole down the middle.
  - The guides don't say whether `radius` on a `poly` point that a revolve then turns is allowed, or what it does. The table only says that profile x is the radius and must be ≥ 0. Rounding those corners did work.
  - Otherwise the guides were enough for this object.

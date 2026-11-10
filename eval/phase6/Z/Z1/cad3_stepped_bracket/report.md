I built the bracket with 1 `run_typescript` call. The model is saved to `program.json`, but its right end may not match `reference.png`, and I only checked it against the brief.

**What I built:** a Z-shaped profile 5 mm thick, extruded 30 mm up:
- **Upper flange:** at the back left (y = 48 to 53), with a 4.5 mm hole and a 90° countersink 8 mm across on its outer side.
- **Web:** runs front to back at x = 31 to 36.
- **Lower flange:** at the front right (y = 0 to 5), with a 7 mm hole.

The brief gives only the overall size, so the 5 mm thickness and the web position (x = 31) are my guesses. The web position is a rough estimate from proportions in the reference image.

**Checks:** `measure` gives a bounding box of 76 × 53 × 30 mm. The solid is closed, in one piece, with no non-manifold edges and 798 triangles. The previews show the Z shape and both holes.

**Possible mismatch with the reference:** I couldn't fully read the right end of `reference.png`. It seems to show a thin top strip running back from the lower flange and a light-coloured inner face behind it, which looks like a fourth wall. My model has no such part, so either the image is shot from an angle I didn't recreate or the real profile has an extra return wall.

- **ITERATIONS:** 1 `run_typescript` call, 0 failed.
- **PROBLEMS:**
  - The guides don't say how an extruded profile should be placed when the brief describes it in profile terms ("upper/lower flange", "vertical web") but also says Z is up. I had to infer that the profile lies flat in XY and is extruded up Z, so its "upper" and "lower" become back and front.
  - The guides don't say how reference images from other tools are oriented (for example, whether they are Y-up or which camera angle they use), which made the reference hard to compare against.
  - It isn't stated which side a hole's countersink is cut on. I assumed the side the `at` frame's +z points out of, which the line "+z out of the material" suggests.

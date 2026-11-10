I built the mounting block in 2 `run_typescript` calls, and the program is saved to `program.json`. It is one closed, single-piece solid of 1,062 triangles, and its bounding box is 99.5 × 80 × 109.5 mm, matching the brief.

- **Block:** an 80 mm cube with every edge chamfered 2 mm.
- **Boss:** 30 mm across and 30 mm tall on the top centre, set 0.5 mm into the block so the two fuse, with a 6 mm hole running 30 mm down it.
- **Rib:** 20 × 4 × 12 mm, sticking out of the −X face near the top. It is what makes the model 99.5 mm wide.
- **Side holes:** four 5 mm holes, 12 mm deep. Two are in the −X face and two in the front (−Y) face, at staggered heights as in the reference.
- **Bottom pocket:** 56 × 56 mm (12 mm margin) and 20 mm deep.

I couldn't look at the pocket directly, because the preview tool refused a view from below (details under PROBLEMS). Instead I checked it by volume: the measured volume is within about 0.1% of what I calculated by hand with the pocket included.

ITERATIONS: 2 run_typescript calls, 0 failed. (The second only shortened the boss so the top sits at 109.5 mm instead of 110.)

PROBLEMS:
- The preview tool rejects a negative elevation (−0.8 and −0.9) with "the preview's elevation is not finite". That message is wrong for the input, and no guide says whether views from below are possible, so a feature on the bottom can't be checked by looking.
- shape.md §8 says to preview "from above", but neither guide says what elevation range is allowed.
- No guide says what face names remain after a `join`, or whether `s.face(part, "left")` still works on the joined part. To be safe I read all the face frames before the joins; the guide could say whether that is necessary.
- For this object, a conflict between brief and reference: the brief puts the rib "at the top" of a side, while the reference shows it below mid-height. I followed the brief, as shape.md says the description wins, so the rib doesn't match the picture.

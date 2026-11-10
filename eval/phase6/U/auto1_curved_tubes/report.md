I built the curved-tube model in one `run_typescript` call, which succeeded, and saved it to `program.json`. Its size matches the brief, but the shape follows my own reading of the brief rather than the reference image, which I couldn't fully work out.

**What I built:**
- **Three identical round tubes stacked end to end**, 254 mm across with a 10 mm wall and hollow all the way through. Each is about 553 mm tall: it rises, bends through two 90° elbows that shift it 448 mm sideways, then rises again.
- **The bottom tube is fixed.** The two upper tubes are grouped and turn together about a vertical axis through the top of the bottom tube (`upper_swivel`).
- **Size:** 702.0 × 495.3 × 1658.35 mm against the brief's 702.17 × 495.47 × 1658.35 mm. To fit the width and depth, I chose the 220 mm bend radius and the turned directions of the two upper tubes (147.4° and −32.6°) myself.
- **Checks:** each tube is a closed solid in one piece, with no bad edges or triangles. The model has 8,250 triangles in total.

**Where it differs from the reference:** in `reference.png` some tube openings face sideways, and the elbows don't look like a neat stack of S-tubes. The brief doesn't say how the three tubes connect, so I chose a straight vertical stack. The finished model has the right overall size but doesn't closely match the picture.

- **ITERATIONS:** 1 run_typescript call, 0 failed.
- **PROBLEMS:**
  - `typescript.md` lists `wire` with `fillet` but doesn't say how much straight length each bend uses up (one bend radius on each side of a corner). I had to assume it to size the path so the bends wouldn't overlap.
  - Neither guide shows how to make a hollow bent pipe. `shell` isn't described for `wire`, so I cut a thinner wire running past both ends, with no confirmation that this is the intended way.
  - `shape.md` says to use "a pipe elbow a part `torus`", but the torus's direction rules ("about +y… a part `angle` turns from +x toward −z") are hard to use for chaining elbows. `wire` with `fillet` was much easier, and the two pieces of advice point different ways.
  - Nothing explains how a part placed inside a group with `on: og.world…` combines with the group's own placement. I assumed the group's frame applies to it.

The lever knob is built and saved as `program.json`, and it looks like the reference. One size is a little off: the width is 50.02 mm, not the 50.54 mm in the brief.

- **Block:** a hexagon 28 mm tall, with a 16.6 mm square recess 21 mm deep on top and a 30 mm bore 4 mm deep underneath.
- **Knob:** a stepped hexagon that narrows in ten bands to the top, with a 16 mm square lug underneath that sits in the recess.
- **Motion:** the knob has a slide joint (`knob_slide`, down 0 to 19 mm) and a spin joint (`knob_spin`, continuous about Z), both on the block.
- **Checks:** both parts are closed, single pieces with no bad edges. The model is 570 triangles. Height (98.94 mm) and depth (57.76 mm) match the brief.
- **Not checked:** I didn't pose the joints. At rest the knob sits right on the block, so sliding it down 19 mm would push it into the block. I kept the rest position the reference shows.

**Width:** a regular hexagon 57.76 mm across its corners is 50.02 mm across its flats, so it can't also be 50.54 mm wide. I kept the hexagon regular and matched the depth.

ITERATIONS: 2 run_typescript calls, 0 failed.

PROBLEMS:
- The guides don't say how to make a polygon's corner point along +y. `og.polygon` puts a corner on +x, and the only way I found was to wrap it in `og.rotate` by 90°.
- The guides don't say how to model hard steps with a loft. The way that worked was repeating each band's section at two heights (a straight band, then a short taper). A plain loft with one section per band came out as a smooth curve, not steps.
- The guides don't cover a joint whose range runs the part into its parent, such as this slide down into the block when the knob rests on it at 0. They don't say whether the rest position or the range should give way.

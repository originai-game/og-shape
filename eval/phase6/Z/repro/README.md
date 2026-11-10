# Run `Z`'s kernel gaps (research §22.5)

Each file is a program for `shape build` or `shape measure`
(`target/release/shape`); the expected and actual answers are below.

| file | crate | what happens | expected |
|---|---|---|---|
| `sweep_end_tangent.json` | shape-gen | a `sweep` (and a `wire`) of radius 127 along one arc whose ends are tangent to +z and to +x: `measure` gives bounds z from −5.85 and x to 282.24 | z from 0, x to 276.39: the end sections square to the path's tangent, not to the first and last chord (an overshoot of r·sin(half a segment)) |
| `sweep_end_tangent_lead_in.json` | shape-gen | the same with a 20 mm straight piece at each end: z from −20, x to 296.39, exact. With 1 mm pieces the overshoot stays (4.6 mm): the mitre between the line and the arc reaches past the short piece | (the guide's workaround) |
| `bevel_tapered_rounded_extrude.json` | shape-bevel | `fillet` of `edges: "top"` (radius 10) on an `extrude` of a rounded `rect` with `taper: 0.1`: refused, "a bevel along a face whose triangles at this vertex are not in one plane (a twisted or converging face) … not supported" | a rounded top edge; the same fillet on a tapered `cylinder` (`radiusTop`) builds, and on the untapered extrude too |
| `bevel_curved_loft_top.json` | shape-bevel | run `Z2`'s toaster housing: a spline `loft` of four rounded rectangles, then that fillet: the same refusal (a straight loft of the same sections too) | as above |

Not reproduced as a kernel gap, and why:
- `shape-tools` `preview` refuses an elevation of 0 or below with "the
  preview's elevation is not finite", a misleading message for a number
  that is finite. `shape-mcp` now refuses it first with the range; the
  kernel's message is unchanged (not this stream's crate).
- The build report's `parent` leaves out a group with no joint of its
  own (flattened in the export), so a joint's parent group is missing
  there; `measure --posed` lists the joint's parent. Documented in
  `guides/program.md`, not changed.
- Posed interference boxes that "do not follow the pose" (`Z2`'s toaster
  agent): the box is round both parts' meeting triangles, so a large
  parent face makes it wide; on a lid sliding into a box it follows the
  pose. Documented in `guides/typescript.md`.

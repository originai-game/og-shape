I built the telescoping slide in one `run_typescript` call and saved it as `program.json`. Its bounding box is exactly 1100 × 240 × 300 mm. Every part measured closed and manifold, with no degenerate triangles, 396 triangles in total. The previews match the reference. I did not preview it posed, so the two slide joints have not been tested at full travel. I checked the clearances by arithmetic only.

**What the model contains:**
- **Wall plate:** 30 × 240 × 300 mm at the +X end, with chamfered edges.
- **Screw heads:** two, on the plate.
- **Standoffs:** two blocks between the plate and the sleeve. They count as two separate pieces in the measures, which is intended.
- **Outer sleeve:** a C-channel, 500 mm long, with a slot along its −Y face.
- **Middle section:** a dark hollow tube. It slides out along −X on joint `middle_slide`, range 0–260 mm.
- **Inner blade:** a thin bar with a 30 mm end block. It slides out of the middle along −X on joint `inner_slide`, range 0–220 mm.

**Clearances:** the middle section has 2 mm a side inside the sleeve, and the blade has plenty of room inside the middle. At full extension the middle still overlaps the sleeve by 190 mm and the blade overlaps the middle by 200 mm.

ITERATIONS: 1 run_typescript call, 0 failed.

PROBLEMS:
- The guides say `og.world` can be the joint frame for a group built in world coordinates, which would be at the origin. But nothing says whether a prismatic joint's `on` frame has to be near the parent. shape.md says only the axis counts for a slide, while the measure section warns about "a pivot far from its parent's surface". It's unclear whether that warning applies to slides.
- The guides have no simple way to make a box from its min and max corners. The box is centred in x and y but starts at z = 0, so I wrote a small helper to place boxes by their extents.
- Nothing says whether `components: 2` is acceptable for a part made by joining two separate pieces (my two standoffs). shape.md says only "1 unless meant otherwise".

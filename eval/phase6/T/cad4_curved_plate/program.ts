export default function (og) {
  const s = og.model({ name: "curved_plate" });
  const width = 304.8, length = 914.4, thickness = 17.526; // overall size
  const plate = curvedPlate(width, length, thickness);
  s.material(plate, { color: "#9a9a9a", roughness: 0.6 });
  return s;

  // Rectangle whose +X long side is replaced by a concave arc of a large circle.
  // The arc runs from the right edge at y = tipY up to (arcTopX, arcTopY), the
  // leftmost point of the circle; a straight edge closes to the top-left corner.
  function curvedPlate(w, l, t, tipY = -139.7, arcTopX = -50.8, arcTopY = 406.4,
                       cx = 784.673, cy = 406.419, r = 835.473) {
    const outline = og.poly({ points: [
      [-w / 2, l / 2], [-w / 2, -l / 2], [w / 2, -l / 2], [w / 2, tipY], [arcTopX, arcTopY],
    ] });
    const p = s.extrude({ name: "plate", profile: outline, height: t });
    const cutter = s.cylinder({ name: "arc_cutter", radius: r, height: t + 2, on: og.world.move(cx, cy, -1) });
    s.cut(p, cutter);
    return p;
  }
}

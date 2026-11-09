export default function (og) {
  const s = og.model({ name: "curved_plate" });
  const width = 304.8, length = 914.4, thickness = 17.53;
  plate(width, length, thickness);
  return s;

  // Wide end at -Y, straight long side at -X, concave arc on +X tapering to the tip at +Y.
  function plate(w, l, t, tipWidth = 100, straightRun = 275) {
    const p = s.box({ name: "plate", width: w, depth: l, height: t });
    // Arc: from (w/2, -l/2 + straightRun) to (-w/2 + tipWidth, l/2), tangent to y at the tip.
    const dx = w - tipWidth, dy = l - straightRun;
    const r = (dx * dx + dy * dy) / (2 * dx);
    const arc = s.extrude({
      name: "arc_cutter",
      profile: og.circle({ radius: r, segments: 512 }),
      height: t + 2,
      on: og.world.move(-w / 2 + tipWidth + r, l / 2, -1),
    });
    s.cut(p, arc);
    s.material(p, { color: "#9a9a9a", roughness: 0.6 });
    return p;
  }
}

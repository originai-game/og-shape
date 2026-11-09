export default function (og) {
  const s = og.model({ name: "curved_plate" });
  const width = 304.8, length = 914.4, thickness = 17.53;
  const tipWidth = 50;      // narrow end, along x
  const straightRun = 316;  // full-width length before the arc starts

  plate(width, length, thickness);
  return s;

  // Centred in x and y; the straight long side is -X, the tip is at +Y.
  function plate(w, l, t) {
    const p = s.box({ name: "plate", width: w, depth: l, height: t });
    // Arc from (w/2, -l/2 + straightRun) to (-w/2 + tipWidth, l/2), its centre level with the tip.
    const a = w - tipWidth, b = l - straightRun;
    const r = (a * a + b * b) / (2 * a);
    const cutter = s.extrude({
      name: "arc_cutter",
      profile: og.circle({ radius: r, segments: 256 }),
      height: t + 2,
      on: og.world.move(-w / 2 + tipWidth + r, l / 2, -1),
    });
    s.cut(p, cutter);
    s.material(p, { color: "#9a9a9a", roughness: 0.6 });
    return p;
  }
}

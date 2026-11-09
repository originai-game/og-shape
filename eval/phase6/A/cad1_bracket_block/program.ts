export default function (og) {
  const s = og.model({ name: "bracket_block" });
  const width = 80, depth = 60, height = 20; // overall size
  const block = bracket(width, depth, height);
  return s;

  // L plate: long leg along x at the front, short leg along y at the right.
  function bracket(w, d, h, leg = 20, r = 8, chamfer = 2) {
    const x0 = -w / 2, x1 = w / 2, y0 = -d / 2, y1 = d / 2;
    const xi = x1 - leg, yi = y0 + leg; // inner corner
    const profile = og.poly({ points: [
      { x: x0, y: y0, radius: r },
      { x: x1, y: y0, radius: r },
      { x: x1, y: y1, radius: r },
      { x: xi, y: y1, radius: r },
      [xi, yi],
      { x: x0, y: yi, radius: r },
    ] });
    const p = s.extrude({ name: "bracket", profile, height: h });
    s.chamfer(p, { distance: chamfer, edges: "bottom" });
    const pocketLen = 30;
    s.pocket(p, { profile: og.rect({ width: pocketLen, height: leg }), depth: 16,
      at: og.world.move(xi - pocketLen / 2, (y0 + yi) / 2, h) });
    s.hole(p, { diameter: 8, at: [og.world.move(x0 + leg / 2, (y0 + yi) / 2, h)] });
    s.material(p, { color: "#b4b7bb", metalness: 1, roughness: 0.4 });
    return p;
  }
}

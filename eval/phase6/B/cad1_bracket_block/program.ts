export default function (og) {
  const s = og.model({ name: "bracket_block" });
  const width = 80, depth = 60, height = 20; // overall x, y, z
  const leg = 20, corner = 8, bevel = 2;
  const pocketLen = 30, pocketDepth = 16, holeD = 8;

  const b = bracket(width, depth, height);
  return s;

  // L plate: long leg along x at the front (y -30..-10), short leg along y at the right end.
  function bracket(w, d, h) {
    const x0 = -w / 2, x1 = w / 2, y0 = -d / 2, y1 = d / 2;
    const profile = og.poly({ points: [
      { x: x0, y: y0, radius: corner },
      { x: x1, y: y0, radius: corner },
      { x: x1, y: y1, radius: corner },
      { x: x1 - leg, y: y1, radius: corner },
      [x1 - leg, y0 + leg],
      { x: x0, y: y0 + leg, radius: corner },
    ] });
    const p = s.extrude({ name: "bracket", profile, height: h });
    s.chamfer(p, { distance: bevel, edges: "bottom" });
    // pocket across the long leg, next to the corner
    const top = og.world.move(0, 0, h);
    s.pocket(p, { profile: og.rect({ width: pocketLen, height: leg + 2 }), depth: pocketDepth,
      at: top.move(x1 - leg - pocketLen / 2, y0 + leg / 2, 0) });
    s.hole(p, { diameter: holeD, at: top.move(x0 + leg / 2, y0 + leg / 2, 0) });
    s.material(p, { color: "#a0a0a0", roughness: 0.6 });
    return p;
  }
}

export default function (og) {
  const s = og.model({ name: "stepped_bracket" });
  const width = 76, depth = 53, thick = 30; // x, y, z
  const t = 5;            // plate thickness
  const webX = 25;        // left edge of the web
  const stepY = 25;       // height of the return leg at the right
  const bracket = body();
  return s;

  function body() {
    const profile = og.poly({ points: [
      [0, depth - t], [webX, depth - t], [webX, 0], [width, 0], [width, stepY],
      [width - t, stepY], [width - t, t], [webX + t, t], [webX + t, depth], [0, depth],
    ] });
    const b = s.extrude({ name: "bracket", profile, height: thick });
    // upper flange: 4.5 mm with 90 deg countersink to 8 mm, from the +Y face
    s.hole(b, { diameter: 4.5, countersink: [8, og.deg(90)],
      at: og.frame({ origin: [webX / 2, depth, thick / 2], normal: [0, 1, 0] }) });
    // lower flange: 7 mm through, from the -Y face
    s.hole(b, { diameter: 7,
      at: og.frame({ origin: [(webX + t + width - t) / 2, 0, thick / 2], normal: [0, -1, 0] }) });
    s.material(b, { color: "#a0a4a8", metalness: 0.8, roughness: 0.4 });
    return b;
  }
}

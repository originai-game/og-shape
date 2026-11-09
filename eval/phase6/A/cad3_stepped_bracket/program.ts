export default function (og) {
  const s = og.model({ name: "stepped_bracket" });
  const width = 76, depth = 53, thick = 30; // overall size, x y z
  const bracket = steppedBracket(width, depth, thick);
  s.material(bracket, { name: "steel", color: "#a0a4a8", metalness: 0.8, roughness: 0.4 });
  return s;

  // Profile in plan (x, y), extruded up z. Upper flange at the back left,
  // web along y, lower flange at the front right, lip stepping back.
  function steppedBracket(w, d, h, t = 5, webX = 27, lip = 45) {
    const profile = og.poly({ points: [
      [0, d - t], [webX, d - t], [webX, 0], [w, 0], [w, lip], [w - t, lip],
      [w - t, t], [webX + t, t], [webX + t, d], [0, d],
    ] });
    const b = s.extrude({ name: "bracket", profile, height: h });
    // countersunk hole through the upper flange, from its front face
    s.hole(b, {
      diameter: 4.5,
      countersink: [8, og.deg(90)],
      at: og.frame({ origin: [webX / 2, d - t, h / 2], normal: [0, -1, 0] }),
    });
    // plain hole through the lower flange
    s.hole(b, {
      diameter: 7,
      at: og.frame({ origin: [(webX + t + w - t) / 2, 0, h / 2], normal: [0, -1, 0] }),
    });
    return b;
  }
}

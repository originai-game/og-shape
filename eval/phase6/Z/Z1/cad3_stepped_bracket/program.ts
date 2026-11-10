export default function (og) {
  const s = og.model({ name: "stepped_bracket" });
  const W = 76, H = 53, T = 30, t = 5, webX = 31;
  const prof = og.poly({ points: [
    [0, H], [webX + t, H], [webX + t, t], [W, t], [W, 0], [webX, 0], [webX, H - t], [0, H - t],
  ] });
  const b = s.extrude({ name: "bracket", profile: prof, height: T });
  s.hole(b, { diameter: 4.5, countersink: [8, og.deg(90)],
    at: og.frame({ origin: [webX / 2, H, T / 2], normal: [0, 1, 0] }) });
  s.hole(b, { diameter: 7,
    at: og.frame({ origin: [(webX + t + W) / 2, 0, T / 2], normal: [0, -1, 0] }) });
  s.material(b, { color: "#a8abb0", metalness: 0.8, roughness: 0.4 });
  return s;
}

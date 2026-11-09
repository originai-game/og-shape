export default function (og) {
  const s = og.model({ name: "bollard" });
  const upright = (z) => og.frame({ origin: [0, 0, z], normal: [0, -1, 0], up: [0, 0, 1] });

  const base = foot();
  s.at(base, upright(0));
  const stem = shaft();
  s.at(stem, upright(18));
  return s;

  function foot(d = 314, h = 20, bevel = 3, neck_d = 192, neck_h = 28) {
    const r = d / 2, n = neck_d / 2;
    const p = og.poly({ points: [[0, 0], [r, 0], [r, h - bevel], [r - bevel, h], [n + 11, h], [n + 3, h + 4], [n, h + 14], [n, h + neck_h], [0, h + neck_h]] });
    const b = s.revolve({ name: "foot", profile: p });
    s.material(b, { color: "#66696b", metalness: 0.3, roughness: 0.6 });
    return b;
  }

  function shaft(d = 240, h = 942, bottom = 29, inner_d = 200, inner_upper = 150, rim_d = 188, recess_d = 134, recess_depth = 17, cap = 4) {
    const p = og.poly({ points: [[d / 2, bottom], [d / 2, h - 2], [d / 2 - 2, h], [rim_d / 2, h], [recess_d / 2, h - recess_depth], [0, h - recess_depth], [0, h - recess_depth - cap], [recess_d / 2, h - recess_depth - cap], [inner_upper / 2, h - 29], [inner_upper / 2, 577], [inner_d / 2, 683], [inner_d / 2, bottom]] });
    const b = s.revolve({ name: "shaft", profile: p });
    s.material(b, { color: "#66696b", metalness: 0.3, roughness: 0.6 });
    return b;
  }
}

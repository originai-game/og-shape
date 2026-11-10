export default function (og) {
  const s = og.model({ name: "curved_tubes" });
  const od = 254, wall = 12, R = 254.4, L = 22;
  const theta = Math.atan2(241.47, 448.17);
  const H = 2 * L + 2 * R;
  const c45 = Math.SQRT1_2;
  const ring = og.subtract(og.circle({ radius: od / 2, segments: 40 }), og.circle({ radius: od / 2 - wall, segments: 40 }));

  const lower = sTube("lower_tube", og.world.turn(0, 0, theta));
  const middle = sTube("middle_tube", og.world.turn(0, 0, theta + Math.PI));
  const top = sTube("top_tube", og.world.move(-2 * R * Math.cos(theta), -2 * R * Math.sin(theta), H).turn(0, 0, theta));
  const upper = s.group({ name: "upper_pair", children: [middle, top] });
  const pivot = og.world.move(2 * R * Math.cos(theta), 2 * R * Math.sin(theta), H);
  s.at(upper, pivot);
  s.joint(upper, { name: "upper_spin", parent: lower, kind: "continuous", on: pivot, axis: "+Z" });
  return s;

  // S of two 90-degree elbows in its own xz plane: up from the origin, offset 2R along +x.
  function sTube(name, on) {
    const path = [
      [0, 0, 0],
      [0, 0, L],
      { arc: { through: [R - R * c45, 0, L + R * c45], to: [R, 0, L + R] } },
      { arc: { through: [R + R * c45, 0, L + 2 * R - R * c45], to: [2 * R, 0, L + 2 * R] } },
      [2 * R, 0, 2 * L + 2 * R],
    ];
    const t = s.sweep({ name, profile: ring, path, up: [0, 1, 0], on });
    s.material(t, { color: "#9a9a9a", metalness: 0.6, roughness: 0.4 });
    return t;
  }
}

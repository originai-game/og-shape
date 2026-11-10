export default function (og) {
  const s = og.model({ name: "curved_tubes" });
  // three S tubes, each 2R tall; the swept caps add ~12 mm to the height, so R is trimmed by it
  const D = 254, wall = 8, R = (1658.35 - 11.7) / 6;
  const upperAngle = og.deg(205), lowerAngle = og.deg(25);

  const lower = sTube("lower_tube", og.world.turn(0, 0, lowerAngle));
  s.material(lower, { color: "#9a9ea3", metalness: 0.8, roughness: 0.4 });

  const top = og.world.turn(0, 0, lowerAngle).move(2 * R, 0, 2 * R);
  const pivot = og.frame({ origin: top.origin });

  const a = sTube("upper_tube_a", og.world.turn(0, 0, upperAngle));
  const b = sTube("upper_tube_b", og.world.turn(0, 0, upperAngle).move(2 * R, 0, 2 * R).turn(0, 0, og.deg(180)));
  const upper = s.group({ name: "upper_tubes", children: [a, b] });
  s.material(upper, { color: "#b4b7bb", metalness: 0.8, roughness: 0.4 });
  s.at(upper, pivot);
  s.joint(upper, { name: "upper_spin", parent: lower, kind: "continuous", on: pivot, axis: "+Z" });
  return s;

  // S tube in its own x-z plane: from the origin going up to (2R, 0, 2R) going up
  function sTube(name, on) {
    const c = Math.SQRT1_2;
    const path = [
      [0, 0, 0],
      { arc: { through: [R * (1 - c), 0, R * c], to: [R, 0, R] } },
      { arc: { through: [R * (1 + c), 0, 2 * R - R * c], to: [2 * R, 0, 2 * R] } },
    ];
    const ring = og.subtract(og.circle({ radius: D / 2, segments: 40 }), og.circle({ radius: D / 2 - wall, segments: 40 }));
    return s.sweep({ name, profile: ring, path, up: [0, 1, 0], on });
  }
}

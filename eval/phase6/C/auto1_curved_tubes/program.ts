export default function (og) {
  const s = og.model({ name: "curved_tubes" });
  // Size: 702 x 495 x 1658 mm. Three S tubes, each 553 mm tall.
  const od = 254, wall = 10;          // tube outer diameter
  const bend = 224;                    // elbow centreline radius: offset 2*bend + od = 702
  const height = 1658.35 / 3;          // each S tube
  const straight = (height - 2 * bend) / 2;
  const turnUp = og.deg(147.5);        // middle tube heads back over the lower one
  const color = "#9a9ca0";

  const lower = sTube("lower_tube");
  const top = og.world.move(2 * bend, 0, height); // top end of the lower tube: the spin axis

  const middle = sTube("middle_tube", og.world.turn(0, 0, turnUp));
  const upperEnd = og.world.turn(0, 0, turnUp).move(2 * bend, 0, height).turn(0, 0, og.deg(-180));
  const upperTop = sTube("top_tube", upperEnd);
  const upper = s.group({ name: "upper_tubes", children: [middle, upperTop] });
  s.at(upper, top);
  s.joint(upper, { name: "upper_spin", parent: lower, kind: "continuous", on: top, axis: "+Z" });
  return s;

  // An S of two 90 degree elbows in its own xz plane: from the origin up,
  // over +x by 2*bend, and up again to z = height.
  function sTube(name, on = og.world) {
    const plane = (z) => og.frame({ origin: [bend, 0, z], normal: [0, -1, 0], up: [0, 0, 1] });
    const n = 9;
    const e1 = s.ring({ n, radius: bend, start: og.deg(90), angle: og.deg(90), on: plane(straight) })
      .map((f) => f.origin).reverse();          // from 180 deg down to 90 deg
    const e2 = s.ring({ n, radius: bend, start: og.deg(270), angle: og.deg(90), on: plane(straight + 2 * bend) })
      .map((f) => f.origin).slice(1);           // 270 to 360 deg; first point shared
    const path = [[0, 0, 0], ...e1, ...e2, [2 * bend, 0, height]];
    const section = og.subtract(og.circle({ radius: od / 2, segments: 32 }), og.circle({ radius: od / 2 - wall, segments: 32 }));
    const t = s.sweep({ name, profile: section, path, up: [0, 1, 0], on });
    s.material(t, { color, roughness: 0.4, metalness: 0.6 });
    return t;
  }
}

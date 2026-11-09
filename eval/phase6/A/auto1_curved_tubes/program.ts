export default function (og) {
  const s = og.model({ name: "curved_tubes" });
  const across = 254, wall = 10, bend = 224, stub = 52.4; // pipe Ø, wall, elbow centre-line radius, straight end
  const rise = 2 * stub + 2 * bend; // height of one S tube
  const turnMid = og.deg(32.54), turnTop = og.deg(180); // headings of the upper tubes, about z

  // Lower tube: fixed, heading -X.
  const lower = sTube("lower_tube");
  const lowerAt = og.world.turn(0, 0, og.deg(180));
  s.at(lower, lowerAt);
  const pivot = og.frame({ origin: lowerAt.move(2 * bend, 0, rise).origin });

  // Upper pair: two tubes in a group, built about the pivot at its origin.
  const middle = sTube("middle_tube");
  const midAt = og.world.turn(0, 0, turnMid);
  s.at(middle, midAt);
  const top = sTube("top_tube");
  s.at(top, midAt.move(2 * bend, 0, rise).turn(0, 0, turnTop));
  const upper = s.group({ name: "upper_pair", children: [middle, top] });
  s.at(upper, pivot);
  s.joint(upper, { name: "upper_spin", parent: lower, kind: "continuous", on: pivot, axis: "+Z" });
  return s;

  // One S tube: up, elbow to +X, elbow back to up. Starts at its origin heading +z.
  function sTube(name, n = 12) {
    const path = [[0, 0, 0]];
    const c1 = og.world.move(bend, 0, stub);
    for (let i = 0; i <= n; i++) path.push(c1.turn(0, og.deg(90 * i / n), 0).move(-bend, 0, 0).origin);
    const c2 = og.world.move(bend, 0, stub + 2 * bend);
    for (let i = 1; i <= n; i++) path.push(c2.turn(0, og.deg(-90 * i / n), 0).move(0, 0, -bend).origin);
    path.push([2 * bend, 0, rise]);
    const profile = og.subtract(og.circle({ radius: across / 2, segments: 32 }), og.circle({ radius: across / 2 - wall, segments: 32 }));
    const t = s.sweep({ name, profile, path: path.map((p) => Array.isArray(p) ? p : [p.x ?? p[0], p.y ?? p[1], p.z ?? p[2]]) });
    s.material(t, { color: "#9a9a9a", metalness: 0.2, roughness: 0.5 });
    return t;
  }
}

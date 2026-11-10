export default function (og) {
  const s = og.model({ name: "curved_tubes" });
  const dia = 254, wall = 10;          // tube outside diameter, wall
  const height = 1658.35;              // whole stack
  const H = height / 3;                // one S tube, end to end
  const offset = 448;                  // sideways shift of one S (centre to centre)
  const bend = 220;                    // elbow bend radius
  const a2 = og.deg(147.4), a3 = og.deg(-32.6); // headings of the upper tubes at rest

  const lower = sTube("lower_tube", og.world);
  s.material(lower, { color: "#9a9ea3", metalness: 0.8, roughness: 0.4 });

  const mid = sTube("middle_tube", og.world.turn(0, 0, a2));
  const top = sTube("upper_tube", og.world.move(offset * Math.cos(a2), offset * Math.sin(a2), H).turn(0, 0, a3));
  const upper = s.group({ name: "upper_tubes", children: [mid, top] });
  s.material(upper, { color: "#b0b4b8", metalness: 0.8, roughness: 0.4 });

  const pivot = og.world.move(offset, 0, H);
  s.at(upper, pivot);
  s.joint(upper, { name: "upper_swivel", parent: lower, kind: "continuous", on: pivot, axis: "+Z" });
  return s;

  // An S tube: up, two 90 degree elbows shifting it `offset` along its +x, up again.
  function sTube(name, on) {
    const half = H / 2;
    const path = (e) => [[0, 0, -e], [0, 0, half], [offset, 0, half], [offset, 0, H + e]];
    const t = s.wire({ name, path: path(0), diameter: dia, fillet: bend, segments: 32, on });
    const bore = s.wire({ name: name + "_bore", path: path(2), diameter: dia - 2 * wall, fillet: bend, segments: 32, on });
    s.cut(t, bore);
    return t;
  }
}

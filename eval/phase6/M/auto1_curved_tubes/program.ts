export default function (og) {
  const s = og.model({ name: "curved_tubes" });
  const width = 702.17, depth = 495.47, height = 1658.35;
  const dia = 254, wall = 12, bend = 160;
  const h = height / 3;            // one S tube, end to end
  const runX = width - dia;        // centreline span in x
  const runY = depth - dia;        // centreline span in y

  const lower = sTube("lower_tube", runX, 0);
  s.material(lower, { color: "#9a9a9a", metalness: 0.6, roughness: 0.4 });

  const top = og.frame({ origin: [runX, 0, h] });
  const upper = upperPair();
  s.at(upper, top);
  s.joint(upper, { name: "upper_spin", parent: lower, kind: "continuous", on: top, axis: "+Z" });
  return s;

  // An S tube built about its own origin: enters going up at the origin,
  // leaves going up at (dx, dy, h); two bends joined by a horizontal run.
  function sTube(name, dx, dy, on = og.world) {
    const path = [[0, 0, 0], [0, 0, h / 2], [dx, dy, h / 2], [dx, dy, h]];
    const pipe = s.wire({ name, path, diameter: dia, fillet: bend, on });
    const borePath = [[0, 0, -1], [0, 0, h / 2], [dx, dy, h / 2], [dx, dy, h + 1]];
    const bore = s.wire({ name: name + "_bore", path: borePath, diameter: dia - 2 * wall, fillet: bend, on });
    s.cut(pipe, bore);
    return pipe;
  }

  // The two upper tubes, stacked, about the spin axis at the origin.
  function upperPair() {
    const a = sTube("middle_tube", -runX / 2, runY);
    const b = sTube("top_tube", -runX / 2, -runY, og.world.move(-runX / 2, runY, h));
    s.material([a, b], { color: "#9a9a9a", metalness: 0.6, roughness: 0.4 });
    return s.group({ name: "upper_pair", children: [a, b] });
  }
}

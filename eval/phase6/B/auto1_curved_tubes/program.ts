export default function (og) {
  const s = og.model({ name: "curved_tubes" });
  const od = 254, wall = 15;           // tube 254 mm across
  const bend = 224;                    // elbow centreline radius
  const leg = 52.4;                    // straight leg at each end
  const rise = 2 * leg + 2 * bend;     // height of one S tube
  const offset = 2 * bend;             // sideways offset of one S tube
  const midTurn = og.deg(147.5), topTurn = og.deg(-32.5);

  const lower = sTube("lower_tube", og.world);
  s.material(lower, { color: "#8a8d91", metalness: 0.6, roughness: 0.4 });

  const upper = upperPair();
  const spin = og.frame({ origin: [offset, 0, rise] }); // top end of the lower tube
  s.at(upper, spin);
  s.joint(upper, { name: "upper_spin", parent: lower, kind: "continuous", on: spin, axis: "+Z" });
  return s;

  // Built about its own origin: the spin axis is its z axis.
  function upperPair() {
    const mid = sTube("middle_tube", og.world.turn(0, 0, midTurn));
    const d = og.world.turn(0, 0, midTurn).move(offset, 0, rise);
    const top = sTube("top_tube", og.frame({ origin: d.origin }).turn(0, 0, topTurn));
    const g = s.group({ name: "upper_pair", children: [mid, top] });
    s.material(g, { color: "#8a8d91", metalness: 0.6, roughness: 0.4 });
    return g;
  }

  // An S of two 90 degree elbows, from the origin up +z, offset along +x.
  function sTube(name, on, n = 12) {
    const path = [[0, 0, 0], [0, 0, leg]];
    for (let i = 1; i <= n; i++) { // first elbow: +z to +x, centre (bend, leg)
      const a = (i / n) * Math.PI / 2;
      path.push([bend - bend * Math.cos(a), 0, leg + bend * Math.sin(a)]);
    }
    for (let i = 1; i <= n; i++) { // second elbow: +x to +z, centre (bend, leg + 2 bend)
      const a = (i / n) * Math.PI / 2;
      path.push([bend + bend * Math.sin(a), 0, leg + 2 * bend - bend * Math.cos(a)]);
    }
    path.push([offset, 0, rise]);
    const profile = og.subtract(og.circle({ radius: od / 2, segments: 32 }), og.circle({ radius: od / 2 - wall, segments: 32 }));
    return s.sweep({ name, profile, path, on });
  }
}

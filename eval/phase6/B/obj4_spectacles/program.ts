export default function (og) {
  const s = og.model({ name: "spectacles" });
  // overall 140 x 12.5 x 26.8
  const lensW = 56, lensH = 24, gap = 18;       // gap = bridge span between lenses
  const cx = gap / 2 + lensW / 2;               // lens centre x
  const R = 100;                                // lens curvature radius (front at y = 0)
  const browH = 3, browTop = lensH + 2.77;
  const hingeX = cx + lensW / 2 + 1.5, hingeZ = 24;
  const armY = [9.8, 11.4];                     // right, left arm centre y (staggered so they fold flat)
  const armL = 128;

  const lensR = lens("lens_right", cx);
  const lensL = lens("lens_left", -cx);
  const frame = frontFrame();
  const padR = nosePad("nose_pad_right", 1);
  const padL = nosePad("nose_pad_left", -1);

  const hingeR = og.frame({ origin: [hingeX, armY[0], hingeZ] });
  const armR = arm("temple_right", 1);
  s.at(armR, hingeR);
  s.joint(armR, { name: "right_hinge", parent: frame, on: hingeR, axis: "-Z", range: [0, og.deg(95)] });

  const hingeL = og.frame({ origin: [-hingeX, armY[1], hingeZ] });
  const armL_ = arm("temple_left", -1);
  s.at(armL_, hingeL);
  s.joint(armL_, { name: "left_hinge", parent: frame, on: hingeL, axis: "+Z", range: [0, og.deg(95)] });
  return s;

  // A curved lens: a rounded slab cut by a cylindrical shell of radius R.
  function lens(name, x, t = 2) {
    const slab = s.extrude({ name, profile: og.rect({ width: lensW, height: lensH, radius: 8 }), height: 14, bothSides: true,
      on: og.frame({ origin: [x, 4, lensH / 2], normal: [0, -1, 0], up: [0, 0, 1] }) });
    const shellT = s.tube({ name: name + "_curve", outer: R, inner: R - t, height: 40, on: og.world.move(x, R, -8) });
    s.intersect(slab, shellT);
    s.material(slab, { color: "#3a3d42", roughness: 0.1, metalness: 0.2, opacity: 0.6 });
    return slab;
  }

  // Brow bars, bridge and hinge blocks fused into one black frame.
  function frontFrame() {
    const bl = brow("frame", -cx), br = brow("brow_right", cx);
    const bridge = s.sweep({ name: "bridge", profile: og.circle({ radius: 0.8, segments: 12 }),
      path: [[-cx + lensW / 2 - 2, 5, 25], [-5, 4, 23], [5, 4, 23], [cx - lensW / 2 + 2, 5, 25]], up: [0, 0, 1] });
    s.join(bl, bridge);
    s.join(bl, br);
    for (const [sx, d] of [[1, armY[0] - 0.8 - 3.5], [-1, armY[1] - 0.8 - 3.5]]) {
      const blk = s.box({ name: sx > 0 ? "hinge_block_right" : "hinge_block_left", width: 5, depth: d, height: 4,
        on: og.world.move(sx * (hingeX + 1), 3.5 + d / 2, hingeZ - 2) });
      s.join(bl, blk);
    }
    s.material(bl, { color: "#111214", roughness: 0.4 });
    return bl;
  }

  function brow(name, x) {
    const b = s.box({ name, width: lensW + 2, depth: 14, height: browH, on: og.world.move(x, 3, browTop - browH) });
    const c = s.tube({ name: name + "_curve", outer: R + 0.3, inner: R - 3, height: 60, on: og.world.move(x, R, -10) });
    s.intersect(b, c);
    return b;
  }

  // A pad on a thin wire from the bridge end, behind the lens.
  function nosePad(name, sx) {
    const x0 = sx * (gap / 2 + 0.5);
    const wire = s.sweep({ name, profile: og.circle({ radius: 0.4, segments: 8 }),
      path: [[x0, 5, 25], [sx * (gap / 2 + 1), 7.5, 17], [sx * (gap / 2 + 2), 8.5, 12]], up: [0, 0, 1] });
    const pad = s.extrude({ name: name + "_pad", profile: og.ellipse({ rx: 2.5, ry: 4 }), height: 1, bothSides: true,
      on: og.frame({ origin: [sx * (gap / 2 + 2), 8.5, 11], normal: [-sx, 0.4, 0], up: [0, 0, 1] }) });
    s.join(wire, pad);
    s.material(wire, { color: "#c8ccd0", metalness: 1, roughness: 0.3 });
    return wire;
  }

  // Temple arm built about its hinge, folded: it runs toward -sx*X, with an ear bend at the tip.
  function arm(name, sx, w = 1.6, h = 3) {
    const bar = s.sweep({ name: name + "_bar", profile: og.rect({ width: w, height: h }),
      path: [[0, 0, 0], [-sx * (armL - 28), 0, -1.5], [-sx * (armL - 10), 0, -4], [-sx * armL, 0, -8]], up: [0, 0, 1] });
    s.material(bar, { color: "#111214", roughness: 0.4 });
    return s.group({ name, children: [bar] });
  }
}

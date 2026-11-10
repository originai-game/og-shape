export default function (og) {
  const s = og.model({ name: "spectacles" });
  const lensHalfW = 28, lensH = 23, lensCx = 38, lensT = 1.5, lensR = 150;
  const browW = 59, browD = 4, browH = 4.5, browTop = 26.77, browY = 1.52;
  const hingeX = 68, hingeW = 4;
  const armL = 125, armTilt = og.deg(9), armT = 2;
  const armY = [7, 11]; // left, right arm mid-planes
  const hingeZ = browTop - browH / 2;

  const lensL = lens("lens_left", -lensCx);
  const lensRt = lens("lens_right", lensCx);
  const fr = frontFrame();
  const pads = nosePads();
  const armLeft = temple("temple_left", -1, armY[0]);
  const armRight = temple("temple_right", 1, armY[1]);
  return s;

  function lens(name, cx) {
    const prof = og.poly({ points: [
      { x: -lensHalfW, y: lensH / 2, radius: 2 },
      { x: lensHalfW, y: lensH / 2, radius: 2 },
      { x: lensHalfW - 1, y: -3, radius: 8 },
      { x: lensHalfW - 10, y: -lensH / 2, radius: 6 },
      { x: -lensHalfW + 10, y: -lensH / 2, radius: 6 },
      { x: -lensHalfW + 1, y: -3, radius: 8 },
    ] });
    const p = s.extrude({ name, profile: prof, height: 12, bothSides: true,
      on: og.frame({ origin: [cx, 3, lensH / 2], normal: [0, -1, 0], up: [0, 0, 1] }) });
    const shell = s.tube({ name: name + "_curve", outer: lensR, inner: lensR - lensT, height: 40,
      segments: 360, on: og.world.move(cx, lensR, -5) });
    s.intersect(p, shell);
    s.material(p, { color: "#2a2e33", metalness: 0.1, roughness: 0.05, opacity: 0.7 });
    return p;
  }

  function frontFrame() {
    const browL = s.box({ name: "frame", width: browW, depth: browD, height: browH,
      on: og.world.move(-(9 + browW / 2), browY, browTop - browH) });
    const browR = s.box({ name: "brow_right", width: browW, depth: browD, height: browH,
      on: og.world.move(9 + browW / 2, browY, browTop - browH) });
    s.fillet(browL, { radius: 1, edges: "side" });
    s.fillet(browR, { radius: 1, edges: "side" });
    const hL = hingeBlock("hinge_left", -1, armY[0]);
    const hR = hingeBlock("hinge_right", 1, armY[1]);
    const bridge = s.wire({ name: "bridge", diameter: 1.6, fillet: 3, segments: 12,
      path: [[-10, browY, 23.5], [-6, browY, 25.5], [6, browY, 25.5], [10, browY, 23.5]] });
    s.join(browL, browR);
    s.join(browL, hL);
    s.join(browL, hR);
    s.join(browL, bridge);
    s.material(browL, { color: "#151515", roughness: 0.4 });
    return browL;
  }

  function hingeBlock(name, side, ay) {
    const y0 = browY - browD / 2 + 0.5, y1 = ay + armT / 2 - 0.2;
    return s.box({ name, width: hingeW, depth: y1 - y0, height: browH + 0.6,
      on: og.world.move(side * (hingeX), (y0 + y1) / 2, browTop - browH - 0.6) });
  }

  function nosePads() {
    const parts = [];
    for (const side of [-1, 1]) {
      const w = s.wire({ name: side < 0 ? "pad_arm_left" : "pad_arm_right", diameter: 0.8, fillet: 1.5, segments: 8,
        path: [[side * 10, 2, 23.5], [side * 9.5, 3.5, 17], [side * 8.2, 4.4, 10]] });
      s.material(w, { color: "#b8bcc2", metalness: 1, roughness: 0.3 });
      const pad = s.extrude({ name: side < 0 ? "pad_left" : "pad_right", profile: og.ellipse({ rx: 2.2, ry: 4.5 }), height: 1.2,
        on: og.frame({ origin: [side * 7.8, 4.6, 9.5], normal: [-side, 0.2, 0], up: [0, 0, 1] }) });
      s.material(pad, { color: "#d8d8d0", roughness: 0.3, opacity: 0.8 });
      parts.push(w, pad);
    }
    return parts;
  }

  // dir: -1 left (hinge at -X, arm folded toward +X), +1 right
  function temple(name, dir, ay) {
    const u = -dir; // arm runs along u * X
    const drop = Math.tan(armTilt);
    const pts = [
      [0, 2.2],
      [u * armL * 0.7, 1.6 - drop * armL * 0.7],
      [u * armL, 1.2 - drop * armL],
      [u * (armL + 1.5), -drop * armL],
      [u * armL, -1.2 - drop * armL],
      [u * armL * 0.7, -1.6 - drop * armL * 0.7],
      [0, -2.2],
    ];
    const arm = s.extrude({ name: name + "_bar", profile: og.poly({ points: dir < 0 ? pts : pts.slice().reverse() }), height: armT,
      on: og.frame({ origin: [0, armT / 2, 0], normal: [0, -1, 0], up: [0, 0, 1] }) });
    s.material(arm, { color: "#151515", roughness: 0.4 });
    const g = s.group({ name, children: [arm] });
    const f = og.frame({ origin: [dir * hingeX, ay, hingeZ] });
    s.at(g, f);
    s.joint(g, { name: name + "_hinge", parent: fr, on: f, axis: dir < 0 ? "+Z" : "-Z", range: [0, og.deg(90)] });
    return g;
  }
}

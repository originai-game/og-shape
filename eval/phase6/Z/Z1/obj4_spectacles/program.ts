export default function (og) {
  const s = og.model({ name: "spectacles" });
  const lensW = 54, lensH = 25, lensCx = 35, R = 120, lensT = 1.5;
  const browW = 60, browH = 3.2, browT = 2.5, topZ = 26.77;
  const hingeX = 67, hingeW = 6;
  const armLen = 124, armT = 1.5, depthY = 12.48;
  const frameColor = { color: "#151515", roughness: 0.4 };
  const armYR = 8.0, armYL = depthY - armT;

  const lensR = lens(1, "lens_right");
  const lensL = lens(-1, "lens_left");
  const browR = brow(1, "brow_right", armYR);
  const browL = brow(-1, "brow_left", armYL);
  const bridge = s.wire({ name: "bridge", path: [[-6, 2.2, 23.5], { arc: { through: [0, 2.2, 25.2], to: [6, 2.2, 23.5] } }], diameter: 1.4, segments: 12 });
  s.material(bridge, { color: "#202020", metalness: 0.8, roughness: 0.3 });
  nosePad(1, "nose_pad_right");
  nosePad(-1, "nose_pad_left");

  const fR = og.frame({ origin: [hingeX, armYR, 24] });
  const armR = arm(1, "temple_right");
  s.at(armR, fR);
  s.joint(armR, { name: "temple_right_hinge", parent: browR, on: fR, axis: "-Z", range: [0, og.deg(95)] });
  const fL = og.frame({ origin: [-hingeX, armYL, 24] });
  const armL = arm(-1, "temple_left");
  s.at(armL, fL);
  s.joint(armL, { name: "temple_left_hinge", parent: browL, on: fL, axis: "+Z", range: [0, og.deg(95)] });
  return s;

  // a slab cut to a cylinder shell of radius R (axis z), its front at y = y0
  function curved(name, sign, h, z0, t, y0, profile) {
    const back = og.frame({ origin: [sign * lensCx, -2, z0 + h / 2], normal: [0, 1, 0] });
    const slab = s.extrude({ name, profile, height: 12, on: back });
    const tool = s.tube({ name: name + "_curve", outer: R, inner: R - t, height: h + 4, segments: 256, on: og.world.move(sign * lensCx, R + y0, z0 - 2) });
    s.intersect(slab, tool);
    return slab;
  }

  function lens(sign, name) {
    const l = curved(name, sign, lensH, 0, lensT, 0.5, og.rect({ width: lensW, height: lensH, radius: 9 }));
    s.material(l, { color: "#3a3f44", metalness: 0.1, roughness: 0.05, opacity: 0.55 });
    return l;
  }

  function brow(sign, name, armY) {
    const b = curved(name, sign, browH, topZ - browH, browT, 0, og.rect({ width: browW, height: browH, radius: 1 }));
    const yBack = armY + armT;
    const block = s.box({ name: name + "_hinge", width: hingeW, depth: yBack - 2.5, height: 5, on: og.world.move(sign * (70 - hingeW / 2), (yBack + 2.5) / 2, topZ - 5) });
    s.join(b, block);
    s.material(b, frameColor);
    return b;
  }

  function nosePad(sign, name) {
    const support = s.wire({ name: name + "_support", path: [[sign * 9, 2.5, 13], [sign * 7, 6, 11], [sign * 5.5, 7.5, 8]], diameter: 0.9, fillet: 1, segments: 10 });
    s.material(support, { color: "#b8b8b8", metalness: 1, roughness: 0.3 });
    const pad = s.sphere({ name, radius: 2.6, segments: 16, on: og.world.move(sign * 5.5, 7.8, 6.5) });
    s.scale(pad, { x: 0.6, y: 0.35, z: 1 });
    s.material(pad, { color: "#d8d8d0", roughness: 0.3, opacity: 0.8 });
    return s.group({ name: name + "_assembly", children: [support, pad] });
  }

  // folded temple, built about its hinge; runs inward and slopes down
  function arm(sign, name) {
    const pts = [[0, 2], [armLen, -17.5], { x: armLen, y: -20.5, radius: 1.2 }, [0, -3]].map(p => Array.isArray(p) ? [sign * p[0], p[1]] : { ...p, x: sign * p.x });
    const a = s.extrude({ name, profile: og.poly({ points: pts }), height: armT, on: og.frame({ origin: [0, 0, 0], normal: [0, 1, 0] }) });
    s.material(a, frameColor);
    return s.group({ name: name + "_group", children: [a] });
  }
}

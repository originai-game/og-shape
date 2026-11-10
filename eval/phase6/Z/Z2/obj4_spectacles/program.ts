export default function (og) {
  const s = og.model({ name: "spectacles" });
  const lensW = 58, lensH = 24, lensCx = 34, lensCz = 12.5;
  const sphereR = 100, lensFront = -3;
  const pivotX = 67.5, pivotZ = 22.5;
  const templeL = 124, templeTilt = og.deg(7);
  const black = { color: "#1b1b1d", roughness: 0.4, metalness: 0.2 };

  const lensP = og.poly({ points: [
    { x: -lensW / 2, y: lensH / 2, radius: 4 }, { x: lensW / 2, y: lensH / 2, radius: 4 },
    { x: lensW / 2 - 1, y: -3, radius: 6 }, { x: 18, y: -lensH / 2, radius: 6 },
    { x: -18, y: -lensH / 2, radius: 6 }, { x: -lensW / 2 + 1, y: -3, radius: 6 },
  ] });

  const parts = [];
  for (const side of [-1, 1]) {
    const tag = side < 0 ? "left" : "right";
    parts.push(lens(side * lensCx, tag), brow(side * lensCx, tag), endpiece(side, tag), nosePad(side, tag));
  }
  parts.push(bridge());
  const front = s.group({ name: "front", children: parts });

  // At rest the temples are folded behind the lenses, each sloping down so they cross.
  const tR = temple("right_temple", -1);
  const fR = og.frame({ origin: [pivotX, 4.6, pivotZ] }).turn(0, -templeTilt, 0);
  s.at(tR, fR);
  s.joint(tR, { name: "right_hinge", parent: front, on: fR, axis: "-Z", range: [0, og.deg(95)] });
  const tL = temple("left_temple", 1);
  const fL = og.frame({ origin: [-pivotX, 6.6, pivotZ] }).turn(0, templeTilt, 0);
  s.at(tL, fL);
  s.joint(tL, { name: "left_hinge", parent: front, on: fL, axis: "+Z", range: [0, og.deg(95)] });
  return s;

  function lensSphere(cx, r, name) {
    return s.sphere({ name, radius: r, segments: 96, on: og.world.move(cx, lensFront - (r - sphereR) + r, lensCz) });
  }

  function lens(cx, tag) {
    const l = s.extrude({ name: tag + "_lens", profile: lensP, height: 5,
      on: og.frame({ origin: [cx, 2, lensCz], normal: [0, -1, 0], up: [0, 0, 1] }) });
    s.intersect(l, lensSphere(cx, sphereR, tag + "_lens_sphere"));
    s.material(l, { color: "#3a3f44", roughness: 0.05, metalness: 0.1, opacity: 0.75 });
    return l;
  }

  function brow(cx, tag) {
    const band = og.intersect(og.offset({ profile: lensP, distance: 1.2 }),
      og.translate({ profile: og.rect({ width: lensW + 10, height: 4.5 }), y: lensH / 2 - 1 }));
    const b = s.extrude({ name: tag + "_brow", profile: band, height: 7,
      on: og.frame({ origin: [cx, 3, lensCz], normal: [0, -1, 0], up: [0, 0, 1] }) });
    s.intersect(b, lensSphere(cx, sphereR + 1, tag + "_brow_sphere"));
    s.material(b, black);
    return b;
  }

  function endpiece(side, tag) {
    const e = s.box({ name: tag + "_endpiece", width: 7, depth: 9, height: 5,
      on: og.world.move(side * 66.5, 3, lensCz + lensH / 2 - 2.5) });
    s.fillet(e, { radius: 0.8 });
    s.material(e, black);
    return e;
  }

  function bridge() {
    const w = s.wire({ name: "bridge", diameter: 1.6, segments: 12, path: [
      [-7, 1.5, 21], { arc: { through: [0, 0.5, 23], to: [7, 1.5, 21] } }] });
    s.material(w, black);
    return w;
  }

  function nosePad(side, tag) {
    const arm = s.wire({ name: tag + "_pad_arm", diameter: 0.8, segments: 8, path: [
      [side * 6, 1.5, 19], [side * 7, 4.5, 14], [side * 8.5, 5.5, 9]], fillet: 1.5 });
    s.material(arm, { color: "#9a9ca0", metalness: 1, roughness: 0.3 });
    const pad = s.sphere({ name: tag + "_pad", radius: 3, segments: 16,
      on: og.frame({ origin: [side * 9, 6.2, 8], normal: [side * 0.6, 1, 0] }) });
    s.scale(pad, { z: 0.3, x: 0.6 });
    s.material(pad, { color: "#d8d8d0", roughness: 0.3, opacity: 0.8 });
    return s.group({ name: tag + "_nose_pad", children: [arm, pad] });
  }

  // Built about its pivot; dir -1 runs toward -X (right temple folded), +1 toward +X.
  function temple(name, dir) {
    const pts = [[1, 1.6], [templeL * 0.8, 0.8], [templeL, 0.2], [templeL, -1.6], [templeL * 0.8, -1.4], [1, -1.6]];
    const prof = og.poly({ points: pts.map(([x, z], i) => ({ x: -dir * x, y: z, radius: i === 2 || i === 3 ? 0.5 : 0 })) });
    const t = s.extrude({ name: name + "_arm", profile: prof, height: 1.6, bothSides: true,
      on: og.frame({ origin: [0, 0, 0], normal: [0, 1, 0], up: [0, 0, 1] }) });
    s.material(t, black);
    return s.group({ name, children: [t] });
  }
}

export default function (og) {
  const s = og.model({ name: "spectacles" });

  // Size at rest: 140 x 12.48 x 26.77 (x, y, z). Front looks -Y.
  const halfWidth = 70;          // outer face of each end piece = hinge line
  const lensX = 32;              // lens centre, each side
  const lensW = 52, lensH = 23, lensT = 2;
  const curveR = 120;            // lens / brow curvature radius (axis z)
  const browW = 54, browZ = 22.5, browH = 4.27; // top at 26.77
  const hingeZ = browZ - 1 + (browH + 1) / 2;
  const armT = 2, armLen = 121;
  const leftHingeY = 7, rightHingeY = 9.48; // right arm folds behind the left
  const black = { color: "#1b1b1d", roughness: 0.4 };
  const mirrorX = og.frame({ origin: [0, 0, 0], normal: [1, 0, 0] });

  // Frame front: brows, end pieces, bridge
  const frame = brow("frame", 1);
  const browL = brow("brow_l", -1);
  s.join(frame, browL);
  s.join(frame, endPiece("end_r", 1, rightHingeY));
  s.join(frame, endPiece("end_l", -1, leftHingeY));
  s.join(frame, s.wire({ name: "bridge", path: [[-7, 3.5, 24], [0, 3.5, 20.5], [7, 3.5, 24]], diameter: 1.8, fillet: 4 }));
  s.material(frame, black);

  // Lenses
  const lensR = lens("lens_r");
  const lensL = s.mirror(lensR, { plane: mirrorX, name: "lens_l" });

  // Nose pads on wire supports
  const padArmR = s.wire({ name: "pad_arm_r", path: [[7, 4, 23.5], [7.5, 5, 14], [9, 8, 9]], diameter: 1, fillet: 1.5 });
  s.material(padArmR, { color: "#c8c8cc", metalness: 1, roughness: 0.3 });
  const padArmL = s.mirror(padArmR, { plane: mirrorX, name: "pad_arm_l" });
  const padR = s.sphere({ name: "pad_r", radius: 3, segments: 16, on: og.world.move(9.5, 8.6, 8.5) });
  s.scale(padR, { x: 0.7, y: 0.35, z: 1.3 });
  s.material(padR, { color: "#e4e4e0", roughness: 0.3, opacity: 0.7 });
  const padL = s.mirror(padR, { plane: mirrorX, name: "pad_l" });

  // Temple arms, folded at rest
  const fR = og.frame({ origin: [halfWidth, rightHingeY, hingeZ] });
  const armR = arm("temple_r", 1);
  s.at(armR, fR);
  s.joint(armR, { name: "temple_r_hinge", parent: frame, on: fR, axis: "-Z", range: [0, og.deg(95)] });

  const fL = og.frame({ origin: [-halfWidth, leftHingeY, hingeZ] });
  const armL = arm("temple_l", -1);
  s.at(armL, fL);
  s.joint(armL, { name: "temple_l_hinge", parent: frame, on: fL, axis: "+Z", range: [0, og.deg(95)] });

  return s;

  // A curved band: a slab cut by a thick big-radius tube, front bowed toward -Y.
  function curved(slab, tag, front, back, x) {
    const band = s.tube({ name: tag + "_curve", outer: curveR + front, inner: curveR - back, height: 60, segments: 256, on: og.world.move(x, curveR, -10) });
    s.intersect(slab, band);
    return slab;
  }

  function brow(name, side) {
    const b = s.box({ name, width: browW, depth: 10, height: browH - 0.6, on: og.world.move(side * lensX, 2, browZ) });
    return curved(b, name, 1, 3, side * lensX);
  }

  function lens(name) {
    const profile = og.poly({ points: [
      { x: -lensW / 2, y: lensH / 2, radius: 3 },
      { x: lensW / 2, y: lensH / 2, radius: 3 },
      { x: lensW / 2 - 3, y: -lensH / 2, radius: 9 },
      { x: -lensW / 2 + 3, y: -lensH / 2, radius: 9 },
    ] });
    const slab = s.extrude({ name, profile, height: 20, bothSides: true, on: og.frame({ origin: [lensX, 2, lensH / 2], normal: [0, 1, 0] }) });
    curved(slab, name, 0, lensT, lensX);
    s.material(slab, { color: "#2e3236", roughness: 0.1, metalness: 0.2, opacity: 0.6 });
    return slab;
  }

  // side: +1 right, -1 left. Reaches from just inside the brow back to the hinge.
  function endPiece(name, side, backY) {
    const x0 = lensX + browW / 2 - 2; // overlaps the brow by 2
    const w = halfWidth - x0, front = 1.5;
    return s.box({ name, width: w, depth: backY - front, height: browH + 1, on: og.world.move(side * (x0 + w / 2), (front + backY) / 2, browZ - 1) });
  }

  // Built about its hinge: pivot at the origin, the arm folded along -X (right) or +X (left),
  // lying behind the pivot in +y.
  function arm(name, side) {
    const profile = og.poly({ points: [
      [0, 2.2],
      { x: 100, y: 1.6, radius: 6 },
      { x: armLen - 3, y: -3, radius: 1 },
      { x: armLen, y: -5.5, radius: 0.5 },
      { x: armLen - 2, y: -6, radius: 0.5 },
      { x: 104, y: -1.4, radius: 6 },
      [0, -2.2],
    ] });
    const bar = s.extrude({ name: name + "_bar", profile, height: armT, bothSides: true,
      on: og.frame({ origin: [0, armT / 2, 0], normal: [0, side, 0] }) });
    s.material(bar, black);
    return s.group({ name, children: [bar] });
  }
}

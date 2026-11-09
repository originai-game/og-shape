export default function (og) {
  const s = og.model({ name: "spectacles" });
  // Right side (+x): lens group origin, as in the source program.
  const lensAt = [32, 0, -12.5];
  const browAt = [lensAt[0] + 1, lensAt[1] + 0.5, lensAt[2] + 20.5];
  const browW = 52;
  const pivot = og.frame({ origin: [browAt[0] + browW / 2 - 1, browAt[1] + 1, browAt[2]] });

  const bridgePart = bridge();
  const lensPart = lens();
  const browPart = brow(browW);
  const [pad, support] = nosePad();
  const right = s.group({ name: "right_rim", children: [lensPart, browPart, pad, support] });
  const arm = foldedArm();
  s.at(arm, pivot);
  s.joint(arm, { name: "temple_fold_right", parent: browPart, on: pivot, axis: "+Z", range: [og.deg(-8), 0] });

  // Left side: the mirror image of the right one.
  const left = s.mirror(right, { plane: og.frame({ origin: [0, 0, 0], normal: [1, 0, 0] }), name: "left_rim" });
  const armL = s.mirror(arm, { plane: og.frame({ origin: [0, 0, 0], normal: [1, 0, 0] }), name: "left_arm" });
  const pivotL = og.frame({ origin: [-pivot.origin[0], pivot.origin[1], pivot.origin[2]] });
  s.joint(armL, { name: "temple_fold_left", parent: left, on: pivotL, axis: "-Z", range: [og.deg(-8), 0] });
  return s;

  // A rectangle w x d centred on (x, y) in a loft section.
  function rectAt(w, d, x = 0, y = 0) {
    return og.poly({ points: [[x - w / 2, y - d / 2], [x + w / 2, y - d / 2], [x + w / 2, y + d / 2], [x - w / 2, y + d / 2]] });
  }

  // A round wire of diameter d through points.
  function wire(name, points, d, on) {
    return s.sweep({ name, profile: og.circle({ radius: d / 2, segments: 12 }), path: points, on });
  }

  function bridge(w = 18, d = 1.5, h = 5) {
    const b = wire("bridge", [[-w / 2, 0, h / 2 + 6], [0, -0.8, h + 6], [w / 2, 0, h / 2 + 6]], d);
    s.material(b, { name: "plastic", color: "#131313" });
    return b;
  }

  // Lofted along its own z (the lens width), turned so z runs along world x.
  function lens(w = 48, d = 1.4, h = 26) {
    const st = [[-w / 2, 3, -22, 1.2], [-20, 20, -14, 0.6], [-12, 25, -h / 2, 0], [0, h, -h / 2, -0.8],
      [12, 25.5, -h / 2, -0.3], [20, 24, -14, 0.4], [w / 2, 4, -22, 1.2]];
    const l = s.loft({ name: "lens", sections: st.map(([z, sw, x, y]) => ({ profile: rectAt(sw, d, x, y), z })),
      on: og.world.move(...lensAt).turn(0, og.deg(90), 0) });
    s.material(l, { name: "glass", color: "#ffffff", roughness: 0.35 });
    return l;
  }

  function brow(w, d = 2, h = 6) {
    const [x0, y0, z0] = browAt;
    const p = (x, y, z) => [x0 + x, y0 + y, z0 + z];
    const b = wire("brow", [p(-w / 2, 0, 2), p(-20, -0.6, 4), p(0, -1.3, 5), p(20, -0.5, 5), p(w / 2, 1, 4)], d);
    const hinge = s.box({ name: "hinge", width: w / 17, depth: d * 2, height: h, on: og.world.move(...p(w / 2 - 1, 1, 0)) });
    s.join(b, hinge);
    s.material(b, { name: "plastic", color: "#131313" });
    return b;
  }

  // Built about its hinge pivot.
  function foldedArm(w = 130, t = 1.2) {
    const st = [[-w / 2, 1, -2, -4], [-57, 4, -4, -3], [-40, 6, -9, 0], [0, 6, -12, 2], [40, 6, -12, -1], [w / 2 - 3, 6, -12, -5]];
    const a = s.loft({ name: "arm", sections: st.map(([z, sw, x, y]) => ({ profile: rectAt(sw, t, x, y), z })),
      on: og.world.move(-38 - (browW / 2 - 1), 7.5 - 1, -9).turn(0, og.deg(90), 0) });
    s.material(a, { name: "plastic_grey", color: "#525252" });
    return s.group({ name: "right_arm", children: [a] });
  }

  function nosePad(w = 2.5, d = 2.5, h = 6) {
    const o = [lensAt[0] - 24, lensAt[1] + 7, lensAt[2] + 8];
    const pad = s.loft({ name: "pad", sections: [
      { profile: og.ellipse({ rx: w / 4, ry: d / 4 }), z: 0 },
      { profile: og.ellipse({ rx: w / 2, ry: d / 2 }), z: h / 2 },
      { profile: og.ellipse({ rx: w / 4, ry: d / 4 }), z: h }], on: og.world.move(...o) });
    const sup = wire("support", [[o[0], o[1], o[2] + h], [o[0], o[1] - 3, o[2] + h + 2], [o[0] + 1, o[1] - 6, o[2] + h + 5]], 1);
    s.material(pad, { name: "pad", color: "#ffffff" });
    s.material(sup, { name: "steel", color: "#525252", metalness: 1 });
    return [pad, sup];
  }
}

export default function (og) {
  const s = og.model({ name: "hinged_straps" });
  const length = 914.4, width = 5.72, height = 40.96;
  const curlRadius = width / 2;
  const pinZ = height - curlRadius; // 38.1: curl axis height when the fixed strap stands on z = 0

  const fixed = strap("fixed_strap");
  s.at(fixed, og.frame({ origin: [0, 0, pinZ] }));

  // Same strap turned half a turn about Y: plate up, curl down, on the same pin.
  const pin = og.frame({ origin: [0, 0, pinZ], normal: [0, 0, -1], up: [0, 1, 0] });
  const pivoting = strap("pivoting_strap");
  s.at(pivoting, pin);
  s.joint(pivoting, { name: "pivoting_strap_spin", parent: fixed, kind: "continuous", on: pin, axis: "+X" });
  return s;

  // Built about its own origin: the curl (pin) axis runs along x at y = z = 0,
  // the plate hangs below it on the +y side.
  function strap(name, l = length, w = width, h = height, holeD = 3, holes = 7) {
    const r = w / 2;
    const wall = r - 1.585;
    const plateH = h - r; // from the bottom edge up to the curl axis
    const tuck = 0.5; // the plate's top stops just under the curl axis, inside the curl wall
    const plate = s.box({ name, width: l, depth: wall, height: plateH - tuck,
      on: og.world.move(0, r - wall / 2, -plateH) });
    const curl = s.tube({ name: name + "_curl", outer: r, inner: r - wall, height: l,
      on: og.frame({ origin: [-l / 2, 0, 0], normal: [1, 0, 0], up: [0, 0, 1] }) });
    s.join(plate, curl);
    const holeLine = og.frame({ origin: [0, r, -(h / 2 - r)], normal: [0, 1, 0], up: [0, 0, 1] });
    s.hole(plate, { diameter: holeD, at: s.grid({ nx: holes, px: l * 0.14, on: holeLine }) });
    s.material(plate, { color: "#4a4d50", metalness: 1, roughness: 0.45 });
    return s.group({ name: name + "_assy", children: [plate] });
  }
}

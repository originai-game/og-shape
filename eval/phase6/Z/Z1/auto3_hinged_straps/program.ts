export default function (og) {
  const s = og.model({ name: "hinged_straps" });
  const L = 914.4, H = 76.2, D = 5.72;
  const zc = H / 2;            // hinge axis height
  const r = D / 2;             // curl radius
  const t = 1.5;               // plate thickness
  const nK = 8, gap = 0.6;     // knuckles
  const kLen = L / nK;
  const holeD = 4.5, nHoles = 7, holePitch = 125;
  const pivot = og.world.move(0, 0, zc);

  const fixedStrap = strap("fixed_strap", 0, -1);
  const fixed = s.group({ name: "fixed", children: [fixedStrap] });
  s.at(fixed, pivot);
  const top = strap("swing_strap", 1, 1);
  const swing = s.group({ name: "swing", children: [top] });
  s.at(swing, pivot);
  s.joint(swing, { name: "strap_hinge", parent: fixed, kind: "continuous", on: pivot, axis: "+X" });
  return s;

  // Built about the hinge axis (local x axis through origin). dir -1: plate hangs below; +1: above.
  function strap(name, parity, dir) {
    const plateH = zc - r - 0.6;
    const plate = s.box({ name, width: L, depth: t, height: plateH,
      on: og.world.move(0, 0, dir < 0 ? -zc : r + 0.6) });
    s.hole(plate, { diameter: holeD, at: s.grid({ nx: nHoles, px: holePitch, on: s.face(plate, "front") }) });
    for (let i = parity; i < nK; i += 2) {
      const x0 = -L / 2 + i * kLen + (i === 0 ? 0 : gap / 2);
      const x1 = -L / 2 + (i + 1) * kLen - (i === nK - 1 ? 0 : gap / 2);
      const len = x1 - x0;
      const tab = s.box({ name: `${name}_tab${i}`, width: len, depth: t, height: r + 1.2,
        on: og.world.move((x0 + x1) / 2, 0, dir < 0 ? -(r + 1.2) + 0.3 : -0.3) });
      s.join(plate, tab);
      const k = s.cylinder({ name: `${name}_curl${i}`, radius: r, height: len, segments: 20,
        on: og.frame({ origin: [x0, 0, 0], normal: [1, 0, 0] }) });
      s.join(plate, k);
    }
    s.material(plate, { color: "#4a4d50", metalness: 0.8, roughness: 0.45 });
    return plate;
  }
}

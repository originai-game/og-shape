export default function (og) {
  const s = og.model({ name: "mouse_and_pad" });
  const padW = 254, padD = 191, padT = 3, padR = 16;
  const mouseL = 114, mouseH = 38;
  const wheelY = -26, wheelR = 8, wheelW = 6;
  const wheelZ = padT + 32 + 3 - wheelR;      // axle height: wheel stands ~3 mm proud
  const recessY = 2, recessFloor = padT + 35; // button recess

  const pad = makePad();
  const shell = makeShell();
  const btn = makeButton();
  const wheel = makeWheel();
  const axle = og.frame({ origin: [0, wheelY, wheelZ], normal: [1, 0, 0] });
  s.at(wheel, axle);
  s.joint(wheel, { name: "wheel_spin", parent: shell, kind: "continuous", on: axle, axis: "+X" });
  return s;

  function makePad() {
    const p = s.extrude({ name: "pad", profile: og.rect({ width: padW, height: padD, radius: padR }), height: padT });
    s.fillet(p, { radius: 1, edges: "top" });
    s.material(p, { color: "#1b1c1e", roughness: 0.95 });
    return p;
  }

  function makeShell() {
    const h = mouseL / 2, endZ = 7;
    const b = s.body({
      name: "mouse_shell",
      plan: { points: [[-h, 0], [-h + 2, 28], [-h + 8, 46], [-35, 57], [-10, 61], [15, 63], [38, 58], [h - 6, 42], [h - 2, 24], [h, 0]], smooth: true },
      top: { points: [[-h, endZ], [-h + 2, 13], [-h + 8, 20], [-35, 28], [-15, 34], [5, 37], [22, mouseH], [40, 33], [h - 6, 20], [h - 2, 13], [h, endZ]], smooth: true },
      bottom: [[-h, endZ], [-h + 1, 4], [-h + 3, 1.5], [-h + 7, 0.3], [-h + 12, 0], [h - 12, 0], [h - 7, 0.3], [h - 3, 1.5], [h - 1, 4], [h, endZ]],
      n: 2.6,
    });
    s.at(b, og.world.move(0, 0, padT));
    const slot = s.box({ name: "wheel_slot", width: wheelW + 2, depth: 2 * wheelR + 4, height: 30, on: og.world.move(0, wheelY, wheelZ - wheelR - 1.5) });
    s.cut(b, slot);
    const rec = s.extrude({ name: "recess", profile: og.ellipse({ rx: 7, ry: 10 }), height: 20, on: og.world.move(0, recessY, recessFloor) });
    s.cut(b, rec);
    s.material(b, { color: "#2a2b2e", roughness: 0.45 });
    return b;
  }

  function makeButton() {
    const k = s.extrude({ name: "button", profile: og.ellipse({ rx: 5.5, ry: 8.5 }), height: 2.5, on: og.world.move(0, recessY, recessFloor) });
    s.fillet(k, { radius: 1, edges: "top" });
    s.material(k, { color: "#3a3c40", roughness: 0.4 });
    return k;
  }

  function makeWheel() {
    const w = s.cylinder({ name: "scroll_wheel", radius: wheelR, height: wheelW, segments: 24, on: og.world.move(0, 0, -wheelW / 2) });
    s.fillet(w, { radius: 1, edges: "top" });
    s.fillet(w, { radius: 1, edges: "bottom" });
    s.material(w, { color: "#555860", roughness: 0.7 });
    return s.group({ name: "wheel", children: [w] });
  }
}

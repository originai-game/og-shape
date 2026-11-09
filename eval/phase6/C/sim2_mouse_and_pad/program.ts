export default function (og) {
  const s = og.model({ name: "mouse_and_pad" });
  const padW = 254, padD = 191, padT = 3;
  const mouseW = 64, mouseL = 115, mouseH = 38;   // padT + mouseH = 41
  const recessY = -10, recessFloor = 31;           // recess centre (y) and floor height above the pad
  const wheelR = 5, wheelT = 4, wheelY = -17;
  const base = padT;                               // the mouse stands on the pad's top

  const pad = makePad();
  const shell = makeShell();
  const button = makeButton();
  const wheel = makeWheel();
  s.group({ name: "mouse", children: [shell, button] });
  const axle = og.frame({ origin: [0, wheelY, base + recessFloor + 2] });
  s.joint(wheel, { name: "scroll_wheel", parent: shell, kind: "continuous", on: axle, axis: "+X" });
  return s;

  function makePad() {
    const p = s.box({ name: "pad", width: padW, depth: padD, height: padT });
    s.fillet(p, { radius: 18, edges: "side" });
    s.material(p, { color: "#1c1c1e", roughness: 0.9 });
    return p;
  }

  // Base on the pad top, centred in x, nose toward -Y, hump toward the back.
  function makeShell() {
    const e = (rx, ry, dy) => ({ translate: { profile: og.ellipse({ rx, ry }), x: 0, y: dy } });
    const L = mouseL / 2, W = mouseW / 2;
    const sh = s.loft({
      name: "mouse_shell",
      on: og.world.move(0, 0, base),
      sections: [
        { profile: e(W * 0.92, L * 0.95, 0), z: 0 },
        { profile: e(W, L, 0), z: mouseH * 0.25 },
        { profile: e(W * 0.9, L * 0.92, 4), z: mouseH * 0.6 },
        { profile: e(W * 0.6, L * 0.7, 8), z: mouseH * 0.88 },
        { profile: e(W * 0.3, L * 0.35, 10), z: mouseH },
      ],
      steps: 4,
    });
    // the oval recess on top, and a deeper slot for the wheel at its front
    const lid = og.world.move(0, 0, base + mouseH + 2);
    s.pocket(sh, { profile: og.ellipse({ rx: 8, ry: 15 }), at: lid.move(0, recessY, 0), depth: mouseH + 2 - recessFloor });
    s.pocket(sh, { profile: og.rect({ width: wheelT + 2, height: 2 * wheelR + 3 }), at: lid.move(0, wheelY, 0), depth: mouseH + 2 - (recessFloor - 4) });
    s.material(sh, { color: "#2a2a2c", roughness: 0.4 });
    return sh;
  }

  function makeButton() {
    const b = s.extrude({ name: "button", profile: og.ellipse({ rx: 5, ry: 6 }), height: 3,
      on: og.world.move(0, recessY + 6, base + recessFloor) });
    s.material(b, { color: "#3a3a3d", roughness: 0.3 });
    return b;
  }

  // Axle along x through the wheel's centre.
  function makeWheel() {
    const w = s.cylinder({ name: "wheel", radius: wheelR, height: wheelT,
      on: og.frame({ origin: [-wheelT / 2, wheelY, base + recessFloor + 2], normal: [1, 0, 0] }) });
    s.material(w, { color: "#555558", roughness: 0.7 });
    return w;
  }
}

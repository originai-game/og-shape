export default function (og) {
  const s = og.model({ name: "mouse_and_pad" });
  const padW = 254, padD = 191, padT = 3;
  const mouseH = 41 - padT; // 38
  const mouseL = 110, mouseW = 62;
  const mx = -20, my = 15; // mouse centre on the pad
  const wheelR = 9, wheelW = 7, wheelY = -30, wheelZ = 21; // in the mouse's own space
  const btnY = -10, btnFloor = 31;

  const pad = buildPad();
  const shell = buildShell();
  const button = buildButton();
  const wheel = buildWheel();

  const hub = og.frame({ origin: [mx, my + wheelY, padT + wheelZ] });
  s.at(wheel, hub);
  s.joint(wheel, { name: "wheel_spin", parent: shell, kind: "continuous", on: hub, axis: "+X" });
  return s;

  function buildPad() {
    const p = s.box({ name: "pad", width: padW, depth: padD, height: padT });
    s.fillet(p, { radius: 16, edges: "side" });
    s.material(p, { color: "#1a1a1a", roughness: 0.95 });
    return p;
  }

  function buildShell() {
    const base = og.world.move(mx, my, padT);
    const h = mouseH, L = mouseL / 2, W = mouseW;
    const b = s.body({
      name: "shell",
      plan: { points: [[-L, 4], [-51, 34], [-40, 50], [-10, 56], [20, W], [42, 56], [51, 36], [L, 4]], smooth: true },
      top: { points: [[-L, 4], [-51, 13], [-35, 26], [-10, 35], [15, h - 0.05], [40, 32], [51, 18], [L, 5]], smooth: true },
      bottom: 0,
      n: 2.5,
      on: base,
    });
    // slot for the scroll wheel
    const slot = s.box({ name: "wheel_slot", width: wheelW + 2, depth: 2 * wheelR + 3, height: 40, on: base.move(0, wheelY, wheelZ - wheelR - 1) });
    s.cut(b, slot);
    // oval recess for the button
    const recess = s.extrude({ name: "button_recess", profile: og.ellipse({ rx: 6, ry: 8 }), height: 30, on: base.move(0, btnY, btnFloor) });
    s.cut(b, recess);
    s.material(b, { color: "#2b2b2d", roughness: 0.45 });
    return b;
  }

  function buildButton() {
    const btn = s.extrude({ name: "button", profile: og.ellipse({ rx: 4.5, ry: 6.5 }), height: 2.5, on: og.world.move(mx, my + btnY, padT + btnFloor) });
    s.fillet(btn, { radius: 1, edges: "top" });
    s.material(btn, { color: "#444448", roughness: 0.4 });
    return btn;
  }

  // Built about its own origin: the axle runs along x through it.
  function buildWheel() {
    const tyre = s.cylinder({ name: "wheel_tyre", radius: wheelR, height: wheelW, segments: 32, on: og.frame({ origin: [-wheelW / 2, 0, 0], normal: [1, 0, 0] }) });
    s.fillet(tyre, { radius: 1.2 });
    s.material(tyre, { color: "#555558", roughness: 0.7 });
    return s.group({ name: "wheel", children: [tyre] });
  }
}

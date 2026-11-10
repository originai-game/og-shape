export default function (og) {
  const s = og.model({ name: "mouse_and_pad" });
  const padW = 254, padD = 191, padT = 4, padR = 16;
  const mx = 10, my = 15;            // mouse centre on the pad
  const halfL = 60, maxW = 64, mouseH = 37;   // padT + mouseH = 41
  const wheelR = 9, wheelW = 6, wheelY = 30, wheelTopZ = padT + 34.5;
  const btnY = 10, btnRx = 3.5, btnRy = 6, recessD = 1.5;

  const pad = makePad();
  const shell = makeShell();
  const btn = makeButton();
  const wf = og.frame({ origin: [mx, my + wheelY, wheelTopZ - wheelR] });
  const wheel = makeWheel();
  s.at(wheel, wf);
  s.joint(wheel, { name: "scroll_wheel_spin", parent: shell, kind: "continuous", on: wf, axis: "+X" });
  return s;

  function makePad() {
    const p = s.extrude({ name: "pad", profile: og.rect({ width: padW, height: padD, radius: padR }), height: padT });
    s.fillet(p, { radius: 1, edges: "top" });
    s.material(p, { color: "#1b1b1d", roughness: 0.95 });
    return p;
  }

  // dense profile lines (y from -halfL to +halfL)
  function line(f) {
    const pts = [];
    const n = 24;
    for (let i = 0; i <= n; i++) {
      const t = -1 + (2 * i) / n;               // -1 rear .. +1 front
      pts.push([t * halfL, f(t)]);
    }
    return pts;
  }

  function makeShell() {
    const width = line(t => Math.max(3, maxW * Math.pow(1 - t * t, 0.4) * (1 - 0.06 * t)));
    const top = line(t => {
      const u = t + 0.25;                        // peak a bit behind the middle
      const span = u < 0 ? 0.75 : 1.25;
      const v = Math.max(0, 1 - (u / span) * (u / span));
      return Math.max(2.5, mouseH * Math.pow(v, 0.55));
    });
    const b = s.body({ name: "mouse_shell", on: og.world.move(mx, my, padT), plan: width, top, bottom: 0, n: 2.6 });
    const slot = s.box({ name: "wheel_slot", width: wheelW + 1.5, depth: 2 * wheelR + 2, height: 40,
      on: og.world.move(mx, my + wheelY, wheelTopZ - wheelR - 4) });
    s.cut(b, slot);
    const rec = s.extrude({ name: "button_recess", profile: og.ellipse({ rx: btnRx + 1, ry: btnRy + 1 }), height: 40,
      on: og.world.move(mx, my + btnY, recessFloor()) });
    s.cut(b, rec);
    s.material(b, { color: "#2a2a2d", roughness: 0.45 });
    return b;
  }

  function recessFloor() { return padT + 35 - recessD - 1.5; }

  function makeButton() {
    const k = s.extrude({ name: "oval_button", profile: og.ellipse({ rx: btnRx, ry: btnRy }), height: 3,
      on: og.world.move(mx, my + btnY, recessFloor()) });
    s.fillet(k, { radius: 0.8, edges: "top" });
    s.material(k, { color: "#3c3c40", roughness: 0.4 });
    return k;
  }

  function makeWheel() {
    const w = s.cylinder({ name: "scroll_wheel", radius: wheelR, height: wheelW, segments: 32,
      on: og.world.move(-wheelW / 2, 0, 0).turn(0, og.deg(90), 0) });
    s.material(w, { color: "#55555a", roughness: 0.7 });
    return s.group({ name: "scroll_wheel_group", children: [w] });
  }
}

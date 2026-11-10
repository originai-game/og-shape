export default function (og) {
  const s = og.model({ name: "mouse_and_pad" });
  const padW = 254, padD = 191, padT = 3;
  const mouseH = 41 - padT;
  const wheelY = 32, wheelR = 9, wheelW = 7;
  const btnY = 12;

  const pad = mousePad();
  const shell = mouseShell();
  const btn = button();
  const wheel = scrollWheel();
  const wf = og.frame({ origin: [0, wheelY, padT + 24] });
  s.at(wheel, wf);
  s.joint(wheel, { name: "wheel_spin", parent: shell, kind: "continuous", on: wf, axis: "+X" });
  return s;

  function mousePad() {
    const p = s.box({ name: "pad", width: padW, depth: padD, height: padT });
    s.fillet(p, { radius: 16, edges: "side" });
    s.material(p, { color: "#1b1c1e", roughness: 0.9 });
    return p;
  }

  function mouseShell() {
    // the smooth top line overshoots its peak point by about 3.9 mm
    const m = s.body({
      name: "mouse_shell",
      plan: { points: [[-58, 6], [-54, 38], [-44, 58], [-15, 66], [20, 62], [44, 54], [54, 34], [58, 6]], smooth: true },
      top: { points: [[-58, 4], [-54, 16], [-42, 29], [-18, mouseH - 0.91], [8, 36.3], [32, 28.5], [50, 17], [58, 4]], smooth: true },
      bottom: 0,
      n: 2.6,
      on: og.world.move(0, 0, padT),
    });
    const slot = s.box({ name: "wheel_slot", width: wheelW + 3, depth: 2 * wheelR + 4, height: 30, on: og.world.move(0, wheelY, padT + 17) });
    s.cut(m, slot);
    s.pocket(m, { profile: og.ellipse({ rx: 5, ry: 7 }), depth: 9, at: og.world.move(0, btnY, padT + 43) });
    s.material(m, { color: "#25272a", roughness: 0.45 });
    return m;
  }

  function button() {
    const b = s.extrude({ name: "button", profile: og.ellipse({ rx: 4, ry: 6 }), height: 8, on: og.world.move(0, btnY, padT + 28) });
    s.fillet(b, { radius: 1.2, edges: "top" });
    s.material(b, { color: "#3a3d42", roughness: 0.5 });
    return b;
  }

  // Built about its axle: axis along x through the origin.
  function scrollWheel() {
    const w = s.cylinder({ name: "scroll_wheel", radius: wheelR, height: wheelW, segments: 32, on: og.world.move(-wheelW / 2, 0, 0).turn(0, og.deg(90), 0) });
    s.material(w, { color: "#555960", roughness: 0.7 });
    return s.group({ name: "wheel", children: [w] });
  }
}

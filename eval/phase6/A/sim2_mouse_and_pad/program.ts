export default function (og) {
  const s = og.model({ name: "mouse_and_pad" });
  const padW = 254, padD = 191, padT = 3;
  const mouseW = 64, mouseL = 112, mouseH = 41 - padT;

  const wheelY = -mouseL * 0.22, wheelZ = padT + mouseH * 0.82;
  const pad = mousePad(padW, padD, padT);
  const top = og.frame({ origin: s.face(pad, "top").origin });
  const mouse = mouseBody(mouseW, mouseL, mouseH);
  s.at(mouse, top);

  // Wheel axle: across x, near the front of the shell.
  const axle = og.frame({ origin: [0, wheelY, wheelZ] });
  const wheel = scrollWheel();
  s.at(wheel, axle);
  s.joint(wheel, { name: "wheel_spin", parent: mouse, kind: "continuous", on: axle, axis: "+X" });
  return s;

  function mousePad(w, d, t, r = 16) {
    const p = s.extrude({ name: "pad", profile: og.rect({ width: w, height: d, radius: r }), height: t });
    s.fillet(p, { radius: 1, edges: "top" });
    s.material(p, { color: "#1b1b1d", roughness: 0.9 });
    return p;
  }

  // Built standing on its own origin, long axis along y, front at -y.
  function mouseBody(w, l, h) {
    const rx = w / 2, ry = l / 2;
    const sec = (f, g, z) => ({ profile: og.ellipse({ rx: rx * f, ry: ry * g }), z });
    const shell = s.loft({
      name: "mouse_shell",
      sections: [sec(0.92, 0.95, 0), sec(1, 1, h * 0.35), sec(0.9, 0.93, h * 0.7), sec(0.6, 0.72, h * 0.92), sec(0.25, 0.33, h)],
      samples: 48,
    });
    s.material(shell, { color: "#26272a", roughness: 0.45 });
    // Slot for the wheel.
    const slot = s.box({ name: "wheel_slot", width: 8, depth: 18, height: 20, on: og.world.move(0, wheelY, h * 0.82 - 7) });
    s.cut(shell, slot);
    // Small oval button in a recess, behind the wheel.
    const bf = og.world.move(0, l * 0.04, h);
    s.pocket(shell, { profile: og.ellipse({ rx: 5, ry: 7 }), depth: 2, at: bf });
    const button = s.extrude({ name: "button", profile: og.ellipse({ rx: 4.2, ry: 6.2 }), height: 1.8, on: bf.offset(-2) });
    s.material(button, { color: "#3a3b3f", roughness: 0.4 });
    return s.group({ name: "mouse", children: [shell, button] });
  }

  function scrollWheel(r = 6, t = 6) {
    const f = og.frame({ origin: [-t / 2, 0, 0], normal: [1, 0, 0], up: [0, 0, 1] });
    const w = s.cylinder({ name: "scroll_wheel", radius: r, height: t, on: f });
    s.material(w, { color: "#555860", roughness: 0.6 });
    return s.group({ name: "wheel", children: [w] });
  }
}

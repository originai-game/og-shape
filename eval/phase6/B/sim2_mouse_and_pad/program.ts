export default function (og) {
  const s = og.model({ name: "mouse_and_pad" });
  const padW = 254, padD = 191, padT = 3;
  const mouseW = 64, mouseL = 112, mouseH = 38; // mouse sits on the pad, top at padT + mouseH
  const mousePos = og.world.move(-20, 10, padT);

  const pad = mousePad(padW, padD, padT);
  const mouse = mouseShell(mouseW, mouseL, mouseH);
  const wheelAxle = og.frame({ origin: mousePos.move(0, -mouseL * 0.24, mouseH - 9).origin });
  const wheel = scrollWheel(8, 6);
  s.at(wheel, wheelAxle);
  s.joint(wheel, { name: "wheel_spin", parent: mouse, kind: "continuous", on: wheelAxle, axis: "+X" });
  const btn = button(mouse);
  return s;

  function mousePad(w, d, t) {
    const p = s.extrude({ name: "pad", profile: og.rect({ width: w, height: d, radius: 16 }), height: t });
    s.material(p, { color: "#1b1b1d", roughness: 0.9 });
    return p;
  }

  function mouseShell(w, l, h) {
    const sec = (rx, ry, dy, z) => ({ profile: { translate: { profile: og.ellipse({ rx, ry }), x: 0, y: dy } }, z });
    const m = s.loft({
      name: "mouse",
      sections: [
        sec(w * 0.47, l * 0.48, 0, 0),
        sec(w * 0.5, l * 0.5, 0, h * 0.35),
        sec(w * 0.44, l * 0.45, l * 0.04, h * 0.7),
        sec(w * 0.3, l * 0.33, l * 0.07, h * 0.92),
        sec(w * 0.12, l * 0.15, l * 0.08, h),
      ],
      steps: 4,
      on: mousePos,
    });
    // slot the wheel turns in
    s.pocket(m, { profile: og.rect({ width: 8, height: 20, radius: 2 }), depth: 20, at: mousePos.move(0, -l * 0.24, h + 6) });
    s.material(m, { color: "#222326", roughness: 0.4 });
    return m;
  }

  // Built about its axle: the axle runs along x through the origin.
  function scrollWheel(r, wd) {
    const disc = s.cylinder({ name: "wheel_disc", radius: r, height: wd, on: og.frame({ origin: [-wd / 2, 0, 0], normal: [1, 0, 0] }) });
    s.fillet(disc, { radius: 1 });
    s.material(disc, { color: "#3a3b3f", roughness: 0.8 });
    return s.group({ name: "scroll_wheel", children: [disc] });
  }

  // A small oval button sitting in an oval recess on the top of the shell.
  function button(m, rx = 4, ry = 6.5, recess = 2) {
    const seat = s.face(m, "top").move(0, 6);
    s.pocket(m, { profile: og.ellipse({ rx: rx + 1, ry: ry + 1 }), depth: recess, at: seat });
    const b = s.extrude({ name: "button", profile: og.ellipse({ rx, ry }), height: recess - 0.2, on: seat.offset(-recess) });
    s.material(b, { color: "#4a4c52", roughness: 0.5 });
    return b;
  }
}

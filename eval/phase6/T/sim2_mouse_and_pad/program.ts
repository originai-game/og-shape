export default function (og) {
  const s = og.model({ name: "mouse_and_pad" });
  const w = 254, d = 191, t = 2, corner = 15;
  const recessL = 35, recessW = 9, recessY = 17, recessZ = 35, slope = og.deg(-23);

  const pad = mousePad(w, d, t, corner);
  const shell = mouseShell(t);
  const recess = og.world.move(0, recessY, recessZ).turn(slope, 0, og.deg(90));
  s.pocket(shell, { profile: og.slot({ length: recessL, width: recessW }), depth: t, at: recess });
  s.material(shell, { name: "plastic", color: "#42464b", roughness: 0.5 });

  const button = ovalButton(recessW - t, t);
  s.at(button, og.world.move(0, recessY - recessW, recessZ + t).turn(slope, 0, 0));

  const wheelAxle = og.frame({ origin: [0, 28, 23.5] });
  const wheel = scrollWheel();
  s.at(wheel, wheelAxle);
  s.group({ name: "mouse", children: [pad, shell, button, wheel] });
  s.joint(wheel, { name: "wheel_spin", parent: shell, on: wheelAxle, axis: "+X", range: [-3.1, 3.1] });
  return s;

  function mousePad(w, d, t, r) {
    const p = s.extrude({ name: "pad", profile: og.rect({ width: w, height: d, radius: r }), height: t });
    s.material(p, { name: "rubber", color: "#292b30", roughness: 0.9 });
    return p;
  }

  // sand3d stations: superellipse section of width w, depth dd, centre y, exponent n.
  function section(w, dd, y, n, seg = 32) {
    const pts = [];
    for (let i = 0; i < seg; i++) {
      const a = (2 * Math.PI * i) / seg;
      const c = Math.cos(a), sn = Math.sin(a);
      const px = (w / 2) * Math.sign(c) * Math.pow(Math.abs(c), 2 / n);
      const py = (dd / 2) * Math.sign(sn) * Math.pow(Math.abs(sn), 2 / n) + y;
      pts.push([px, py]);
    }
    return og.poly({ points: pts });
  }

  function mouseShell(z0) {
    const sh = s.loft({
      name: "shell",
      sections: [
        { profile: section(57, 96, -5, 2.6), z: z0 },
        { profile: section(61, 107, -3, 2.7), z: 11 },
        { profile: section(61, 93, -5, 2.3), z: 22 },
        { profile: section(56, 78, -4, 1.6), z: 30 },
        { profile: section(28, 35, -11, 1.8), z: 38 },
        { profile: section(1, 5, -10, 2), z: 41 },
      ],
    });
    return sh;
  }

  function ovalButton(dia, h) {
    const b = s.cylinder({ name: "oval_button", radius: dia / 2, height: h });
    s.fillet(b, { radius: h * 0.6, edges: "top" });
    s.material(b, { name: "plastic_dark", color: "#24272a", roughness: 0.4 });
    return b;
  }

  // Built about its axle: the axle on the origin, along x.
  function scrollWheel(width = 6, dia = 19) {
    const wh = s.cylinder({ name: "wheel", radius: dia / 2, height: width,
      on: og.frame({ origin: [-width / 2, 0, 0], normal: [1, 0, 0], up: [0, 0, 1] }) });
    s.material(wh, { name: "wheel_rubber", color: "#161719", roughness: 0.8 });
    return s.group({ name: "scroll_wheel", children: [wh] });
  }
}

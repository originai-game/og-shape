export default function (og) {
  const s = og.model({ name: "framed_panel" });
  const width = 1016, height = 685.8, panelT = 25, frameT = 35.56 - 25, border = 23;

  const back = panel();
  const pivot = og.frame({ origin: [0, 0, height] }); // panel's top front edge, along x
  const fr = frame();
  s.at(fr, pivot);
  s.joint(fr, { name: "frame_spin", parent: back, kind: "continuous", on: pivot, axis: "-X" });
  return s;

  function panel() {
    const p = s.box({ name: "panel", width, depth: panelT, height, on: og.world.move(0, panelT / 2, 0) });
    s.material(p, { color: "#454545", roughness: 0.7 });
    return p;
  }

  // Built about its pivot: the hinge line runs along x at y = 0, z = 0; the frame hangs below, in front (-y).
  function frame() {
    const profile = og.subtract(
      og.rect({ width, height }),
      og.rect({ width: width - 2 * border, height: height - 2 * border }),
    );
    const f = s.extrude({ name: "frame_border", profile, height: frameT,
      on: og.frame({ origin: [0, 0, -height / 2], normal: [0, -1, 0], up: [0, 0, 1] }) });
    s.material(f, { color: "#b0b0b0", roughness: 0.5 });
    return s.group({ name: "frame", children: [f] });
  }
}

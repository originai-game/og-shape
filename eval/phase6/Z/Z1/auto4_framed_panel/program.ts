export default function (og) {
  const s = og.model({ name: "framed_panel" });
  const W = 1016, H = 685.8, T = 25, frameT = 35.56 - 25, border = 23;

  const p = panel();
  const hinge = og.frame({ origin: [0, -T / 2, H] }); // top front edge of the panel
  const f = frame();
  s.at(f, hinge);
  s.joint(f, { name: "frame_spin", parent: p, kind: "continuous", on: hinge, axis: "-X" });
  return s;

  function panel() {
    const b = s.box({ name: "panel", width: W, depth: T, height: H });
    s.material(b, { color: "#454545", roughness: 0.8 });
    return b;
  }

  // Built about the hinge line: x axis at the top back edge of the frame.
  function frame() {
    const profile = og.subtract(
      og.rect({ width: W, height: H }),
      og.rect({ width: W - 2 * border, height: H - 2 * border }),
    );
    const ring = s.extrude({
      name: "frame_border", profile, height: frameT,
      on: og.frame({ origin: [0, 0, -H / 2], normal: [0, -1, 0], up: [0, 0, 1] }),
    });
    s.material(ring, { color: "#b0b0b0", roughness: 0.5 });
    return s.group({ name: "frame", children: [ring] });
  }
}

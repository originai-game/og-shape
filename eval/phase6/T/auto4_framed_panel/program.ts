export default function (og) {
  const s = og.model({ name: "framed_panel" });
  const width = 1016, height = 685.8, panelThick = 25.4, frameThick = 10.16, border = 22.86;

  const panel = backingPanel(width, panelThick, height);
  // The frame spins about the panel's top back edge (a horizontal axis along x).
  const hinge = og.frame({ origin: s.bbox(panel, "+Y+Z").origin });
  const frame = pictureFrame(width, height, frameThick, border);
  s.at(frame, hinge);
  s.joint(frame, { name: "frame_spin", parent: panel, kind: "continuous", on: hinge, axis: "+X" });
  return s;

  function backingPanel(w, t, h) {
    const p = s.box({ name: "backing_panel", width: w, depth: t, height: h, on: og.world.move(0, -5.09, 0) });
    s.material(p, { color: "#444444", roughness: 0.7 });
    return p;
  }

  // Built about the hinge: hangs down from z = 0, behind the hinge line (y from 0 to t).
  function pictureFrame(w, h, t, b) {
    const face = og.frame({ origin: [0, t / 2, -h / 2], normal: [0, -1, 0], up: [0, 0, 1] });
    const outer = s.extrude({ name: "frame_piece", profile: og.rect({ width: w, height: h }), height: t, bothSides: true, on: face });
    const opening = s.extrude({ name: "opening", profile: og.rect({ width: w - 2 * b, height: h - 2 * b }), height: t + 2, bothSides: true, on: face });
    s.cut(outer, opening);
    s.material(outer, { color: "#9a9a9a", roughness: 0.6 });
    return s.group({ name: "frame", children: [outer] });
  }
}

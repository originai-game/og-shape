export default function (og) {
  const s = og.model({ name: "framed_panel" });
  const width = 1016, height = 685.8, panelT = 25, depth = 35.56, border = 23;
  const frameT = depth - panelT;

  const back = panel(width, panelT, height);
  const hinge = og.frame({ origin: s.bbox(back, "-Y+Z").origin }); // front top edge of the panel
  const fr = frame(width, frameT, height, border);
  s.at(fr, hinge);
  s.joint(fr, { name: "frame_spin", parent: back, kind: "continuous", on: hinge, axis: "+X" });
  return s;

  // Back face at y = +t, front face at y = 0.
  function panel(w, t, h) {
    const p = s.box({ name: "panel", width: w, depth: t, height: h, on: og.world.move(0, t / 2, 0) });
    s.material(p, { color: "#454545", roughness: 0.8 });
    return p;
  }

  // Built about its own origin: the spin axis runs along x at its top back edge.
  function frame(w, t, h, b) {
    const outer = s.box({ name: "frame", width: w, depth: t, height: h, on: og.world.move(0, -t / 2, -h) });
    const opening = s.box({ name: "opening", width: w - 2 * b, depth: t + 2, height: h - 2 * b, on: og.world.move(0, -t / 2, -h + b) });
    s.cut(outer, opening);
    s.material(outer, { color: "#a0a0a0", roughness: 0.6 });
    return s.group({ name: "frame_group", children: [outer] });
  }
}

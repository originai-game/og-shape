export default function (og) {
  const s = og.model({ name: "framed_panel" });
  const width = 1016, height = 685.8, depth = 35.56;
  const panelT = 25, frameT = depth - panelT, border = 23;
  const backY = depth / 2;          // panel back face
  const seamY = backY - panelT;     // panel front face = frame back face

  const back = panel(width, panelT, height);
  // Spin axis: along the panel's top front edge, world X.
  const pivot = og.frame({ origin: [0, seamY, height] });
  const pic = frame(width, frameT, height, border);
  s.at(pic, pivot);
  s.joint(pic, { name: "frame_spin", parent: back, kind: "continuous", on: pivot, axis: "-X" });
  return s;

  function panel(w, t, h) {
    const p = s.box({ name: "panel", width: w, depth: t, height: h, on: og.world.move(0, backY - t / 2, 0) });
    s.material(p, { color: "#444444", roughness: 0.8 });
    return p;
  }

  // Built about its pivot: top back edge on the origin, hanging down, in -Y.
  function frame(w, t, h, b) {
    const outer = s.box({ name: "frame_border", width: w, depth: t, height: h, on: og.world.move(0, -t / 2, -h) });
    const opening = s.box({ name: "frame_opening", width: w - 2 * b, depth: t + 2, height: h - 2 * b, on: og.world.move(0, -t / 2, -h + b) });
    s.cut(outer, opening);
    s.material(outer, { color: "#b0b0b0", roughness: 0.6 });
    return s.group({ name: "frame", children: [outer] });
  }
}

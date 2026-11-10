export default function (og) {
  const s = og.model({ name: "closet" });
  // Doors face -X (the 914.6 mm side is the front), as the per-axis size asks.
  const depth = 622.2, width = 914.6, height = 2023;
  const wall = 20, doorT = 20, gap = 3, margin = 2;
  const doorW = (width - 2 * wall - gap) / 2;
  const doorH = height - 2 * wall - 2 * margin;
  const grey = { color: "#5b5d58", metalness: 0.6, roughness: 0.5 };

  const body = caseBody();
  // Hinges on the outer edges of the opening, at the front face.
  const hingeL = og.frame({ origin: [-depth / 2, width / 2 - wall, wall + margin] });
  const hingeR = og.frame({ origin: [-depth / 2, -(width / 2 - wall), wall + margin] });
  const left = door("door_left", -1);
  s.at(left, hingeL);
  s.joint(left, { name: "door_left_hinge", parent: body, on: hingeL, axis: "-Z", range: [0, og.deg(90)] });
  const right = door("door_right", 1);
  s.at(right, hingeR);
  s.joint(right, { name: "door_right_hinge", parent: body, on: hingeR, axis: "+Z", range: [0, og.deg(90)] });
  return s;

  function caseBody() {
    const b = s.box({ name: "case", width: depth, depth: width, height });
    s.shell(b, { thickness: wall, open: "left" });
    s.material(b, grey);
    return b;
  }

  // Hinge line on z at the origin; the panel runs +X (into the case) and along `dir` in y.
  function door(name, dir) {
    const p = s.box({ name: name + "_panel", width: doorT, depth: doorW, height: doorH, on: og.world.move(doorT / 2, dir * doorW / 2, 0) });
    s.material(p, { ...grey, color: "#62645f" });
    return s.group({ name, children: [p] });
  }
}

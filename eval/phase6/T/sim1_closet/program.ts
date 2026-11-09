// Closet: a tall metal case open at +X, two doors hinged at their outer edges.
// Translated from sand3d; the front faces +X as in the source.
export default function (og) {
  const s = og.model({ name: "closet" });
  const w = 622.2, d = 914.6, h = 2023; // overall size
  const t = 16, bottom = 23, doorZ = 59, lip = 37, front = 595;
  const ox = w / 2, oy = d / 2; // sand3d placed the closet with its min corner on the origin
  const at = (x, y, z) => og.world.move(ox + x, oy + y, z);
  const grey = { name: "painted_metal", color: "#858581", roughness: 0.5, metalness: 0.3 };

  const body = caseBody();
  const doorH = h - lip - doorZ;
  const hingeX = 293, hingeY = 414;

  // door_02 on +Y side, opens about +Z; door_01 on -Y side, opens about -Z direction (range negative)
  const f2 = at(hingeX, hingeY, doorZ);
  const door2 = door("door_02", 21, 420, doorH, 294.4 - hingeX, 210.7 - hingeY);
  s.at(door2, f2);
  s.joint(door2, { name: "door_02_hinge", parent: body, on: f2, axis: "+Z", range: [0, og.deg(90)] });

  const f1 = at(hingeX, -hingeY, doorZ);
  const door1 = door("door_01", 23, 420, doorH, 295.4 - hingeX, -210.3 + hingeY);
  s.at(door1, f1);
  s.joint(door1, { name: "door_01_hinge", parent: body, on: f1, axis: "+Z", range: [og.deg(-90), 0] });

  return s;

  function caseBody() {
    const c = s.box({ name: "case", width: w, depth: d, height: h - bottom, on: at(0, 0, bottom) });
    const cavity = s.box({ name: "interior", width: front - t, depth: d - 2 * t, height: h - bottom - 2 * t,
      on: at((front + t - w) / 2, 0, bottom + t) });
    s.cut(c, cavity);
    const opening = s.box({ name: "opening", width: w - front + t, depth: d - 2 * lip, height: h - lip - doorZ,
      on: at((front + w + t) / 2 - w / 2, 0, doorZ) });
    s.cut(c, opening);
    const back = s.box({ name: "back_edge", width: t, depth: d, height: bottom + t, on: at((t - w) / 2, 0, 0) });
    s.join(c, back);
    s.material(c, grey);
    return c;
  }

  // Built about its hinge: the pivot is the group's origin; (cx, cy) is the panel centre from it.
  function door(name, dw, dd, dh, cx, cy) {
    const panel = s.box({ name: name + "_panel", width: dw, depth: dd, height: dh, on: og.world.move(cx, cy, 0) });
    s.material(panel, grey);
    return s.group({ name, children: [panel] });
  }
}

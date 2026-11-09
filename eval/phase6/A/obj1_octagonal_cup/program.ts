export default function (og) {
  const s = og.model({ name: "octagonal_cup" });
  const width = 193, depth = 352.2, height = 137; // overall size from the brief

  const body = cup(width, height);
  const bar = handle(depth - width, og.frame({ origin: s.bbox(body, "+Y+Z").origin })); // back top edge
  s.join(body, bar);
  s.material(body, { name: "metal", color: "#8a8d90", metalness: 0.8, roughness: 0.4 });
  return s;

  // Eight-sided cup, flats across = across.
  function cup(across, h, wall = 14, chamfer = 6) {
    const r = across / 2 / 0.9238795; // circumradius from the across-flats size
    const c = s.extrude({ name: "cup", profile: og.polygon({ sides: 8, radius: r }), height: h,
      on: og.world.turn(0, 0, og.deg(22.5)) }); // flats facing x and y
    s.chamfer(c, { distance: chamfer, edges: "bottom" });
    s.shell(c, { thickness: wall, open: "top" });
    return c;
  }

  // Flat bar sticking out along +y from the wall at `edge`, sunk into the wall by `sink`.
  function handle(reach, edge, w = 40, t = 12, sink = 6, drop = 4) {
    return s.box({ name: "handle", width: w, depth: reach + sink, height: t,
      on: edge.move(0, (reach - sink) / 2, -t - drop) });
  }
}

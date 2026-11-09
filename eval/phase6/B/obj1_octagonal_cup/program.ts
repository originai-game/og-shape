export default function (og) {
  const s = og.model({ name: "octagonal_cup" });
  const flats = 193, height = 137, reach = 352.2 - 193; // handle sticks out this far past the cup

  const cup = body(flats, height);
  const bar = handle(reach, height);
  s.join(cup, bar);
  s.material(cup, { name: "steel", color: "#9a9da1", metalness: 1, roughness: 0.4 });
  return s;

  function body(f, h, wall = 20, bevel = 10) {
    const r = f / 2 / Math.cos(Math.PI / 8); // polygon radius to the corners
    const oct = { rotate: { profile: og.polygon({ sides: 8, radius: r }), angle: og.deg(22.5) } };
    const b = s.extrude({ name: "cup", profile: oct, height: h });
    s.chamfer(b, { distance: bevel, edges: "bottom" });
    s.shell(b, { thickness: wall, open: "top" });
    return b;
  }

  // A flat bar out of the back wall, its top 10 mm under the rim.
  function handle(len, h, w = 40, t = 14, sink = 6) {
    return s.box({ name: "handle", width: w, depth: len + sink, height: t,
      on: og.world.move(0, flats / 2 + (len - sink) / 2, h - 10 - t) });
  }
}

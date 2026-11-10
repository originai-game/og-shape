export default function (og) {
  const s = og.model({ name: "octagonal_cup" });
  const flats = 193, height = 137, wall = 12, chamferD = 10;
  const totalDepth = 352.2;
  const hW = 40, hT = 12, hTopBelowRim = 8, overlap = 6;

  const cup = body();
  const handle = bar();
  s.join(cup, handle);
  s.material(cup, { color: "#9a9ea3", metalness: 1, roughness: 0.4 });
  return s;

  function body() {
    const oct = og.rotate({ profile: og.polygon({ sides: 8, flats }), angle: og.deg(22.5) });
    const c = s.extrude({ name: "cup", profile: oct, height });
    s.chamfer(c, { distance: chamferD, edges: "bottom" });
    s.shell(c, { thickness: wall, open: "top" });
    return c;
  }

  function bar() {
    const y0 = flats / 2 - overlap;
    const y1 = totalDepth - flats / 2;
    const len = y1 - y0;
    return s.box({ name: "handle", width: hW, depth: len, height: hT,
      on: og.world.move(0, (y0 + y1) / 2, height - hTopBelowRim - hT) });
  }
}

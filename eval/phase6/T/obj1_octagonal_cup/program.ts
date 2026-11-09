export default function (og) {
  const s = og.model({ name: "octagonal_cup" });
  const width = 193, height = 137; // across flats, overall height

  const body = cup(width, height);
  const bar = handle();
  // The bar sticks straight out along -Y near the top; its inner end sits inside the wall.
  s.at(bar, og.world.move(-2, -168.4, 117));
  s.group({ name: "cup_group", children: [body, bar] });
  return s;

  // Octagonal cup, flats facing the axes, a flat rim of `wall` across the top
  // with an octagonal opening `opening` across flats.
  function cup(w, h, bevel = 18, opening = 128, wall = 1.2) {
    const cosHalf = Math.sqrt((1 + Math.SQRT1_2) / 2); // cos 22.5 deg: across flats -> radius
    const oct = (flats) => og.polygon({ sides: 8, radius: flats / 2 / cosHalf });
    const turn = og.world.turn(0, 0, og.deg(22.5));

    // Thin-walled body, its wall ending inside the rim plate (no coincident faces for the join).
    const b = s.extrude({ name: "cup", profile: oct(w), height: h - wall / 2, on: turn });
    s.chamfer(b, { distance: bevel, edges: "bottom" });
    s.shell(b, { thickness: wall, open: "top" });

    // Flat rim plate across the top, inset half a wall so its sides lie inside the wall.
    const rim = s.extrude({ name: "rim", profile: og.subtract(oct(w - wall), oct(opening)), height: wall, on: turn.move(0, 0, h - wall) });
    s.join(b, rim);

    s.material(b, { name: "aluminium", color: "#e6e6e6", metalness: 1, roughness: 0.4 });
    return b;
  }

  function handle(w = 47, d = 174.6, h = 16) {
    const bar = s.box({ name: "handle", width: w, depth: d, height: h });
    s.material(bar, { name: "aluminium_dark", color: "#888888", metalness: 1, roughness: 0.4 });
    return bar;
  }
}

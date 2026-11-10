export default function (og) {
  const s = og.model({ name: "curved_plate" });
  const width = 304.8, length = 914.4, thick = 17.53;
  const tip = 125, straightRun = 300, sag = 50; // tip width, straight part of the arc side, arc depth
  const hw = width / 2, hl = length / 2;
  const plate = s.extrude({ name: "plate", height: thick, profile: og.poly({ points: [
    [hw, -hl],
    [hw, hl],
    { x: hw - tip, y: hl, bulge: -sag }, // concave arc back to the straight run
    [-hw, -hl + straightRun],
    [-hw, -hl],
  ] }) });
  s.material(plate, { color: "#a0a0a0", roughness: 0.6 });
  return s;
}

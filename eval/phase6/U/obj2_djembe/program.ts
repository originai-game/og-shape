export default function (og) {
  const s = og.model({ name: "djembe" });
  const black = { color: "#151210", roughness: 0.6 };

  // shell: base cone, neck, goblet bowl (one wooden turned piece)
  const shell = s.revolve({ name: "shell", axis: "+Z", segments: 64, profile: og.poly({ points: [
    [0, 0], [122, 0], [74, 320], [72, 335], [132, 348], [166, 382], [186, 460], [193, 558], [0, 558],
  ] }) });
  s.material(shell, { color: "#6e2a10", roughness: 0.55 });

  // neck binding + band where the bowl meets the neck
  const neck = s.revolve({ name: "neck_binding", axis: "+Z", segments: 48, profile: og.poly({ points: [
    [60, 318], [80, 318], [80, 342], [136, 342], [136, 356], [60, 356],
  ] }) });
  s.material(neck, black);

  // tension strips up the bowl
  const strips = [];
  const frames = s.ring({ n: 18, radius: 0, on: og.world });
  frames.forEach((f, i) => {
    const w = s.wire({ name: `strip_${i}`, diameter: 4, segments: 8, on: f, path: [
      [136, 0, 352], [169, 0, 383], [189, 0, 460], [196, 0, 572],
    ] });
    strips.push(w);
  });
  const strip = strips[0];
  for (let i = 1; i < strips.length; i++) s.join(strip, strips[i]);
  s.material(strip, black);

  // band under the head
  const topBand = s.torus({ name: "top_band", radius: 194, tube: 5, segments: 64, tubeSegments: 12, axis: "+Z", on: og.world.move(0, 0, 551) });
  s.material(topBand, black);

  // leather head
  const head = s.revolve({ name: "head", axis: "+Z", segments: 64, profile: og.poly({ points: [
    [0, 556], [195, 556], [195, 586], [182, 600], [0, 600],
  ] }) });
  s.material(head, { color: "#c9b48a", roughness: 0.8 });

  // black hoop
  const hoop = s.torus({ name: "hoop", radius: 196, tube: 7.95, segments: 64, tubeSegments: 12, axis: "+Z", on: og.world.move(0, 0, 572) });
  s.material(hoop, black);
  return s;
}

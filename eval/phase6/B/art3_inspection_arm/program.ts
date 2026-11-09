export default function (og) {
  const s = og.model({ name: "inspection_arm" });
  const plateL = 456, plateW = 300, plateT = 25, plateShift = 78; // plate runs back past the column along +x
  const columnH = 355, collarH = 30;
  const pivotZ = plateT + columnH + collarH; // 410: underside of the first fork
  const forkW = 110, forkH = 70, capW = 80, capT = 8;
  const barD = 40, barT = 12, barZ = 30; // bar bottom above the fork underside
  const lengths = [700, 580, 480, 380];
  const swing = og.deg(77);

  const base = baseUnit();
  let parent = base;
  let x = 0;
  for (let i = 0; i < lengths.length; i++) {
    const seg = segment(i + 1, lengths[i], i === lengths.length - 1);
    const f = og.frame({ origin: [x, 0, pivotZ] });
    s.at(seg, f);
    s.joint(seg, { name: `hinge_${i + 1}`, parent, on: f, axis: "+Z", range: [-swing, swing] });
    parent = seg;
    x -= lengths[i];
  }
  return s;

  function baseUnit() {
    const plate = s.box({ name: "base", width: plateL, depth: plateW, height: plateT, on: og.world.move(plateShift, 0, 0) });
    const column = s.cylinder({ name: "column", radius: 35, height: columnH + 1, on: og.world.move(0, 0, plateT - 1) });
    s.join(plate, column);
    const collar = s.cylinder({ name: "collar", radius: 50, height: collarH + 1, on: og.world.move(0, 0, plateT + columnH - 1) });
    s.join(plate, collar);
    s.material(plate, { color: "#1a1a1a", roughness: 0.6 });
    return plate;
  }

  // Built about its own origin: the hinge axis is the z axis, the bar runs to -x.
  function segment(n, len, last) {
    const fork = s.box({ name: `fork_${n}`, width: forkW, depth: forkW, height: forkH });
    s.material(fork, { color: "#1a1a1a", roughness: 0.6 });
    const cap = s.box({ name: `cap_${n}`, width: capW, depth: capW, height: capT, on: og.world.move(0, 0, forkH - 0.5) });
    s.material(cap, { color: "#d35400", roughness: 0.5 });
    const bar = s.box({ name: `bar_${n}`, width: len, depth: barD, height: barT, on: og.world.move(-len / 2, 0, barZ) });
    s.material(bar, { color: "#b8bcc2", metalness: 1, roughness: 0.35 });
    const children = [fork, cap, bar];
    if (last) children.push(...camera(-len));
    return s.group({ name: `segment_${n}`, children });
  }

  // The camera seated on the bar end at (xEnd, 0), facing -X.
  function camera(xEnd) {
    const bodyL = 150, bodyW = 80, bodyH = 70;
    const body = s.box({ name: "camera", width: bodyL, depth: bodyW, height: bodyH, on: og.world.move(xEnd - bodyL / 2 + 20, 0, barZ + barT / 2 - bodyH / 2) });
    s.material(body, { color: "#1a1a1a", roughness: 0.5 });
    const front = s.face(body, "left");
    const lens = s.cylinder({ name: "lens", radius: 18, height: 31, on: front.offset(-1) });
    s.material(lens, { color: "#1f4e79", metalness: 0.5, roughness: 0.2 });
    const leds = [];
    let k = 0;
    for (const f of s.grid({ nx: 2, ny: 2, px: 56, py: 44, on: front })) {
      const led = s.cylinder({ name: `led_${++k}`, radius: 6, height: 6, on: f.offset(-1) });
      s.material(led, { color: "#ff8c1a", emissive: "#ff6a00" });
      leds.push(led);
    }
    return [body, lens, ...leds];
  }
}

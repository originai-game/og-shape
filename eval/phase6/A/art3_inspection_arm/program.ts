export default function (og) {
  const s = og.model({ name: "inspection_arm" });
  const lengths = [700, 580, 480, 380];
  const plateW = 400, plateD = 300, plateT = 25, plateX = 96;
  const pivotZ = 410; // top of the turntable collar: the forks sit here
  const swing = og.deg(77);

  const dark = { color: "#1b1b1d", metalness: 0.3, roughness: 0.6 };
  const orange = { color: "#d2601a", roughness: 0.5 };
  const alu = { color: "#b8bcc2", metalness: 1, roughness: 0.35 };

  const base = baseStand();
  let parent = base;
  let x = 0;
  for (let i = 0; i < lengths.length; i++) {
    const seg = segment(`segment_${i + 1}`, lengths[i], i === lengths.length - 1);
    const pivot = og.frame({ origin: [x, 0, pivotZ] });
    s.at(seg, pivot);
    s.joint(seg, { name: `hinge_${i + 1}`, parent, on: pivot, axis: "+Z", range: [-swing, swing] });
    parent = seg;
    x -= lengths[i];
  }
  return s;

  function baseStand(colR = 40, collarR = 70, collarH = 30) {
    const plate = s.box({ name: "floor_plate", width: plateW, depth: plateD, height: plateT, on: og.world.move(plateX, 0, 0) });
    const column = s.cylinder({ name: "column", radius: colR, height: pivotZ - collarH - plateT + 1, on: og.world.move(0, 0, plateT - 0.5) });
    s.join(plate, column);
    s.material(plate, dark);
    const collar = s.cylinder({ name: "turntable_collar", radius: collarR, height: collarH, on: og.world.move(0, 0, pivotZ - collarH) });
    s.material(collar, alu);
    return s.group({ name: "base", children: [plate, collar] });
  }

  // Built about its own origin: the pivot axis is the z axis, the bar runs along -X.
  function segment(name, len, last, forkW = 100, forkH = 70, capT = 8, barW = 40, barT = 12) {
    const fork = s.box({ name: `${name}_fork`, width: forkW, depth: forkW, height: forkH });
    s.material(fork, dark);
    const cap = s.box({ name: `${name}_cap`, width: forkW - 20, depth: forkW - 20, height: capT, on: s.face(fork, "top").offset(-0.5) });
    s.material(cap, orange);
    const barLen = len - forkW / 2 - (last ? 0 : forkW / 2);
    const bar = s.box({ name: `${name}_bar`, width: barLen + 1, depth: barW, height: barT,
      on: og.world.move(-forkW / 2 - barLen / 2, 0, forkH / 2 - barT / 2) });
    s.material(bar, alu);
    const children = [fork, cap, bar];
    if (last) children.push(...camera(-len, forkH / 2));
    return s.group({ name, children });
  }

  // Camera at the tip, centred on (tipX, 0, zc), looking -X.
  function camera(tipX, zc, w = 150, h = 60, lensR = 18, lensL = 20) {
    const body = s.box({ name: "camera_body", width: w, depth: h, height: h, on: og.world.move(tipX - w / 2 + 1, 0, zc - h / 2) });
    s.material(body, dark);
    const front = s.face(body, "left");
    const lens = s.cylinder({ name: "camera_lens", radius: lensR, height: lensL, on: front.offset(-0.5) });
    s.material(lens, { color: "#20364d", metalness: 0.6, roughness: 0.2 });
    const leds = [];
    for (const f of s.ring({ n: 4, radius: 34, start: og.deg(45), on: front })) {
      const led = s.cylinder({ name: `led_${leds.length + 1}`, radius: 4, height: 4, on: f.offset(-0.5) });
      s.material(led, { ...orange, emissive: "#ff7a1a" });
      leds.push(led);
    }
    return [body, lens, ...leds];
  }
}

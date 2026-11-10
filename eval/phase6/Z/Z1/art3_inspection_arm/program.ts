export default function (og) {
  const s = og.model({ name: "inspection_arm" });
  const lens = [700, 580, 480, 380];
  const gap = 60;                 // bar end to next hinge
  const plateW = 300, plateT = 20, colR = 40, colTop = 400;
  const collarR = 60, collarH = 31;
  const forkZ = colTop + collarH; // 431
  const hubW = 80, hubH = 45, capW = 64, capT = 12;
  const barW = 40, barT = 12, barZ = 440;
  const camL = 120, lensL = 37;
  const dark = "#1d1d20", orange = "#e8701a";

  const base = baseUnit();
  const hinges = [0];
  for (let i = 1; i < 4; i++) hinges.push(hinges[i - 1] + lens[i - 1] + gap);

  let parent = base;
  for (let i = 0; i < 4; i++) {
    const seg = segment(i);
    const f = og.world.move(hinges[i], 0, 0);
    s.at(seg, f);
    s.joint(seg, { name: `hinge_${i + 1}`, parent, on: f, axis: "+Z", range: [og.deg(-77), og.deg(77)] });
    parent = seg;
  }
  return s;

  function baseUnit() {
    const plate = s.box({ name: "floor_plate", width: plateW, depth: plateW, height: plateT });
    const col = s.cylinder({ name: "column", radius: colR, height: colTop - plateT + 1, on: og.world.move(0, 0, plateT - 1) });
    s.join(plate, col);
    const collar = s.cylinder({ name: "turntable_collar", radius: collarR, height: collarH + 1, on: og.world.move(0, 0, colTop - 1) });
    s.join(plate, collar);
    s.material(plate, { color: dark, roughness: 0.6 });
    return plate;
  }

  // Built about the hinge point (x = 0), arm along +x.
  function segment(i) {
    const n = i + 1;
    const L = lens[i];
    const kids = [];
    const hub = s.box({ name: `hub_${n}`, width: hubW, depth: hubW, height: hubH, on: og.world.move(0, 0, forkZ) });
    s.material(hub, { color: dark, roughness: 0.6 });
    const cap = s.box({ name: `cap_${n}`, width: capW, depth: capW, height: capT, on: og.world.move(0, 0, forkZ + hubH) });
    s.material(cap, { color: orange, roughness: 0.5 });
    const bar = s.box({ name: `segment_${n}`, width: L, depth: barW, height: barT, on: og.world.move(L / 2, 0, barZ) });
    s.material(bar, { color: "#b8bcc2", metalness: 1, roughness: 0.35 });
    kids.push(hub, cap, bar);
    if (i < 3) {
      const fl = 120;
      const fork = s.box({ name: `fork_${n}`, width: fl, depth: 100, height: 12, on: og.world.move(L + gap - 70 + fl / 2, 0, forkZ - 12) });
      const riser = s.box({ name: `fork_riser_${n}`, width: 20, depth: barW, height: barZ - forkZ + 13, on: og.world.move(L - 6, 0, forkZ - 1) });
      s.join(fork, riser);
      s.material(fork, { color: dark, roughness: 0.6 });
      kids.push(fork);
    } else {
      const ch = 60;
      const cam = s.box({ name: "camera", width: camL, depth: 60, height: ch, on: og.world.move(L + camL / 2 - 20, 0, barZ + barT / 2 - ch / 2) });
      s.material(cam, { color: dark, roughness: 0.5 });
      const front = L + camL - 20;
      const cz = barZ + barT / 2;
      const lensP = s.cylinder({ name: "lens", radius: 18, height: lensL, segments: 24, on: og.world.move(front - 1, 0, cz).turn(0, og.deg(90), 0) });
      s.material(lensP, { color: "#1e3a5a", metalness: 0.3, roughness: 0.15 });
      kids.push(cam, lensP);
      const leds = [[-1, -1], [1, -1], [-1, 1], [1, 1]];
      leds.forEach(([a, b], k) => {
        const led = s.cylinder({ name: `led_${k + 1}`, radius: 5, height: 6, segments: 12, on: og.world.move(front - 1, a * 22, cz + b * 20).turn(0, og.deg(90), 0) });
        s.material(led, { color: orange, emissive: orange });
        kids.push(led);
      });
    }
    return s.group({ name: `arm_${n}`, children: kids });
  }
}

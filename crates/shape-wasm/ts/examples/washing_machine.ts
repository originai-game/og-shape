/**
 * sand3d's washing machine (`programs/washing_machine.py` at
 * Sandflow-ai/sand3d 84c980a, Apache-2.0), rebuilt through the og-shape
 * binding: plan phase 4's acceptance.
 *
 * Millimetres; the front looks +X, Z up. Four joints, each on a frame
 * read on the cabinet: the door centred on the porthole, hinged at its
 * outer −Y edge so that it clears the cabinet (it opens outward about −Z,
 * 0–143°), the drum endless about the front's normal, the detergent
 * drawer sliding out 0–200 mm, the dial endless. The drawer's bay and the
 * dial's seat are cut by `seat`, as they sit at rest; the door's glass is
 * seated in a groove of its ring. Built at rest, then posed as sand3d's
 * `washer_open()`.
 *
 * Differences from the Python, all of convention:
 * - og-shape's face frames sit on the face's centroid; sand3d's on the
 *   world origin projected onto the face. `base` below is the front
 *   frame moved down to sand3d's origin, so every offset reads as in the
 *   original.
 * - Sizes are radii where sand3d takes diameters; angles are radians.
 * - Materials are written as three.js values (sand3d's presets: plastic,
 *   brushed_steel, painted_metal, glass), not looked up by preset name.
 *
 * Run: `bun examples/washing_machine.ts [outdir]` after `../build.sh`.
 * Writes `washing_machine.glb`, `washing_machine.png` and the recorded
 * program `washing_machine.json` into `outdir` (default `out/`).
 */

import { mkdirSync, writeFileSync } from "node:fs";
import { load, type Og, type Shape } from "../src/index.ts";

export function washer(og: Og, w = 600, d = 600, h = 850, porthole = 440, doorD = 480, drumD = 420): Shape {
  const s = og.model({ name: "washing_machine" });
  const cab = s.box({ name: "cabinet", width: w, depth: d, height: h });
  s.fillet(cab, { radius: 12, edges: "side" });
  const front = s.face(cab, "+X"); // X is +Y, Y is up
  const base = front.move(0, -h / 2); // sand3d's origin for this face
  s.pocket(cab, { profile: og.circle({ radius: porthole / 2 }), depth: d - 100, at: base.move(0, 400) });
  s.material(cab, { name: "plastic", color: "#f2f2f0", roughness: 0.5 });

  const back = base.move(0, 400).offset(-(d - 200)); // a solid's height runs along its frame's normal
  const drum = s.cylinder({ name: "drum", radius: drumD / 2, height: d - 220, on: back });
  s.cut(drum, s.cylinder({ name: "bore", radius: (drumD - 20) / 2, height: d - 229, on: back.offset(10) }));
  s.join(drum, s.cylinder({ name: "axle", radius: 20, height: 60, on: back.offset(-60) }));
  s.material(drum, { name: "brushed_steel", color: "#b4b7bb", roughness: 0.35, metalness: 1 });

  const ring = s.tube({ name: "ring", outer: doorD / 2, inner: 150, height: 45, on: base.move(0, 400) }); // centred on the porthole
  const glass = s.cylinder({ name: "glass", radius: 160, height: 16, on: base.move(0, 400).offset(14) });
  const hinge = base.move(-doorD / 2, 400).offset(45); // the ring's outer −Y edge: it clears the cabinet
  s.seat(ring, glass);
  s.material(ring, { name: "painted_metal", color: "#3a3d42", roughness: 0.4 });
  s.material(glass, { name: "glass", color: "#cfe3e6", roughness: 0, opacity: 0.4 });
  const handle = s.box({ name: "handle", width: 24, depth: 110, height: 30, on: s.bbox(ring, "+X").move(doorD / 2 - 40) });
  const door = s.group({ name: "door", children: [ring, glass, handle] }); // the handle and the glass move with it

  const topBand = base.move(0, h - 75);
  const drawer = s.box({ name: "drawer", width: 200, depth: 100, height: 231, on: topBand.move(150).offset(-230) });
  s.join(drawer, s.box({ name: "drawer_front", width: 220, depth: 120, height: 15, on: topBand.move(150) }));
  s.seat(cab, drawer);
  const dial = s.cylinder({ name: "dial", radius: 30, height: 30, on: topBand.move(-150).offset(-10) });
  s.seat(cab, dial);
  s.material([drawer, dial], { name: "plastic #e6e6e3", color: "#e6e6e3", roughness: 0.5 });

  s.joint(door, { name: "door_hinge", parent: cab, on: hinge, axis: "-Z", range: [0, og.deg(143)] });
  s.joint(drum, { name: "drum_spin", parent: cab, on: base.move(0, 400), kind: "continuous" });
  s.joint(drawer, { name: "drawer_slide", parent: cab, on: topBand.move(150), kind: "prismatic", axis: "+X", range: [0, 200] });
  s.joint(dial, { name: "dial_turn", parent: cab, on: topBand.move(-150), kind: "continuous" });
  s.assembly({ name: "washing_machine", children: [cab, drum, door, drawer, dial], anchors: { front: base } });
  return s;
}

/** sand3d's `washer_open()`. */
export function washerOpen(og: Og): Shape {
  const s = washer(og);
  s.pose("door_hinge", og.deg(92));
  s.pose("drawer_slide", 120);
  s.pose("dial_turn", og.deg(30));
  return s;
}

if (import.meta.main) {
  const out = process.argv[2] ?? "out";
  mkdirSync(out, { recursive: true });
  const og = await load();
  const t0 = performance.now();
  const s = washerOpen(og);
  const t1 = performance.now();
  writeFileSync(`${out}/washing_machine.glb`, s.glb());
  writeFileSync(`${out}/washing_machine.json`, s.program());
  const t2 = performance.now();
  writeFileSync(`${out}/washing_machine.png`, s.preview({ width: 320, views: 4, columns: 4, elevation: og.deg(20) }));
  const t3 = performance.now();
  const m = s.measure();
  console.log(
    JSON.stringify({
      build_ms: Math.round(t1 - t0),
      export_ms: Math.round(t2 - t1),
      preview_ms: Math.round(t3 - t2),
      parts: m.parts.map((p: any) => [p.name, p.triangles, p.closed]),
      triangles: m.triangles,
      joints: m.joints,
    }),
  );
}

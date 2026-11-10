/**
 * Stream Y's acceptance (`docs/research.md` §21): a plate with two holes
 * and a filleted top, a lofted handle joined on, every part with UVs, and
 * a checker map spanning 20 mm. Its recorded program is
 * `golden/programs/uv_plate.json`; `golden/programs/checker.png` is the map.
 *
 * Run: `bun examples/uv_plate.ts out.glb` (after `../build.sh`).
 */

import { readFileSync, writeFileSync } from "node:fs";
import { load, type Og, type Shape } from "../src/index.ts";

export function uvPlate(og: Og, checker: Uint8Array): Shape {
  const s = og.model({ name: "uv_plate", uvs: true, textures: { "checker.png": checker } });
  const plate = s.extrude({ name: "plate", profile: og.rect({ width: 80, height: 50, radius: 4 }), height: 6 });
  const above = (x: number) => og.frame({ origin: [x, 0, 10] });
  s.hole(plate, { diameter: 8, at: [above(-28), above(28)] });
  s.fillet(plate, { radius: 1.5, edges: ["top"] });
  const handle = s.loft({
    name: "handle",
    on: og.frame({ origin: [0, 0, 5] }),
    sections: [
      { profile: og.rect({ width: 30, height: 12, radius: 3 }), z: 0 },
      { profile: og.ellipse({ rx: 9, ry: 5 }), z: 12 },
      { profile: og.rect({ width: 16, height: 8, radius: 2 }), z: 24 },
    ],
    steps: 4,
  });
  s.join(plate, handle);
  s.material(plate, { color: "#ffffff", map: "checker.png", mapSize: 20 });
  return s;
}

if (import.meta.main) {
  const og = await load();
  const checker = readFileSync(new URL("../../../../golden/programs/checker.png", import.meta.url));
  const s = uvPlate(og, new Uint8Array(checker));
  writeFileSync(process.argv[2] ?? "uv_plate.glb", s.glb());
  console.log(s.program());
}

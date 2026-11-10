/**
 * Stream Y's acceptance in three.js (`docs/research.md` §21): the UV
 * plate (`examples/uv_plate.ts`) built through the binding, its GLB read
 * by GLTFLoader in headless Chromium, every vertex with a UV, the checker
 * map 64×64 with a repeat sampler; screenshots go to `docs/previews/uv/`
 * for a person to look at (never compared by pixel).
 *
 * Run: `node tools/check_uvs.ts [screenshot dir]` after `../build.sh` and
 * `bun install`. Needs Playwright with its Chromium.
 */

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { load } from "../src/index.ts";
import { uvPlate } from "../examples/uv_plate.ts";
import { build, serve } from "./consumers/serve.ts";

const ts = fileURLToPath(new URL("../", import.meta.url));
const repo = fileURLToPath(new URL("../../../../", import.meta.url));
const shots = process.argv[2] ?? `${repo}docs/previews/uv`;

let failures = 0;
function check(what: string, ok: boolean, detail = "") {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}${detail ? `: ${detail}` : ""}`);
  if (!ok) failures++;
}

mkdirSync(`${ts}out`, { recursive: true });
mkdirSync(shots, { recursive: true });
const og = await load();
const checker = new Uint8Array(readFileSync(`${repo}golden/programs/checker.png`));
const s = uvPlate(og, checker);
const m = s.measure();
const uv = m.parts[0].uv;
console.log(`     measure: coverage ${uv.coverage}, overlap ${uv.overlap}, flipped ${uv.flipped}, zero area ${uv.zeroArea}, outside ${uv.outside} of ${m.parts[0].triangles}`);
check("measure: no flipped or flat UV triangle", uv.flipped === 0 && uv.zeroArea === 0);
writeFileSync(`${ts}out/uv_plate.glb`, s.glb());

await build({ entry: "tools/consumers/page_uvs.ts", out: "out/page_uvs.js", platform: "browser" });
const server = await serve(ts, "/out/page_uvs.js");
const { chromium } = createRequire(import.meta.url)("playwright");
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
  const errors: string[] = [];
  page.on("pageerror", (e: Error) => errors.push(e.message));
  await page.goto(server.url);
  await page.waitForFunction(() => (window as any).ready === true, null, { timeout: 60_000 });
  for (const [name, az] of [["plate_checker", 0.9], ["plate_checker_back", 3.6]] as const) {
    const meshes: any[] = await page.evaluate((a: number) => (window as any).check.plate(a), az);
    if (name === "plate_checker") {
      for (const x of meshes) console.log(`     ${JSON.stringify(x)}`);
      check("one mesh", meshes.length === 1);
      check("a UV on every vertex", meshes.every((x) => x.uvs === x.vertices && x.finite));
      check("the checker map, 64×64, repeating", meshes.every((x) => x.map?.[0] === 64 && x.map?.[1] === 64 && x.wrap?.[0] && x.wrap?.[1]));
    }
    await page.locator("canvas").screenshot({ path: `${shots}/${name}.png` });
  }
  check("no page errors", errors.length === 0, errors.join("; "));
} finally {
  await browser.close();
  server.close();
}
console.log(failures === 0 ? "all checks pass" : `${failures} checks failed`);
process.exit(failures === 0 ? 0 : 1);

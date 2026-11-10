/**
 * three.js with pixels (research §19.2): the washing machine's GLB and
 * phase 5b's skinned figure loaded by GLTFLoader in headless Chromium,
 * moved, posed and rendered by WebGLRenderer.
 *
 * Checks by measurement, never by comparing images:
 * - the scene graph: joint nodes and their `s3d_joint` extras, bones;
 * - joint motion: the open file re-posed to rest in three.js equals the
 *   rest file, and the rest file posed open equals the open file; the
 *   door swings out and the drawer slides 120 mm;
 * - the skin: the figure posed by the kernel's turns, every vertex where
 *   the kernel's `pose` puts it, vertices that share a rest position
 *   still together (no tear), and the largest edge stretch.
 * Screenshots go to `docs/previews/validate/` for a person to look at.
 *
 * Run: `node tools/check_render.ts [screenshot dir]` after `../build.sh`
 * and `bun install`; it runs `cargo run -p shape-skin --example
 * figure_poses` for the figure. Needs Playwright with its Chromium.
 */

import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { load } from "../src/index.ts";
import { washer, washerOpen } from "../examples/washing_machine.ts";
import { build, serve } from "./consumers/serve.ts";

const ts = fileURLToPath(new URL("../", import.meta.url));
const repo = fileURLToPath(new URL("../../../../", import.meta.url));
const shots = process.argv[2] ?? `${repo}docs/previews/validate`;

let failures = 0;
function check(what: string, ok: boolean, detail = "") {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}${detail ? `: ${detail}` : ""}`);
  if (!ok) failures++;
}

mkdirSync(`${ts}out`, { recursive: true });
mkdirSync(shots, { recursive: true });
const og = await load();
writeFileSync(`${ts}out/washer_rest.glb`, washer(og).glb());
writeFileSync(`${ts}out/washer_open.glb`, washerOpen(og).glb());
execFileSync(
  "cargo",
  ["run", "--release", "--locked", "-q", "-p", "shape-skin", "--example", "figure_poses", "--", `${ts}out/figure.glb`, `${ts}out/figure_poses.json`],
  { cwd: repo, stdio: "inherit" },
);

await build({ entry: "tools/consumers/page_render.ts", out: "out/page_render.js", platform: "browser" });
const server = await serve(ts, "/out/page_render.js");
const { chromium } = createRequire(import.meta.url)("playwright");
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 640, height: 480 } });
  const errors: string[] = [];
  page.on("pageerror", (e: Error) => errors.push(e.message));
  await page.goto(server.url);
  await page.waitForFunction(() => (window as any).ready === true, null, { timeout: 60_000 });
  const gl: string = await page.evaluate(() => {
    const g = document.querySelector("canvas")!.getContext("webgl2")!;
    const info = g.getExtension("WEBGL_debug_renderer_info");
    return info ? g.getParameter(info.UNMASKED_RENDERER_WEBGL) : g.getParameter(g.RENDERER);
  });
  console.log(`Chromium ${browser.version()}, WebGL: ${gl}`);
  const shot = async (name: string) => {
    await page.locator("canvas").screenshot({ path: `${shots}/${name}.png` });
  };

  // The washing machine.
  const w: any = await page.evaluate(() => (window as any).check.washer());
  console.log(w.tree.map((l: string) => `     ${l}`).join("\n"));
  check("four joint nodes with s3d_joint extras", w.joints.length === 4, w.joints.map((j: any) => `${j.name} ${j.type}`).join(", "));
  check("open file re-posed to rest in three.js = rest file", w.openToRest < 1e-6, `${w.openToRest.toExponential(2)} m`);
  check("rest file posed open in three.js = open file", w.restToOpen < 1e-6, `${w.restToOpen.toExponential(2)} m`);
  check("door closed on the front at rest", Math.abs(w.doorRest[0] - 0.3) < 1e-4 && Math.abs(w.doorRest[1] - 0.375) < 1e-4, w.doorRest.map((x: number) => x.toFixed(4)).join("–"));
  await page.evaluate(() => (window as any).check.showWasher("rest"));
  await shot("washer_rest");
  await page.evaluate(() => (window as any).check.showWasher("open"));
  await shot("washer_open");
  const moved: any = await page.evaluate(() => (window as any).check.moveWasher({ door_hinge: (143 * Math.PI) / 180, drawer_slide: 0.2, dial_turn: Math.PI / 2 }));
  check("door at 143° in three.js swings out past the front", moved.doorMaxX > 0.6, `max x ${moved.doorMaxX.toFixed(3)} m`);
  check("drawer at its 200 mm limit", Math.abs(moved.drawerFront - w.drawerFront - 0.2) < 1e-6, `${(moved.drawerFront - w.drawerFront).toFixed(6)} m`);
  await shot("washer_moved_in_three");

  // The skinned figure.
  const f: any = await page.evaluate(() => (window as any).check.figure());
  console.log(`     figure: ${f.vertices} GLB vertices (${f.kernelVertices} in the kernel), ${f.triangles} triangles, bones ${f.bones.join(", ")}`);
  check("10 bones in the skeleton", f.bones.length === 10, String(f.bones.length));
  for (const name of ["rest", "bend", "wave"]) {
    const r: any = await page.evaluate((n) => (window as any).check.poseFigure(n), name);
    await shot(`figure_${name}`);
    check(`${name}: every GLB vertex has a kernel vertex`, r.unmatched === 0, `${r.unmatched} unmatched`);
    check(`${name}: three.js skinning = kernel pose`, r.maxToKernel < 1e-5, `${(r.maxToKernel * 1000).toExponential(2)} mm`);
    check(`${name}: no tear (shared rest positions stay together)`, r.sharedRestMaxApart < 1e-6 && r.kernelSplit === 0, `${(r.sharedRestMaxApart * 1000).toExponential(2)} mm apart, ${r.kernelSplit} kernel splits`);
    console.log(`     ${name}: largest edge stretch ×${r.maxEdgeStretch.toFixed(3)}`);
  }
  check("no page errors", errors.length === 0, errors.join("; "));
} finally {
  await browser.close();
  server.close();
}
console.log(failures === 0 ? "render: all checks pass" : `render: ${failures} failed`);
process.exit(failures === 0 ? 0 : 1);

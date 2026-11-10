/**
 * Textured renders of mesh files in three.js, side by side (stream AG,
 * `docs/research.md` §28): a generated mesh as its file has it (every
 * map, vertex colours) next to what the mesh tools made of it. For a
 * person to look at, never compared by pixel.
 *
 *   node tools/render_glb.ts <out.png> <tile px> <azimuth rad> <file> [<file>...]
 *
 * Files are copied under `out/render/` (git-ignored) and served from
 * there with what lies beside them (an OBJ's MTL and texture). Prints,
 * per file, the triangles and maps three.js read. Needs `bun install`
 * and Playwright with its Chromium.
 */

import { cpSync, mkdirSync, readdirSync } from "node:fs";
import { createRequire } from "node:module";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { build, serve } from "./consumers/serve.ts";

const ts = fileURLToPath(new URL("../", import.meta.url));
const [out, tileArg, azArg, ...files] = process.argv.slice(2);
if (!out || files.length === 0) {
  console.error("usage: node tools/render_glb.ts <out.png> <tile px> <azimuth rad> <file>...");
  process.exit(2);
}
const tile = Number(tileArg);
const urls: string[] = [];
files.forEach((f, i) => {
  const dir = join(ts, "out/render", String(i));
  mkdirSync(dir, { recursive: true });
  // The file, and for an OBJ what lies beside it.
  const beside = f.endsWith(".obj") ? readdirSync(dirname(f)) : [basename(f)];
  for (const b of beside) cpSync(join(dirname(f), b), join(dir, b));
  urls.push(`/out/render/${i}/${basename(f)}`);
});

await build({ entry: "tools/consumers/page_glb.ts", out: "out/page_glb.js", platform: "browser" });
const server = await serve(ts, "/out/page_glb.js");
const { chromium } = createRequire(import.meta.url)("playwright");
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: tile * files.length, height: tile } });
  const errors: string[] = [];
  page.on("pageerror", (e: Error) => errors.push(e.message));
  await page.goto(server.url);
  await page.waitForFunction(() => (window as any).ready === true, null, { timeout: 60_000 });
  const read = await page.evaluate(
    ([u, a, t]: [string[], number, number]) => (window as any).view.render(u, a, t),
    [urls, Number(azArg), tile] as [string[], number, number],
  );
  read.forEach((r: any, i: number) => console.log(`${files[i]}\t${r.triangles}\t${r.maps.join(",")}`));
  await page.locator("canvas").screenshot({ path: out });
  if (errors.length) {
    console.error(errors.join("\n"));
    process.exitCode = 1;
  }
} finally {
  await browser.close();
  server.close();
}

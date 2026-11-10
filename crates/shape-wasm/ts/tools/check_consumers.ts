/**
 * The binding outside `bun test` (research §19.1): the washing machine's
 * GLB and PNG must hash as `golden/hashes.txt` says in every consumer.
 *
 * - `node`: this runtime imports `src/index.ts` as it is (node ≥ 22.18
 *   strips the types; bun runs it too);
 * - `bundle`: esbuild bundles the binding for node, and node runs it;
 * - `browser`: esbuild bundles it for the browser, and headless Chromium
 *   (Playwright) loads the page, fetches the wasm and hashes in the page.
 *
 * Run: `node tools/check_consumers.ts [node] [bundle] [browser]` (all
 * three by default) after `../build.sh`; `bundle` and `browser` need
 * `bun install` (esbuild), `node` needs nothing else (the gate runs it). `browser`
 * needs Playwright with its Chromium (`require("playwright")`). Exits 1
 * on any failure; a consumer that cannot run here is reported as NOT RUN
 * and fails too, so a green run means every consumer named was checked.
 */

import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { load } from "../src/index.ts";
import { type WasherHashes, washerHashes } from "./consumers/golden.ts";

const ts = fileURLToPath(new URL("../", import.meta.url));
const root = new URL("../../../../", import.meta.url);

function golden(name: string): string {
  const line = readFileSync(new URL("golden/hashes.txt", root), "utf8")
    .split("\n")
    .find((l) => l.startsWith(`${name} `));
  if (!line) throw new Error(`no golden line ${name}`);
  return line.split(" ")[1];
}

const want = { glb: golden("model.washing_machine.glb"), png: golden("model.washing_machine.png") };
let failures = 0;

function report(consumer: string, h: WasherHashes | { error: string }) {
  if ("error" in h) {
    console.log(`FAIL ${consumer}: ${h.error}`);
    failures++;
    return;
  }
  const ok = h.glb === want.glb && h.png === want.png;
  console.log(
    `${ok ? "ok  " : "FAIL"} ${consumer}: GLB ${h.glb === want.glb ? "golden" : h.glb}, PNG ${h.png === want.png ? "golden" : h.png} (${h.ms.toFixed(0)} ms)`,
  );
  if (!ok) failures++;
}

async function inProcess() {
  const runtime = typeof (globalThis as { Bun?: unknown }).Bun === "undefined" ? `node ${process.version}` : "bun";
  report(`${runtime}, src/index.ts as it is`, await washerHashes(await load()));
}

async function bundle() {
  const { build } = await import("./consumers/serve.ts");
  // Next to pkg/'s parent, so that the binding's `../pkg/` finds the wasm.
  await build({ entry: "tools/consumers/node_entry.ts", out: "out/node_entry.mjs", platform: "node" });
  const out = execFileSync(process.execPath, [`${ts}out/node_entry.mjs`], { encoding: "utf8" });
  report(`esbuild bundle (${process.execPath.endsWith("bun") ? "bun" : "node"})`, JSON.parse(out));
}

async function browser() {
  const { build, serve } = await import("./consumers/serve.ts");
  let chromium: any;
  try {
    chromium = createRequire(import.meta.url)("playwright").chromium;
  } catch {
    console.log("FAIL browser: NOT RUN: Playwright is not installed here");
    failures++;
    return;
  }
  await build({ entry: "tools/consumers/page_hash.ts", out: "out/page_hash.js", platform: "browser" });
  const server = await serve(ts, "/out/page_hash.js");
  const b = await chromium.launch();
  try {
    const page = await b.newPage();
    const errors: string[] = [];
    page.on("pageerror", (e: Error) => errors.push(e.message));
    await page.goto(server.url);
    await page.waitForFunction(() => (window as { result?: unknown }).result !== undefined, null, { timeout: 120_000 });
    const r: any = await page.evaluate(() => (window as { result?: unknown }).result);
    report(`Chromium ${b.version()} (esbuild bundle)`, errors.length ? { error: errors.join("; ") } : r);
    if (r.located) console.log(`     an error in the page reads: ${r.located}`);
  } finally {
    await b.close();
    server.close();
  }
}

const steps = process.argv.slice(2);
const all = steps.length === 0;
mkdirSync(`${ts}out`, { recursive: true });
if (all || steps.includes("node")) await inProcess();
if (all || steps.includes("bundle")) await bundle();
if (all || steps.includes("browser")) await browser();
console.log(failures === 0 ? "consumers: all match golden/hashes.txt" : `consumers: ${failures} failed`);
process.exit(failures === 0 ? 0 : 1);

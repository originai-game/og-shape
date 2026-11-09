/**
 * The phase 6 evaluation's runner: builds one og-shape TypeScript program
 * through the binding and writes what an author reviews.
 *
 *   bun eval/phase6/run.ts <program.ts> <out dir> [--pose joint=value joint=value ...]
 *
 * Writes `<out>/program.json` (the recorded program), `<out>/preview.png`
 * (four views, at rest) and, with `--pose`, `<out>/posed.png` (the same
 * model with those joints posed: radians or millimetres). Prints the
 * measures an author checks: bounds, triangles, and per part closed,
 * components, non-manifold edges, degenerate triangles and triangles. Every run is appended to `<out>/runs.log`, so
 * the review iterations can be counted. Needs the binding built
 * (`crates/shape-wasm/build.sh`).
 */

import { appendFileSync, mkdirSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { load } from "../../crates/shape-wasm/ts/src/index.ts";

const [programPath, outDir, ...rest] = process.argv.slice(2);
if (!programPath || !outDir) {
  console.error("usage: bun eval/phase6/run.ts <program.ts> <out dir> [--pose joint=value ...]");
  process.exit(2);
}
const poses: [string, number][] = [];
for (const arg of rest) {
  if (arg === "--pose") continue;
  const [j, v] = arg.split("=");
  if (!j || v === undefined || !Number.isFinite(Number(v))) {
    console.error(`not a joint=value pose: ${arg}`);
    process.exit(2);
  }
  poses.push([j, Number(v)]);
}
mkdirSync(outDir, { recursive: true });
const log = (line: string) => appendFileSync(`${outDir}/runs.log`, `${new Date().toISOString()} ${line}\n`);

let shape;
try {
  const mod = await import(resolve(programPath));
  const og = await load();
  shape = await mod.default(og);
  if (!shape || typeof shape.program !== "function") {
    throw new Error("the default export must return the Shape it built (`og.model({ name })`)");
  }
} catch (e) {
  log(`error ${String((e as Error).message ?? e).split("\n")[0]}`);
  console.error(`error: ${(e as Error).message ?? e}`);
  process.exit(1);
}

writeFileSync(`${outDir}/program.json`, shape.program());
writeFileSync(`${outDir}/preview.png`, shape.preview({ width: 320, views: 4, columns: 2 }));
const m = shape.measure();
const size = m.boundsMax.map((v: number, k: number) => +(v - m.boundsMin[k]).toFixed(2));
const parts = m.parts.map((p: any) => ({
  name: p.name,
  closed: p.closed,
  components: p.components,
  nonManifoldEdges: p.nonManifoldEdges,
  degenerateTriangles: p.degenerateTriangles,
  triangles: p.triangles,
  boundsMin: p.boundsMin.map((v: number) => +v.toFixed(2)),
  boundsMax: p.boundsMax.map((v: number) => +v.toFixed(2)),
}));
const report = {
  triangles: m.triangles,
  boundsMin: m.boundsMin.map((v: number) => +v.toFixed(2)),
  boundsMax: m.boundsMax.map((v: number) => +v.toFixed(2)),
  size,
  joints: m.joints,
  parts,
};
if (poses.length) {
  try {
    for (const [j, v] of poses) shape.pose(j, v);
    writeFileSync(`${outDir}/posed.png`, shape.preview({ width: 320, views: 4, columns: 2 }));
  } catch (e) {
    console.error(`pose error: ${(e as Error).message}`);
  }
}
writeFileSync(`${outDir}/measure.json`, JSON.stringify(report, null, 1));
const open = parts.filter((p: any) => !p.closed).length;
log(`ok parts ${parts.length} open ${open} joints ${m.joints.length} triangles ${m.triangles}`);
console.log(JSON.stringify(report));

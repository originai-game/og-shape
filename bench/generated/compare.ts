// Before and after for docs/research.md §32 (stream AK): two run.ts
// results side by side.
//
//   bun bench/generated/compare.ts BEFORE/results.jsonl AFTER/results.jsonl
//
// Markdown on stdout: per mode and budget, within budget, closed, both,
// files whose output crosses itself, time and peak memory; crossing
// pairs before the chain and after it, per file; what the repair did to
// the open files; why the keep runs over budget stopped. Counts and
// measures only.
import { readFileSync } from "node:fs";

const load = (p: string): any[] =>
  readFileSync(p, "utf8")
    .trim()
    .split("\n")
    .map((l) => JSON.parse(l));
const [before, after] = [load(process.argv[2]), load(process.argv[3])];

const med = (xs: number[]) => {
  if (!xs.length) return NaN;
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
};
const f = (x: number, d = 0) => (Number.isFinite(x) ? x.toFixed(d) : "–");
const k = (x: number) => (x >= 1000 ? `${f(x / 1000, x >= 10000 ? 0 : 1)}k` : f(x));
const count = <T>(xs: T[], p: (x: T) => boolean) => xs.filter(p).length;
const run = (r: any, mode: string, b: number) => r.runs.find((x: any) => x.mode === mode && x.budget === b);
const crossing = (x: any) => x?.measures?.self_intersections ?? 0;

console.log("### The chain, before and after\n");
console.log(
  "| mode | budget | within budget | closed | within budget and closed | output crosses itself (files; pairs, total) | time (median, max s) | peak memory (median, max MiB) |",
);
console.log("|---|---|---|---|---|---|---|---|");
for (const mode of ["keep", "fresh"]) {
  for (const b of [2000, 10000, 50000]) {
    const cells = [before, after].map((rows) => {
      const xs = rows.map((r) => run(r, mode, b)).filter((x) => x?.ok);
      const m = xs.map((x) => x.measures);
      const ms = rows.map((r) => run(r, mode, b)?.ms ?? 0);
      const peak = rows.map((r) => run(r, mode, b)?.peak_mib ?? 0);
      return {
        within: count(m, (x) => x.triangles <= b),
        closed: count(m, (x) => x.closed),
        both: count(m, (x) => x.triangles <= b && x.closed),
        crossing: `${count(xs, (x) => crossing(x) > 0)}; ${xs.reduce((a, x) => a + crossing(x), 0)}`,
        time: `${f(med(ms) / 1000, 1)}, ${f(Math.max(...ms) / 1000, 1)}`,
        mem: `${med(peak)}, ${Math.max(...peak)}`,
      };
    });
    const [x, y] = cells;
    const pair = (key: keyof (typeof cells)[0]) => `${x[key]} → ${y[key]}`;
    console.log(
      `| ${mode} | ${k(b)} | ${pair("within")} | ${pair("closed")} | ${pair("both")} | ${pair("crossing")} | ${pair("time")} | ${pair("mem")} |`,
    );
  }
}

console.log("\n### Crossing pairs, per file: the input, and the keep and fresh outputs at 10k (before → after)\n");
console.log("| file | input | keep 10k | fresh 10k | keep 2k | collapses refused for folding (keep 10k) |");
console.log("|---|---|---|---|---|---|");
for (const a of after) {
  const b = before.find((r) => r.name === a.name);
  const cells = [
    ["keep", 10000],
    ["fresh", 10000],
    ["keep", 2000],
  ].map(([mode, budget]) => `${crossing(run(b, mode as string, budget as number))} → ${crossing(run(a, mode as string, budget as number))}`);
  const input = a.measure.self_intersections ?? 0;
  const any = [input, ...[b, a].flatMap((r) => [run(r, "keep", 10000), run(r, "fresh", 10000), run(r, "keep", 2000)].map(crossing))];
  if (any.every((x) => x === 0)) continue;
  console.log(`| ${a.name} | ${input} | ${cells.join(" | ")} | ${run(a, "keep", 10000)?.decimate?.folds_refused ?? "–"} |`);
}

console.log("\n### The repair on the files that come in with non-manifold edges or holes left (keep 10k)\n");
console.log(
  "| file | non-manifold edges in | split; unpaired; left; crossings added | holes left (before → after) | large holes filled | fuse | closed (before → after) |",
);
console.log("|---|---|---|---|---|---|---|");
for (const a of after) {
  const b = before.find((r) => r.name === a.name);
  const [rb, ra] = [run(b, "keep", 10000), run(a, "keep", 10000)];
  const nm = ra?.repair?.non_manifold;
  if (!(a.measure.non_manifold_edges > 0 || rb?.repair?.holes_left > 0 || ra?.repair?.holes_left > 0)) continue;
  const fuse = (x: any) => (x?.repair?.fused?.refused ? "refused" : x?.repair?.fused ? "ok" : "–");
  console.log(
    `| ${a.name} | ${a.measure.non_manifold_edges} | ${nm ? `${nm.split}; ${nm.unpaired}; ${nm.left}; ${nm.crossings_added}` : "–"} | ${rb?.repair?.holes_left} → ${ra?.repair?.holes_left} | ${ra?.repair?.large_holes_filled ?? "–"} | ${fuse(rb)} → ${fuse(ra)} | ${rb?.measures?.closed ? "yes" : "no"} → ${ra?.measures?.closed ? "yes" : "no"} |`,
  );
}

console.log("\n### Pieces and pockets (keep 10k)\n");
console.log("| file | pieces in | pieces out (before → after) | pockets the untangling left | dropped as floaters | fuse time (keep 10k run, s; before → after) |");
console.log("|---|---|---|---|---|---|");
for (const a of after) {
  const b = before.find((r) => r.name === a.name);
  const [rb, ra] = [run(b, "keep", 10000), run(a, "keep", 10000)];
  if (!(ra?.repair?.pockets > 0) && rb?.measures?.pieces === ra?.measures?.pieces) continue;
  console.log(
    `| ${a.name} | ${a.measure.pieces} | ${rb?.measures?.pieces} → ${ra?.measures?.pieces} | ${ra?.repair?.pockets ?? "–"} | ${ra?.repair?.pockets_dropped ?? "–"} | ${f((rb?.ms ?? NaN) / 1000, 1)} → ${f((ra?.ms ?? NaN) / 1000, 1)} |`,
  );
}

console.log("\n### Keep runs over budget: where they stopped and why (after)\n");
console.log("| file | budget | triangles | UV islands → charts | edges left: fixed / link / turn / uv / fold / not retried |");
console.log("|---|---|---|---|---|");
for (const a of after) {
  for (const b of [2000, 10000, 50000]) {
    const x = run(a, "keep", b);
    if (!x?.ok || x.measures.triangles <= b || !x.decimate) continue;
    const bl = x.decimate.blocked;
    console.log(
      `| ${a.name} | ${k(b)} | ${x.decimate.triangles} | ${x.decimate.uv_islands} → ${x.decimate.uv_charts} | ${bl.fixed} / ${bl.link} / ${bl.turn} / ${bl.uv} / ${bl.fold} / ${bl.not_retried} |`,
    );
  }
}

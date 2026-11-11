// Prints the tables of docs/research.md §34 from bake.ts's output.
//
//   bun bench/generated/bake-table.ts OUT/bake.jsonl
import { readFileSync } from "node:fs";

const rows = readFileSync(process.argv[2], "utf8")
  .split("\n")
  .filter((l) => l.trim())
  .map((l) => JSON.parse(l));

const med = (v: number[]) => {
  const s = v.filter((x) => Number.isFinite(x)).sort((a, b) => a - b);
  return s.length ? s[Math.floor((s.length - 1) / 2)] : NaN;
};
const max = (v: number[]) => Math.max(...v.filter((x) => Number.isFinite(x)));
const f = (x: number, d = 1) => (Number.isFinite(x) ? x.toFixed(d) : "–");

const runs = rows.flatMap((r) => r.runs.map((x: any) => ({ ...x, name: r.name, generator: r.generator })));
const within = (x: any) => x.ok && x.triangles <= x.budget;

console.log("| mode | budget | ran | within budget | closed | within budget and closed | base-colour map out | normal map out | vertex colours out | look: mean, p95 (median over files) | texture out (median MiB) | time (median, max s) | peak memory (median, max MiB) |");
console.log("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
for (const b of [2000, 10000]) {
  for (const mode of ["keep", "bake"]) {
    const xs = runs.filter((x) => x.mode === mode && x.budget === b);
    const ok = xs.filter((x) => x.ok);
    console.log(
      `| ${mode} | ${b / 1000}k | ${ok.length}/${xs.length} | ${xs.filter(within).length} | ${ok.filter((x) => x.closed).length} | ${xs.filter((x) => within(x) && x.closed).length} | ${ok.filter((x) => x.maps.includes("map")).length} | ${ok.filter((x) => x.maps.includes("normal_map")).length} | ${ok.filter((x) => x.colors).length} | ${f(med(ok.map((x) => x.look?.mean)))}, ${f(med(ok.map((x) => x.look?.p95)))} | ${f(med(ok.map((x) => x.texture_mib)))} | ${f(med(ok.map((x) => x.ms / 1000)))}, ${f(max(ok.map((x) => x.ms / 1000)))} | ${f(med(ok.map((x) => x.peak_mib)), 0)}, ${f(max(ok.map((x) => x.peak_mib)), 0)} |`,
    );
  }
}

console.log("\nWhere both reach the budget (same files), the look's error, median of mean and of p95:\n");
console.log("| budget | files | keep: mean, p95 | bake: mean, p95 | bake lower mean (files) |");
console.log("|---|---|---|---|---|");
for (const b of [2000, 10000]) {
  const pairs = rows
    .map((r) => {
      const k = r.runs.find((x: any) => x.mode === "keep" && x.budget === b);
      const k2 = r.runs.find((x: any) => x.mode === "bake" && x.budget === b);
      return within(k) && within(k2) && k.look && k2.look ? [k, k2] : null;
    })
    .filter(Boolean) as any[];
  console.log(
    `| ${b / 1000}k | ${pairs.length} | ${f(med(pairs.map((p) => p[0].look.mean)))}, ${f(med(pairs.map((p) => p[0].look.p95)))} | ${f(med(pairs.map((p) => p[1].look.mean)))}, ${f(med(pairs.map((p) => p[1].look.p95)))} | ${pairs.filter((p) => p[1].look.mean < p[0].look.mean).length} |`,
  );
}

console.log("\nPer generator, bake (2k / 10k): within budget and closed; look mean (median); normal map's angle without → with (median of means, °); texels with no source (total); taken from the nearest point (median %)\n");
console.log("| generator | files | within and closed 2k | 10k | look mean 2k | 10k | normals without → with, 2k | no source (2k) | nearest % (2k) | facing away % (2k) | ambiguous % (2k) | atlas coverage (median) | density min / median (median ratio) |");
console.log("|---|---|---|---|---|---|---|---|---|---|---|---|---|");
const gens = [...new Set(rows.map((r) => r.generator))];
for (const g of gens) {
  const xs = runs.filter((x) => x.generator === g && x.mode === "bake");
  const a = xs.filter((x) => x.budget === 2000);
  const c = xs.filter((x) => x.budget === 10000);
  const pct = (x: any, k: string) => (x.bake ? (100 * x.bake[k]) / x.bake.texels_covered : NaN);
  console.log(
    `| ${g} | ${a.length} | ${a.filter((x) => within(x) && x.closed).length} | ${c.filter((x) => within(x) && x.closed).length} | ${f(med(a.map((x) => x.look?.mean)))} | ${f(med(c.map((x) => x.look?.mean)))} | ${f(med(a.map((x) => x.bake?.normals?.mean_degrees_without_map)))} → ${f(med(a.map((x) => x.bake?.normals?.mean_degrees)), 2)} | ${a.reduce((s, x) => s + (x.bake?.no_source ?? 0), 0)} | ${f(med(a.map((x) => pct(x, "nearest"))), 2)} | ${f(med(a.map((x) => pct(x, "facing_away"))), 2)} | ${f(med(a.map((x) => pct(x, "ambiguous"))), 2)} | ${f(med(a.map((x) => x.unwrap?.measures?.coverage)), 2)} | ${f(med(a.map((x) => x.unwrap?.measures?.density_texels_per_mm?.min / x.unwrap?.measures?.density_texels_per_mm?.median)), 2)} |`,
  );
}

console.log("\nPer file at 2k: keep's triangles, bake's; look mean / p95 keep and bake; bake's counts\n");
console.log("| file | keep triangles | bake triangles | closed (bake) | look keep | look bake | normals without → with (°) | no source | nearest | facing away | ambiguous | largest / limit (mm) | time keep → bake (s) | peak keep → bake (MiB) |");
console.log("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|");
for (const r of rows) {
  const k = r.runs.find((x: any) => x.mode === "keep" && x.budget === 2000);
  const b = r.runs.find((x: any) => x.mode === "bake" && x.budget === 2000);
  if (!k || !b) continue;
  const bk = b.bake ?? {};
  console.log(
    `| ${r.name} | ${k.triangles}${k.triangles > 2000 ? " (over)" : ""} | ${b.triangles} | ${b.closed} | ${f(k.look?.mean)} / ${f(k.look?.p95)} | ${f(b.look?.mean)} / ${f(b.look?.p95)} | ${f(bk.normals?.mean_degrees_without_map)} → ${f(bk.normals?.mean_degrees, 2)} | ${bk.no_source} | ${bk.nearest} | ${bk.facing_away} | ${bk.ambiguous} | ${f(bk.largest_distance)} / ${f(bk.distance)} | ${f(k.ms / 1000)} → ${f(b.ms / 1000)} | ${k.peak_mib} → ${b.peak_mib} |`,
  );
}

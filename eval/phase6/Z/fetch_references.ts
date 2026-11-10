/**
 * The reference meshes for run `Z`'s surface distance (research §22).
 *
 *   node eval/phase6/Z/fetch_references.ts <cache dir> [--record]
 *
 * `--record` looks each object's row up in the Hugging Face dataset
 * server and writes its `usda_path`, `usda_sha256` and `usda_bytes` into
 * `references.json` (done once). Without it, downloads every object's
 * OpenUSD file into `<cache dir>/<object>.usda` and checks its SHA-256
 * against `references.json`. The files are the datasets' (CC BY 4.0) and
 * are not committed; only their path and digest are.
 */
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const refPath = fileURLToPath(new URL("../references.json", import.meta.url));
const refs = JSON.parse(readFileSync(refPath, "utf8"));
const [cache, flag] = process.argv.slice(2);
if (!cache) throw new Error("usage: fetch_references.ts <cache dir> [--record]");

if (flag === "--record") {
  for (const [object, r] of Object.entries<any>(refs)) {
    let row: any;
    for (const split of ["validation", "train"]) {
      const where = encodeURIComponent(`"id"='${r.id}'`);
      const url = `https://datasets-server.huggingface.co/filter?dataset=${r.dataset}&config=default&split=${split}&where=${where}&length=1`;
      let d: any = {};
      for (let attempt = 0; attempt < 4 && !d.rows; attempt++) {
        // The dataset server answers an error object now and then.
        if (attempt) await new Promise((r) => setTimeout(r, 2000 * attempt));
        d = await (await fetch(url)).json().catch(() => ({}));
      }
      if (d.rows?.length) {
        row = d.rows[0].row;
        break;
      }
    }
    if (!row) throw new Error(`${object}: no row for ${r.id}`);
    r.usda_path = row.usda_path;
    r.usda_sha256 = row.usda_sha256;
    r.usda_bytes = Number(row.usda_bytes);
    console.log(`${object}: ${row.usda_path}`);
  }
  writeFileSync(refPath, `${JSON.stringify(refs, null, 1)}\n`);
} else {
  mkdirSync(cache, { recursive: true });
  for (const [object, r] of Object.entries<any>(refs)) {
    const file = `${cache}/${object}.usda`;
    if (!existsSync(file)) {
      const url = `https://huggingface.co/datasets/${r.dataset}/resolve/main/${r.usda_path}`;
      const res = await fetch(url);
      if (!res.ok) throw new Error(`${object}: ${res.status} ${url}`);
      writeFileSync(file, Buffer.from(await res.arrayBuffer()));
    }
    const sha = createHash("sha256").update(readFileSync(file)).digest("hex");
    if (sha !== r.usda_sha256) throw new Error(`${object}: sha256 ${sha} is not ${r.usda_sha256}`);
    console.log(`${object}: ok`);
  }
}

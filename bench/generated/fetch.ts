// Downloads the generated meshes listed in manifest.tsv into DIR, one
// directory per entry (DIR/<name>/), checking each file's SHA-256.
//
//   bun bench/generated/fetch.ts DIR [--print-hashes]
//
// An entry whose `member` column is set is a file inside a remote zip:
// only that member is fetched, by HTTP range requests on the zip's
// central directory and the member's own bytes, so a 1 GB archive costs
// a few MB. Several members (an OBJ, its MTL and its texture) are
// separated by `;`; the first is the mesh. `member=name` saves a member
// under another name (an OBJ whose `mtllib` names its MTL differently
// from the archive). The meshes are never
// committed (docs/research.md §28). A generic client: no headers beyond
// Range, no credentials.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { inflateRawSync } from "node:zlib";

type Entry = {
  name: string;
  generator: string;
  category: string;
  licence: string;
  url: string;
  members: string[];
  sha256: string[];
};

export function readManifest(path: string): Entry[] {
  const out: Entry[] = [];
  for (const line of readFileSync(path, "utf8").split("\n")) {
    if (!line.trim() || line.startsWith("#")) continue;
    const [name, generator, category, licence, url, member, sha] = line.split("\t");
    out.push({
      name,
      generator,
      category,
      licence,
      url,
      members: member && member !== "-" ? member.split(";") : [],
      sha256: sha && sha !== "-" ? sha.split(";") : [],
    });
  }
  return out;
}

async function get(url: string, range?: [number, number]): Promise<Uint8Array> {
  for (let attempt = 0; ; attempt++) {
    try {
      const r = await fetch(url, range ? { headers: { Range: `bytes=${range[0]}-${range[1]}` } } : {});
      if (range ? r.status !== 206 : !r.ok) throw new Error(`${url}: HTTP ${r.status}`);
      return new Uint8Array(await r.arrayBuffer());
    } catch (e) {
      if (attempt >= 3) throw e;
      await new Promise((ok) => setTimeout(ok, 2000 << attempt));
    }
  }
}

type Member = { method: number; csize: number; offset: number };
const directories = new Map<string, Map<string, Member>>();

// The zip's central directory (zip64 too), cached per URL.
async function directory(url: string): Promise<Map<string, Member>> {
  const known = directories.get(url);
  if (known) return known;
  const head = await fetch(url, { method: "HEAD" });
  const size = Number(head.headers.get("content-length"));
  const start = Math.max(0, size - 65558);
  const tail = await get(url, [start, size - 1]);
  const t = new DataView(tail.buffer, tail.byteOffset, tail.byteLength);
  let eocd = -1;
  for (let i = tail.length - 22; i >= 0; i--) {
    if (t.getUint32(i, true) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error(`${url}: no zip directory`);
  let count = t.getUint16(eocd + 10, true);
  let cdSize = t.getUint32(eocd + 12, true);
  let cdOffset = t.getUint32(eocd + 16, true);
  if (cdOffset === 0xffffffff || count === 0xffff) {
    const z64 = Number(t.getBigUint64(eocd - 20 + 8, true));
    const z = await get(url, [z64, z64 + 55]);
    const d = new DataView(z.buffer, z.byteOffset, z.byteLength);
    count = Number(d.getBigUint64(32, true));
    cdSize = Number(d.getBigUint64(40, true));
    cdOffset = Number(d.getBigUint64(48, true));
  }
  const cd = await get(url, [cdOffset, cdOffset + cdSize - 1]);
  const c = new DataView(cd.buffer, cd.byteOffset, cd.byteLength);
  const map = new Map<string, Member>();
  let p = 0;
  for (let k = 0; k < count; k++) {
    const method = c.getUint16(p + 10, true);
    let csize = c.getUint32(p + 20, true);
    let usize = c.getUint32(p + 24, true);
    const nameLen = c.getUint16(p + 28, true);
    const extraLen = c.getUint16(p + 30, true);
    const commentLen = c.getUint16(p + 32, true);
    let offset = c.getUint32(p + 42, true);
    const name = new TextDecoder().decode(cd.subarray(p + 46, p + 46 + nameLen));
    for (let x = p + 46 + nameLen; x < p + 46 + nameLen + extraLen; ) {
      const id = c.getUint16(x, true);
      const len = c.getUint16(x + 2, true);
      if (id === 1) {
        let q = x + 4;
        if (usize === 0xffffffff) {
          usize = Number(c.getBigUint64(q, true));
          q += 8;
        }
        if (csize === 0xffffffff) {
          csize = Number(c.getBigUint64(q, true));
          q += 8;
        }
        if (offset === 0xffffffff) offset = Number(c.getBigUint64(q, true));
      }
      x += 4 + len;
    }
    map.set(name, { method, csize, offset });
    p += 46 + nameLen + extraLen + commentLen;
  }
  directories.set(url, map);
  return map;
}

async function member(url: string, name: string): Promise<Uint8Array> {
  const m = (await directory(url)).get(name);
  if (!m) throw new Error(`${url}: no member ${name}`);
  const local = await get(url, [m.offset, m.offset + 29]);
  const l = new DataView(local.buffer, local.byteOffset, local.byteLength);
  const dataAt = m.offset + 30 + l.getUint16(26, true) + l.getUint16(28, true);
  const data = await get(url, [dataAt, dataAt + m.csize - 1]);
  if (m.method === 0) return data;
  if (m.method === 8) return new Uint8Array(inflateRawSync(data));
  throw new Error(`${name}: zip method ${m.method}`);
}

const sha = (b: Uint8Array) => createHash("sha256").update(b).digest("hex");

if (import.meta.main) {
  const dir = process.argv[2];
  if (!dir) {
    console.error("usage: bun bench/generated/fetch.ts DIR [--print-hashes]");
    process.exit(2);
  }
  const print = process.argv.includes("--print-hashes");
  const manifest = readManifest(join(dirname(new URL(import.meta.url).pathname), "manifest.tsv"));
  let bad = 0;
  for (const e of manifest) {
    const into = join(dir, e.name);
    mkdirSync(into, { recursive: true });
    const names = e.members.length ? e.members : [e.url];
    const hashes: string[] = [];
    for (let i = 0; i < names.length; i++) {
      const [source, saveAs] = names[i].split("=");
      const file = join(into, saveAs ?? basename(source));
      let bytes: Uint8Array;
      if (existsSync(file)) bytes = new Uint8Array(readFileSync(file));
      else {
        bytes = e.members.length ? await member(e.url, source) : await get(e.url);
        writeFileSync(file, bytes);
      }
      const h = sha(bytes);
      hashes.push(h);
      if (!print && e.sha256[i] && e.sha256[i] !== h) {
        console.error(`${e.name}: ${basename(file)}: SHA-256 ${h}, manifest ${e.sha256[i]}`);
        bad++;
      }
    }
    if (print) console.log(`${e.name}\t${hashes.join(";")}`);
  }
  if (bad) process.exit(1);
}

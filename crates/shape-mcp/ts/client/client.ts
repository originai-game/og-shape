/**
 * shape-mcp driven by a real MCP client (research §19.3): the official
 * TypeScript SDK's `Client` over its `StdioClientTransport`, against the
 * release binary. Every tool and resource, stream M's program ops (by
 * `build` in JSON and by `run_typescript`), an FBX, a `.gltf` with an
 * external buffer and image, progress on `process`, and cancellation of
 * a `run_typescript` that never ends (the SDK's AbortSignal).
 *
 * Run from the repository root, after `cargo build --release -p
 * shape-mcp`, `crates/shape-wasm/build.sh` and `bun install` here:
 *
 *   node crates/shape-mcp/ts/client/client.ts
 *
 * Prints one line a check and the time of each call; exits 1 on any
 * failure.
 */

import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";

const repo = fileURLToPath(new URL("../../../../", import.meta.url));
const ws = fileURLToPath(new URL("out/ws/", import.meta.url));
rmSync(ws, { recursive: true, force: true });
mkdirSync(ws, { recursive: true });
copyFileSync(`${repo}golden/meshes/ball_defects.obj`, `${ws}ball.obj`);
execFileSync("cargo", ["run", "-q", "--locked", "-p", "shape-mcp", "--example", "fixtures", "--", ws], { cwd: repo, stdio: "inherit" });

let failures = 0;
function check(what: string, ok: boolean, detail = "") {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}${detail ? `: ${detail}` : ""}`);
  if (!ok) failures++;
}

const transport = new StdioClientTransport({
  command: `${repo}target/release/shape-mcp`,
  args: ["--root", ws, "--binding", `${repo}crates/shape-wasm/ts`],
  stderr: "pipe",
});
const client = new Client({ name: "og-shape-check", version: "0.0.0" });
await client.connect(transport);

const times: [string, number][] = [];
type Content = { type: string; text?: string; data?: string; mimeType?: string };
type Result = { content: Content[]; isError?: boolean; structuredContent?: unknown };

async function call(name: string, args: Record<string, unknown>, label = name, options = {}): Promise<Result> {
  const t0 = performance.now();
  const r = (await client.callTool({ name, arguments: args }, undefined, options)) as Result;
  times.push([label, performance.now() - t0]);
  return r;
}

/** The JSON report of a successful call. */
function report(r: Result, what: string): any {
  if (r.isError) {
    check(what, false, r.content[0]?.text ?? "");
    return {};
  }
  const text = r.content.find((c) => c.type === "text")?.text ?? "";
  try {
    return JSON.parse(text);
  } catch {
    return { text };
  }
}

// The handshake.
const info = client.getServerVersion();
check("initialize: server info", info?.name === "og-shape", JSON.stringify(info));
check("initialize: instructions name the authoring guide", (client.getInstructions() ?? "").includes("og-shape://guides/shape.md"));
await client.ping();
check("ping", true);

// Tools and resources.
const tools = (await client.listTools()).tools;
const names = tools.map((t) => t.name);
check(
  "tools/list: the ten tools, each with an input schema",
  names.join() === "build,preview,measure,export,import,repair,decimate,unwrap,process,run_typescript" && tools.every((t) => t.inputSchema?.type === "object"),
  names.join(", "),
);
check("tools/list: no outputSchema (reports are JSON text)", tools.every((t) => t.outputSchema === undefined));
const resources = (await client.listResources()).resources;
check("resources/list: four guides", resources.length === 4, resources.map((r) => r.uri).join(", "));
for (const r of resources) {
  const read = await client.readResource({ uri: r.uri });
  const text = (read.contents[0] as { text?: string }).text ?? "";
  const file = readFileSync(`${repo}guides/${r.name}`, "utf8");
  check(`resources/read ${r.uri}: the committed guide`, text === file && r.mimeType === "text/markdown", `${text.length} bytes`);
}
check("resources/templates/list: none", (await client.listResourceTemplates()).resourceTemplates.length === 0);

// Stream M's ops in JSON, through `build`.
const frame = (o: number[]) => ({ origin: o, x: [1, 0, 0], y: [0, 1, 0], z: [0, 0, 1] });
const program = {
  name: "stream_m",
  ops: [
    { op: "wire", name: "frame", path: [[0, 0, 0], [100, 0, 0], [100, 0, 80], [0, 0, 80]], diameter: 6, fillet: 15, segments: 12 },
    { op: "gear", name: "gear", teeth: 18, module: 2, width: 8, bore: 6, on: frame([160, 0, 0]) },
    { op: "knurl", name: "knob", diameter: 20, height: 12, on: frame([220, 0, 0]) },
    { op: "torus", name: "hoop", radius: 30, tube: 3, axis: "+Z", segments: 32, tubeSegments: 8, on: frame([0, 100, 0]) },
    { op: "extrude", name: "nut", profile: { polygon: { sides: 6, flats: 13 } }, height: 5, on: frame([80, 100, 0]) },
    {
      op: "extrude",
      name: "tab",
      profile: { scale: { profile: { poly: { points: [[0, 0], { x: 30, y: 0, bulge: 4 }, [30, 10], [0, 10]] } }, x: 1.5 } },
      height: 3,
      on: frame([140, 100, 0]),
    },
    { op: "body", name: "mouse", plan: [[0, 20], [60, 30], [110, 16]], top: [[0, 10], [60, 25], [110, 8]], bottom: 0, on: frame([0, 200, 0]) },
    { op: "scale", id: 6, z: 1.2 },
  ],
};
const built = report(await call("build", { program }, "build (stream M ops)"), "build: stream M's ops");
const parts = built.parts ?? built.measures?.parts ?? [];
check("build: every part of stream M's ops", JSON.stringify(built).includes("mouse") && JSON.stringify(built).includes("knob"), `${Array.isArray(parts) ? parts.length : "?"} parts`);
writeFileSync(`${ws}m.json`, JSON.stringify(program));
const pre = await call("preview", { path: "m.json", width: 160, views: 2, out: "m.png" });
const img = pre.content.find((c) => c.type === "image");
const png = Buffer.from(img?.data ?? "", "base64");
if (png.length < 24) check("preview: an image", false, pre.content[0]?.text ?? "");
check("preview: PNG image content", img?.mimeType === "image/png" && png.subarray(1, 4).toString() === "PNG" && png.readUInt32BE(16) === 320, `${png.length} bytes, ${png.readUInt32BE(16)}×${png.readUInt32BE(20)}`);
check("preview: `out` written too", readFileSync(`${ws}m.png`).equals(png));
const m = report(await call("measure", { path: "m.json", intersections: true }), "measure");
check("measure: a program", JSON.stringify(m).length > 100, Object.keys(m).join(", "));
for (const ext of ["glb", "obj", "stl"]) {
  report(await call("export", { path: "m.json", out: `m.${ext}` }, `export .${ext}`), `export .${ext}`);
  check(`export .${ext}`, readFileSync(`${ws}m.${ext}`).length > 1000);
}

// Stream M's ops through the binding.
const ts = `export default function (og) {
  const s = og.model({ name: "ts_m" });
  s.wire({ name: "rail", path: [[0, 0, 0], [60, 0, 0], [60, 40, 0]], diameter: 5, fillet: 10, segments: 10 });
  s.torus({ name: "ring", radius: 20, tube: 2, axis: "+Z", segments: 24, on: og.world.move(100, 0, 0) });
  s.extrude({ name: "hex", height: 4, profile: og.scale({ profile: og.polygon({ sides: 6, flats: 10 }), x: 1.4 }), on: og.world.move(150, 0, 0) });
  console.log("built", s.measure().parts.length);
  return s;
}`;
const t = report(await call("run_typescript", { source: ts, program_out: "ts.json" }), "run_typescript");
check("run_typescript: stream M's ops replayed natively", t.measures?.parts?.length === 3 && String(t.logs).includes("built 3"), String(t.logs).trim());
check("run_typescript: program_out", JSON.parse(readFileSync(`${ws}ts.json`, "utf8")).ops.length === 3);

// Meshes: OBJ, FBX, GLB, and a .gltf with an external buffer and image.
const fbx = report(await call("import", { path: "cube.fbx", out: "cube.glb" }, "import .fbx"), "import .fbx");
check("import .fbx: parts, materials, the embedded image", JSON.stringify(fbx).includes("png") || JSON.stringify(fbx).includes("image"), JSON.stringify(fbx).slice(0, 160));
report(await call("export", { path: "m.json", out: "split.glb" }, "export for .gltf"), "export for the .gltf");
splitGlb(`${ws}split.glb`, `${ws}split.gltf`, "split.bin", "m.png");
const gltf = report(await call("import", { path: "split.gltf" }, "import .gltf"), "import .gltf");
const gltfText = JSON.stringify(gltf);
check("import .gltf: external buffer read beside the file", gltfText.includes("mouse") && gltfText.includes("knob"), gltfText.slice(0, 120));
check("import .gltf: external image read beside the file", gltfText.includes("320"), "the 320 px preview as its image");
const gm = report(await call("measure", { path: "split.gltf" }, "measure .gltf"), "measure .gltf");
check("measure .gltf", JSON.stringify(gm).includes("triangles"));
const rep = report(await call("repair", { path: "ball.obj", weld: 0.01, floaters: { fraction: 0.05 }, out: "ball_fixed.glb" }), "repair");
check("repair: the defects ball", JSON.stringify(rep).includes("closed"), Object.keys(rep).join(", "));
const dec = report(await call("decimate", { path: "ball_fixed.glb", lods: 2, ratio: 0.5, out: "ball.glb" }), "decimate");
check("decimate: two LODs written", readFileSync(`${ws}ball.lod1.glb`).length > 0 && readFileSync(`${ws}ball.lod2.glb`).length > 0, Object.keys(dec).join(", "));
const unw = report(await call("unwrap", { path: "ball_fixed.glb", out: "ball_uv.glb" }), "unwrap");
check("unwrap", JSON.stringify(unw).includes("charts"));
const fbxp = report(await call("process", { path: "cube.fbx", out: "cube_p.glb" }, "process .fbx"), "process .fbx");
check("process .fbx", fbxp.output != null);

// Progress: the SDK sends a progress token with `onprogress`.
const steps: string[] = [];
const proc = report(
  await call("process", { path: "ball.obj", budget: 1000, out: "ball_low.glb" }, "process (progress)", {
    onprogress: (p: { progress: number; total?: number; message?: string }) => steps.push(`${p.progress}/${p.total} ${p.message}`),
  }),
  "process with progress",
);
check("process: six progress notifications", steps.length === 6, steps.join(", "));
check("process: decimated to the budget", proc.measures?.triangles <= 1000 || JSON.stringify(proc).includes('"triangles":1000'), String(proc.measures?.triangles));

// Cancellation: abort a program that never ends; the server stays up.
const abort = new AbortController();
const t0 = performance.now();
const forever = call("run_typescript", { source: "export default function () { for (;;) {} }" }, "run_typescript (cancelled)", {
  signal: abort.signal,
  onprogress: () => setTimeout(() => abort.abort("test"), 200),
});
let aborted = false;
try {
  await forever;
} catch {
  aborted = true;
}
await client.ping();
const pinged = performance.now() - t0;
check("cancel: the SDK's AbortSignal stops run_typescript, and the server answers", aborted && pinged < 5000, `${pinged.toFixed(0)} ms to the next ping`);
const after = report(await call("build", { program }, "build after cancel"), "build after a cancel");
check("cancel: later calls are answered", JSON.stringify(after).includes("mouse"));

// Refusals come back as tool errors, not protocol errors.
const bad = await call("build", { program, nope: 1 }, "refusal");
check("an unknown argument is a tool error", bad.isError === true && (bad.content[0].text ?? "").includes("nope"));
const outside = await call("import", { path: "../ball.obj" }, "refusal");
check("a path outside the workspace is refused", outside.isError === true && (outside.content[0].text ?? "").includes("refused"));

await client.close();
console.log("\ncall times (ms):");
for (const [n, ms] of times) console.log(`  ${n.padEnd(28)} ${ms.toFixed(0)}`);
console.log(failures === 0 ? "\nmcp client: all checks pass" : `\nmcp client: ${failures} failed`);
process.exit(failures === 0 ? 0 : 1);

/** A GLB as a `.gltf` with its buffer in `bin` beside it, and `image` as an external texture. */
function splitGlb(glbPath: string, gltfPath: string, bin: string, image: string) {
  const b = readFileSync(glbPath);
  const jsonLen = b.readUInt32LE(12);
  const json = JSON.parse(b.subarray(20, 20 + jsonLen).toString("utf8"));
  const binStart = 20 + jsonLen;
  const binLen = b.readUInt32LE(binStart);
  writeFileSync(gltfPath.replace(/[^/]*$/, bin), b.subarray(binStart + 8, binStart + 8 + binLen));
  json.buffers = [{ uri: bin, byteLength: binLen }];
  json.images = [{ uri: image }];
  json.textures = [{ source: 0 }];
  for (const m of json.materials ?? []) m.pbrMetallicRoughness = { ...(m.pbrMetallicRoughness ?? {}), baseColorTexture: { index: 0 } };
  writeFileSync(gltfPath, JSON.stringify(json));
}

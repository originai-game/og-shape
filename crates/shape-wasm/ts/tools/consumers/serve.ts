/**
 * Bundling and serving for the consumer checks: esbuild bundles an entry
 * (node or browser), and a small static server serves this package's
 * directory with a page that runs one module.
 */

import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";
import * as esbuild from "esbuild";

const ts = fileURLToPath(new URL("../../", import.meta.url));

/**
 * In a browser bundle, `node:` modules are never reached (the wasm is
 * passed to `load`, and the example's file writing runs only as a
 * script); they bundle as stubs that throw.
 */
const nodeStubs: esbuild.Plugin = {
  name: "node-stubs",
  setup(b) {
    b.onResolve({ filter: /^node:/ }, (a) => ({ path: a.path, namespace: "node-stub" }));
    b.onLoad({ filter: /.*/, namespace: "node-stub" }, (a) => ({
      contents: `const no = () => { throw new Error("${a.path} in a browser"); };
        export const readFileSync = no, writeFileSync = no, mkdirSync = no;
        export default {};`,
      loader: "js",
    }));
  },
};

export interface Build {
  /** The entry, relative to the package. */
  entry: string;
  /** The bundle, relative to the package. */
  out: string;
  platform: "node" | "browser";
}

/** Bundles `entry` into one ES module. */
export async function build(d: Build): Promise<void> {
  await esbuild.build({
    entryPoints: [join(ts, d.entry)],
    outfile: join(ts, d.out),
    bundle: true,
    format: "esm",
    platform: d.platform,
    target: "es2022",
    // A bundle is one module: the example's `if (import.meta.main)` script
    // would run as part of whatever imports it.
    define: { "import.meta.main": "false" },
    plugins: d.platform === "browser" ? [nodeStubs] : [],
    logLevel: "warning",
  });
}

const types: Record<string, string> = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".wasm": "application/wasm",
  ".glb": "model/gltf-binary",
  ".json": "application/json",
};

export interface Served {
  url: string;
  close(): void;
}

/** Serves `dir` on a free local port; `/` is a page that runs `module`. */
export function serve(dir: string, module: string): Promise<Served> {
  const server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url ?? "/", "http://x").pathname);
    if (path === "/") {
      res.writeHead(200, { "content-type": "text/html" });
      res.end(`<!doctype html><meta charset="utf-8"><body style="margin:0"><script type="module" src="${module}"></script>`);
      return;
    }
    const file = normalize(join(dir, path));
    if (!file.startsWith(dir)) {
      res.writeHead(403).end();
      return;
    }
    try {
      const body = await readFile(file);
      res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" });
      res.end(body);
    } catch {
      res.writeHead(404).end();
    }
  });
  return new Promise((resolve) =>
    server.listen(0, "127.0.0.1", () => {
      const a = server.address();
      const port = typeof a === "object" && a ? a.port : 0;
      resolve({ url: `http://127.0.0.1:${port}/`, close: () => server.close() });
    }),
  );
}

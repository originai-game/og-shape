/**
 * The golden check every consumer runs (node, bun, a browser, a bundle):
 * the washing machine, posed as `washer_open()`, its GLB and its preview
 * PNG hashed with the gate's framing. WebCrypto only, so the same module
 * runs in a page.
 */

import type { Og } from "../../src/index.ts";
import { washerOpen } from "../../examples/washing_machine.ts";

/** The gate's framing: SHA-256 over the length (u64, little-endian) and the bytes. */
export async function goldenHash(bytes: Uint8Array): Promise<string> {
  const framed = new Uint8Array(8 + bytes.length);
  new DataView(framed.buffer).setBigUint64(0, BigInt(bytes.length), true);
  framed.set(bytes, 8);
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", framed));
  return Array.from(digest, (b) => b.toString(16).padStart(2, "0")).join("");
}

export interface WasherHashes {
  glb: string;
  png: string;
  /** Milliseconds to build the model, write the GLB and render the preview. */
  ms: number;
}

/** The hashes of `model.washing_machine.glb` and `model.washing_machine.png`. */
export async function washerHashes(og: Og): Promise<WasherHashes> {
  const t0 = performance.now();
  const s = washerOpen(og);
  const glb = s.glb();
  const png = s.preview({ width: 320, views: 4, columns: 4, elevation: og.deg(20) });
  const ms = performance.now() - t0;
  return { glb: await goldenHash(glb), png: await goldenHash(png), ms };
}

/** The browser side of `check_consumers.ts`: loads the wasm by fetch and hashes the washing machine. */

import { load } from "../../src/index.ts";
import { washerHashes } from "./golden.ts";

declare global {
  interface Window {
    result?: unknown;
  }
}

try {
  const wasm = await (await fetch("/pkg/shape_wasm_bg.wasm")).arrayBuffer();
  const og = await load(wasm);
  const hashes = await washerHashes(og);
  // An error from the kernel says where the author's call was.
  let located = "";
  try {
    og.model({ name: "e" }).cylinder({ name: "rod", radius: 1, height: 2, segmentz: 8 } as never);
  } catch (e) {
    located = (e as Error).message;
  }
  window.result = { ...hashes, located, userAgent: navigator.userAgent };
} catch (e) {
  window.result = { error: String((e as Error).stack ?? e) };
}

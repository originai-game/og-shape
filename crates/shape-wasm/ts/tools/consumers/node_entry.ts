/** A node consumer through a bundle: prints the washing machine's hashes as JSON. */

import { load } from "../../src/index.ts";
import { washerHashes } from "./golden.ts";

const og = await load();
console.log(JSON.stringify(await washerHashes(og)));

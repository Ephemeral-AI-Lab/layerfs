// The native addon: beside the package when installed, at the root of a
// worktree build bundle (tools/worktree.py), else the one
// scripts/build-native.mjs builds beside its crate in this repository.
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const candidates = [
  fileURLToPath(new URL("../layerfs.node", import.meta.url)),
  fileURLToPath(new URL("../../../layerfs.node", import.meta.url)),
  fileURLToPath(new URL("../../../engine/crates/layerfs-node/layerfs.node", import.meta.url)),
];
const found = candidates.find((path) => existsSync(path));
if (!found) {
  throw new Error(`the LayerFS native addon is not built; run node api/scripts/build-native.mjs (looked in ${candidates.join(", ")})`);
}

/** `{ table(), call(name, argsJson, home, scopeJson, onEvent) }`. */
export const native = require(found);

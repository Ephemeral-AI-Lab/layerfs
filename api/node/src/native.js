// The native addon: the `layerfs.node` beside the package's `src`, or the
// file `LAYERFS_NATIVE` names. Nothing else is tried: a missing addon is an
// error, never a different build found elsewhere. `node api/scripts/build-native.mjs`
// builds the package's own.
import { existsSync } from "node:fs";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const explicit = process.env.LAYERFS_NATIVE;
const path = explicit ? explicit : fileURLToPath(new URL("../layerfs.node", import.meta.url));
const where = `${process.platform}/${process.arch}, Node ${process.version}`;
const build = "build it with `node api/scripts/build-native.mjs` (or `npm --prefix api run build:native`) in the LayerFS repository, or point LAYERFS_NATIVE at a built addon";

if (!existsSync(path)) {
  throw new Error(`the LayerFS native addon is missing: ${path} (${explicit ? "named by LAYERFS_NATIVE" : "the package's own"}; this is ${where}); ${build}`);
}

/** `{ table(), call(name, argsJson, home, scopeJson, onEvent), doctor(home, optionsJson?), close() }`;
 * `call` resolves to JSON `{ events, answer }`. */
export let native;
try {
  native = require(path);
} catch (cause) {
  throw new Error(`the LayerFS native addon at ${path} could not be loaded on ${where}: ${cause.message}; ${build}`, { cause });
}

// The LayerFS SDK of a ported Live project (macOS): the real SDK of the
// LayerFS build (LAYERFS_BUNDLE), with its native calls run in the Store
// container (binding.js). The dsh Host gives Live bash calls on macOS this as
// DSH_LAYERFS_SDK, so their scripts do not change.
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { setting } from "./env.js";

process.env.LAYERFS_NATIVE = fileURLToPath(new URL("./binding.js", import.meta.url));
const bundle = setting("BUNDLE");
if (!bundle) throw new Error("LAYERFS_BUNDLE is not set: the ported SDK runs the SDK of a LayerFS build");

export const sdkEntry = (dir) => pathToFileURL(join(dir, "sdk/js/src/index.js")).href;
export const operatorEntry = (dir) => pathToFileURL(join(dir, "sdk/js/operator/index.js")).href;
export const { LayerFS, LayerFSError, isRefusal, CODES } = await import(sdkEntry(bundle));

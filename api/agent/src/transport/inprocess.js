// The in-process transport: the real native addon in this process, from
// LAYERFS_NATIVE, else engine/crates/layerfs-node/layerfs.node. Store calls open the
// project's Store here (Linux only, #352 F-020).
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);

// Every binding call runs on Node's worker pool, four threads unless the
// process sets UV_THREADPOOL_SIZE. A Sandbox exec holds its thread for the
// command's whole run, so with four agents' commands running every other call
// queued behind them (#352 F-017). Node reads the size when the pool first
// starts; a process that set it keeps its own value.
export const WORKERS = 64;
process.env.UV_THREADPOOL_SIZE ??= String(WORKERS);

/** The addon file this process loads. */
export const addonPath = () => process.env.LAYERFS_NATIVE || fileURLToPath(new URL("../../../../engine/crates/layerfs-node/layerfs.node", import.meta.url));

/** The loaded addon. */
export const inprocess = () => require(addonPath());

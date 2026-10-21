// The ported SDK's controls for an adapter's own process: the Store container
// (start it, name it, where it keeps a project's Store and its connection
// file), the hook a call that finds it down tells, and the frames its worker
// speaks. The ported SDK itself is `@layerfs/port/sdk` (and `/operator`).
export { buildOf, DAEMON_HOST, ensureBox, storeFile, storeName, storePathOf, VOLUMES } from "./box.js";
export { onUnreachable, UNREACHABLE } from "./binding.js";
export { frame, readFrames } from "./frames.mjs";

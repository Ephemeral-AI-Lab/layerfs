// The LayerFS SDK of this process; the only place Linux and macOS differ.
// LayerFS makes Store calls only in a Linux process: on Linux this process
// makes them with the SDK beside this package; on macOS it uses the ported
// SDK (port/), whose calls run in the LayerFS home's Store container, started
// and restarted here as the dsh Host starts its own.
const PORTED = process.platform === "darwin";

/** The loaded SDK's refusal test (each SDK has its own LayerFSError). */
export let isRefusal;

/**
 * A client of `project` (no Store or Docker call yet) and the start of what
 * its calls need: on macOS the Store container, which is started again when a
 * call finds it down.
 */
export async function connect(project) {
  const sdk = await import(PORTED ? "../../../port/src/sdk.js" : "../../../sdk/js/src/index.js");
  ({ isRefusal } = sdk);
  const port = PORTED ? await import("../../../port/src/index.js") : null;
  const report = (error) => console.error(`layerfs: the Store container did not start again: ${error.message}`);
  return { lf: new sdk.LayerFS(project), start: async () => port?.useStoreContainer(report) };
}

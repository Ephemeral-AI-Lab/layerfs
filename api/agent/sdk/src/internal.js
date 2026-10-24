// @layerfs/agent/internal: NOT a public API. It exists only for
// @layerfs/backend and @layerfs/runtime at the same exact version as this
// package (1.1.0); it may change in any release without notice. Integrations
// and agent scripts use the public entry points (@layerfs/agent,
// @layerfs/backend, @layerfs/runtime). Don't add new uses.
// It exports exactly:
//   from client.js: STATE, invokeBackend, invokeClient, invokeClientBackend,
//     live, outcome, page, pageOf, projectArgs, sandboxArg, storeArgs,
//     workspaceArgs, deployment
//   from transport/frames.js: decode, encode, frame, readFrames
//   from transport/worker_client.js: connection, endpointOf
//   from transport/inprocess.js: addonPath
//   from transport/index.js: useWorker
export { STATE, invokeBackend, invokeClient, invokeClientBackend, live, outcome, page, pageOf, projectArgs, sandboxArg, storeArgs, workspaceArgs, deployment } from "./client.js";
export { decode, encode, frame, readFrames } from "./transport/frames.js";
export { connection, endpointOf } from "./transport/worker_client.js";
export { addonPath } from "./transport/inprocess.js";
export { useWorker } from "./transport/index.js";

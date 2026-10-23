// backend.daemon.process.start: one background command in a Workspace,
// through its Sandbox's command launcher (the daemon supervises no command):
// { process, ... }, an id `proc_<sandbox>_<hex>` that names its Sandbox.
import { invokeClientBackend, workspaceArgs } from "@layerfs/agent/internal";
export function start(shop, ws, command) {
  return invokeClientBackend(shop, "backend.daemon.process.start", { ...workspaceArgs(ws), command, background: true });
}

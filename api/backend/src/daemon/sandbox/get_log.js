// backend.daemon.sandbox.get_log: the last `lines` lines (default 200, at
// most 2000; at most 64 KiB) of one Sandbox's daemon log, read once through
// the Docker Engine by the binding, so it works where this process has no
// Docker access (macOS and Windows: the worker reads it). A stopped Sandbox's
// log is readable until its records are reclaimed: { sandbox, container,
// text, bytes }. Refused NoSuchSandbox when the project has no such Sandbox.
import { LayerFSError } from "@layerfs/agent";
import { invokeClientBackend, sandboxArg } from "@layerfs/agent/internal";
export function get_log(shop, sandbox, options = {}) {
  const lines = options?.lines ?? 200;
  if (!Number.isSafeInteger(lines) || lines < 1 || lines > 2000 || options?.since !== undefined) {
    throw new LayerFSError("InvalidInput", "options are { lines }: 1..2000 (a `since` time is not supported)", null);
  }
  return invokeClientBackend(shop, "backend.daemon.sandbox.get_log", { sandbox: sandboxArg(sandbox), lines });
}

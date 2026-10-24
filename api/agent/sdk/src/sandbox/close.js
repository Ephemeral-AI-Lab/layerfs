// sandbox.close: stops one Sandbox now, made by sandbox.create. Refused MountsActive while a Workspace is mounted in it and
// NoSuchSandbox when it is not running; stop, then delete, each once.
import { invokeClient, sandboxArg } from "../client.js";
export function close(client, sandbox) {
  return invokeClient(client, "sandbox.close", { sandbox: sandboxArg(sandbox) });
}

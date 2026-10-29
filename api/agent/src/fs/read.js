// fs.read: one positional SDK operation.
import { LayerFSError } from "../errors.js";

export function read(client, ref, path, startLine = 1, endLine, options = {}) {
  if (options === null || typeof options !== "object" || Array.isArray(options)) {
    throw new LayerFSError("InvalidInput", "options must be { binary? }", null);
  }
  for (const key of Object.keys(options)) {
    if (key !== "binary") throw new LayerFSError("InvalidInput", `unknown fs.read option "${key}"`, null);
  }
  if (options.binary !== undefined && typeof options.binary !== "boolean") {
    throw new LayerFSError("InvalidInput", "binary must be a boolean", null);
  }
  return client.call("fs.read", { branch: ref, path, startLine, endLine, binary: options.binary ?? false });
}

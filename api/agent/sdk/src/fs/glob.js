// fs.glob: one positional SDK operation.
import { page } from "../client.js";
export function glob(client, ref, pattern, path = "/", pageSize = 50, pagePos = 0) {
  return client.call("fs.glob", { branch: ref, pattern, path, ...page(pageSize, pagePos) });
}

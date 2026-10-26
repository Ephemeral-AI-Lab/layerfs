// fs.grep: one positional SDK operation.
import { page } from "../client.js";
export function grep(client, ref, pattern, path = "/", pageSize = 50, pagePos = 0) {
  return client.call("fs.grep", { branch: ref, pattern, path, ...page(pageSize, pagePos) });
}

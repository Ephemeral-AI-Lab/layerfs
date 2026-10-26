// fs.list: one positional SDK operation.
import { page } from "../client.js";
export function list(client, ref, path = "/", pageSize = 50, pagePos = 0) {
  return client.call("fs.list", { branch: ref, path, ...page(pageSize, pagePos) });
}

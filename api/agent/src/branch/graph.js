// branch.graph: one positional SDK operation.
import { page } from "../client.js";
export function graph(client, branch, pageSize = 50, pagePos = 0) {
  return client.call("branch.graph", { branch, ...page(pageSize, pagePos) });
}

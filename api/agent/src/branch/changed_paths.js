// branch.changed_paths: one positional SDK operation.
import { page } from "../client.js";
export function changed_paths(client, commit, pageSize = 50, pagePos = 0) {
  return client.call("branch.changed_paths", { commit, ...page(pageSize, pagePos) });
}

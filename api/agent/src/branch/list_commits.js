// branch.list_commits: one positional SDK operation.
import { page } from "../client.js";
export function list_commits(client, branch, pageSize = 50, pagePos = 0) {
  return client.call("branch.list_commits", { branch, ...page(pageSize, pagePos) });
}

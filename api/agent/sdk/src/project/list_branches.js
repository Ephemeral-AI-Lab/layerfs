// project.list_branches: one positional SDK operation.
import { page } from "../client.js";
export function list_branches(client, pageSize = 50, pagePos = 0) {
  return client.call("project.list_branches", page(pageSize, pagePos));
}

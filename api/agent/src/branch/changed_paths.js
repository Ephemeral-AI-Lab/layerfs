// branch.changed_paths: one positional SDK operation.
import { diffOptions, page } from "../client.js";
// `options.cursor` resumes after the `nextCursor` of the previous page.
export function changed_paths(client, commit, pageSize = 50, pagePos = 0, options = {}) {
  return client.call("branch.changed_paths", { commit, ...page(pageSize, pagePos), ...diffOptions(options) });
}

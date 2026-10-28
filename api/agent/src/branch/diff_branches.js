// branch.diff_branches: one positional SDK operation.
import { diffOptions, page } from "../client.js";
// `options.cursor` resumes after the `nextCursor` of the previous page;
// `options.hunks: false` lists changes without line hunks.
export function diff_branches(client, fromBranch, toBranch, pageSize = 50, pagePos = 0, options = {}) {
  return client.call("branch.diff_branches", { from: fromBranch, to: toBranch, ...page(pageSize, pagePos), ...diffOptions(options) });
}

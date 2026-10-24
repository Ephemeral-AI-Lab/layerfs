// branch.diff_commits: one positional SDK operation.
import { diffOptions, page } from "../client.js";
// `options.cursor` resumes after the `nextCursor` of the previous page;
// `options.hunks: false` lists changes without line hunks.
export function diff_commits(client, fromCommit, toCommit, pageSize = 50, pagePos = 0, options = {}) {
  return client.call("branch.diff_commits", { from: fromCommit, to: toCommit, ...page(pageSize, pagePos), ...diffOptions(options) });
}

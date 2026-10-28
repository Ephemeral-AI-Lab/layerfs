// layerfs branch diff_commits: branch.diff_commits(from, to, pageSize?, pagePos?, { cursor, hunks }).
import { DIFF } from "../common.js";

export default {
  args: ["from-commit", "to-commit"],
  options: DIFF,
  summary: "files that differ between two Commits, with their line changes; --no-hunks lists paths only",
  run: (lf, a, o) => lf.branch.diff_commits(a.fromCommit, a.toCommit, o.pageSize, o.pagePos, { cursor: o.cursor, hunks: o.noHunks ? false : undefined }),
};

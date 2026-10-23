// layerfs branch diff_branches: branch.diff_branches(from, to, pageSize?, pagePos?, { cursor, hunks }).
import { DIFF } from "../common.js";

export default {
  args: ["from-branch", "to-branch"],
  options: DIFF,
  summary: "files that differ between two Branches; --hunks adds line changes",
  run: (lf, a, o) => lf.branch.diff_branches(a.fromBranch, a.toBranch, o.pageSize, o.pagePos, { cursor: o.cursor, hunks: o.hunks }),
};

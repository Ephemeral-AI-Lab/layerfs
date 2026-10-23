// layerfs branch list_commits: branch.list_commits(branch, pageSize?, pagePos?).
import { PAGE } from "../common.js";

export default {
  args: ["branch"],
  options: PAGE,
  summary: "a Branch's Commits, newest first",
  run: (lf, a, o) => lf.branch.list_commits(a.branch, o.pageSize, o.pagePos),
};

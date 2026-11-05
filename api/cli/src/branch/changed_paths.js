// layerfs branch changed_paths: branch.changed_paths(commit, pageSize?, pagePos?).
import { PAGE } from "../common.js";

export default {
  args: ["commit"],
  options: PAGE,
  summary: "paths a Commit changed, without line hunks",
  run: (lf, a, o) => lf.branch.changed_paths(a.commit, o.pageSize, o.pagePos),
};

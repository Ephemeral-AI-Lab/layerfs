// layerfs branch graph: branch.graph(branch, pageSize?, pagePos?).
import { PAGE } from "../common.js";

export default {
  args: ["branch"],
  options: PAGE,
  summary: "a Branch's own Commits, fork point and publish state",
  run: (lf, a, o) => lf.branch.graph(a.branch, o.pageSize, o.pagePos),
};

// layerfs project list_branches: project.list_branches(pageSize?, pagePos?).
import { PAGE } from "../common.js";

export default {
  args: [],
  options: PAGE,
  summary: "the project's Branches, a page at a time",
  run: (lf, a, o) => lf.project.list_branches(o.pageSize, o.pagePos),
};

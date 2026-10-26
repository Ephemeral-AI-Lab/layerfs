// layerfs project list_sandboxes: project.list_sandboxes(pageSize?, pagePos?).
import { PAGE } from "../common.js";

export default {
  args: [],
  options: PAGE,
  summary: "the project's running Sandboxes and how many Workspaces each holds",
  run: (lf, a, o) => lf.project.list_sandboxes(o.pageSize, o.pagePos),
};

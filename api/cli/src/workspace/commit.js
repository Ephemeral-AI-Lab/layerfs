// layerfs workspace commit: workspace.commit(ws, message?, force?).
import { MESSAGE } from "../common.js";

export default {
  args: ["ws"],
  options: { ...MESSAGE, force: "bool" },
  summary: "Commit the Workspace's changes to its Branch",
  run: (lf, a, o) => lf.workspace.commit(a.ws, o.message, o.force),
};

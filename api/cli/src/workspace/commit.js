// layerfs workspace commit: workspace.commit(ws, message?, force?, options?).
import { MESSAGE } from "../common.js";
import { UsageError } from "../argv.js";

export default {
  args: ["ws"],
  options: { ...MESSAGE, force: "bool", author: "text", labels: "json" },
  summary: "Commit the Workspace's changes to its Branch",
  prepare(_a, o) {
    if (o.labels !== undefined && (!Array.isArray(o.labels) || o.labels.some((label) => typeof label !== "string"))) {
      throw new UsageError("--labels must be a JSON array of strings");
    }
    return {};
  },
  run: (lf, a, o) => lf.workspace.commit(a.ws, o.message, o.force, { author: o.author, labels: o.labels }),
};

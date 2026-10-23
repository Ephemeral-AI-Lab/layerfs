// layerfs fs edit: fs.edit(branch, path, oldText, newText, message?).
import { MESSAGE, either } from "../common.js";

export default {
  args: ["branch", "path"],
  options: { ...MESSAGE, old: "text", "old-from": "file", new: "text", "new-from": "file" },
  summary: "replace one exact match of the old text with the new; one Commit",
  prepare: async (a, o) => ({ oldText: either(o, "old"), newText: either(o, "new") }),
  run: (lf, a, o) => lf.fs.edit(a.branch, a.path, a.oldText, a.newText, o.message),
};

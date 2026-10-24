// layerfs fs apply: fs.apply(branch, changes, message?), changes a JSON array
// from stdin or --from.
import { MESSAGE, body } from "../common.js";
import { UsageError } from "../argv.js";

export default {
  args: ["branch"],
  options: { ...MESSAGE, from: "file" },
  summary: "a JSON array of changes from stdin (or --from), all or none; one Commit",
  prepare: async (a, o, io) => {
    const text = (await body(o, io)).toString("utf8");
    try {
      return { changes: JSON.parse(text) };
    } catch (error) {
      throw new UsageError(`the changes are not JSON: ${error.message}`);
    }
  },
  run: (lf, a, o) => lf.fs.apply(a.branch, a.changes, o.message),
};

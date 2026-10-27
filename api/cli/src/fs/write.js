// layerfs fs write: fs.write(branch, path, content, message?), content from stdin or --from.
import { MESSAGE, body } from "../common.js";

export default {
  args: ["branch", "path"],
  options: { ...MESSAGE, from: "file" },
  summary: "write a file from stdin (or --from), bytes kept exactly; one Commit",
  prepare: async (a, o, io) => ({ content: await body(o, io) }),
  run: (lf, a, o) => lf.fs.write(a.branch, a.path, a.content, o.message),
};

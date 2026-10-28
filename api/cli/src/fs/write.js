// layerfs fs write: fs.write(branch, path, content, { message?, mode? }), content from stdin or --from.
import { WRITE, body } from "../common.js";

export default {
  args: ["branch", "path"],
  options: { ...WRITE, from: "file" },
  summary: "write a file from stdin (or --from), bytes kept exactly; --mode sets its permission bits; one Commit",
  prepare: async (a, o, io) => ({ content: await body(o, io) }),
  run: (lf, a, o) => lf.fs.write(a.branch, a.path, a.content, { message: o.message, mode: o.mode }),
};

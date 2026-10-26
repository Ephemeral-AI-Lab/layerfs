// layerfs fs grep: fs.grep(ref, pattern, path?, pageSize?, pagePos?).
import { PAGE } from "../common.js";

export default {
  args: ["ref", "pattern", "path?"],
  options: PAGE,
  summary: "lines matching a regular expression",
  run: (lf, a, o) => lf.fs.grep(a.ref, a.pattern, a.path, o.pageSize, o.pagePos),
};

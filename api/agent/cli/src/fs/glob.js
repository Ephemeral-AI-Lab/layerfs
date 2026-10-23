// layerfs fs glob: fs.glob(ref, pattern, path?, pageSize?, pagePos?).
import { PAGE } from "../common.js";

export default {
  args: ["ref", "pattern", "path?"],
  options: PAGE,
  summary: "paths matching a glob pattern",
  run: (lf, a, o) => lf.fs.glob(a.ref, a.pattern, a.path, o.pageSize, o.pagePos),
};

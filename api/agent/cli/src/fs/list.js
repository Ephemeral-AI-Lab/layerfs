// layerfs fs list: fs.list(ref, path?, pageSize?, pagePos?).
import { PAGE } from "../common.js";

export default {
  args: ["ref", "path?"],
  options: PAGE,
  summary: "a folder's entries; path / by default",
  run: (lf, a, o) => lf.fs.list(a.ref, a.path, o.pageSize, o.pagePos),
};

// layerfs fs rm: fs.rm(branch, path, message?).
import { MESSAGE } from "../common.js";

export default {
  args: ["branch", "path"],
  options: MESSAGE,
  summary: "remove a file or folder; one Commit",
  run: (lf, a, o) => lf.fs.rm(a.branch, a.path, o.message),
};

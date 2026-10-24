// layerfs fs mkdir: fs.mkdir(branch, path, message?).
import { MESSAGE } from "../common.js";

export default {
  args: ["branch", "path"],
  options: MESSAGE,
  summary: "make a folder; one Commit",
  run: (lf, a, o) => lf.fs.mkdir(a.branch, a.path, o.message),
};

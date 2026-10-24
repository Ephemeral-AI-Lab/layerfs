// layerfs fs mv: fs.mv(branch, fromPath, toPath, message?).
import { MESSAGE } from "../common.js";

export default {
  args: ["branch", "from-path", "to-path"],
  options: MESSAGE,
  summary: "move a file or folder; one Commit",
  run: (lf, a, o) => lf.fs.mv(a.branch, a.fromPath, a.toPath, o.message),
};

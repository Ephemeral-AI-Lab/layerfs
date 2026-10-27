// layerfs fs cp: fs.cp(branch, fromPath, toPath, message?).
import { MESSAGE } from "../common.js";

export default {
  args: ["branch", "from-path", "to-path"],
  options: MESSAGE,
  summary: "copy a file or folder; one Commit",
  run: (lf, a, o) => lf.fs.cp(a.branch, a.fromPath, a.toPath, o.message),
};

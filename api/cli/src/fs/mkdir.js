// layerfs fs mkdir: fs.mkdir(branch, path, { message?, mode? }).
import { WRITE } from "../common.js";

export default {
  args: ["branch", "path"],
  options: WRITE,
  summary: "make a folder (--mode for its permission bits); one Commit",
  run: (lf, a, o) => lf.fs.mkdir(a.branch, a.path, { message: o.message, mode: o.mode }),
};

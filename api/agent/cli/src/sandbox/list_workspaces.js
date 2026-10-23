// layerfs sandbox list_workspaces: sandbox.list_workspaces(sandbox, pageSize?, pagePos?).
import { PAGE } from "../common.js";

export default {
  args: ["sandbox"],
  options: PAGE,
  summary: "a running Sandbox's mounted Workspaces",
  run: (lf, a, o) => lf.sandbox.list_workspaces(a.sandbox, o.pageSize, o.pagePos),
};

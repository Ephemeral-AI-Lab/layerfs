// layerfs workspace exec: workspace.exec(ws, command, message?, sinks?); --stream passes output and exit code through.
import { MESSAGE, exitOf } from "../common.js";

export default {
  args: ["ws", "command"],
  options: { ...MESSAGE, stream: "bool" },
  summary: "run one bash command in the Workspace; --stream passes its output and exit code through",
  run: async (lf, a, o, io) => {
    if (!o.stream) return lf.workspace.exec(a.ws, a.command, o.message);
    const result = await lf.workspace.exec(a.ws, a.command, o.message, { stdout: io.stdout, stderr: io.stderr });
    return io.exit(exitOf(result.exitCode));
  },
};

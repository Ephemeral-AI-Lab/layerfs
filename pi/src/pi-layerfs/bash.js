// Pi's bash on a LayerFS Branch, as Pi's BashOperations: each call mounts a
// new Workspace of the Branch, runs the command there from the folder that
// matches Pi's working directory, commits whatever it changed (a failing
// command too) and unmounts. The Branch records one Commit per call that
// changed files, and nothing stays mounted. The model's bash tool and the
// user's !command share these operations.
import { truncateTail } from "@earendil-works/pi-coding-agent";
import { errorText } from "./session.js";

const TITLE_CHARS = 200;
/** The exit code of `timeout` when it stopped the command. */
const TIMED_OUT = 124;

const quote = (value) => `'${value.replaceAll("'", `'\\''`)}'`;

/**
 * @param {import("../../../sdk/js/src/index.js").LayerFS} lf
 * @param {string} branch
 * @param {ReturnType<import("./paths.js").branchPaths>} paths
 * @returns {import("@earendil-works/pi-coding-agent").BashOperations}
 */
export function bashOperations(lf, branch, paths) {
  return {
    async exec(command, cwd, { onData, signal, timeout }) {
      if (signal?.aborted) throw new Error("aborted");
      // exec runs `bash -c` at the Workspace root
      const folder = paths.branch(cwd).slice(1);
      const run = timeout ? `timeout ${timeout} bash -c ${quote(command)}` : command;
      const title = command.split("\n")[0].slice(0, TITLE_CHARS);
      const ws = await lf.workspace.mount(branch);
      let r;
      try {
        r = await lf.workspace.exec(ws, folder ? `cd ${quote(folder)} && ${run}` : run, title);
        onData(Buffer.from(r.stdout + r.stderr));
        await lf.workspace.commit(ws, `${title} (exit ${r.exitCode})`);
      } catch (error) {
        const discarded = await lf.workspace.unmount(ws, true).then(
          () => "its Workspace was discarded",
          (failure) => `discarding its Workspace failed too: ${errorText(failure)}`,
        );
        // Pi's bash tool reports a failed call by its error alone, so the error carries the output
        throw new Error([r && output(r), errorText(error), `Nothing of this call was committed; ${discarded}.`].filter(Boolean).join("\n\n"));
      }
      try {
        await lf.workspace.unmount(ws);
      } catch (error) {
        // a background process the command left keeps the mount Busy; the Workspace stays for it to end
        throw new Error([output(r), errorText(error), "The call's changes are committed; its Workspace stays mounted."].filter(Boolean).join("\n\n"));
      }
      // exec cannot be cancelled: an aborted call ends when its command ends, with its changes committed
      if (signal?.aborted) throw new Error("aborted");
      if (timeout && r.exitCode === TIMED_OUT) throw new Error(`timeout:${timeout}`);
      return { exitCode: r.exitCode };
    },
  };
}

const output = (r) => truncateTail(r.stdout + r.stderr).content;

// The layerfs call table (#428 CLI), assembled from one file per command:
// api/cli/src/<family>/<call>.js mirrors api/agent/src/<family>/<call>.js,
// and runs as `layerfs <family> <call>`. Parsing (argv.js), help (help.js)
// and dispatch (main.js) read the table; api/docs/agent.md is its reference.
// LayerFS.open and close happen inside every command, so they have none.
//
// Each command file exports { args, options, summary, run, prepare? }.
// `args` are positional names in the SDK's order; a trailing `?` marks an
// optional one, left undefined so the SDK applies its own default.
// `options` map flag names to kinds (argv.js). `prepare(a, o, io)` reads a
// command's input before anything opens, so a bad input is a usage error
// with nothing called. `run(layerfs, a, o, io)` makes the one SDK call and
// returns its result, printed as JSON; a run that prints for itself returns
// io.exit(code).
import branch from "./branch/index.js";
import fs from "./fs/index.js";
import init from "./project/init.js";
import project from "./project/index.js";
import sandbox from "./sandbox/index.js";
import workspace from "./workspace/index.js";

const GROUPS = { project, branch, fs, workspace, sandbox };

export const CALLS = Object.entries(GROUPS).flatMap(([family, calls]) =>
  Object.entries(calls).map(([call, command]) => ({ family, call, ...command })));

export const FAMILIES = Object.keys(GROUPS);

/** `layerfs init`: LayerFS.init, the one command that opens no project first. */
export const INIT = { family: null, call: "init", init: true, ...init };

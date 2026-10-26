// layerfs help, written from the call table: one usage line per command,
// with the fixed sections (settings, shared arguments, output) around them.
import { CALLS, FAMILIES, INIT } from "./calls.js";

const KIND = { text: "<text>", int: "<n>", json: "<json>", file: "<file>", octal: "<octal>" };

/** One command's usage line. */
export function usage(entry) {
  const head = entry.family ? `layerfs ${entry.family} ${entry.call}` : `layerfs ${entry.call}`;
  const args = entry.args.map((n) => (n.endsWith("?") ? `[<${n.slice(0, -1)}>]` : `<${n}>`));
  const options = Object.entries(entry.options).map(([n, k]) => {
    if (n === "message") return "[-m <text>]";
    return k === "bool" ? `[--${n}]` : `[--${n} ${KIND[k]}]`;
  });
  return [head, ...args, ...options].join(" ");
}

const block = (entries) => entries.map((e) => `  ${usage(e).replace(/^layerfs /, "")}\n      ${e.summary}`).join("\n");

const HEAD = `layerfs: every @layerfs/agent call as one command

usage:  layerfs <family> <call> [args] [options]
        layerfs help [<family> [<call>]]
        layerfs --version

GLOBAL OPTIONS
  --project <name>      the project (default $LAYERFS_PROJECT)
  --text                readable output instead of JSON
  -h, --help            help for the command

SETTINGS come from the environment, as for the SDK: LAYERFS_HOME, LAYERFS_STORE_PATH,
  LAYERFS_DOCKER, LAYERFS_IMAGE, LAYERFS_VOLUME, LAYERFS_DAEMON, LAYERFS_CONTAINER_PREFIX,
  LAYERFS_DAEMON_HOST, LAYERFS_STORE_POLICY, LAYERFS_TRACE, LAYERFS_TRACE_STEP,
  LAYERFS_TRACE_START.

ARGUMENTS
  <ref>                 main, a Branch, a Commit id or a Layer id
  <path>                absolute, starting with /
  <ws>                  a Workspace, <sandbox>:<path> (mount prints it as ws)
  <sandbox>             a Sandbox number, such as 2
  -m <text>             the Commit message, UTF-8, at most 4096 bytes
  --page-size <n>       default 50; --page-pos <n> default 0`;

const TAIL = `OUTPUT
  stdout    the call's result as one line of JSON (--text: readable)
  stderr    a refusal as JSON: {"code", "message", "detail"}
  exit 0    the call succeeded (exec without --stream: the command's exitCode is in the JSON)
  exit 2    the command line is wrong; nothing was called
  exit 125  LayerFS refused or failed the call
  exec --stream exits with the command's own code; 125 still means LayerFS failed`;

/** The whole help, or one family's. */
export function overview(family) {
  if (family) return `${block(CALLS.filter((c) => c.family === family))}\n`;
  const families = FAMILIES.map((f) => `${f.toUpperCase()}\n${block(CALLS.filter((c) => c.family === f))}`);
  return [HEAD, `CLIENT\n${block([INIT])}`, ...families, TAIL].join("\n\n") + "\n";
}

/** One command's help. */
export function commandHelp(entry) {
  return `usage: ${usage(entry)}\n  ${entry.summary}\n`;
}

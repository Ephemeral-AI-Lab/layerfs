// The `layerfs` package (#533 step 14): one client per LayerFS root (and optional
// folder scope, O32), ten families built from the library's call table.
// Each function takes its positionals in the table's order, then an options
// object; every call is one call into the library, made once. Any call's
// options may also carry `log_tag`, the tag of its call log line (#659),
// `detail: true` for the long form of its answer, and `with: ["section"]`
// for the sections its table entry lists (call surface 2.0).
//
//   import layerfs from "@layerfs/node";
//   await layerfs.store.create("shop");
//   await layerfs.project.init("shop/api", "/src/api", { on_progress: console.log });
//   const lfs = layerfs.client({ root: "/data/lfs", scope: { project: "shop/api", folder: "/web", branch: "agent" } });
import { native } from "./native.js";
import { encode, decode, isPlain } from "./values.js";
import { LayerFSError } from "./errors.js";

/** The table: `[{ family, call, args, options, help }]`. */
export const table = decode(native.table());
/** The ten families, in help order. */
export const families = [...new Set(table.map((c) => c.family))];

const FUNCTION = "function";
// passed with any call's arguments; the library takes them out before
// checking: the log tag, and the global options `detail` and `with`
const GLOBAL = new Set(["log_tag", "detail", "with"]);

function invalid(message) {
  return new LayerFSError("InvalidInput", message, null);
}

/** One table entry as a function of `client`. */
function bind(client, spec) {
  const name = `${spec.family}.${spec.call}`;
  const handlers = spec.options.filter((o) => o.kind === FUNCTION).map((o) => o.name);
  const known = new Set(spec.options.map((o) => o.name));
  return async function call(...given) {
    let options = {};
    // a trailing plain object past the positionals, or where a positional
    // takes a name, text or number, is the options
    const last = given[given.length - 1];
    const slot = spec.args[given.length - 1];
    const json = slot && slot.kind === "json";
    if (given.length > spec.args.length || (isPlain(last) && slot && !json)) {
      options = given.pop() ?? {};
      if (!isPlain(options)) throw invalid(`${name}: options must be an object`);
    }
    if (given.length > spec.args.length) throw invalid(`${name}: takes ${spec.args.length} positional arguments`);
    const args = {};
    spec.args.forEach((arg, i) => {
      if (given[i] !== undefined && given[i] !== null) args[arg.name] = given[i];
    });
    const on = {};
    for (const [key, value] of Object.entries(options)) {
      if (!known.has(key) && !GLOBAL.has(key)) throw invalid(`${name}: unknown option "${key}"`);
      if (handlers.includes(key)) {
        if (typeof value !== "function") throw invalid(`${name}: ${key} must be a function`);
        on[key] = value;
      } else if (value !== undefined) {
        args[key] = value;
      }
    }
    // `stream: true` passes the command's output on as it comes: to the
    // stdout and stderr functions, else to this process's own
    if (args.stream === true) {
      on.stdout ??= (bytes) => process.stdout.write(bytes);
      on.stderr ??= (bytes) => process.stderr.write(bytes);
    }
    // Events arrive on their own handle: count them, and settle only once
    // every event the call queued has been passed on. A handler that
    // throws does not stop the call: its error is reported as a warning.
    let received = 0;
    let settle = null;
    const onEvent = Object.keys(on).length === 0 ? undefined : (text) => {
      try {
        const event = decode(text);
        if (event.progress !== undefined) on.on_progress?.(event.progress);
        else if (event.stream === "stdout") on.stdout?.(event.bytes);
        else if (event.stream === "stderr") on.stderr?.(event.bytes);
      } catch (error) {
        process.emitWarning(`${name}: an event handler threw: ${error?.stack ?? error}`, "LayerFSHandlerWarning");
      } finally {
        received += 1;
        settle?.();
      }
    };
    const reply = decode(await native.call(
      name,
      encode(args),
      client.root ?? null,
      client.scope ? encode(client.scope) : null,
      onEvent,
    ));
    if (received < reply.events) {
      await new Promise((resolve) => {
        settle = () => { if (received >= reply.events) resolve(); };
      });
    }
    const answer = reply.answer;
    if (answer.refused) throw LayerFSError.from(answer.refused);
    return answer.value;
  };
}

/** A client of the LayerFS root `root` (default `LAYERFS_ROOT`, else
 * `~/.layerfs`), whose home is `<root>/home`, or the root itself while it
 * holds `layerfs.json`; with an optional folder scope
 * `{ project, folder, branch? }`. */
export function client(options = {}) {
  const { root, scope } = options;
  // a client names a root; an old `home` key would silently fall back to
  // the default root, so any other key is refused
  for (const key of Object.keys(options)) {
    if (key !== "root" && key !== "scope") {
      throw invalid(`client() takes { root, scope }, not ${key}`);
    }
  }
  // the root is the client's own and not a field: `client.home` is the
  // `home` family
  const where = { root, scope };
  const made = { scope };
  for (const family of families) made[family] = {};
  for (const spec of table) made[spec.family][spec.call] = bind(where, spec);
  return Object.freeze(made);
}

/** Closes every Store this process holds open with no call running,
 * releasing its leases, so it leaves none behind when it exits (a lease
 * of a process that ended is stale until the Store's next call). Call it
 * before the process exits; a later call opens its Store again. It also
 * ends this process's idle Sandbox daemon sessions inside a short bound, so
 * the exit is not a dropped connection in a daemon's log. Answers how many
 * Stores it closed. Outside the call table. */
export function close() {
  return native.close();
}

const fallback = client();
export const { home, project, branch, commit, file, sandbox, workspace, machine, store, backup } = fallback;
export { LayerFSError };
export default { ...fallback, client, close, table, families, LayerFSError };

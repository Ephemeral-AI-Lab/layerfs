// The `layerfs` package (#533 step 14): one client per home (and optional
// folder scope, O32), nine families built from the library's call table.
// Each function takes its positionals in the table's order, then an options
// object; every call is one call into the library, made once.
//
//   import layerfs from "layerfs";
//   await layerfs.store.create("shop");
//   await layerfs.project.init("shop/api", "/src/api", { on_progress: console.log });
//   const lfs = layerfs.client({ home: "/data/lfs", scope: { project: "shop/api", folder: "/web", branch: "agent" } });
import { native } from "./native.js";
import { encode, decode, isPlain } from "./values.js";
import { LayerFSError } from "./errors.js";

/** The table: `[{ family, call, args, options, help }]`. */
export const table = decode(native.table());
/** The nine families, in help order. */
export const families = [...new Set(table.map((c) => c.family))];

const FUNCTION = "function";

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
      if (!known.has(key)) throw invalid(`${name}: unknown option "${key}"`);
      if (handlers.includes(key)) {
        if (typeof value !== "function") throw invalid(`${name}: ${key} must be a function`);
        on[key] = value;
      } else if (value !== undefined) {
        args[key] = value;
      }
    }
    const onEvent = Object.keys(on).length === 0 ? undefined : (text) => {
      const event = decode(text);
      if (event.progress !== undefined) on.on_progress?.(event.progress);
      else if (event.stream === "stdout") on.stdout?.(event.bytes);
      else if (event.stream === "stderr") on.stderr?.(event.bytes);
    };
    const answer = decode(await native.call(
      name,
      encode(args),
      client.home ?? null,
      client.scope ? encode(client.scope) : null,
      onEvent,
    ));
    if (answer.refused) throw new LayerFSError(answer.refused.code, answer.refused.message, answer.refused.detail);
    return answer.value;
  };
}

/** A client of `home` (default `LAYERFS_HOME`, else `~/.layerfs`), with an
 * optional folder scope `{ project, folder, branch? }`. */
export function client({ home, scope } = {}) {
  const made = { home, scope };
  for (const family of families) made[family] = {};
  for (const spec of table) made[spec.family][spec.call] = bind(made, spec);
  return Object.freeze(made);
}

const fallback = client();
export const { project, branch, commit, file, sandbox, workspace, machine, store, backup } = fallback;
export { LayerFSError };
export default { ...fallback, client, table, families, LayerFSError };

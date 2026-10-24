// Command-line parsing for layerfs: the global options, then one command's
// positional arguments and options as its call table entry declares them.
// Option kinds: text, int (a whole number), bool (a flag with no value),
// json (parsed here) and file (a path the command reads). A bad command
// line throws UsageError before anything is opened or called.

/** A wrong command line: exit 2, nothing called. */
export class UsageError extends Error {
  constructor(message, entry = null) {
    super(message);
    this.entry = entry;
  }
}

const camel = (name) => name.replace(/-([a-z])/g, (_, c) => c.toUpperCase());

/** Splits the global options out of argv: --project <name>, --text, -h/--help, --version. */
export function globals(argv) {
  const g = { rest: [], project: undefined, text: false, help: false, version: false };
  for (let i = 0; i < argv.length; i++) {
    const t = argv[i];
    if (t === "--") { g.rest.push(...argv.slice(i)); break; }
    if (t === "--text") g.text = true;
    else if (t === "-h" || t === "--help") g.help = true;
    else if (t === "--version") g.version = true;
    else if (t === "--project" || t.startsWith("--project=")) {
      const value = t === "--project" ? argv[++i] : t.slice("--project=".length);
      if (!value) throw new UsageError("--project needs a project name");
      g.project = value;
    } else g.rest.push(t);
  }
  return g;
}

/** A Workspace handle, `<sandbox>:<path>`, as the SDK's { sandbox, path }. */
export function workspace(text, entry) {
  const at = text.indexOf(":");
  if (at < 1 || text[at + 1] !== "/") throw new UsageError(`a Workspace is <sandbox>:<path>, such as 1:/workspaces/3, not ${JSON.stringify(text)}`, entry);
  return { sandbox: text.slice(0, at), path: text.slice(at + 1) };
}

function value(kind, name, text, entry) {
  if (kind === "int") {
    if (!/^\d+$/.test(text)) throw new UsageError(`--${name} takes a whole number, not ${JSON.stringify(text)}`, entry);
    return Number(text);
  }
  if (kind === "octal") {
    if (!/^(0o)?[0-7]+$/.test(text)) throw new UsageError(`--${name} takes octal permission bits such as 755, not ${JSON.stringify(text)}`, entry);
    return parseInt(text.replace(/^0o/, ""), 8);
  }
  if (kind === "json") {
    try {
      return JSON.parse(text);
    } catch (error) {
      throw new UsageError(`--${name} is not JSON: ${error.message}`, entry);
    }
  }
  return text;
}

/** One command's tokens as `a` (positional, by camel-cased name) and `o` (options). */
export function parse(entry, tokens) {
  const a = {};
  const o = {};
  const positional = [];
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (t === "--") { positional.push(...tokens.slice(i + 1)); break; }
    if (t !== "-m" && !t.startsWith("--")) {
      if (t.length > 1 && t.startsWith("-")) throw new UsageError(`unknown option ${t}`, entry);
      positional.push(t);
      continue;
    }
    const eq = t.indexOf("=");
    const name = t === "-m" ? "message" : t.slice(2, eq < 0 ? undefined : eq);
    const kind = entry.options[name];
    if (!kind) throw new UsageError(`unknown option ${t === "-m" ? "-m" : `--${name}`}`, entry);
    if (camel(name) in o) throw new UsageError(`--${name} is given twice`, entry);
    if (kind === "bool") {
      if (eq >= 0) throw new UsageError(`--${name} takes no value`, entry);
      o[camel(name)] = true;
      continue;
    }
    const text = eq >= 0 ? t.slice(eq + 1) : tokens[++i];
    if (text === undefined) throw new UsageError(`${t} needs a value`, entry);
    o[camel(name)] = value(kind, name, text, entry);
  }
  const required = entry.args.filter((n) => !n.endsWith("?")).length;
  if (positional.length < required) throw new UsageError(`missing <${entry.args[positional.length]}>`, entry);
  if (positional.length > entry.args.length) throw new UsageError(`unexpected argument ${JSON.stringify(positional[entry.args.length])}`, entry);
  entry.args.forEach((spec, k) => {
    const name = spec.replace(/\?$/, "");
    const given = positional[k];
    a[camel(name)] = given !== undefined && name === "ws" ? workspace(given, entry) : given;
  });
  return { a, o };
}

// Generates the call-by-front-end matrix of docs/architecture/api/call-table/coverage.md
// from the three front ends' own call lists:
//   Node    the package's `table` and its bound functions
//   Python  `layerfs.TABLE`, the sync functions and the `layerfs.aio` coroutines
//   CLI     `layerfs help <family>`, one usage line per call
//   node api/scripts/gen-coverage.mjs          rewrite the generated block
//   node api/scripts/gen-coverage.mjs --check  exit 1 when the block is not what the front ends give,
//                                              or when a front end disagrees with the Node table
// It needs the built addon, the Python module (`LAYERFS_PYTHON`, default `python3`, with
// api/python/src on its path) and the built command (`LAYERFS_CLI`, default target/debug/layerfs,
// or under CARGO_TARGET_DIR). A missing one is an error, never an empty column.
import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
export const document = join(root, "docs/architecture/api/call-table/coverage.md");
const BEGIN = "<!-- BEGIN GENERATED: api/scripts/gen-coverage.mjs -->";
const END = "<!-- END GENERATED -->";

const FUNCTION = "function";
const piped = (arg) => arg.kind === "content" || arg.kind === "json";
const nameOf = (spec) => `${spec.family}.${spec.call}`;
const shape = (list) => list.map((arg) => `${arg.optional ? "?" : ""}${arg.name}`).join(", ");

/** Where the Python interpreter and the command are: an error naming what to set when one is not. */
export function locate(env = process.env) {
  const python = env.LAYERFS_PYTHON || "python3";
  const target = env.CARGO_TARGET_DIR ? resolve(root, env.CARGO_TARGET_DIR) : join(root, "target");
  const cli = env.LAYERFS_CLI || join(target, "debug", "layerfs");
  if (!existsSync(cli)) throw new Error(`the layerfs command is not built: ${cli} (cargo build -p layerfs-cli, or set LAYERFS_CLI)`);
  return { python, cli };
}

const PYTHON_VIEW = `
import json, keyword, inspect
import layerfs
from layerfs import aio
names = lambda spec: spec["family"] + "." + spec["call"]
fn = lambda module, spec: getattr(getattr(module, spec["family"], None), spec["call"], None)
print(json.dumps({
  "table": layerfs.TABLE,
  "sync": [names(s) for s in layerfs.TABLE if callable(fn(layerfs, s))],
  "aio": [names(s) for s in layerfs.TABLE if inspect.iscoroutinefunction(fn(aio, s))],
  "keywords": sorted({a["name"] for s in layerfs.TABLE for a in s["args"] + s["options"] if keyword.iskeyword(a["name"])}),
}))
`;

/** Each front end's own view of the calls: `{ node, python, cli }`. */
export async function collect({ python, cli }) {
  const node = await import("../node/src/index.js");
  const view = {
    node: { table: node.table, bound: node.table.filter((s) => typeof node.default[s.family]?.[s.call] === "function").map(nameOf) },
  };
  const run = spawnSync(python, ["-B", "-c", PYTHON_VIEW], {
    cwd: join(root, "api/python"),
    env: { ...process.env, PYTHONPATH: join(root, "api/python/src") },
    encoding: "utf8",
    timeout: 60_000,
  });
  if (run.error || run.status !== 0) {
    throw new Error(`the Python view failed (${python}; build the module with maturin develop in api/python): ${run.error ?? run.stderr}`);
  }
  view.python = JSON.parse(run.stdout);
  view.cli = {};
  for (const family of node.families) {
    const help = spawnSync(cli, ["help", family], { encoding: "utf8", timeout: 60_000 });
    if (help.error || help.status !== 0) throw new Error(`layerfs help ${family} failed: ${help.error ?? help.stderr}`);
    for (const line of help.stdout.split("\n")) {
      const found = line.match(/^layerfs (\S+) (\S+)(.*)$/);
      // a call's shell name has `-` for its `_` (`workspace exec-cancel`)
      if (found) view.cli[`${found[1]}.${found[2].replaceAll("-", "_")}`] = usage(found[3]);
    }
  }
  return view;
}

/** A usage line's words as `{ positionals, options, from }`; names in snake_case. */
function usage(words) {
  const out = { positionals: [], options: [], from: false };
  for (const [, optionalPositional, positional, flag] of words.matchAll(/\[<([\w-]+)>\]|<([\w-]+)>|\[--([\w-]+)(?: <[\w-]+>)?\]/g)) {
    const snake = (text) => text.replaceAll("-", "_");
    if (optionalPositional) out.positionals.push({ name: snake(optionalPositional), optional: true });
    else if (positional) out.positionals.push({ name: snake(positional), optional: false });
    else if (flag === "from") out.from = true;
    else out.options.push(snake(flag));
  }
  return out;
}

/** What the command line of `spec` must be, by the table: a content or JSON positional is read from
 * stdin or `--from`, `on_progress` is `--progress`, the output functions are `--stream`. */
function expectedCli(spec) {
  return {
    positionals: spec.args.filter((a) => !piped(a)).map((a) => ({ name: a.name, optional: a.optional })),
    options: spec.options.filter((o) => o.kind !== FUNCTION || o.name === "on_progress").map((o) => (o.name === "on_progress" ? "progress" : o.name)),
    from: spec.args.some(piped),
  };
}

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const describeCli = (v) => `${shape(v.positionals)} | ${v.options.map((o) => `--${o}`).join(" ")}${v.from ? " | --from" : ""}`;

/** The generated block for `view` (from `collect`) and the problems it found. */
export function render(view) {
  const problems = [];
  const table = view.node.table;
  const python = new Map(view.python.table.map((s) => [nameOf(s), s]));
  const bound = new Set(view.node.bound);
  const sync = new Set(view.python.sync);
  const aio = new Set(view.python.aio);
  const names = new Set(table.map(nameOf));
  for (const name of [...python.keys(), ...Object.keys(view.cli)]) {
    if (!names.has(name)) problems.push(`${name}: a call of a front end that the Node table does not have`);
  }
  const counts = { node: 0, python: 0, aio: 0, cli: 0 };
  const rows = table.map((spec) => {
    const name = nameOf(spec);
    const cell = (ok, why) => (ok ? "yes" : (problems.push(`${name}: ${why}`), "NO"));
    counts.node += bound.has(name);
    const node = cell(bound.has(name), "no Node function");
    const other = python.get(name);
    const pythonOk = other !== undefined && same(other, spec) && sync.has(name);
    counts.python += pythonOk;
    const py = cell(pythonOk, other === undefined ? "not in the Python table" : !same(other, spec) ? "the Python table differs from the Node table" : "no Python function");
    counts.aio += aio.has(name);
    const coroutine = cell(aio.has(name), "no aio coroutine");
    const given = view.cli[name];
    const want = expectedCli(spec);
    const cliOk = given !== undefined && same(given, want);
    counts.cli += cliOk;
    const cli = cell(cliOk, given === undefined ? "no command" : `the command line differs: ${describeCli(given)} is not ${describeCli(want)}`);
    const notes = [
      spec.args.some(piped) ? "stdin/--from" : "",
      spec.options.some((o) => o.name === "on_progress") ? "--progress" : "",
      spec.options.some((o) => o.name === "stream") ? "--stream" : "",
    ].filter(Boolean);
    const options = spec.options.map((o) => (o.kind === FUNCTION ? `${o.name}()` : o.name)).join(", ");
    return `| \`${name}\` | ${shape(spec.args) || "-"} | ${options || "-"} | ${node} | ${py} | ${coroutine} | ${cli}${cli === "yes" && notes.length ? ` (${notes.join(", ")})` : ""} |`;
  });
  const families = [...new Set(table.map((s) => s.family))];
  const lines = [
    BEGIN,
    "",
    `${table.length} calls in ${families.length} families, counted from the Node table. Calls present: Node ${counts.node}, Python ${counts.python}, Python \`aio\` ${counts.aio}, CLI ${counts.cli}.`,
    "",
    `Argument names that are Python keywords: ${view.python.keywords.length === 0 ? "none" : view.python.keywords.join(", ")}.`,
    "",
    `Front-end problems: ${problems.length === 0 ? "none" : problems.length}.`,
    ...problems.map((p) => `- ${p}`),
    "",
    "| Call | Positionals (`?` optional) | Options (`()` a function) | Node | Python | Python `aio` | CLI |",
    "| --- | --- | --- | --- | --- | --- | --- |",
    ...rows,
    "",
    END,
  ];
  return { block: lines.join("\n"), problems };
}

/** `text` (the document) with its generated block replaced by `block`. */
export function splice(text, block) {
  const start = text.indexOf(BEGIN);
  const end = text.indexOf(END);
  if (start < 0 || end < start) throw new Error(`${document} has no "${BEGIN}" ... "${END}" block`);
  return text.slice(0, start) + block + text.slice(end + END.length);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { block, problems } = render(await collect(locate()));
  const current = readFileSync(document, "utf8");
  const next = splice(current, block);
  if (process.argv.includes("--check")) {
    if (next !== current) {
      console.error("coverage.md differs from the front ends' call lists; run node api/scripts/gen-coverage.mjs");
      process.exit(1);
    }
  } else {
    writeFileSync(document, next);
    console.log(`Wrote ${document}`);
  }
  if (problems.length > 0) {
    console.error(`${problems.length} front-end problem(s):\n${problems.join("\n")}`);
    process.exit(1);
  }
}

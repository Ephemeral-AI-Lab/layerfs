// layerfs (#428): one @layerfs/agent call per command. It finds the command
// in the call table, parses its arguments, reads any input, opens the
// project with LayerFS.open, makes the call once, prints the result and
// closes. Nothing is retried. Exit 0 on success, 2 for a wrong command line
// (nothing called), 125 when LayerFS refused or failed the call; exec
// --stream exits with the command's own code.
import { readFileSync } from "node:fs";
import { globals, parse, UsageError } from "./argv.js";
import { CALLS, FAMILIES, INIT } from "./calls.js";
import { commandHelp, overview, usage } from "./help.js";
import { refusal, render } from "./output.js";

export const VERSION = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8")).version;

/** What a run returns when it printed for itself. */
const EXITED = Symbol("layerfs.exited");

const name = (text) => text?.replaceAll("-", "_");

/** The table entry `tokens` name, and the tokens left for its arguments. */
function find(tokens) {
  const [first, second, ...rest] = tokens;
  if (first === "init") return [INIT, tokens.slice(1)];
  if (!FAMILIES.includes(first)) throw new UsageError(`unknown command ${JSON.stringify(first)}; see layerfs help`);
  const entry = CALLS.find((c) => c.family === first && c.call === name(second));
  if (!entry) throw new UsageError(`unknown ${first} call ${JSON.stringify(second ?? "")}; ${first} has ${CALLS.filter((c) => c.family === first).map((c) => c.call).join(", ")}`);
  return [entry, rest];
}

/** Help for `layerfs help [<family> [<call>]]`. */
function help(tokens) {
  if (!tokens.length) return overview();
  if (FAMILIES.includes(tokens[0]) && tokens.length === 1) return overview(tokens[0]);
  return commandHelp(find(tokens)[0]);
}

/**
 * Runs one command. `io` is { out(text), err(text), input(), stdout, stderr }:
 * out/err resolve once the text is written, input() is stdin's bytes, and
 * stdout/stderr are the Writable sinks exec streams into. Returns the exit code.
 */
export async function main(argv, io) {
  let text = false;
  const exits = { ...io, exit: (code) => ({ [EXITED]: code }) };
  try {
    const g = globals(argv);
    text = g.text;
    if (g.version) { await io.out(`${VERSION}\n`); return 0; }
    if (g.rest[0] === "help") { await io.out(help(g.rest.slice(1))); return 0; }
    if (g.help) { await io.out(help(g.rest.filter((t) => !t.startsWith("-")).slice(0, 2))); return 0; }
    if (!g.rest.length) { await io.err(overview()); return 2; }
    const [entry, tokens] = find(g.rest);
    const { a, o } = parse(entry, tokens);
    if (entry.prepare) Object.assign(a, await entry.prepare(a, o, io));
    const { LayerFS } = await import("@layerfs/agent");
    let result;
    if (entry.init) {
      result = await entry.run(LayerFS, a, o, exits);
    } else {
      const project = g.project ?? process.env.LAYERFS_PROJECT;
      if (!project) throw new UsageError("no LayerFS project: pass --project <name> or set LAYERFS_PROJECT");
      const layerfs = await LayerFS.open(project);
      try {
        result = await entry.run(layerfs, a, o, exits);
      } finally {
        await layerfs.close().catch((e) => io.err(`layerfs: closing the client failed: ${e.message}\n`));
      }
    }
    if (result && typeof result === "object" && EXITED in result) return result[EXITED];
    await io.out(render(result, text));
    return 0;
  } catch (error) {
    if (error instanceof UsageError) {
      await io.err(`layerfs: ${error.message}\n${error.entry ? `usage: ${usage(error.entry)}\n` : ""}`);
      return 2;
    }
    await io.err(refusal(error, text));
    return 125;
  }
}

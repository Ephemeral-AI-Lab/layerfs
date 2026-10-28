// The command table of api/docs/agent.md, written from the call table so the
// doc cannot drift: every SDK call with its command.
// cli.test.mjs checks the doc holds exactly these; after changing the table,
// rewrite it with:
//   node api/cli/test/doc-tables.mjs --write
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { CALLS, INIT } from "../src/calls.js";
import { usage } from "../src/help.js";

export const DOC = fileURLToPath(new URL("../../docs/agent.md", import.meta.url));

const row = (cells) => `| ${cells.join(" | ")} |`;

/** Each marked section's text, by name. */
export function tables() {
  const calls = [
    row(["SDK call", "Command"]),
    row(["---", "---"]),
    row(["`LayerFS.init(name, folder, timeout?)`", `\`${usage(INIT)}\``]),
    ...CALLS.map((c) => row([`\`${c.family}.${c.call}\``, `\`${usage(c)}\``])),
  ];
  return { calls: calls.join("\n") };
}

const marked = (name) => new RegExp(`(<!-- ${name}:start -->\\n)[\\s\\S]*?(\\n<!-- ${name}:end -->)`);

/** The doc's current text of each marked section. */
export function written(text = readFileSync(DOC, "utf8")) {
  return Object.fromEntries(Object.keys(tables()).map((name) => {
    const m = new RegExp(`<!-- ${name}:start -->\\n([\\s\\S]*?)\\n<!-- ${name}:end -->`).exec(text);
    return [name, m ? m[1] : null];
  }));
}

if (process.argv[2] === "--write") {
  let text = readFileSync(DOC, "utf8");
  for (const [name, table] of Object.entries(tables())) {
    if (!marked(name).test(text)) throw new Error(`${DOC} has no ${name} markers`);
    text = text.replace(marked(name), (_, start, end) => `${start}${table}${end}`);
  }
  writeFileSync(DOC, text);
}

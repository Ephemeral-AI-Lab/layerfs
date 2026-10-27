// What several commands share: option sets, reading a command's input
// (stdin or a file) and the exit code of a streamed command.
import { readFileSync } from "node:fs";
import { UsageError } from "./argv.js";

export const PAGE = { "page-size": "int", "page-pos": "int" };
export const MESSAGE = { message: "text" };
// A write's message and permission bits (`--mode 755`).
export const WRITE = { ...MESSAGE, mode: "octal" };
// --no-hunks sends hunks: false (paths without line changes); left out, hunks is not
// sent and the SDK lists line changes, its default.
export const DIFF = { ...PAGE, cursor: "text", "no-hunks": "bool" };

/** The bytes of `--from <file>`, else of stdin. */
export async function body(o, io) {
  if (o.from === undefined) return io.input();
  try {
    return readFileSync(o.from);
  } catch (error) {
    throw new UsageError(`cannot read ${o.from}: ${error.message}`);
  }
}

/** One of `--<name> <text>` or `--<name>-from <file>`, as text. */
export function either(o, name) {
  const inline = o[name];
  const file = o[`${name}From`];
  if ((inline === undefined) === (file === undefined)) throw new UsageError(`give exactly one of --${name} and --${name}-from`);
  if (inline !== undefined) return inline;
  try {
    return readFileSync(file, "utf8");
  } catch (error) {
    throw new UsageError(`cannot read ${file}: ${error.message}`);
  }
}

/** A process exit code from a command's exit status; 125 when it has none. */
export const exitOf = (code) => (Number.isInteger(code) && code >= 0 && code <= 255 ? code : 125);

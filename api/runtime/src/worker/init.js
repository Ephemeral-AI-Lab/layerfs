// Init in a container of its own with the source folder mounted read-only
// (macOS; the agent's worker client starts it). Copied from the dsh Host's
// port/run.mjs (#428). stdin: the call, encoded JSON { fn, op, args, open };
// stdout: its answer, encoded JSON, and nothing else. LAYERFS_NATIVE is the
// Linux addon. With `open`, the project's Store is opened before the call
// and closed after it. The call is made once.
import { createRequire } from "node:module";
import { decode, encode } from "@layerfs/agent/internal";
import { made, storeOf } from "./call.js";

console.log = console.error; // stdout carries only the answer

const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const { fn, op, args, open } = decode(Buffer.concat(chunks));
const binding = createRequire(import.meta.url)(process.env.LAYERFS_NATIVE);

let answer;
try {
  const held = open ? await binding.storeOpen(storeOf(args)) : null;
  if (held?.refused) answer = held;
  else {
    try {
      answer = await made(binding, fn, op, args);
    } finally {
      if (held) await binding.storeClose(storeOf(args));
    }
  }
} catch (error) {
  answer = { thrown: String(error?.stack ?? error) };
}
process.stdout.write(encode(answer));

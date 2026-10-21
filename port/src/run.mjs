// Init of a ported Live project, in a container of its own with the source
// folder mounted read-only (binding.js); every other call goes to the Store container's
// worker. stdin: the call, v8-serialized { fn, op, args, open }; stdout: its
// answer, v8-serialized, and nothing else. LAYERFS_NATIVE is the Linux
// binding. With `open`, the project's Store is opened before the call and
// closed after it. The call is made once.
import { createRequire } from "node:module";
import { deserialize, serialize } from "node:v8";
import { made, storeOf } from "./call.mjs";

console.log = console.error; // stdout carries only the answer

const chunks = [];
for await (const chunk of process.stdin) chunks.push(chunk);
const { fn, op, args, open } = deserialize(Buffer.concat(chunks));
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
process.stdout.write(serialize(answer));

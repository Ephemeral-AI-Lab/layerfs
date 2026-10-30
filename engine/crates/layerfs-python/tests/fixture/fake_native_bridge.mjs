// JSON-lines adapter to the existing P6 stand-in. It is test infrastructure,
// not a product transport: Python contract tests drive fake-native.cjs itself.
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import * as readline from "node:readline";

const require = createRequire(import.meta.url);
const native = require(fileURLToPath(new URL("../../../../../api/agent/test/fixture/fake-native.cjs", import.meta.url)));

function plain(value) {
  if (Buffer.isBuffer(value) || value instanceof Uint8Array) return { base64: Buffer.from(value).toString("base64") };
  if (Array.isArray(value)) return value.map(plain);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, plain(item)]));
  return value;
}
function binary(value) {
  if (Array.isArray(value)) return value.map(binary);
  if (value && typeof value === "object") {
    if (Object.keys(value).length === 1 && typeof value.base64 === "string") return Buffer.from(value.base64, "base64");
    return Object.fromEntries(Object.entries(value).map(([key, item]) => [key, binary(item)]));
  }
  return value;
}
function send(id, answer) {
  process.stdout.write(`${JSON.stringify({ id, answer: plain(answer) })}\n`);
}
const lines = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
for await (const line of lines) {
  if (!line) continue;
  const request = JSON.parse(line);
  const { id, cmd } = request;
  try {
    if (cmd === "call" || cmd === "backendCall") {
      process.env.CLI_FAKE_REFUSE = (request.refuse ?? []).join(",");
      const answer = await native[cmd](request.op, binary(request.args));
      delete process.env.CLI_FAKE_REFUSE;
      send(id, answer);
    } else if (cmd === "storeOpen" || cmd === "storeClose") {
      send(id, await native[cmd](binary(request.args)));
    } else if (cmd === "execOutput") {
      send(id, { refused: { code: "Failed", message: "the P6 stand-in has no spooled output", detail: null } });
    } else if (cmd === "prepare") {
      for (const [key, value] of Object.entries(request.values)) native[key] = binary(value);
      send(id, { value: null });
    } else if (cmd === "reset") {
      native.calls.length = 0;
      native.args.length = 0;
      send(id, { value: null });
    } else if (cmd === "inspect") {
      send(id, { value: { calls: native.calls, args: native.args, stores: native.stores } });
    } else if (cmd === "close") {
      send(id, { value: null });
      process.exit(0);
    } else {
      send(id, { refused: { code: "Failed", message: `unknown test bridge command ${cmd}`, detail: null } });
    }
  } catch (error) {
    delete process.env.CLI_FAKE_REFUSE;
    send(id, { refused: { code: "Failed", message: String(error?.stack ?? error), detail: null } });
  }
}

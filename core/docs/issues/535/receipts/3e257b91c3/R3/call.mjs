// One SDK call per process: node call.mjs <home> <family.call> <json args array>
import { client } from "./sdk/api/node/src/index.js";
const [home, name, args] = process.argv.slice(2);
const [family, call] = name.split(".");
const lfs = client({ home });
try {
  const value = await lfs[family][call](...JSON.parse(args));
  console.log(JSON.stringify({ ok: true, value }));
} catch (error) {
  console.log(JSON.stringify({ ok: false, code: error?.code ?? null, message: String(error?.message ?? error) }));
  process.exitCode = 1;
}

// LayerFS.openReadOnly over the stand-in binding: the Store is opened with
// readOnly, reads send it, and file-changing calls are refused ReadOnly.
//   node api/agent/test/open_read_only.mjs
import assert from "node:assert/strict";
import { mkdtempSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { ENV } from "./fixture/cases.mjs";

const FAKE = fileURLToPath(new URL("./fixture/fake-native.cjs", import.meta.url));
Object.assign(process.env, ENV, { LAYERFS_NATIVE: FAKE });
delete process.env.CLI_FAKE_LOG;
delete process.env.LAYERFS_WORKER_ENDPOINT;
const native = createRequire(import.meta.url)(FAKE);
const { LayerFS } = await import("../src/index.js");

await assert.rejects(LayerFS.openReadOnly("website", {}), {
  code: "InvalidInput",
  message: /takes no options: settings come from the environment/,
});

const reader = await LayerFS.openReadOnly("website");
assert.equal(reader.options.readOnly, true);
assert.deepEqual(native.stores.at(-1), ["open", ENV.LAYERFS_STORE_PATH, true]);
await reader.fs.read("main", "/README.md");
assert.equal(native.args.at(-1).readOnly, true);
await assert.rejects(reader.fs.write("feat", "/README.md", "nope\n"), {
  code: "ReadOnly",
  message: /this Store is open read-only/,
});
await assert.rejects(reader.branch.fork("other"), { code: "ReadOnly" });
await reader.close();

const { fakeWorker } = await import("./fixture/fake-worker.mjs");
const { useWorker } = await import("../src/internal.js");
const worker = await fakeWorker();
const dir = mkdtempSync(join(tmpdir(), "layerfs-open-read-only-"));
try {
  const file = join(dir, "endpoint.json");
  worker.endpoint(file, ENV.LAYERFS_HOME);
  useWorker(file);
  const through = await LayerFS.openReadOnly("website");
  await through.fs.read("main", "/README.md");
  assert.equal(worker.calls.at(-1).args.readOnly, true);
  await through.close();
} finally {
  worker.close();
  rmSync(dir, { recursive: true, force: true });
}

console.log("PASS open_read_only");

import assert from "node:assert/strict";
import { test } from "node:test";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { createRecordingFactory } from "../../packages/testkit/dist/index.js";

const root = path.resolve(import.meta.dirname, "../..");
test("M0 architecture and exports are locked", () => {
  execFileSync(process.execPath, ["scripts/check-architecture.mjs"], { cwd: root });
  execFileSync(process.execPath, ["scripts/check-exports.mjs"], { cwd: root });
  assert.ok(true);
});

test("recording testkit fixtures preserve labels, seeds, restart hooks, and disposal", async () => {
  const events = [];
  const adapter = Object.freeze({});
  let disposed = 0;
  const factory = createRecordingFactory({
    name: "fake",
    async create() {
      return {
        adapter,
        capabilities: ["physical-reopen", "second-connection"],
        async reopen() { return adapter; },
        async openSecondConnection() { return adapter; },
        async dispose() { disposed += 1; },
      };
    },
  }, events);
  const fixture = await factory.create({ label: "m0", seed: 0x5eedc0de });
  assert.equal(await fixture.reopen({ readOnly: true, physical: true }), adapter);
  assert.equal(await fixture.openSecondConnection(), adapter);
  await fixture.dispose();
  await fixture.dispose();
  assert.equal(disposed, 1);
  assert.deepEqual(events, [
    { type: "create", factory: "fake", label: "m0", seed: 0x5eedc0de },
    { type: "reopen", readOnly: true, physical: true },
    { type: "second-connection" },
    { type: "dispose" },
  ]);
});

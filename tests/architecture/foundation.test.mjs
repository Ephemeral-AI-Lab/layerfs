import assert from "node:assert/strict";
import { test } from "node:test";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";
import { createRecordingFactory } from "../../packages/testkit/dist/index.js";

const root = path.resolve(import.meta.dirname, "../..");
test("M0 architecture and exports are locked", () => {
  execFileSync(process.execPath, ["scripts/check-architecture.mjs"], { cwd: root });
  execFileSync(process.execPath, ["scripts/check-exports.mjs"], { cwd: root });
  assert.ok(true);
});

test("CI invokes only the explicit highest accepted milestone gate", () => {
  const manifest = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
  assert.match(manifest.scripts["validate:accepted"], /^pnpm validate:m\d+$/);
  const workflow = readFileSync(path.join(root, ".github", "workflows", "ci.yml"), "utf8");
  assert.match(workflow, /- run: pnpm validate:accepted\s*$/m);
  assert.doesNotMatch(workflow, /- run: pnpm validate\s*$/m);
});

test("milestone gates select only their owned suites and sequential predecessors", () => {
  const scripts = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8")).scripts;
  const suites = {
    0: "tests/architecture",
    1: "tests/algorithms",
    2: "tests/storage tests/node-integration",
    3: "tests/conformance",
    4: "tests/branches",
    5: "tests/maintenance",
    6: "tests/durable-object-integration",
    7: "tests/node-vfs",
    8: "tests/replication",
    9: "tests/fault tests/smoke tests/performance",
    10: "tests/computer-integration",
  };
  for (const [milestone, owned] of Object.entries(suites)) {
    assert.equal(scripts[`test:m${milestone}`], `node scripts/run-test-suite.mjs ${owned}`);
    assert.match(scripts[`validate:m${milestone}`], new RegExp(`(?:^|&& pnpm )test:m${milestone}(?:$| )`));
    assert.doesNotMatch(scripts[`validate:m${milestone}`], /test:unit|test:smoke:built|test:fault:built|test:performance:built/);
    if (Number(milestone) > 0) assert.match(scripts[`validate:m${milestone}`], new RegExp(`^pnpm validate:m${Number(milestone) - 1} && `));
  }
  assert.equal(scripts["validate:accepted"], "pnpm validate:m0");
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

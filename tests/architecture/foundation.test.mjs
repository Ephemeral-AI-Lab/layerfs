import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import path from "node:path";
import { createRecordingFactory } from "../../packages/testkit/dist/index.js";
import { load as parseYaml } from "js-yaml";
import { documentationLinkErrors } from "../../scripts/documentation-links.mjs";
import { workflowPolicyErrors } from "../../scripts/workflow-policy.mjs";

const root = path.resolve(import.meta.dirname, "../..");
test("CI invokes only the explicit highest accepted milestone gate", () => {
  const manifest = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
  assert.match(manifest.scripts["validate:accepted"], /^pnpm validate:m\d+$/);
  const workflow = readFileSync(
    path.join(root, ".github", "workflows", "ci.yml"),
    "utf8",
  );
  const parsed = parseYaml(workflow);
  assert.deepEqual(workflowPolicyErrors(parsed), []);
  const runSteps = parsed.jobs.validate.steps
    .filter((step) => Object.hasOwn(step, "run"))
    .map((step) => step.run);
  assert.equal(
    runSteps.filter((command) => command === "pnpm validate:accepted").length,
    1,
  );
  assert.ok(runSteps.includes("pnpm install --frozen-lockfile"));
  assert.ok(!runSteps.includes("pnpm validate"));

  for (const fixture of [
    "comment-spoof.yml",
    "disabled-job.yml",
    "disabled-step.yml",
    "continue-on-error.yml",
    "paths-ignore.yml",
    "matrix-unused.yml",
  ]) {
    const invalid = parseYaml(
      readFileSync(path.join(root, "tests/fixtures/ci-bypasses", fixture), "utf8"),
    );
    assert.ok(workflowPolicyErrors(invalid).length > 0, fixture);
  }
});

test("milestone gates select only their owned suites and sequential predecessors", () => {
  const scripts = JSON.parse(
    readFileSync(path.join(root, "package.json"), "utf8"),
  ).scripts;
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
    assert.equal(
      scripts[`test:m${milestone}`],
      `node scripts/run-test-suite.mjs ${owned}`,
    );
    const expectedValidation =
      Number(milestone) === 0
        ? "pnpm fixtures:check && pnpm check:docs && pnpm check:evidence && pnpm check:style && pnpm check:architecture && pnpm build && pnpm check:exports && pnpm test:m0:validated"
        : Number(milestone) === 1
          ? "pnpm validate:m0 && pnpm test:m1 && pnpm test:workerd"
          : `pnpm validate:m${Number(milestone) - 1} && pnpm test:m${milestone}`;
    assert.equal(scripts[`validate:m${milestone}`], expectedValidation);
    assert.doesNotMatch(
      scripts[`validate:m${milestone}`],
      /test:unit|test:smoke:built|test:fault:built|test:performance:built/,
    );
    if (Number(milestone) > 0)
      assert.match(
        scripts[`validate:m${milestone}`],
        new RegExp(`^pnpm validate:m${Number(milestone) - 1} && `),
      );
  }
  assert.equal(scripts["validate:accepted"], "pnpm validate:m1");
});

test("documentation links resolve inline and reference-style targets", async () => {
  const filename = path.join(root, "docs", "fixture.md");
  const read = async (target) => {
    if (target !== path.join(root, "README.md")) throw new Error("missing");
    return "# Ephemeral AI FS\n";
  };
  assert.deepEqual(
    await documentationLinkErrors(
      "[inline](../README.md) [full][root] [collapsed][]\n\n[root]: ../README.md\n[collapsed]: ../README.md\n",
      filename,
      { root, read },
    ),
    [],
  );
  assert.deepEqual(
    await documentationLinkErrors("[broken][absent]", filename, { root, read }),
    ["undefined reference [absent]"],
  );
  assert.deepEqual(
    await documentationLinkErrors("[broken]: missing.md", filename, { root, read }),
    ["missing target missing.md"],
  );
  assert.deepEqual(
    await documentationLinkErrors(
      "```md\n[example][missing]\n```\n`[inline][missing]`\n",
      filename,
      { root, read },
    ),
    [],
  );
  assert.deepEqual(
    await documentationLinkErrors("[anchor](../README.md#not-present)", filename, {
      root,
      read,
    }),
    ["missing anchor #not-present in ../README.md"],
  );
  assert.deepEqual(
    await documentationLinkErrors("[escape](../../outside.md)", filename, {
      root: path.join(root, "docs"),
      read,
    }),
    ["target escapes repository: ../../outside.md"],
  );
});

test("recording testkit fixtures preserve labels, seeds, restart hooks, and disposal", async () => {
  const events = [];
  const adapter = Object.freeze({});
  let disposed = 0;
  const factory = createRecordingFactory(
    {
      name: "fake",
      async create() {
        return {
          adapter,
          capabilities: ["physical-reopen", "second-connection"],
          async reopen() {
            return adapter;
          },
          async openSecondConnection() {
            return adapter;
          },
          async dispose() {
            disposed += 1;
          },
        };
      },
    },
    events,
  );
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

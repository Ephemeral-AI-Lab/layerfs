import assert from "node:assert/strict";
import { test } from "node:test";
import { readFileSync } from "node:fs";
import path from "node:path";
import { createRecordingFactory } from "../../packages/testkit/dist/index.js";
import { load as parseYaml } from "js-yaml";
import { documentationLinkErrors } from "../../scripts/documentation-links.mjs";
import { workflowPolicyErrors } from "../../scripts/workflow-policy.mjs";
import eslintConfig from "../../eslint.config.js";

const root = path.resolve(import.meta.dirname, "../..");
test("lint exceptions are limited to deliberate code-generation fixtures", () => {
  const exception = eslintConfig.find((configuration) =>
    configuration.files?.includes(
      "tests/fixtures/architecture-bypasses/operations/direct-eval.ts",
    ),
  );
  assert.deepEqual(exception?.files, [
    "tests/fixtures/architecture-bypasses/operations/bound-eval.ts",
    "tests/fixtures/architecture-bypasses/operations/direct-eval.ts",
    "tests/fixtures/architecture-bypasses/operations/function-constructor.ts",
    "tests/fixtures/architecture-bypasses/operations/global-eval.ts",
  ]);
  assert.deepEqual(exception?.rules, {
    "no-eval": "off",
    "no-new-func": "off",
  });
});

test("CI invokes only the explicit highest accepted milestone gate", () => {
  const manifest = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8"));
  assert.match(manifest.scripts["validate:accepted"], /^pnpm validate:m\d+$/);
  const workflow = readFileSync(
    path.join(root, ".github", "workflows", "ci.yml"),
    "utf8",
  );
  const parsed = parseYaml(workflow);
  assert.deepEqual(workflowPolicyErrors(parsed), []);
  const checkoutSteps = parsed.jobs.validate.steps.filter((step) =>
    /^actions\/checkout@/u.test(step.uses ?? ""),
  );
  assert.equal(checkoutSteps.length, 1);
  assert.equal(checkoutSteps[0].with?.["fetch-depth"], 0);
  const runSteps = parsed.jobs.validate.steps
    .filter((step) => Object.hasOwn(step, "run"))
    .map((step) => step.run);
  assert.equal(
    runSteps.filter((command) => command === "pnpm validate:accepted").length,
    1,
  );
  assert.ok(runSteps.includes("pnpm install --frozen-lockfile"));
  assert.ok(!runSteps.includes("pnpm validate"));

  const fuseJob = parsed.jobs["m7-real-fuse"];
  assert.deepEqual(fuseJob["runs-on"], ["self-hosted", "linux", "x64", "fuse"]);
  assert.equal(fuseJob["timeout-minutes"], 10);
  const fuseRunSteps = fuseJob.steps
    .filter((step) => Object.hasOwn(step, "run"))
    .map((step) => step.run);
  assert.deepEqual(fuseRunSteps, [
    "pnpm install --frozen-lockfile",
    "pnpm build",
    "pnpm test:m7:fuse",
  ]);

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

  for (const mutate of [
    (copy) => delete copy.jobs["m7-real-fuse"],
    (copy) => (copy.jobs["m7-real-fuse"].if = "false"),
    (copy) => (copy.jobs["m7-real-fuse"]["continue-on-error"] = true),
    (copy) => (copy.jobs["m7-real-fuse"]["runs-on"] = ["ubuntu-latest"]),
    (copy) => (copy.jobs["m7-real-fuse"]["timeout-minutes"] = 11),
    (copy) => (copy.jobs["m7-real-fuse"].steps.at(-1).run = "pnpm test:m7:local"),
  ]) {
    const invalid = structuredClone(parsed);
    mutate(invalid);
    assert.ok(workflowPolicyErrors(invalid).length > 0);
  }
});

test("milestone gates select only their owned suites and sequential predecessors", () => {
  const scripts = JSON.parse(
    readFileSync(path.join(root, "package.json"), "utf8"),
  ).scripts;
  const testCommands = {
    0: "node scripts/run-test-suite.mjs tests/architecture",
    1: "node scripts/run-test-suite.mjs tests/algorithms",
    2: "node scripts/run-test-suite.mjs tests/storage tests/node-integration tests/maintenance",
    3: "node scripts/run-test-suite.mjs tests/conformance",
    4: "node scripts/run-test-suite.mjs tests/branches",
    5: "node scripts/run-test-suite.mjs tests/maintenance tests/fault",
    6: "node scripts/run-m6-local-gate.mjs",
    7: "pnpm test:m7:local",
    8: "node scripts/run-test-suite.mjs tests/replication",
    9: "node scripts/run-test-suite.mjs tests/fault tests/smoke tests/performance",
    10: "node scripts/run-test-suite.mjs tests/computer-integration",
  };
  for (const [milestone, command] of Object.entries(testCommands)) {
    assert.equal(scripts[`test:m${milestone}`], command);
    const expectedValidation =
      Number(milestone) === 7
        ? "pnpm validate:m6 && pnpm test:m7:local && pnpm check:evidence"
        : Number(milestone) < 7
          ? `pnpm validate:m${milestone}:pre-evidence && pnpm check:evidence`
          : `pnpm validate:m${Number(milestone) - 1} && pnpm test:m${milestone}`;
    assert.equal(scripts[`validate:m${milestone}`], expectedValidation);
    assert.doesNotMatch(scripts[`validate:m${milestone}`], /test:unit/);
    if (Number(milestone) > 7)
      assert.match(
        scripts[`validate:m${milestone}`],
        new RegExp(`^pnpm validate:m${Number(milestone) - 1} && `),
      );
  }
  assert.equal(
    scripts["validate:m0:pre-evidence"],
    "pnpm fixtures:check && pnpm check:docs && pnpm check:style && pnpm check:architecture && pnpm build && pnpm check:exports && node --test tests/architecture/foundation.test.mjs",
  );
  assert.equal(
    scripts["validate:m1:pre-evidence"],
    "pnpm validate:m0:pre-evidence && pnpm test:m1 && pnpm test:workerd",
  );
  assert.equal(
    scripts["validate:m2:pre-evidence"],
    "pnpm validate:m1:pre-evidence && pnpm test:m2",
  );
  assert.equal(
    scripts["validate:m3:pre-evidence"],
    "pnpm validate:m2:pre-evidence && pnpm test:m3 && pnpm test:smoke:built && pnpm bench:m3",
  );
  assert.equal(
    scripts["validate:m4:pre-evidence"],
    "pnpm validate:m3:pre-evidence && pnpm test:m4 && pnpm bench:branches",
  );
  assert.equal(
    scripts["validate:m5:pre-evidence"],
    "node scripts/run-accepted-node-gate.mjs",
  );
  assert.equal(
    scripts["validate:m6:pre-evidence"],
    "pnpm validate:m5:pre-evidence && node scripts/run-m6-local-gate.mjs --skip-build",
  );
  assert.equal(
    scripts["validate:m7:pre-evidence"],
    "pnpm validate:m6 && pnpm test:m7:local && pnpm test:m7:fuse",
  );
  const acceptedNodeGate = readFileSync(
    path.join(root, "scripts", "run-accepted-node-gate.mjs"),
    "utf8",
  );
  for (const requiredSelection of [
    "const deadlineMs = 600_000;",
    'runPnpm("workspace-build", ["build"])',
    'runPnpm("fixtures-check", ["fixtures:check"])',
    'runPnpm("docs-check", ["check:docs"])',
    'runPnpm("style-check", ["check:style"])',
    'runPnpm("architecture-check", ["check:architecture"])',
    'runPnpm("exports-check", ["check:exports"])',
    '"tests/architecture"',
    '"tests/algorithms"',
    '"tests/storage"',
    '"tests/node-integration"',
    '"tests/conformance"',
    '"tests/branches"',
    '"tests/maintenance"',
    '"tests/fault"',
    '"tests/smoke"',
    '"scripts/check-workerd-algorithms.mjs"',
    '"tests/performance/mini-bench.mjs"',
    '"tests/performance/branch-bench.mjs"',
  ])
    assert.ok(
      acceptedNodeGate.includes(requiredSelection),
      `accepted Node gate omitted ${requiredSelection}`,
    );
  const m6LocalGate = readFileSync(
    path.join(root, "scripts", "run-m6-local-gate.mjs"),
    "utf8",
  );
  for (const requiredSelection of [
    "const deadlineMs = 600_000;",
    'runPnpm("workspace-build", ["build"])',
    '"scripts/check-cloudflare-preview.mjs"',
    '"scripts/check-workerd-algorithms.mjs"',
    '"tests/durable-object-integration/vitest.node.config.ts"',
    '"tests/durable-object-integration/vitest.config.ts"',
  ])
    assert.ok(
      m6LocalGate.includes(requiredSelection),
      `M6 local gate omitted ${requiredSelection}`,
    );
  assert.ok(
    ["pnpm validate:m7", "pnpm validate:m8"].includes(scripts["validate:accepted"]),
  );
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

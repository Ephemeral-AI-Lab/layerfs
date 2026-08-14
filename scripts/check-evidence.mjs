import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import { load as parseYaml } from "js-yaml";
import ts from "typescript";
import { workflowPolicyErrors } from "./workflow-policy.mjs";

const execute = promisify(execFile);
const root = path.resolve(import.meta.dirname, "..");
const packageManifest = JSON.parse(
  await readFile(path.join(root, "package.json"), "utf8"),
);
const acceptedValidation = packageManifest.scripts?.["validate:accepted"];
const acceptedMatch = /^pnpm validate:(m\d+)$/u.exec(acceptedValidation ?? "");
if (!acceptedMatch)
  throw new Error("validate:accepted must select one milestone validation command");
const activeAcceptedMilestone = acceptedMatch[1];

function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}
if (
  !new Set(["m0", "m1", "m2", "m3", "m4", "m5", "m6", "m7", "m8"]).has(
    activeAcceptedMilestone,
  )
)
  throw new Error(
    `evidence checker has no validation schema for ${activeAcceptedMilestone}`,
  );

function requireObject(value, name) {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error(`${name} must be an object`);
  return value;
}
function requirePositiveInteger(value, name) {
  if (!Number.isSafeInteger(value) || value <= 0)
    throw new Error(`${name} must be a positive safe integer`);
}
function requireNonemptyString(value, name) {
  if (typeof value !== "string" || value.length === 0)
    throw new Error(`${name} must be a nonempty string`);
}
function requireScalarRecord(value, name) {
  const record = requireObject(value, name);
  for (const [key, item] of Object.entries(record)) {
    if (
      !key ||
      (item !== null &&
        typeof item !== "string" &&
        typeof item !== "number" &&
        typeof item !== "boolean")
    )
      throw new Error(`${name}.${key || "<empty>"} must be a scalar`);
  }
  return record;
}
function logLineObject(source, schema, name) {
  const values = source
    .split(/\r?\n/u)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return undefined;
      }
    })
    .filter((value) => value?.schema === schema);
  if (values.length !== 1)
    throw new Error(`${name} must contain exactly one ${schema} record`);
  return requireObject(values[0], `${name}.${schema}`);
}
function m7LogMeta(source, name) {
  const matches = [
    ...source.matchAll(
      /^M7_LOG_META exitCode=(\d+) elapsedMs=(\d+) candidate=([0-9a-f]{40}) command=([a-z0-9_]+)$/gmu,
    ),
  ];
  if (matches.length !== 1)
    throw new Error(`${name} must contain one exact M7_LOG_META`);
  return {
    exitCode: Number(matches[0][1]),
    elapsedMs: Number(matches[0][2]),
    candidate: matches[0][3],
    command: matches[0][4],
  };
}
function validateM6ResultContexts(artifact) {
  const profiles = requireObject(artifact.contextProfiles, "m6.contextProfiles");
  for (const required of ["node", "durableObject", "durableObjectScale", "workerd"])
    if (!(required in profiles))
      throw new Error(`m6.contextProfiles.${required} is required`);
  for (const [name, value] of Object.entries(profiles)) {
    const profile = requireObject(value, `m6.contextProfiles.${name}`);
    requireNonemptyString(profile.driver, `m6.contextProfiles.${name}.driver`);
    const capabilities = requireScalarRecord(
      profile.capabilities,
      `m6.contextProfiles.${name}.capabilities`,
    );
    if (!Object.keys(capabilities).length)
      throw new Error(`m6.contextProfiles.${name}.capabilities must not be empty`);
    const limits = requireObject(profile.limits, `m6.contextProfiles.${name}.limits`);
    if (!Object.keys(limits).length)
      throw new Error(`m6.contextProfiles.${name}.limits must not be empty`);
    for (const [key, item] of Object.entries(limits))
      requirePositiveInteger(item, `m6.contextProfiles.${name}.limits.${key}`);
    const environment = requireScalarRecord(
      profile.environment,
      `m6.contextProfiles.${name}.environment`,
    );
    for (const key of [
      "platform",
      "architecture",
      "node",
      "pnpm",
      "cpu",
      "storage",
      "sqlite",
    ])
      requireNonemptyString(
        environment[key],
        `m6.contextProfiles.${name}.environment.${key}`,
      );
  }
  if (
    profiles.node.driver !== "sqlite-node" ||
    profiles.node.capabilities.schemaIdentityMode !== "sqlite-header" ||
    profiles.node.environment.sqlite !== "3.50.4" ||
    profiles.durableObject.driver !== "sqlite-cloudflare" ||
    profiles.durableObject.capabilities.schemaIdentityMode !== "durable-table" ||
    profiles.durableObject.environment.sqlite !== "3.47.0" ||
    profiles.durableObjectScale.driver !== "sqlite-cloudflare" ||
    profiles.durableObjectScale.capabilities.schemaIdentityMode !== "durable-table" ||
    profiles.durableObjectScale.capabilities.maxPhysicalDatabaseBytes !==
      512 * 1024 * 1024 ||
    profiles.durableObjectScale.capabilities.maxJournalBytes !== 512 * 1024 * 1024
  )
    throw new Error("m6 result context profiles do not identify the exact target");

  const requiredNames = new Set([
    "workerd-algorithms",
    "node-storage",
    "node-filesystem",
    "node-driver",
    "node-branch",
    "node-maintenance-restart",
    "node-maintenance-corruption",
    "node-maintenance-quota",
    "durable-object-storage",
    "durable-object-filesystem",
    "durable-object-driver",
    "durable-object-branch",
    "durable-object-maintenance-restart",
    "durable-object-maintenance-corruption",
    "durable-object-maintenance-quota",
    "node-schema-migration",
    "durable-object-schema-migration",
    "node-initialization-identity",
    "durable-object-initialization-identity",
    "node-cow",
    "durable-object-cow",
    "node-restart",
    "durable-object-restart",
    "node-filesystem-fault",
    "durable-object-filesystem-fault",
    "node-publication-fault",
    "durable-object-publication-fault",
    "node-maintenance-fault",
    "durable-object-maintenance-fault",
    "node-scale",
    "durable-object-scale",
    "durable-object-smoke",
    "durable-object-raw-resource-control",
    "preview-bundle",
  ]);
  if (!Array.isArray(artifact.resultContexts))
    throw new Error("m6.resultContexts must be an array");
  const contexts = new Map();
  for (const [index, value] of artifact.resultContexts.entries()) {
    const context = requireObject(value, `m6.resultContexts[${index}]`);
    requireNonemptyString(context.name, `m6.resultContexts[${index}].name`);
    if (contexts.has(context.name))
      throw new Error(`m6 result context ${context.name} is duplicated`);
    requireNonemptyString(context.profile, `m6.resultContexts[${index}].profile`);
    const profile = profiles[context.profile];
    if (!profile)
      throw new Error(`m6 result context ${context.name} names an absent profile`);
    if (context.commit !== artifact.commit)
      throw new Error(`m6 result context ${context.name} has a different commit`);
    if (context.schemaVersion !== artifact.schemaVersion)
      throw new Error(`m6 result context ${context.name} has a different schema`);
    if (context.formatVersion !== artifact.formatVersion)
      throw new Error(`m6 result context ${context.name} has a different format`);
    if (context.driver !== profile.driver)
      throw new Error(`m6 result context ${context.name} has a different driver`);
    if (!Number.isSafeInteger(context.seed) || context.seed < 0)
      throw new Error(`m6 result context ${context.name} has an invalid seed`);
    if (!/^[0-9a-f]{64}$/u.test(context.fixtureDigest ?? ""))
      throw new Error(`m6 result context ${context.name} lacks a fixture digest`);
    requireNonemptyString(context.faultPoint, `m6.resultContexts[${index}].faultPoint`);
    contexts.set(context.name, context);
    requiredNames.delete(context.name);
  }
  if (requiredNames.size)
    throw new Error(
      `m6 result contexts are incomplete: ${[...requiredNames].join(", ")}`,
    );
  const portableFixtures = {
    "node-storage": ["portable-storage", 0x57a61e],
    "node-filesystem": ["portable-m6", 0x5eedc0de],
    "node-driver": ["portable-driver", 0xd21e],
    "node-branch": ["portable-branches", 0xb2a6c4],
    "node-maintenance-restart": ["portable-maintenance-restart", 0x6d61696e],
    "node-maintenance-corruption": ["portable-maintenance-corruption", 0xc011ec7],
    "node-maintenance-quota": ["portable-maintenance-quota", 0x71756f74],
    "durable-object-storage": ["portable-storage", 0x57a61e],
    "durable-object-filesystem": ["portable-m6", 0x5eedc0de],
    "durable-object-driver": ["portable-driver", 0xd21e],
    "durable-object-branch": ["portable-branches", 0xb2a6c4],
    "durable-object-maintenance-restart": ["portable-maintenance-restart", 0x6d61696e],
    "durable-object-maintenance-corruption": [
      "portable-maintenance-corruption",
      0xc011ec7,
    ],
    "durable-object-maintenance-quota": ["portable-maintenance-quota", 0x71756f74],
  };
  for (const [name, [label, seed]] of Object.entries(portableFixtures)) {
    const context = contexts.get(name);
    const expectedDigest = createHash("sha256")
      .update(`efs-portable-fixture-context-v1\n${label}\n${seed}\n`)
      .digest("hex");
    if (
      context.fixtureLabel !== label ||
      context.seed !== seed ||
      context.fixtureDigest !== expectedDigest ||
      context.fixtureDigestBasis !== "sha256-utf8-canonical-fixture-descriptor"
    )
      throw new Error(`m6 result context ${name} has an invalid fixture identity`);
  }
  return { profiles, contexts };
}
function validateStructuredDeviations(artifact, name) {
  if (!Array.isArray(artifact.deviations))
    throw new Error(`${name}.deviations must be an array`);
  for (const [index, value] of artifact.deviations.entries()) {
    const deviation = requireObject(value, `${name}.deviations[${index}]`);
    requireNonemptyString(
      deviation.description,
      `${name}.deviations[${index}].description`,
    );
    requireNonemptyString(deviation.owner, `${name}.deviations[${index}].owner`);
    requireNonemptyString(
      deviation.followUpMilestone,
      `${name}.deviations[${index}].followUpMilestone`,
    );
  }
}
function candidateFromExit(source, milestone) {
  const match = source.match(/^- Candidate commit: `([0-9a-f]{40})`$/mu);
  if (!match) throw new Error(`${milestone} exit is missing an exact candidate commit`);
  return match[1];
}
async function evidenceCommit(filename) {
  return (
    await execute("git", ["log", "-1", "--format=%H", "--", filename], {
      cwd: root,
      windowsHide: true,
    })
  ).stdout.trim();
}
function ownedByMilestone(milestone, filename) {
  const rootFiles = new Set([
    ".prettierignore",
    ".prettierrc.json",
    ".markdownlint-cli2.jsonc",
    "eslint.config.js",
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "tsconfig.base.json",
    ".github/workflows/ci.yml",
  ]);
  const m0 =
    rootFiles.has(filename) ||
    (filename.startsWith("scripts/") &&
      filename !== "scripts/check-workerd-algorithms.mjs") ||
    /^packages\/[^/]+\/(?:package\.json|tsconfig\.json|README\.md|LICENSE)$/u.test(
      filename,
    ) ||
    filename.includes("/api-snapshots/") ||
    filename.startsWith("tests/architecture/") ||
    filename.startsWith("tests/fixtures/");
  if (milestone === "m0") return m0;
  const m1 =
    m0 ||
    filename.startsWith("tests/algorithms/") ||
    filename.startsWith("tests/workerd/") ||
    filename === "scripts/check-workerd-algorithms.mjs" ||
    filename.startsWith("docs/spec/") ||
    filename.startsWith("docs/testing/") ||
    filename === "docs/implementation/implementation-plan.md";
  if (milestone === "m1") return m1;
  const m2 =
    m1 ||
    filename.startsWith("packages/fs/src/") ||
    filename.startsWith("packages/sqlite-node/src/") ||
    filename.startsWith("tests/storage/") ||
    filename.startsWith("tests/node-integration/") ||
    filename.startsWith("tests/maintenance/");
  if (milestone === "m2") return m2;
  const m3 =
    m2 ||
    filename.startsWith("tests/conformance/") ||
    filename.startsWith("packages/sqlite-cloudflare/src/") ||
    filename.startsWith("packages/testkit/src/") ||
    filename.startsWith("tests/smoke/") ||
    filename === "tests/helpers/runtime-environment.mjs" ||
    filename === "tests/performance/mini-bench.mjs" ||
    filename.startsWith("docs/benchmarks/");
  if (milestone === "m3") return m3;
  const m4 =
    m3 ||
    filename.startsWith("tests/branches/") ||
    filename === "tests/performance/branch-bench.mjs";
  if (milestone === "m4") return m4;
  const m5 =
    m4 ||
    filename.startsWith("tests/fault/") ||
    filename === "docs/implementation/m5-handoff.md";
  if (milestone === "m5") return m5;
  const m6 =
    m5 ||
    filename.startsWith("tests/durable-object-integration/") ||
    filename.startsWith("examples/durable-object-workspace/") ||
    filename === "docs/implementation/m6-handoff.md";
  if (milestone === "m6") return m6;
  return (
    m6 ||
    filename.startsWith("packages/node-vfs/src/") ||
    filename.startsWith("tests/node-vfs/") ||
    filename === "scripts/run-m7-local-gate.mjs" ||
    filename === "scripts/run-m7-fuse-gate.mjs" ||
    filename === "README.md" ||
    filename === "docs/implementation/m7-handoff.md"
  );
}
function ownedByM7Candidate(filename) {
  return new Set([
    ".github/workflows/ci.yml",
    "README.md",
    "package.json",
    "pnpm-lock.yaml",
    "docs/implementation/implementation-plan.md",
    "docs/implementation/m7-handoff.md",
    "packages/fs/api-snapshots/integrations-node-vfs.d.ts",
    "packages/fs/api-snapshots/integrations-node-vfs.rollup.d.ts",
    "packages/fs/api-snapshots/integrations-node-vfs.symbols.json",
    "packages/fs/src/integrations/node-vfs.ts",
    "packages/fs/src/operations/durable-edit-prepare.ts",
    "packages/fs/src/operations/filesystem.ts",
    "packages/fs/src/operations/node-vfs-bridge.ts",
    "packages/fs/src/operations/streaming-prepare.ts",
    "packages/node-vfs/api-snapshots/root.d.ts",
    "packages/node-vfs/api-snapshots/root.rollup.d.ts",
    "packages/node-vfs/src/index.ts",
    "packages/testkit/api-snapshots/root.d.ts",
    "packages/testkit/api-snapshots/root.rollup.d.ts",
    "packages/testkit/api-snapshots/root.symbols.json",
    "packages/testkit/src/index.ts",
    "packages/testkit/src/node-vfs.ts",
    "scripts/check-evidence.mjs",
    "scripts/run-m7-fuse-gate.mjs",
    "scripts/run-m7-local-gate.mjs",
    "scripts/workflow-policy.mjs",
    "tests/architecture/foundation.test.mjs",
    "tests/node-vfs/node-vfs-regression.test.mjs",
    "tests/node-vfs/node-vfs.test.mjs",
    "tests/node-vfs/real-fuse-server.mjs",
    "tests/node-vfs/real-fuse-smoke.mjs",
  ]).has(filename);
}
const m1SourceEntrypoints = [
  "packages/fs/src/cas/sha256.ts",
  "packages/fs/src/cdc/fastcdc.ts",
  "packages/fs/src/cow/pages.ts",
  "packages/fs/src/patches/patches.ts",
  "packages/fs/src/manifests/builder.ts",
  "packages/fs/src/manifests/codec.ts",
  "packages/fs/src/manifests/cursor.ts",
  "packages/fs/src/manifests/grouping.ts",
  "packages/fs/src/operations/full-rebuild.ts",
  "packages/fs/src/operations/local-rebuild.ts",
  "packages/fs/src/operations/streamed-rebuild.ts",
];
async function gitFile(commit, filename) {
  return (
    await execute("git", ["show", `${commit}:${filename}`], {
      cwd: root,
      windowsHide: true,
      maxBuffer: 16 * 1024 * 1024,
    })
  ).stdout;
}
function relativeModuleSpecifiers(filename, source) {
  const parsed = ts.createSourceFile(
    filename,
    source,
    ts.ScriptTarget.Latest,
    true,
    ts.ScriptKind.TS,
  );
  const result = [];
  const visit = (node) => {
    let specifier;
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier &&
      ts.isStringLiteralLike(node.moduleSpecifier)
    )
      specifier = node.moduleSpecifier.text;
    else if (ts.isImportTypeNode(node) && ts.isLiteralTypeNode(node.argument)) {
      const literal = node.argument.literal;
      if (ts.isStringLiteralLike(literal)) specifier = literal.text;
    }
    if (specifier?.startsWith(".")) result.push(specifier);
    ts.forEachChild(node, visit);
  };
  visit(parsed);
  return result;
}
async function m1SourceClosure(commit, available) {
  const closure = new Set();
  const pending = [...m1SourceEntrypoints];
  while (pending.length) {
    const filename = pending.pop();
    if (closure.has(filename) || !available.has(filename)) continue;
    closure.add(filename);
    const source = await gitFile(commit, filename);
    for (const specifier of relativeModuleSpecifiers(filename, source)) {
      const base = path.posix.normalize(
        path.posix.join(path.posix.dirname(filename), specifier),
      );
      const candidates = base.endsWith(".js")
        ? [`${base.slice(0, -3)}.ts`]
        : [base, `${base}.ts`, `${base}/index.ts`];
      const target = candidates.find((candidate) => available.has(candidate));
      if (target) pending.push(target);
    }
  }
  return closure;
}
async function ownedTreeDigest(milestone, commit) {
  const output = (
    await execute("git", ["ls-tree", "-r", "--full-tree", commit], {
      cwd: root,
      windowsHide: true,
      maxBuffer: 16 * 1024 * 1024,
    })
  ).stdout;
  const records = output
    .trim()
    .split(/\r?\n/u)
    .map((line) => {
      const match = line.match(/^\d+ blob ([0-9a-f]{40})\t(.+)$/u);
      return match ? { hash: match[1], filename: match[2] } : undefined;
    })
    .filter(Boolean);
  const available = new Set(records.map((record) => record.filename));
  const sourceClosure =
    milestone === "m1" ? await m1SourceClosure(commit, available) : new Set();
  const ownedRecords = records
    .filter(
      (record) =>
        ownedByMilestone(milestone, record.filename) ||
        sourceClosure.has(record.filename),
    )
    .sort((left, right) => left.filename.localeCompare(right.filename));
  const digest = createHash("sha256");
  for (const record of ownedRecords)
    digest.update(record.filename).update("\0").update(record.hash).update("\n");
  return digest.digest("hex");
}
async function assertOwnedWorktreeClean(milestone) {
  if (process.env.M8_PRECOMMIT === "1") return;
  const status = (
    await execute("git", ["status", "--porcelain=v1", "-z", "--untracked-files=all"], {
      cwd: root,
      windowsHide: true,
      maxBuffer: 16 * 1024 * 1024,
    })
  ).stdout;
  if (status)
    throw new Error(`${milestone} evidence requires a completely clean worktree`);
}
async function validateMilestone(
  name,
  requiredMetrics,
  {
    requireCurrentDigest = false,
    requireStructuredContext = false,
    allowLegacyExitFollowups = false,
  } = {},
) {
  const directory = path.join(root, "docs", "evidence", name);
  const jsonFilename = path.join(directory, "correctness.json");
  const exitFilename = path.join(directory, "exit.md");
  const artifact = requireObject(
    JSON.parse(await readFile(jsonFilename, "utf8")),
    `${name} correctness artifact`,
  );
  if (artifact.schema !== "efs-correctness-result-v1")
    throw new Error(`${name} correctness artifact has an invalid schema`);
  if (!/^[0-9a-f]{40}$/u.test(artifact.commit))
    throw new Error(`${name} correctness artifact has an invalid commit`);
  if (requireStructuredContext) {
    requireNonemptyString(artifact.adapter, `${name}.adapter`);
    requireNonemptyString(artifact.driver, `${name}.driver`);
    const capabilities = requireScalarRecord(
      artifact.capabilities,
      `${name}.capabilities`,
    );
    const limits = requireObject(artifact.limits, `${name}.limits`);
    if (!Object.keys(capabilities).length)
      throw new Error(`${name}.capabilities must not be empty`);
    if (!Object.keys(limits).length)
      throw new Error(`${name}.limits must not be empty`);
    for (const [key, value] of Object.entries(limits))
      requirePositiveInteger(value, `${name}.limits.${key}`);
    if (!Array.isArray(artifact.commands) || !artifact.commands.length)
      throw new Error(`${name}.commands must be a nonempty array`);
    for (const [index, command] of artifact.commands.entries())
      requireNonemptyString(command, `${name}.commands[${index}]`);
    const environment = requireScalarRecord(
      artifact.environment,
      `${name}.environment`,
    );
    for (const key of ["platform", "architecture", "node", "pnpm"])
      requireNonemptyString(environment[key], `${name}.environment.${key}`);
    requireNonemptyString(artifact.fixtureDigest, `${name}.fixtureDigest`);
    if (!/^[0-9a-f]{64}$/u.test(artifact.fixtureDigest))
      throw new Error(`${name}.fixtureDigest must be a SHA-256 digest`);
    if (typeof artifact.faultPoint !== "string" || !artifact.faultPoint.length)
      throw new Error(`${name}.faultPoint must identify the exercised fault boundary`);
  }
  requirePositiveInteger(artifact.schemaVersion, `${name}.schemaVersion`);
  requirePositiveInteger(artifact.passed, `${name}.passed`);
  requirePositiveInteger(artifact.elapsedMs, `${name}.elapsedMs`);
  if (artifact.failed !== 0) throw new Error(`${name} correctness artifact failed > 0`);
  const metrics = requireObject(artifact.metrics, `${name}.metrics`);
  for (const metric of requiredMetrics)
    requirePositiveInteger(metrics[metric], `${name}.metrics.${metric}`);
  if (metrics.operatingSystems * metrics.nodeVersions !== metrics.matrixRuns)
    throw new Error(`${name} matrix metrics are inconsistent`);
  const exit = await readFile(exitFilename, "utf8");
  const candidate = candidateFromExit(exit, name);
  if (candidate !== artifact.commit)
    throw new Error(`${name} exit candidate differs from correctness commit`);
  await execute("git", ["cat-file", "-e", `${candidate}^{commit}`], {
    cwd: root,
    windowsHide: true,
  });
  const recordCommit = await evidenceCommit(path.relative(root, jsonFilename));
  const exitRecordCommit = await evidenceCommit(path.relative(root, exitFilename));
  if (exitRecordCommit !== recordCommit) {
    if (!allowLegacyExitFollowups)
      throw new Error(`${name} exit and correctness artifacts have different commits`);
    try {
      await execute(
        "git",
        ["merge-base", "--is-ancestor", recordCommit, exitRecordCommit],
        { cwd: root, windowsHide: true },
      );
    } catch {
      throw new Error(
        `${name} legacy exit follow-up is not descended from its correctness record`,
      );
    }
  }
  const parents = (
    await execute("git", ["show", "-s", "--format=%P", recordCommit], {
      cwd: root,
      windowsHide: true,
    })
  ).stdout
    .trim()
    .split(/\s+/u);
  if (parents[0] !== candidate)
    throw new Error(
      `${name} evidence commit is not directly parented by its candidate`,
    );
  if (!/^[0-9a-f]{64}$/u.test(artifact.ownedTreeDigest ?? ""))
    throw new Error(`${name} correctness artifact lacks an owned-tree digest`);
  const candidateDigest = await ownedTreeDigest(name, candidate);
  if (artifact.ownedTreeDigest !== candidateDigest)
    throw new Error(`${name} evidence digest differs from its candidate tree`);
  if (requireCurrentDigest) {
    const currentDigest = await ownedTreeDigest(name, "HEAD");
    if (artifact.ownedTreeDigest !== currentDigest)
      throw new Error(`${name} accepted evidence is stale for milestone-owned files`);
  }
  await assertOwnedWorktreeClean(name);
  return { artifact, exit, candidate, recordCommit };
}

async function validateBenchmarkArtifact(filename, candidate, evidenceRecordCommit) {
  const relative = path.posix.normalize(filename.replaceAll("\\", "/"));
  const artifact = requireObject(
    JSON.parse(await readFile(path.join(root, relative), "utf8")),
    `${relative} benchmark artifact`,
  );
  if (artifact.schema !== "efs-benchmark-result-v1")
    throw new Error(`${relative} has an invalid benchmark schema`);
  if (artifact.commit !== candidate)
    throw new Error(`${relative} is not bound to its accepted candidate`);
  if (artifact.worktreeDirty !== false)
    throw new Error(`${relative} was not measured from a clean worktree`);
  requireNonemptyString(artifact.driver, `${relative}.driver`);
  const environment = requireScalarRecord(
    artifact.environment,
    `${relative}.environment`,
  );
  for (const key of [
    "platform",
    "architecture",
    "node",
    "pnpm",
    "cpu",
    "storage",
    "sqlite",
  ])
    requireNonemptyString(environment[key], `${relative}.environment.${key}`);
  requirePositiveInteger(
    environment.totalMemoryBytes,
    `${relative}.environment.totalMemoryBytes`,
  );
  const resourceLimits = requireObject(
    artifact.configuration?.resourceLimits,
    `${relative}.configuration.resourceLimits`,
  );
  for (const domain of ["filesystem", "storage", "runtime", "branch"]) {
    const values = requireObject(
      resourceLimits[domain],
      `${relative}.configuration.resourceLimits.${domain}`,
    );
    if (!Object.keys(values).length)
      throw new Error(`${relative} has an empty ${domain} resource-limit domain`);
    for (const [key, value] of Object.entries(values))
      requirePositiveInteger(
        value,
        `${relative}.configuration.resourceLimits.${domain}.${key}`,
      );
  }
  const fixture = requireObject(artifact.fixture, `${relative}.fixture`);
  requireNonemptyString(fixture.name, `${relative}.fixture.name`);
  if (!/^[0-9a-f]{64}$/u.test(fixture.sha256 ?? ""))
    throw new Error(`${relative} fixture lacks a SHA-256 digest`);
  requirePositiveInteger(artifact.trials, `${relative}.trials`);
  if (!Array.isArray(artifact.samples) || artifact.samples.length !== artifact.trials)
    throw new Error(`${relative} does not retain every raw measured trial`);
  if (artifact.pass !== true) throw new Error(`${relative} benchmark did not pass`);
  if ((await evidenceCommit(relative)) !== evidenceRecordCommit)
    throw new Error(`${relative} was not committed with its milestone evidence`);
  return artifact;
}

if (process.argv[2] === "--owned-tree-digest") {
  const milestone = process.argv[3];
  const commit = process.argv[4] ?? "HEAD";
  if (!new Set(["m0", "m1", "m2", "m3", "m4", "m5", "m6"]).has(milestone))
    throw new Error("owned-tree digest milestone must be m0 through m6");
  console.log(await ownedTreeDigest(milestone, commit));
  process.exit(0);
}

const m0 = await validateMilestone(
  "m0",
  [
    "operatingSystems",
    "nodeVersions",
    "matrixRuns",
    "architectureTestsPerCell",
    "coreSourceFiles",
    "negativeArchitectureFixtures",
    "publishablePackages",
    "publicEntrypoints",
    "exportedSymbols",
    "cleanDistFiles",
    "packedTarballs",
    "packedFiles",
  ],
  { requireCurrentDigest: activeAcceptedMilestone === "m0" },
);
if (
  m0.artifact.passed !==
  m0.artifact.metrics.matrixRuns * m0.artifact.metrics.architectureTestsPerCell
)
  throw new Error("m0 passed count differs from the recorded tests per matrix cell");

const m1 = await validateMilestone(
  "m1",
  [
    "operatingSystems",
    "nodeVersions",
    "matrixRuns",
    "nodeAlgorithmTests",
    "workerdChecks",
    "streamedManifestEntries",
    "streamedManifestReadBatchRecords",
    "streamedManifestPeakRetainedRecords",
  ],
  { requireCurrentDigest: activeAcceptedMilestone === "m1" },
);
if (
  m1.artifact.passed !==
  m1.artifact.metrics.nodeAlgorithmTests + m1.artifact.metrics.workerdChecks
)
  throw new Error("m1 passed count differs from Node plus workerd checks");
const predecessor = m1.exit.match(
  /Sequential predecessor:[\s\S]*?`([0-9a-f]{40})`/u,
)?.[1];
if (predecessor !== m0.candidate)
  throw new Error("m1 sequential predecessor differs from the accepted m0 candidate");

const m2 = await validateMilestone(
  "m2",
  [
    "operatingSystems",
    "nodeVersions",
    "matrixRuns",
    "nodeStorageTests",
    "maintenanceTests",
    "streamedBytes",
    "streamManagedPeakBytes",
    "fallbackManagedPeakBytes",
    "fallbackSourceReadCalls",
    "fallbackStorageTransactions",
    "observedWalBytes",
    "sealedManifestEntries",
    "finalCertificateValidationStatements",
  ],
  {
    requireCurrentDigest: activeAcceptedMilestone === "m2",
    // The accepted M2 correctness record was committed directly after its candidate;
    // two later documentation-only commits repaired its benchmark link and restored
    // the M1 predecessor. M3 and later evidence is required to remain atomic.
    allowLegacyExitFollowups: true,
  },
);
if (
  m2.artifact.passed !==
  m2.artifact.metrics.nodeStorageTests + m2.artifact.metrics.maintenanceTests
)
  throw new Error("m2 passed count differs from storage plus maintenance checks");
if (m2.artifact.independentAudit !== "approved")
  throw new Error("m2 correctness artifact lacks independent audit approval");
const m2Predecessor = m2.exit.match(
  /Sequential predecessor:[\s\S]*?`([0-9a-f]{40})`/u,
)?.[1];
if (m2Predecessor !== m1.candidate)
  throw new Error("m2 sequential predecessor differs from the accepted m1 candidate");

const m3 = await validateMilestone(
  "m3",
  [
    "operatingSystems",
    "nodeVersions",
    "matrixRuns",
    "nodeAlgorithmTests",
    "workerdChecks",
    "nodeStorageTests",
    "maintenanceTests",
    "conformanceTests",
    "nodeSmokeTests",
    "readMiBPerSecondCold",
    "readMiBPerSecondWarm",
    "readTransactionsPerHundredMiB",
    "readStatementsPerHundredMiB",
    "smallReadMicrosPerOp",
    "localEditMsTotal",
    "scatteredEditCompleted",
    "workerdWriteHashingMiBPerSecond",
    "workerdWriteHashingSpeedupPercent",
  ],
  {
    requireCurrentDigest: activeAcceptedMilestone === "m3",
    requireStructuredContext: true,
  },
);
if (
  m3.artifact.passed !==
  m3.artifact.metrics.nodeAlgorithmTests +
    m3.artifact.metrics.workerdChecks +
    m3.artifact.metrics.nodeStorageTests +
    m3.artifact.metrics.maintenanceTests +
    m3.artifact.metrics.conformanceTests +
    m3.artifact.metrics.nodeSmokeTests
)
  throw new Error("m3 passed count differs from the recorded suite checks");
if (
  m3.artifact.metrics.nodeAlgorithmTests !== 40 ||
  m3.artifact.metrics.workerdChecks !== 12 ||
  m3.artifact.metrics.nodeStorageTests !== 92 ||
  m3.artifact.metrics.maintenanceTests < 31 ||
  m3.artifact.metrics.conformanceTests !== 5 ||
  m3.artifact.metrics.nodeSmokeTests !== 1
)
  throw new Error("m3 suite metrics do not match the accepted baseline");
if (m3.artifact.independentAudit !== "approved")
  throw new Error("m3 correctness artifact lacks independent audit approval");
const m3Predecessor = m3.exit.match(
  /Sequential predecessor:[\s\S]*?`([0-9a-f]{40})`/u,
)?.[1];
if (m3Predecessor !== m2.candidate)
  throw new Error("m3 sequential predecessor differs from the accepted m2 candidate");
const m3Benchmarks = {};
for (const name of [
  "A1-cold-write",
  "A2-rewrite-identical",
  "A3-cold-read",
  "A4-warm-read",
  "A5-one-byte-edit",
  "A6-scattered-edits",
  "A6-small-reads",
  "A7-materialization",
])
  m3Benchmarks[name] = await validateBenchmarkArtifact(
    `docs/evidence/m3/benchmarks/${name}.json`,
    m3.candidate,
    m3.recordCommit,
  );
for (const artifact of Object.values(m3Benchmarks))
  if (
    artifact.trials !== 5 ||
    artifact.configuration?.databaseIsolation !== "fresh-database-per-trial" ||
    artifact.configuration?.operatingSystemCacheDropAttempted !== false ||
    artifact.configuration?.operatingSystemCacheDropSucceeded !== false ||
    artifact.configuration?.manifestFormat !== "efs-merkle-manifest-v1" ||
    !Number.isSafeInteger(artifact.configuration?.cacheTargetBytes) ||
    !Number.isSafeInteger(artifact.configuration?.maxManagedResidentBytes) ||
    !Number.isSafeInteger(artifact.configuration?.sqlitePageSize) ||
    !["p50", "p95", "p99", "min", "max", "mean"].every((metric) =>
      Number.isFinite(artifact.latencyMs?.[metric]),
    ) ||
    artifact.samples.some(
      (sample) =>
        !/^[0-9a-f]{64}$/u.test(sample.measuredCounters?.verifiedDigest ?? "") ||
        sample.pass !== true,
    )
  )
    throw new Error(
      `${artifact.benchmark} lacks five fresh isolated, digest-verified trials`,
    );
if (
  m3Benchmarks["A6-scattered-edits"].samples.some(
    (sample) => sample.physicalAfter - sample.physicalBefore < 100 * 1024 * 1024,
  )
)
  throw new Error("m3 A6 edit trials do not start from equivalent fresh fixtures");
if (
  m3Benchmarks["A3-cold-read"].counters.mibPerSec < 250 ||
  m3Benchmarks["A3-cold-read"].counters.transactions > 55 ||
  m3Benchmarks["A3-cold-read"].counters.statements > 250 ||
  m3Benchmarks["A4-warm-read"].counters.mibPerSec < 250 ||
  m3Benchmarks["A4-warm-read"].counters.warmToColdRatio < 1.2 ||
  m3Benchmarks["A5-one-byte-edit"].counters.editCount < 100 ||
  m3Benchmarks["A5-one-byte-edit"].counters.canonicalThreeEditMs >= 1000 ||
  m3Benchmarks["A5-one-byte-edit"].samples.some(
    (sample) =>
      sample.measuredCounters.editCount < 100 ||
      !Array.isArray(sample.measuredCounters.perEditMs) ||
      sample.measuredCounters.perEditMs.length < 100,
  ) ||
  m3Benchmarks["A6-scattered-edits"].counters.completedEdits < 500 ||
  m3Benchmarks["A6-scattered-edits"].counters.wallMs > 20_000 ||
  m3Benchmarks["A6-small-reads"].counters.smallReadMsPerOp > 1
)
  throw new Error("m3 retained benchmark artifacts miss an acceptance threshold");

const m4 = await validateMilestone(
  "m4",
  [
    "operatingSystems",
    "nodeVersions",
    "matrixRuns",
    "cumulativePredecessorChecks",
    "branchTests",
    "independentWriterCount",
    "sameInodeWriterCount",
    "branchBenchmarkCells",
  ],
  {
    requireCurrentDigest: activeAcceptedMilestone === "m4",
    requireStructuredContext: true,
  },
);
if (
  m4.artifact.passed !==
  m4.artifact.metrics.cumulativePredecessorChecks + m4.artifact.metrics.branchTests
)
  throw new Error("m4 passed count differs from predecessor plus branch checks");
if (m4.artifact.independentAudit !== "approved")
  throw new Error("m4 correctness artifact lacks independent audit approval");
if (
  m4.artifact.metrics.cumulativePredecessorChecks !== m3.artifact.passed ||
  m4.artifact.metrics.branchTests !== 58 ||
  m4.artifact.metrics.independentWriterCount !== 50 ||
  m4.artifact.metrics.sameInodeWriterCount !== 50 ||
  m4.artifact.metrics.branchBenchmarkCells !== 20
)
  throw new Error("m4 evidence metrics miss an accepted threshold");
const m4Predecessor = m4.exit.match(
  /Sequential predecessor:[\s\S]*?`([0-9a-f]{40})`/u,
)?.[1];
if (m4Predecessor !== m3.candidate)
  throw new Error("m4 sequential predecessor differs from the accepted m3 candidate");
const m4IndexRelative = "docs/evidence/m4/benchmarks/index.json";
const m4Index = requireObject(
  JSON.parse(await readFile(path.join(root, m4IndexRelative), "utf8")),
  "m4 benchmark index",
);
const m4Environment = requireScalarRecord(
  m4Index.environment,
  "m4 benchmark environment",
);
if (
  m4Index.schema !== "efs-branch-bench-v1" ||
  m4Index.commit !== m4.candidate ||
  m4Index.worktreeDirty !== false ||
  !/^[0-9a-f]{64}$/u.test(m4Index.fixture?.sha256 ?? "") ||
  !Array.isArray(m4Index.artifacts) ||
  m4Index.artifacts.length !== 20 ||
  !["platform", "architecture", "node", "pnpm", "cpu", "sqlite"].every(
    (key) => typeof m4Environment[key] === "string" && m4Environment[key].length > 0,
  ) ||
  typeof m4Environment.storage !== "string" ||
  m4Environment.storage.length === 0 ||
  !Number.isSafeInteger(m4Environment.totalMemoryBytes) ||
  m4Environment.totalMemoryBytes <= 0 ||
  m4Index.artifacts.some(
    (artifact) =>
      artifact.pass !== true ||
      artifact.configuration?.databaseIsolation !== "fresh-database-per-trial" ||
      artifact.configuration?.operatingSystemCacheDropAttempted !== false ||
      !Number.isSafeInteger(artifact.configuration?.cacheTargetBytes) ||
      !Number.isSafeInteger(artifact.configuration?.mmapLimitBytes) ||
      !["filesystem", "storage", "runtime", "branch"].every((domain) => {
        const limits = artifact.configuration?.resourceLimits?.[domain];
        return (
          limits &&
          typeof limits === "object" &&
          !Array.isArray(limits) &&
          Object.keys(limits).length > 0 &&
          Object.values(limits).every(
            (value) => Number.isSafeInteger(value) && value > 0,
          )
        );
      }) ||
      !Array.isArray(artifact.samples) ||
      artifact.samples.length !== artifact.trials,
  )
)
  throw new Error("m4 retained branch benchmark index is incomplete or stale");
if ((await evidenceCommit(m4IndexRelative)) !== m4.recordCommit)
  throw new Error("m4 branch benchmark index was not committed with its evidence");

const m5 = await validateMilestone(
  "m5",
  [
    "operatingSystems",
    "nodeVersions",
    "matrixRuns",
    "cumulativePredecessorChecks",
    "maintenanceTests",
    "faultTests",
    "observedFaultPositions",
    "committedBatchFaultPositions",
    "namespaceRows",
    "reachableObjects",
    "manifestRootRows",
    "manifestNodeRows",
    "baselineScaleRows",
    "baselineScaleManagedResidentBytes",
    "fullScaleManagedResidentBytes",
    "peakStorageMarks",
    "peakGcMarks",
    "maxWalBytes",
    "maxMaintenanceBatchMs",
    "peakManagedResidentBytes",
    "heapHighWaterBytes",
    "rssHighWaterBytes",
  ],
  {
    requireCurrentDigest: activeAcceptedMilestone === "m5",
    requireStructuredContext: true,
  },
);
if (
  m5.artifact.passed !==
  m5.artifact.metrics.cumulativePredecessorChecks +
    m5.artifact.metrics.maintenanceTests +
    m5.artifact.metrics.faultTests
)
  throw new Error(
    "m5 passed count differs from predecessor plus maintenance/fault checks",
  );
if (m5.artifact.independentAudit !== "approved")
  throw new Error("m5 correctness artifact lacks independent audit approval");
if (
  m5.artifact.metrics.cumulativePredecessorChecks !== m4.artifact.passed ||
  m5.artifact.metrics.maintenanceTests < 31 ||
  m5.artifact.metrics.faultTests !== 3 ||
  m5.artifact.metrics.observedFaultPositions !== 328 ||
  m5.artifact.metrics.committedBatchFaultPositions !== 150 ||
  m5.artifact.metrics.namespaceRows < 100_000 ||
  m5.artifact.metrics.reachableObjects < 100_000 ||
  m5.artifact.metrics.manifestRootRows < 100_000 ||
  m5.artifact.metrics.manifestNodeRows < 100_000 ||
  m5.artifact.metrics.baselineScaleRows >= 100_000 ||
  m5.artifact.metrics.baselineScaleManagedResidentBytes >= 16 * 1024 * 1024 ||
  m5.artifact.metrics.fullScaleManagedResidentBytes >= 16 * 1024 * 1024 ||
  m5.artifact.metrics.fullScaleManagedResidentBytes >
    m5.artifact.metrics.baselineScaleManagedResidentBytes + 512 * 1024 ||
  m5.artifact.metrics.peakStorageMarks < 300_000 ||
  m5.artifact.metrics.peakGcMarks < 300_000 ||
  m5.artifact.metrics.peakManagedResidentBytes >= 16 * 1024 * 1024 ||
  m5.artifact.metrics.maxMaintenanceBatchMs >= 5_000 ||
  m5.artifact.metrics.maxWalBytes > m5.artifact.limits.maxJournalBytes ||
  m5.artifact.metrics.heapHighWaterBytes >= 512 * 1024 * 1024 ||
  m5.artifact.metrics.rssHighWaterBytes >= 768 * 1024 * 1024
)
  throw new Error("m5 evidence metrics miss a crash, scale, or resource threshold");
const m5Predecessor = m5.exit.match(
  /Sequential predecessor:[\s\S]*?`([0-9a-f]{40})`/u,
)?.[1];
if (m5Predecessor !== m4.candidate)
  throw new Error("m5 sequential predecessor differs from the accepted m4 candidate");

const m6 = await validateMilestone(
  "m6",
  [
    "operatingSystems",
    "nodeVersions",
    "matrixRuns",
    "cumulativePredecessorChecks",
    "workerdChecks",
    "nodePortableTests",
    "durableObjectPortableTests",
    "durableObjectScaleTests",
    "releasedSchemaVersions",
    "migrationStatementPositions",
    "filesystemFaultFamilies",
    "nodeFilesystemFaultPositions",
    "durableObjectFilesystemFaultPositions",
    "publicationFaultFamilies",
    "publicationFaultPositions",
    "maintenanceFaultFamilies",
    "nodeMaintenanceFaultPositions",
    "durableObjectMaintenanceFaultPositions",
    "portableStorageCases",
    "stagingCrashRestarts",
    "smokeElapsedMs",
    "smokeCompletedOperations",
    "smokeRuntimeRestarts",
    "scaleRows",
    "scaleBaselineRows",
    "scaleObjectRows",
    "scaleNamespaceRows",
    "scaleManifestRootRows",
    "scaleManifestNodeRows",
    "scalePeakStorageMarks",
    "scalePeakGcMarks",
    "scaleVerifiedRows",
    "scaleBaselineManagedBytes",
    "scaleManagedPeakBytes",
    "scalePhysicalRestarts",
    "scaleMainFileBytes",
    "scaleMaxMaintenanceCallMs",
    "workerdControlRssGrowthBytes",
    "workerdRssGrowthBytes",
    "workerdPeakRssBytes",
    "workerdProcessRssLimitBytes",
    "previewBundleBytes",
    "nodeInitializationIdentityWrites",
    "nodeInitializationIdentityBoundaries",
    "durableObjectInitializationIdentityWrites",
    "durableObjectInitializationIdentityBoundaries",
    "nodeTargetElapsedMs",
    "durableObjectTargetElapsedMs",
    "nodeTargetDeadlineMs",
    "durableObjectTargetDeadlineMs",
  ],
  {
    requireCurrentDigest: false,
    requireStructuredContext: true,
  },
);
const m6Context = validateM6ResultContexts(m6.artifact);
validateStructuredDeviations(m6.artifact, "m6");
const m6Preview = requireObject(m6.artifact.preview, "m6.preview");
const m6Resource = requireObject(m6.artifact.resourceEvidence, "m6.resourceEvidence");
const m6LogicalChecks =
  m6.artifact.metrics.workerdChecks +
  m6.artifact.metrics.nodePortableTests +
  m6.artifact.metrics.durableObjectPortableTests +
  m6.artifact.metrics.durableObjectScaleTests +
  m6.artifact.metrics.releasedSchemaVersions +
  m6.artifact.metrics.filesystemFaultFamilies +
  m6.artifact.metrics.publicationFaultFamilies +
  m6.artifact.metrics.maintenanceFaultFamilies;
if (
  m6.artifact.passed !==
  m6.artifact.metrics.cumulativePredecessorChecks + m6LogicalChecks
)
  throw new Error("m6 passed count differs from predecessor plus M6 target checks");
if (m6.artifact.independentAudit !== "approved")
  throw new Error("m6 correctness artifact lacks independent audit approval");
if (
  m6.artifact.metrics.cumulativePredecessorChecks !== m5.artifact.passed ||
  m6.artifact.metrics.workerdChecks !== 12 ||
  m6.artifact.metrics.nodePortableTests !== 16 ||
  m6.artifact.metrics.durableObjectPortableTests !== 23 ||
  m6.artifact.metrics.durableObjectScaleTests !== 1 ||
  m6.artifact.metrics.releasedSchemaVersions !== 3 ||
  m6.artifact.metrics.migrationStatementPositions !== 996 ||
  m6.artifact.metrics.filesystemFaultFamilies !== 12 ||
  m6.artifact.metrics.nodeFilesystemFaultPositions !== 1218 ||
  m6.artifact.metrics.durableObjectFilesystemFaultPositions !== 1218 ||
  m6.artifact.metrics.publicationFaultFamilies !== 2 ||
  m6.artifact.metrics.publicationFaultPositions !== 186 ||
  m6.artifact.metrics.maintenanceFaultFamilies !== 3 ||
  m6.artifact.metrics.nodeMaintenanceFaultPositions !== 633 ||
  m6.artifact.metrics.durableObjectMaintenanceFaultPositions !== 633 ||
  m6.artifact.metrics.portableStorageCases < 7 ||
  m6.artifact.metrics.stagingCrashRestarts !== 3 ||
  m6.artifact.metrics.smokeElapsedMs >= 60_000 ||
  m6.artifact.metrics.smokeCompletedOperations !== 9_056 ||
  m6.artifact.metrics.smokeRuntimeRestarts !== 3 ||
  m6.artifact.metrics.scaleRows < 100_000 ||
  m6.artifact.metrics.scaleBaselineRows >= m6.artifact.metrics.scaleRows ||
  m6.artifact.metrics.scaleObjectRows < 100_000 ||
  m6.artifact.metrics.scaleNamespaceRows < 100_000 ||
  m6.artifact.metrics.scaleManifestRootRows < 100_000 ||
  m6.artifact.metrics.scaleManifestNodeRows < 100_000 ||
  m6.artifact.metrics.scalePeakStorageMarks < 300_000 ||
  m6.artifact.metrics.scalePeakGcMarks < 300_000 ||
  m6.artifact.metrics.scaleVerifiedRows < 1_000_000 ||
  m6.artifact.metrics.scaleBaselineManagedBytes >= 16 * 1024 * 1024 ||
  m6.artifact.metrics.scaleManagedPeakBytes >= 16 * 1024 * 1024 ||
  m6.artifact.metrics.scaleManagedPeakBytes >
    m6.artifact.metrics.scaleBaselineManagedBytes + 512 * 1024 ||
  m6.artifact.metrics.scalePhysicalRestarts < 5 ||
  m6.artifact.metrics.scaleMainFileBytes >
    m6Context.profiles.durableObjectScale.capabilities.maxPhysicalDatabaseBytes ||
  m6.artifact.metrics.scaleMaxMaintenanceCallMs >= 5_000 ||
  m6.artifact.metrics.workerdControlRssGrowthBytes < 32 * 1024 * 1024 ||
  m6.artifact.metrics.workerdPeakRssBytes >=
    m6.artifact.metrics.workerdProcessRssLimitBytes ||
  m6.artifact.metrics.nodeTargetElapsedMs >= m6.artifact.metrics.nodeTargetDeadlineMs ||
  m6.artifact.metrics.durableObjectTargetElapsedMs >=
    m6.artifact.metrics.durableObjectTargetDeadlineMs ||
  m6.artifact.metrics.nodeTargetDeadlineMs !== 600_000 ||
  m6.artifact.metrics.durableObjectTargetDeadlineMs !== 600_000 ||
  m6.artifact.capabilities.schemaIdentityMode !== "durable-table" ||
  m6.artifact.capabilities.pageMetricsMode !== "runtime-size-only" ||
  m6.artifact.capabilities.maxPhysicalDatabaseBytes !== 1_000_000_000 ||
  m6.artifact.capabilities.maxJournalBytes !== 1_000_000_000 ||
  m6.artifact.capabilities.hostedDeployment !== false ||
  m6.artifact.capabilities.exactProcessBoundAvailable !== false ||
  m6Resource.filesystemCachesInstantiated !== false ||
  m6Resource.rawRuntimeEffectReproduced !== true ||
  m6Resource.exactIsolateAttributionAvailable !== false ||
  m6Preview.dryRun !== true ||
  m6Preview.deployed !== false ||
  m6Preview.compatibilityDate !== "2026-08-10" ||
  m6Preview.bindingName !== "FILESYSTEM" ||
  m6Preview.className !== "FilesystemObject" ||
  m6Preview.migrationTag !== "v1" ||
  m6Preview.newSqliteClass !== "FilesystemObject" ||
  m6Preview.bundleBytes !== m6.artifact.metrics.previewBundleBytes ||
  m6Preview.bundleSha256 !== m6.artifact.metrics.previewBundleSha256 ||
  m6.artifact.metrics.nodeInitializationIdentityWrites !== 12 ||
  m6.artifact.metrics.nodeInitializationIdentityBoundaries !== 24 ||
  m6.artifact.metrics.durableObjectInitializationIdentityWrites !== 13 ||
  m6.artifact.metrics.durableObjectInitializationIdentityBoundaries !== 26 ||
  !/^[0-9a-f]{64}$/u.test(m6.artifact.metrics.smokeFixtureDigest ?? "") ||
  m6Context.contexts.get("durable-object-smoke").fixtureDigest !==
    m6.artifact.metrics.smokeFixtureDigest ||
  !/^[0-9a-f]{64}$/u.test(m6.artifact.metrics.previewBundleSha256 ?? "")
)
  throw new Error("m6 evidence metrics miss a parity, restart, or resource threshold");
const m6Predecessor = m6.exit.match(
  /Sequential predecessor:[\s\S]*?`([0-9a-f]{40})`/u,
)?.[1];
if (m6Predecessor !== m5.candidate)
  throw new Error("m6 sequential predecessor differs from the accepted m5 candidate");

async function validateM6CurrentOrM7Descendant() {
  if (activeAcceptedMilestone !== "m6") return;
  const currentDigest = await ownedTreeDigest("m6", "HEAD");
  if (currentDigest === m6.artifact.ownedTreeDigest) return;
  const head = (
    await execute("git", ["rev-parse", "HEAD"], { cwd: root, windowsHide: true })
  ).stdout.trim();
  const headParent = (
    await execute("git", ["show", "-s", "--format=%P", head], {
      cwd: root,
      windowsHide: true,
    })
  ).stdout.trim();
  let candidate = head;
  if (headParent !== m6.recordCommit) {
    if (!/^[0-9a-f]{40}$/u.test(headParent))
      throw new Error("unaccepted M7 evidence must have exactly one candidate parent");
    candidate = headParent;
    const candidateParent = (
      await execute("git", ["show", "-s", "--format=%P", candidate], {
        cwd: root,
        windowsHide: true,
      })
    ).stdout.trim();
    if (candidateParent !== m6.recordCommit)
      throw new Error(
        "unaccepted M7 candidate is not directly parented by M6 evidence",
      );
    const evidenceChanges = (
      await execute("git", ["diff", "--name-only", `${candidate}..${head}`], {
        cwd: root,
        windowsHide: true,
      })
    ).stdout
      .trim()
      .split(/\r?\n/u)
      .filter(Boolean);
    if (
      !evidenceChanges.length ||
      evidenceChanges.some((filename) => !filename.startsWith("docs/evidence/m7/"))
    )
      throw new Error("unaccepted M7 evidence commit changes non-evidence files");
  }
  const candidateChanges = (
    await execute("git", ["diff", "--name-only", `${m6.recordCommit}..${candidate}`], {
      cwd: root,
      windowsHide: true,
      maxBuffer: 16 * 1024 * 1024,
    })
  ).stdout
    .trim()
    .split(/\r?\n/u)
    .filter(Boolean);
  if (
    !candidateChanges.length ||
    candidateChanges.some((name) => !ownedByM7Candidate(name))
  )
    throw new Error("current tree has drift outside the exact unaccepted M7 candidate");
}

await validateM6CurrentOrM7Descendant();

async function validateOptionalM7Evidence() {
  const directory = path.join(root, "docs", "evidence", "m7");
  const jsonFilename = path.join(directory, "correctness.json");
  let artifact;
  try {
    artifact = requireObject(
      JSON.parse(await readFile(jsonFilename, "utf8")),
      "m7 correctness artifact",
    );
  } catch (error) {
    if (error?.code === "ENOENT" && activeAcceptedMilestone !== "m7") return;
    throw error;
  }
  if (artifact.schema !== "efs-m7-evidence-v1")
    throw new Error("m7 correctness artifact has an invalid schema");
  if (!new Set(["blocked", "passed"]).has(artifact.status))
    throw new Error("m7 correctness artifact has an invalid status");
  if (activeAcceptedMilestone === "m7" && artifact.status !== "passed")
    throw new Error("accepted M7 evidence cannot be blocked");
  if (artifact.passed !== 23 || artifact.failed !== 0)
    throw new Error("m7 evidence must record all 23 local tests with zero failures");
  for (const [name, value] of [
    ["candidate", artifact.candidate],
    ["predecessorCandidate", artifact.predecessorCandidate],
    ["candidateParent", artifact.candidateParent],
  ])
    if (!/^[0-9a-f]{40}$/u.test(value ?? ""))
      throw new Error(`m7.${name} must be an exact commit`);
  if (artifact.predecessorCandidate !== m6.candidate)
    throw new Error("m7 predecessor differs from the accepted M6 candidate");
  if (artifact.candidateParent !== m6.recordCommit)
    throw new Error("m7 candidate parent differs from the M6 evidence commit");
  const candidateParents = (
    await execute("git", ["show", "-s", "--format=%P", artifact.candidate], {
      cwd: root,
      windowsHide: true,
    })
  ).stdout.trim();
  if (candidateParents !== artifact.candidateParent)
    throw new Error("m7 candidate is not a single-parent child of M6 evidence");
  const changed = (
    await execute(
      "git",
      ["diff", "--name-only", `${artifact.candidateParent}..${artifact.candidate}`],
      { cwd: root, windowsHide: true, maxBuffer: 16 * 1024 * 1024 },
    )
  ).stdout
    .trim()
    .split(/\r?\n/u)
    .filter(Boolean);
  for (const filename of changed)
    if (!ownedByM7Candidate(filename))
      throw new Error(`m7 candidate changes non-M7-owned path ${filename}`);
  const ownedDigest = await ownedTreeDigest("m7", artifact.candidate);
  if (artifact.candidateOwnedTreeDigest !== ownedDigest)
    throw new Error("m7 candidate owned-tree digest differs");
  if (
    JSON.stringify(artifact.commands) !==
    JSON.stringify(["pnpm validate:m6", "pnpm test:m7:local", "pnpm test:m7:fuse"])
  )
    throw new Error("m7 evidence does not identify the exact required commands");
  const capabilities = requireObject(artifact.capabilities, "m7.capabilities");
  for (const name of [
    "supportsDirectRangeIo",
    "supportsWriteSessions",
    "sharedAdmissionController",
    "sharedContentCache",
    "durablePinnedReadLease",
    "boundedManifestCursor",
    "realMountedFuseRequired",
  ])
    if (capabilities[name] !== true)
      throw new Error(`m7.capabilities.${name} must be true`);

  const limits = requireObject(artifact.limits, "m7.limits");
  if (
    limits.maxWriteSessionBytes !== 16 * 1024 * 1024 ||
    limits.maxPendingWriteBytes !== 64 * 1024 * 1024 ||
    limits.maxManagedResidentBytes !== 128 * 1024 * 1024 ||
    limits.maxOpenNodeVfsSessions !== 256
  )
    throw new Error("m7 evidence does not retain the normative default limits");
  if (JSON.stringify(artifact.cowPageBytes) !== JSON.stringify([4096, 8192, 16384]))
    throw new Error("m7 evidence does not cover all persisted COW page formats");
  const metrics = requireObject(artifact.metrics, "m7.metrics");
  for (const name of [
    "localElapsedMs",
    "localDeadlineMs",
    "nodeVfsTests",
    "faultStagePositions",
    "faultCommitPositions",
    "largeFixtureBytes",
    "largeEditSourceBytes",
    "peakManagedResidentBytes",
  ])
    requirePositiveInteger(metrics[name], `m7.metrics.${name}`);
  if (
    metrics.localElapsedMs >= metrics.localDeadlineMs ||
    metrics.localDeadlineMs !== 600_000 ||
    metrics.nodeVfsTests !== 23 ||
    metrics.faultStagePositions < 20 ||
    metrics.faultCommitPositions < 20 ||
    metrics.largeFixtureBytes < 100 * 1024 * 1024 ||
    metrics.largeEditSourceBytes !==
      Math.ceil(metrics.totalCowEditSourceBytes / metrics.cowEditCount) ||
    metrics.largeEditSourceBytes >= metrics.largeFixtureBytes ||
    metrics.peakManagedResidentBytes > limits.maxManagedResidentBytes
  )
    throw new Error("m7 local evidence misses a correctness or resource threshold");
  const environment = requireScalarRecord(artifact.environment, "m7.environment");
  for (const name of ["platform", "architecture", "node", "pnpm", "sqlite"])
    requireNonemptyString(environment[name], `m7.environment.${name}`);
  if (!Array.isArray(artifact.logs) || artifact.logs.length !== 3)
    throw new Error("m7 evidence must record predecessor, local, and FUSE logs");
  const expectedLogs = [
    {
      name: "accepted-m6-predecessor",
      command: "pnpm validate:m6",
      path: "docs/evidence/m7/logs/predecessor-m6.log",
      metaCommand: "pnpm_validate_m6",
    },
    {
      name: "m7-local",
      command: "pnpm test:m7:local",
      path: "docs/evidence/m7/logs/m7-local.log",
      metaCommand: "pnpm_test_m7_local",
    },
    {
      name: "m7-real-fuse-selection",
      command: "pnpm test:m7:fuse",
      path: "docs/evidence/m7/logs/m7-real-fuse.log",
      metaCommand: "pnpm_test_m7_fuse",
    },
  ];
  const logSources = [];
  for (const [index, value] of artifact.logs.entries()) {
    const log = requireObject(value, `m7.logs[${index}]`);
    const expected = expectedLogs[index];
    if (
      log.name !== expected.name ||
      log.command !== expected.command ||
      log.path !== expected.path
    )
      throw new Error(`m7.logs[${index}] does not identify the required exact gate`);
    if (!/^[0-9a-f]{64}$/u.test(log.sha256 ?? ""))
      throw new Error(`m7.logs[${index}].sha256 is invalid`);
    const expectedExitCode = index === 2 && artifact.status === "blocked" ? 2 : 0;
    if (log.exitCode !== expectedExitCode)
      throw new Error(`m7.logs[${index}].exitCode differs from its gate status`);
    requirePositiveInteger(log.elapsedMs, `m7.logs[${index}].elapsedMs`);
    const bytes = await readFile(path.join(root, log.path));
    if (createHash("sha256").update(bytes).digest("hex") !== log.sha256)
      throw new Error(`m7 log integrity differs for ${log.path}`);
    const source = bytes.toString("utf8");
    const meta = m7LogMeta(source, `m7.logs[${index}]`);
    if (
      meta.exitCode !== expectedExitCode ||
      meta.elapsedMs !== log.elapsedMs ||
      meta.candidate !== artifact.candidate ||
      meta.command !== expected.metaCommand
    )
      throw new Error(`m7.logs[${index}] metadata differs from its evidence record`);
    logSources.push(source);
  }
  if (
    !logSources[0].includes("accepted-node-gate: PASS") ||
    !logSources[0].includes("m6-local-gate: PASS") ||
    !logSources[0].includes("evidence: preserved predecessor candidates")
  )
    throw new Error("m7 predecessor log lacks a complete passing M6 selection");
  const predecessorNodeElapsed = Number(
    /accepted-node-gate: PASS \((\d+) ms\)/u.exec(logSources[0])?.[1],
  );
  const predecessorM6Elapsed = Number(
    /m6-local-gate: PASS \((\d+) ms\)/u.exec(logSources[0])?.[1],
  );
  const localGateElapsed = Number(
    /^m7-local-gate: PASS \((\d+) ms\)$/mu.exec(logSources[1])?.[1],
  );
  const localTargetElapsed = Number(
    /m7-local-gate: PASS node-vfs-correctness-fault-resource \((\d+) ms\)/u.exec(
      logSources[1],
    )?.[1],
  );
  if (
    artifact.logs[0].elapsedMs !== metrics.predecessorElapsedMs ||
    predecessorNodeElapsed !== metrics.predecessorNodeTargetElapsedMs ||
    predecessorM6Elapsed !== metrics.predecessorM6TargetElapsedMs ||
    localTargetElapsed !== metrics.localElapsedMs ||
    localGateElapsed !== metrics.localGateElapsedMs
  )
    throw new Error("m7 predecessor or local elapsed evidence differs from its log");
  if (
    !/m7-local-gate: PASS \(\d+ ms\)/u.test(logSources[1]) ||
    !logSources[1].includes("ℹ pass 23") ||
    !logSources[1].includes("ℹ fail 0")
  )
    throw new Error("m7 local log lacks its complete zero-failure PASS markers");
  const conformance = logLineObject(
    logSources[1],
    "efs-m7-conformance-v1",
    "m7 local log",
  );
  const pressure = logLineObject(
    logSources[1],
    "efs-m7-default-pressure-v1",
    "m7 local log",
  );
  const cow = logLineObject(logSources[1], "efs-m7-cow-resource-v1", "m7 local log");
  const fault = logLineObject(logSources[1], "efs-m7-fault-matrix-v1", "m7 local log");
  if (
    !Array.isArray(conformance.cases) ||
    conformance.cases.length !== metrics.sharedConformanceCases ||
    conformance.commitCloseOrders !== metrics.threeSessionCommitCloseOrders ||
    JSON.stringify(conformance.sessionCounts) !== JSON.stringify([1, 16, 64]) ||
    pressure.sessions !== 64 ||
    pressure.residentBoundaryBytes !== metrics.defaultPressureResidentBytes ||
    pressure.aggregateLimitBytes !== limits.maxManagedResidentBytes ||
    pressure.peakManagedResidentBytes !==
      metrics.defaultPressurePeakManagedResidentBytes ||
    cow.fixtureBytes !== metrics.largeFixtureBytes ||
    cow.fixtureDigest !== artifact.fixtureDigest ||
    cow.edits !== metrics.cowEditCount ||
    cow.cowEditCount !== metrics.cowEditCount ||
    cow.sourceBytesRead !== metrics.totalCowEditSourceBytes ||
    cow.peakManagedResidentBytes !== metrics.peakManagedResidentBytes ||
    fault.faultPoint !== artifact.faultPoint ||
    fault.stagingPositions !== metrics.faultStagePositions ||
    fault.commitPositions !== metrics.faultCommitPositions
  )
    throw new Error("m7 local structured log differs from its evidence metrics");
  const fuse = requireObject(artifact.realFuse, "m7.realFuse");
  if (fuse.required !== true || fuse.selectionDeadlineMs !== 600_000)
    throw new Error("m7 evidence weakens the mandatory real-FUSE selection");
  if (artifact.status === "blocked") {
    if (
      fuse.available !== false ||
      fuse.smokePassed !== false ||
      fuse.blocker !== "non-linux-host" ||
      !logSources[2].includes("M7_FUSE_BLOCKED") ||
      logSources[2].includes("m7-real-fuse-gate: PASS") ||
      packageManifest.scripts?.["validate:accepted"] !== "pnpm validate:m6"
    )
      throw new Error("blocked M7 evidence or accepted-milestone selection is invalid");
  } else {
    const fuseLog = logSources[2];
    const fuseGateMatch = /^m7-real-fuse-gate: PASS \((\d+) ms\)$/mu.exec(fuseLog);
    if (!fuseGateMatch)
      throw new Error("passed M7 FUSE log lacks the gate PASS marker");
    const smoke = logLineObject(
      fuseLog,
      "efs-m7-real-fuse-smoke-v2",
      "m7 real FUSE log",
    );
    const requiredSmokeEnvironment = [
      "candidate",
      "platform",
      "architecture",
      "node",
      "pnpm",
      "kernel",
      "cpu",
      "storage",
      "sqlite",
      "fuseVersion",
      "device",
      "fusermount",
      "manifestFormat",
    ];
    for (const name of requiredSmokeEnvironment)
      requireNonemptyString(smoke[name], `m7.realFuse.log.${name}`);
    const active = requireObject(
      smoke.activeDurableState,
      "m7.realFuse.log.activeDurableState",
    );
    const sqliteCapabilities = requireObject(
      smoke.sqliteCapabilities,
      "m7.realFuse.log.sqliteCapabilities",
    );
    const filesystemCapabilities = requireObject(
      smoke.filesystemCapabilities,
      "m7.realFuse.log.filesystemCapabilities",
    );
    const providerCapabilities = requireObject(
      smoke.providerCapabilities,
      "m7.realFuse.log.providerCapabilities",
    );
    const providerRuntime = requireObject(
      providerCapabilities.runtime,
      "m7.realFuse.log.providerCapabilities.runtime",
    );
    const fastCdc = requireObject(smoke.fastCdc, "m7.realFuse.log.fastCdc");
    const storageSnapshot = requireObject(
      smoke.storageSnapshot,
      "m7.realFuse.log.storageSnapshot",
    );
    const physicalStorage = requireObject(
      smoke.physicalStorage,
      "m7.realFuse.log.physicalStorage",
    );
    const usage = requireScalarRecord(smoke.usage, "m7.realFuse.log.usage");
    const providerMetrics = requireObject(
      smoke.providerMetrics,
      "m7.realFuse.log.providerMetrics",
    );
    const editBatchProof = requireObject(
      smoke.editBatchProof,
      "m7.realFuse.log.editBatchProof",
    );
    const processPids = smoke.processPids;
    const mounts = smoke.mountIdentity;
    const mountCycleIds = smoke.mountCycleIds;
    const expectedFinalPayloadDigest =
      "3238fa53923434d162289488f802739eecc4a45303799b7ca4c4b38fddba5d1a";
    if (
      fuse.available !== true ||
      fuse.smokePassed !== true ||
      fuse.device !== "/dev/fuse" ||
      fuse.smokeDeadlineMs !== 60_000 ||
      fuse.platform !== "linux" ||
      smoke.candidate !== artifact.candidate ||
      smoke.platform !== "linux" ||
      smoke.device !== "/dev/fuse" ||
      smoke.deviceIsCharacter !== true ||
      smoke.fixtureBytes !== 16 * 1024 * 1024 ||
      smoke.seed !== 0x5eed5eed ||
      smoke.oneByteEditCount !== 5_000 ||
      smoke.mountedPayloadOneByteWriteCallbacks !== 5_000 ||
      editBatchProof.callbackCount !== 5_000 ||
      editBatchProof.flushCountDelta !== 1 ||
      editBatchProof.failedFlushCountDelta !== 0 ||
      editBatchProof.cowEditCountDelta !== 1 ||
      !Number.isSafeInteger(editBatchProof.cowEditSourceBytesDelta) ||
      editBatchProof.cowEditSourceBytesDelta <= 0 ||
      editBatchProof.cowEditSourceBytesDelta > smoke.fixtureBytes + 524_288 ||
      !Number.isSafeInteger(editBatchProof.coreBatchCountDelta) ||
      editBatchProof.coreBatchCountDelta <= 0 ||
      !Number.isSafeInteger(smoke.providerCowEditCount) ||
      smoke.providerCowEditCount < 0 ||
      !Number.isSafeInteger(smoke.transactionCount) ||
      smoke.transactionCount <= 0 ||
      !Number.isSafeInteger(providerMetrics.coreBatchCount) ||
      providerMetrics.coreBatchCount <= 0 ||
      !Number.isSafeInteger(providerMetrics.flushCount) ||
      providerMetrics.flushCount <= 0 ||
      providerMetrics.failedFlushCount !== 0 ||
      smoke.namespaceOperationCount !== 2_000 ||
      smoke.readerActors !== 16 ||
      smoke.writerActors !== 16 ||
      smoke.operationsPerActor !== 64 ||
      smoke.completedOperationCount !== 9_056 ||
      smoke.processRestarts !== 3 ||
      smoke.restartUnmounts !== 3 ||
      smoke.finalUnmounted !== true ||
      smoke.fsyncCrashVerified !== true ||
      smoke.fsyncCloseNoopVerified !== true ||
      smoke.closeDurabilityVerified !== true ||
      smoke.collectionInterrupted !== true ||
      smoke.collectionResumed !== true ||
      smoke.finalCollectionComplete !== true ||
      !Number.isSafeInteger(smoke.finalCollectionCommittedBatches) ||
      smoke.finalCollectionCommittedBatches <= 0 ||
      smoke.verificationComplete !== true ||
      smoke.usageVerified !== true ||
      active.leases !== 0 ||
      active.staging !== 0 ||
      active.reservations !== 0 ||
      !Array.isArray(processPids) ||
      processPids.length !== 4 ||
      processPids.some((value) => !Number.isSafeInteger(value) || value <= 0) ||
      new Set(processPids).size !== processPids.length ||
      JSON.stringify(mountCycleIds) !== JSON.stringify([1, 2, 3, 4]) ||
      !Array.isArray(mounts) ||
      mounts.length !== 4 ||
      mounts.some((value) => !/ - fuse(?:\.[^ ]+)? \/dev\/fuse /u.test(value)) ||
      smoke.smokeDeadlineMs !== 60_000 ||
      !Number.isSafeInteger(smoke.elapsedMs) ||
      smoke.elapsedMs <= 0 ||
      smoke.elapsedMs >= 60_000 ||
      smoke.fixtureDigest !== fuse.fixtureDigest ||
      smoke.finalPayloadDigest !== expectedFinalPayloadDigest ||
      smoke.expectedFinalPayloadDigest !== expectedFinalPayloadDigest ||
      !/^[0-9a-f]{64}$/u.test(smoke.namespaceDigest ?? "") ||
      !Array.isArray(smoke.slowestOperations) ||
      smoke.slowestOperations.length === 0 ||
      !Number.isSafeInteger(smoke.peakManagedResidentBytes) ||
      smoke.peakManagedResidentBytes <= 0 ||
      smoke.peakManagedResidentBytes > limits.maxManagedResidentBytes ||
      smoke.aggregateLimitBytes !== limits.maxManagedResidentBytes ||
      !Number.isSafeInteger(smoke.peakRssBytes) ||
      smoke.peakRssBytes <= 0 ||
      !Number.isSafeInteger(smoke.totalMemoryBytes) ||
      smoke.totalMemoryBytes <= 0 ||
      smoke.operatingSystemCacheDropAttempted !== false ||
      smoke.operatingSystemCacheDropSucceeded !== false ||
      sqliteCapabilities.journalMode !== "wal" ||
      sqliteCapabilities.cacheTargetBytes !== 16 * 1024 * 1024 ||
      sqliteCapabilities.mmapLimitBytes !== 0 ||
      sqliteCapabilities.maxPhysicalDatabaseBytes <= 0 ||
      sqliteCapabilities.maxJournalBytes <= 0 ||
      filesystemCapabilities.format?.cowPageBytes !== 8192 ||
      filesystemCapabilities.format?.manifestFormat !== "efs-merkle-manifest-v1" ||
      providerRuntime.maxWriteSessionBytes !== limits.maxWriteSessionBytes ||
      providerRuntime.maxPendingWriteBytes !== limits.maxPendingWriteBytes ||
      providerRuntime.maxManagedResidentBytes !== limits.maxManagedResidentBytes ||
      providerRuntime.maxOpenNodeVfsSessions !== limits.maxOpenNodeVfsSessions ||
      fastCdc.minimumBytes !== 32_768 ||
      fastCdc.averageBytes !== 131_072 ||
      fastCdc.maximumBytes !== 524_288 ||
      storageSnapshot.state !== "complete" ||
      !Number.isSafeInteger(storageSnapshot.reclaimablePayloadBytes) ||
      storageSnapshot.reclaimablePayloadBytes < 0 ||
      Object.keys(usage).length === 0 ||
      !Number.isSafeInteger(physicalStorage.mainFileBytes) ||
      physicalStorage.mainFileBytes <= 0 ||
      !Number.isSafeInteger(fuse.elapsedMs) ||
      fuse.elapsedMs <= 0 ||
      fuse.elapsedMs >= 60_000 ||
      fuse.elapsedMs !== smoke.elapsedMs ||
      fuse.fixtureBytes !== smoke.fixtureBytes ||
      fuse.fixtureDigest !== smoke.fixtureDigest ||
      fuse.finalPayloadDigest !== smoke.finalPayloadDigest ||
      fuse.namespaceDigest !== smoke.namespaceDigest ||
      JSON.stringify(fuse.processPids) !== JSON.stringify(processPids) ||
      JSON.stringify(fuse.mountIdentity) !== JSON.stringify(mounts) ||
      JSON.stringify(fuse.mountCycleIds) !== JSON.stringify(mountCycleIds) ||
      fuse.processRestarts !== 3 ||
      fuse.completedOperationCount !== 9_056 ||
      fuse.namespaceOperationCount !== 2_000 ||
      fuse.oneByteEditCount !== 5_000 ||
      fuse.mountedPayloadOneByteWriteCallbacks !== 5_000 ||
      JSON.stringify(fuse.editBatchProof) !== JSON.stringify(editBatchProof) ||
      fuse.providerCowEditCount !== smoke.providerCowEditCount ||
      fuse.transactionCount !== smoke.transactionCount ||
      fuse.readerActors !== 16 ||
      fuse.writerActors !== 16 ||
      fuse.operationsPerActor !== 64 ||
      fuse.fsyncCrashVerified !== true ||
      fuse.fsyncCloseNoopVerified !== true ||
      fuse.closeDurabilityVerified !== true ||
      fuse.collectionInterrupted !== true ||
      fuse.collectionResumed !== true ||
      fuse.finalCollectionComplete !== true ||
      fuse.finalCollectionCommittedBatches !== smoke.finalCollectionCommittedBatches ||
      fuse.usageVerified !== true ||
      fuse.platform !== smoke.platform ||
      fuse.architecture !== smoke.architecture ||
      fuse.kernel !== smoke.kernel ||
      fuse.node !== smoke.node ||
      fuse.fuseVersion !== smoke.fuseVersion ||
      fuse.device !== smoke.device ||
      fuse.fusermount !== smoke.fusermount ||
      fuse.storage !== smoke.storage ||
      fuse.uid !== smoke.uid ||
      fuse.schemaVersion !== smoke.schemaVersion ||
      fuse.sqlite !== smoke.sqlite ||
      fuse.gateElapsedMs !== Number(fuseGateMatch[1]) ||
      fuse.selectionElapsedMs !== artifact.logs[2].elapsedMs ||
      fuse.gateElapsedMs >= 60_000 ||
      !Number.isSafeInteger(fuse.gateElapsedMs) ||
      fuse.gateElapsedMs <= 0 ||
      !Number.isSafeInteger(fuse.selectionElapsedMs) ||
      fuse.selectionElapsedMs <= 0 ||
      fuse.selectionElapsedMs >= fuse.selectionDeadlineMs ||
      fuse.peakManagedResidentBytes > limits.maxManagedResidentBytes
    )
      throw new Error(
        "passed M7 evidence lacks a real mounted-FUSE identity or threshold",
      );
  }
  const exitFilename = path.join(directory, "exit.md");
  const exit = await readFile(exitFilename, "utf8");
  if (
    candidateFromExit(exit, "m7") !== artifact.candidate ||
    !exit.includes(
      `- Sequential predecessor: accepted M6 candidate \`${m6.candidate}\``,
    ) ||
    !exit.includes(`- M7 status: ${artifact.status}`)
  )
    throw new Error("m7 exit record differs from its structured artifact");
  const recordCommit = await evidenceCommit(path.relative(root, jsonFilename));
  if ((await evidenceCommit(path.relative(root, exitFilename))) !== recordCommit)
    throw new Error("m7 exit and correctness files must be atomic");
  const evidenceParents = (
    await execute("git", ["show", "-s", "--format=%P", recordCommit], {
      cwd: root,
      windowsHide: true,
    })
  ).stdout.trim();
  if (evidenceParents !== artifact.candidate)
    throw new Error("m7 evidence commit is not the direct child of its candidate");
  const evidenceChanges = (
    await execute(
      "git",
      ["diff", "--name-only", `${artifact.candidate}..${recordCommit}`],
      {
        cwd: root,
        windowsHide: true,
      },
    )
  ).stdout
    .trim()
    .split(/\r?\n/u)
    .filter(Boolean)
    .sort();
  const exactEvidenceFiles = [
    "docs/evidence/m7/correctness.json",
    "docs/evidence/m7/exit.md",
    "docs/evidence/m7/logs/m7-local.log",
    "docs/evidence/m7/logs/m7-real-fuse.log",
    "docs/evidence/m7/logs/predecessor-m6.log",
  ];
  if (JSON.stringify(evidenceChanges) !== JSON.stringify(exactEvidenceFiles))
    throw new Error(
      "m7 evidence commit does not contain the exact atomic evidence set",
    );
  for (const log of artifact.logs)
    if ((await evidenceCommit(log.path)) !== recordCommit)
      throw new Error(`m7 log ${log.path} was not committed atomically with evidence`);
  let m8EvidenceInProgress = false;
  try {
    await readFile(
      path.join(root, "docs", "evidence", "m8", "correctness.json"),
      "utf8",
    );
    m8EvidenceInProgress = true;
  } catch {}
  if (activeAcceptedMilestone === "m7" && !m8EvidenceInProgress) {
    const head = (
      await execute("git", ["rev-parse", "HEAD"], { cwd: root, windowsHide: true })
    ).stdout.trim();
    const acceptanceParents = (
      await execute("git", ["show", "-s", "--format=%P", head], {
        cwd: root,
        windowsHide: true,
      })
    ).stdout.trim();
    if (acceptanceParents !== recordCommit)
      throw new Error(
        "accepted M7 HEAD is not the single direct child of its evidence",
      );
    const acceptanceChanges = (
      await execute("git", ["diff", "--name-only", `${recordCommit}..${head}`], {
        cwd: root,
        windowsHide: true,
      })
    ).stdout
      .trim()
      .split(/\r?\n/u)
      .filter(Boolean);
    const acceptanceAllowlist = new Set([
      ".github/workflows/ci.yml",
      "README.md",
      "docs/implementation/implementation-plan.md",
      "docs/implementation/m7-handoff.md",
      "package.json",
      "tests/architecture/foundation.test.mjs",
    ]);
    if (
      acceptanceChanges.length === 0 ||
      acceptanceChanges.some((filename) => !acceptanceAllowlist.has(filename))
    )
      throw new Error(
        "accepted M7 HEAD changes files outside its acceptance allowlist",
      );
    const candidateRuntimePaths = [
      "packages/fs/src",
      "packages/node-vfs/src",
      "packages/testkit/src",
      "scripts/check-evidence.mjs",
      "scripts/run-m7-fuse-gate.mjs",
      "scripts/run-m7-local-gate.mjs",
      "tests/node-vfs",
    ];
    const runtimeDrift = (
      await execute(
        "git",
        [
          "diff",
          "--name-only",
          artifact.candidate,
          head,
          "--",
          ...candidateRuntimePaths,
        ],
        { cwd: root, windowsHide: true },
      )
    ).stdout.trim();
    if (runtimeDrift)
      throw new Error("accepted M7 HEAD drifts from its validated candidate runtime");
    const candidatePackage = JSON.parse(
      await gitFile(artifact.candidate, "package.json"),
    );
    if (
      packageManifest.scripts?.["validate:accepted"] !== "pnpm validate:m7" ||
      packageManifest.scripts?.["validate:m7"] !==
        "pnpm validate:m6 && pnpm test:m7:local && pnpm check:evidence" ||
      packageManifest.scripts?.["validate:m7:pre-evidence"] !==
        "pnpm validate:m6 && pnpm test:m7:local && pnpm test:m7:fuse" ||
      packageManifest.scripts?.["test:m7:fuse"] !== "node scripts/run-m7-fuse-gate.mjs"
    )
      throw new Error("accepted M7 package selectors differ from the exact gate");
    for (const manifest of [candidatePackage, packageManifest]) {
      delete manifest.scripts["validate:accepted"];
      delete manifest.scripts["validate:m7"];
    }
    if (JSON.stringify(candidatePackage) !== JSON.stringify(packageManifest))
      throw new Error("accepted M7 package manifest changes more than its selectors");
    const candidateWorkflow = parseYaml(
      await gitFile(artifact.candidate, ".github/workflows/ci.yml"),
    );
    const currentWorkflow = parseYaml(
      await readFile(path.join(root, ".github", "workflows", "ci.yml"), "utf8"),
    );
    const workflowErrors = workflowPolicyErrors(currentWorkflow);
    if (workflowErrors.length)
      throw new Error(`accepted M7 CI policy differs: ${workflowErrors.join("; ")}`);
    if (currentWorkflow.jobs.validate["timeout-minutes"] !== 30)
      throw new Error("accepted M7 portable matrix lacks its thirty-minute deadline");
    delete candidateWorkflow.jobs.validate["timeout-minutes"];
    delete currentWorkflow.jobs.validate["timeout-minutes"];
    if (JSON.stringify(candidateWorkflow) !== JSON.stringify(currentWorkflow))
      throw new Error("accepted M7 workflow changes more than its portable timeout");
  }
  if (!m8EvidenceInProgress) await assertOwnedWorktreeClean("m7");
}

await validateOptionalM7Evidence();

async function validateOptionalM8Evidence() {
  const directory = path.join(root, "docs", "evidence", "m8");
  const jsonFilename = path.join(directory, "correctness.json");
  const exitFilename = path.join(directory, "exit.md");
  let artifact;
  try {
    artifact = requireObject(
      JSON.parse(await readFile(jsonFilename, "utf8")),
      "m8 correctness artifact",
    );
  } catch (error) {
    if (error?.code === "ENOENT" && activeAcceptedMilestone !== "m8") return;
    throw error;
  }
  if (artifact.schema !== "efs-m8-evidence-v1" || artifact.status !== "passed")
    throw new Error("m8 evidence must be a passing efs-m8-evidence-v1 artifact");
  for (const [name, value] of [
    ["candidate", artifact.candidate],
    ["candidateParent", artifact.candidateParent],
    ["computerCandidate", artifact.computerCandidate],
    ["protectedOriginal.head", artifact.protectedOriginal?.head],
  ])
    if (!/^[0-9a-f]{40}$/u.test(value ?? ""))
      throw new Error(`m8.${name} must be an exact commit`);
  const candidateParents = (
    await execute("git", ["show", "-s", "--format=%P", artifact.candidate], {
      cwd: root,
      windowsHide: true,
    })
  ).stdout.trim();
  if (candidateParents !== artifact.candidateParent)
    throw new Error("m8 candidate parent does not match the production commit");
  const currentComputerCandidate = (
    await execute("git", ["rev-parse", "HEAD"], {
      cwd: "C:\\Users\\yifan\\code\\Ephemeral-AI-Lab\\ephemeral-ai-computer",
      windowsHide: true,
    })
  ).stdout.trim();
  if (currentComputerCandidate !== artifact.computerCandidate)
    throw new Error("m8 Computer candidate drifted after the gate");
  const candidateChanges = (
    await execute(
      "git",
      ["diff", "--name-only", `${artifact.candidateParent}..${artifact.candidate}`],
      {
        cwd: root,
        windowsHide: true,
      },
    )
  ).stdout
    .trim()
    .split(/\r?\n/u)
    .filter(Boolean);
  const m8CandidatePrefixes = [
    "packages/fs/",
    "packages/replication/",
    "packages/node-vfs/api-snapshots/",
    "packages/testkit/api-snapshots/",
    "scripts/",
  ];
  if (
    !candidateChanges.length ||
    candidateChanges.some(
      (filename) => !m8CandidatePrefixes.some((prefix) => filename.startsWith(prefix)),
    )
  )
    throw new Error("m8 production candidate changes an unowned path");
  if (
    JSON.stringify(artifact.commands) !==
    JSON.stringify([
      "pnpm check:api",
      "pnpm test:m8",
      "pnpm test:quick",
      "npm.cmd test --workspace @cloudflare/computer-rpc",
      "npm.cmd test --workspace @cloudflare/computerd",
      "wsl.exe -- bash -lc set -e; printf 'uname=%s\\n' \"$(uname -srmo)\"; test -c /dev/fuse; stat -c 'fuse=%F mode=%a device=%t:%T' /dev/fuse; fusermount3 --version | head -1; node --version",
    ])
  )
    throw new Error("m8 evidence does not identify the exact controlling commands");
  const totals = requireObject(artifact.testTotals, "m8.testTotals");
  for (const [name, expected] of [
    ["fsM8", [40, 40, 0, 0]],
    ["fsQuick", [231, 231, 0, 0]],
    ["computerRpc", [70, 70, 0, 0]],
    ["computerd", [145, 144, 0, 1]],
  ]) {
    const value = requireObject(totals[name], `m8.testTotals.${name}`);
    if (
      [value.tests, value.passed, value.failed, value.skipped].join(",") !==
      expected.join(",")
    )
      throw new Error(`m8.${name} totals differ from the measured gate output`);
  }
  if (
    !Array.isArray(artifact.gates) ||
    artifact.gates.length !== 17 ||
    artifact.gates.some((gate) => gate.status !== "passed")
  )
    throw new Error("m8 evidence must contain all 17 passing gates");
  const carrier = requireObject(artifact.carrier, "m8.carrier");
  for (const [name, expected] of [
    ["path", "/efs"],
    ["protocol", "computer-efs-carrier-v1"],
    ["perMessageDeflate", false],
    ["rawFrameBytes", 4 * 1024 * 1024 + 64 * 1024],
    ["decodedEnvelopeBytes", 3 * 1024 * 1024],
    ["acknowledgementBytes", 64 * 1024],
    ["scratchBytes", 2 * 1024 * 1024],
    ["maxReservationBytes", 17.25 * 1024 * 1024],
  ])
    if (carrier[name] !== expected)
      throw new Error(`m8 carrier ${name} is not normative`);
  const fuse = requireObject(artifact.fuse, "m8.fuse");
  if (
    fuse.topology !== "PowerShell -> wsl.exe -> Linux Node/computerd -> /dev/fuse" ||
    fuse.requiredIdentity !== "character-device /dev/fuse" ||
    requireObject(fuse.backend, "m8.fuse.backend").kind !== "fuse"
  )
    throw new Error("m8 evidence does not prove the required real-FUSE topology");
  const fuseLog = await readFile(path.join(root, fuse.log), "utf8");
  if (
    !/uname=Linux .*WSL2/iu.test(fuseLog) ||
    !/fuse=character special file/iu.test(fuseLog) ||
    !/fusermount3 version/iu.test(fuseLog)
  )
    throw new Error("m8 FUSE log lacks Linux WSL2 /dev/fuse identity");
  const memory = requireObject(artifact.memory, "m8.memory");
  if (
    !Number.isSafeInteger(memory.daemonRssBytes) ||
    memory.daemonRssBytes <= 0 ||
    !Number.isSafeInteger(memory.daemonHeapUsedBytes) ||
    memory.daemonHeapUsedBytes <= 0 ||
    memory.daemonCarrierReservedBytes !== 0
  )
    throw new Error("m8 memory or carrier-reservation evidence is invalid");
  const databases = requireObject(artifact.databases, "m8.databases");
  if (
    !Number.isSafeInteger(databases.replicaBytes) ||
    databases.replicaBytes <= 0 ||
    databases.replicaWalBytes !== 0
  )
    throw new Error("m8 database/WAL evidence is invalid");
  if (
    !Number.isSafeInteger(artifact.restarts) ||
    artifact.restarts < 2 ||
    !Array.isArray(artifact.transfers) ||
    artifact.transfers.length !== 3
  )
    throw new Error("m8 restart or transfer evidence is incomplete");
  const identities = requireObject(artifact.identities, "m8.identities");
  if (
    !/^[0-9a-f-]{36}$/u.test(identities.filesystemId) ||
    identities.authorityId !== "m8-authority" ||
    identities.branchId !== "m8-branch" ||
    !/^[0-9a-f]{64}$/u.test(identities.branchGenerationDigest ?? "")
  )
    throw new Error("m8 identity or generation digest evidence is invalid");
  const cleanup = requireObject(artifact.cleanup, "m8.cleanup");
  if (
    cleanup.daemonCarrierReservedBytes !== 0 ||
    cleanup.replicaWalBytesAfterCheckpoint !== 0 ||
    cleanup.temporaryDatabasesRemoved !== true ||
    cleanup.activeSessionsAfterGate !== 0 ||
    cleanup.activeLeasesAfterGate !== 0 ||
    cleanup.stagingReservationsAfterGate !== 0 ||
    cleanup.stubsAfterGate !== 0
  )
    throw new Error("m8 cleanup evidence is incomplete");
  if (!Array.isArray(artifact.logs) || artifact.logs.length !== 6)
    throw new Error("m8 evidence must contain six hashed gate logs");
  for (const [index, value] of artifact.logs.entries()) {
    const log = requireObject(value, `m8.logs[${index}]`);
    requireNonemptyString(log.path, `m8.logs[${index}].path`);
    requirePositiveInteger(log.elapsedMs, `m8.logs[${index}].elapsedMs`);
    if (log.exitCode !== 0 || !/^[0-9a-f]{64}$/u.test(log.sha256 ?? ""))
      throw new Error(`m8.logs[${index}] has an invalid exit or hash`);
    const source = await readFile(path.join(root, log.path), "utf8");
    if (
      sha256(source) !== log.sha256 ||
      !source.includes(`candidate=${artifact.candidate}`) ||
      !source.includes(`computerCandidate=${artifact.computerCandidate}`)
    )
      throw new Error(`m8 log integrity differs for ${log.path}`);
    if (!source.includes("M8_LOG_META") || !source.includes("exitCode=0"))
      throw new Error(`m8 log ${log.path} lacks its exact pass marker`);
  }
  if (artifact.protectedOriginal.head !== "42954593e59395654718ef675d62a1f68a93f47b")
    throw new Error("m8 protected original repository HEAD differs");
  const protectedStatus = (
    await execute("git", ["status", "--porcelain=v1", "-z", "--untracked-files=all"], {
      cwd: "C:\\Users\\yifan\\code\\Ephemeral-AI-Lab\\ephemeral-ai-fs",
      windowsHide: true,
    })
  ).stdout.trim();
  if (sha256(protectedStatus) !== artifact.protectedOriginal.statusSha256)
    throw new Error("m8 protected original repository status changed");
  const recordCommit = await evidenceCommit(path.relative(root, jsonFilename));
  if (
    recordCommit &&
    recordCommit !== "fatal: bad revision 'HEAD'" &&
    !process.env.M8_PRECOMMIT
  ) {
    const evidenceParents = (
      await execute("git", ["show", "-s", "--format=%P", recordCommit], {
        cwd: root,
        windowsHide: true,
      })
    ).stdout.trim();
    if (evidenceParents !== artifact.candidate)
      throw new Error(
        "m8 evidence commit is not the direct child of its production candidate",
      );
    const evidenceChanges = (
      await execute(
        "git",
        ["diff", "--name-only", `${artifact.candidate}..${recordCommit}`],
        { cwd: root, windowsHide: true },
      )
    ).stdout
      .trim()
      .split(/\r?\n/u)
      .filter(Boolean);
    const exactEvidenceFiles = [
      "docs/evidence/m8/correctness.json",
      "docs/evidence/m8/exit.md",
      ...artifact.logs.map((log) => log.path),
      "scripts/check-evidence.mjs",
    ].sort();
    if (JSON.stringify(evidenceChanges.sort()) !== JSON.stringify(exactEvidenceFiles))
      throw new Error(
        "m8 evidence commit contains files outside the exact evidence set",
      );
  }
  if (activeAcceptedMilestone === "m8") {
    const head = (
      await execute("git", ["rev-parse", "HEAD"], { cwd: root, windowsHide: true })
    ).stdout.trim();
    const acceptanceParent = (
      await execute("git", ["show", "-s", "--format=%P", head], {
        cwd: root,
        windowsHide: true,
      })
    ).stdout.trim();
    if (acceptanceParent !== recordCommit)
      throw new Error("accepted M8 HEAD is not the direct child of M8 evidence");
    const changes = (
      await execute("git", ["diff", "--name-only", `${recordCommit}..${head}`], {
        cwd: root,
        windowsHide: true,
      })
    ).stdout
      .trim()
      .split(/\r?\n/u)
      .filter(Boolean);
    if (JSON.stringify(changes) !== JSON.stringify(["package.json"]))
      throw new Error("M8 acceptance changes more than package.json");
    if (packageManifest.scripts?.["validate:accepted"] !== "pnpm validate:m8")
      throw new Error("validate:accepted did not advance to M8");
  }
  if (!process.env.M8_PRECOMMIT) await assertOwnedWorktreeClean("m8");
}

await validateOptionalM8Evidence();

console.log(
  `evidence: preserved predecessor candidates and current ${activeAcceptedMilestone.toUpperCase()} schemas, zero-failure results, candidate parents, sequential predecessors, independent audit, and required metrics are internally consistent`,
);

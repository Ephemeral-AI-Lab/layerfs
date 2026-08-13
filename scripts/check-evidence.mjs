import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import ts from "typescript";

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
if (!new Set(["m0", "m1", "m2", "m3", "m4", "m5", "m6"]).has(activeAcceptedMilestone))
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
    "node-portable",
    "durable-object-portable",
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
  return (
    m5 ||
    filename.startsWith("tests/durable-object-integration/") ||
    filename.startsWith("examples/durable-object-workspace/") ||
    filename === "docs/implementation/m6-handoff.md"
  );
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
    .split("\n")
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
    requireCurrentDigest: activeAcceptedMilestone === "m6",
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

console.log(
  `evidence: preserved predecessor candidates and current ${activeAcceptedMilestone.toUpperCase()} schemas, zero-failure results, candidate parents, sequential predecessors, independent audit, and required metrics are internally consistent`,
);

import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";
import ts from "typescript";

const execute = promisify(execFile);
const root = path.resolve(import.meta.dirname, "..");

function requireObject(value, name) {
  if (!value || typeof value !== "object" || Array.isArray(value))
    throw new Error(`${name} must be an object`);
  return value;
}
function requirePositiveInteger(value, name) {
  if (!Number.isSafeInteger(value) || value <= 0)
    throw new Error(`${name} must be a positive safe integer`);
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
  return (
    m1 ||
    filename.startsWith("packages/fs/src/") ||
    filename.startsWith("packages/sqlite-node/src/") ||
    filename.startsWith("tests/storage/") ||
    filename.startsWith("tests/node-integration/") ||
    filename.startsWith("tests/maintenance/")
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
async function validateMilestone(name, requiredMetrics) {
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
  const currentDigest = await ownedTreeDigest(name, "HEAD");
  if (artifact.ownedTreeDigest !== currentDigest)
    throw new Error(`${name} accepted evidence is stale for milestone-owned files`);
  await assertOwnedWorktreeClean(name);
  return { artifact, exit, candidate };
}

if (process.argv[2] === "--owned-tree-digest") {
  const milestone = process.argv[3];
  const commit = process.argv[4] ?? "HEAD";
  if (!new Set(["m0", "m1", "m2"]).has(milestone))
    throw new Error("owned-tree digest milestone must be m0, m1, or m2");
  console.log(await ownedTreeDigest(milestone, commit));
  process.exit(0);
}

const m0 = await validateMilestone("m0", [
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
]);
if (
  m0.artifact.passed !==
  m0.artifact.metrics.matrixRuns * m0.artifact.metrics.architectureTestsPerCell
)
  throw new Error("m0 passed count differs from the recorded tests per matrix cell");

const m1 = await validateMilestone("m1", [
  "operatingSystems",
  "nodeVersions",
  "matrixRuns",
  "nodeAlgorithmTests",
  "workerdChecks",
  "streamedManifestEntries",
  "streamedManifestReadBatchRecords",
  "streamedManifestPeakRetainedRecords",
]);
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

const m2 = await validateMilestone("m2", [
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
]);
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

console.log(
  `evidence: M0/M1/M2 schemas, zero-failure results, candidate parents, sequential predecessors, independent audit, and required metrics are internally consistent`,
);

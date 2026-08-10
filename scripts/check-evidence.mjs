import { execFile } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { promisify } from "node:util";

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
    "eslint.config.mjs",
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
  return (
    m0 ||
    /^(?:packages\/fs\/src\/(?:cas|cdc|cow|patches|manifests)\/|packages\/fs\/src\/operations\/(?:full-rebuild|local-rebuild|streamed-rebuild)\.ts$)/u.test(
      filename,
    ) ||
    filename.startsWith("tests/algorithms/") ||
    filename.startsWith("tests/workerd/") ||
    filename === "scripts/check-workerd-algorithms.mjs" ||
    filename.startsWith("docs/spec/") ||
    filename.startsWith("docs/testing/") ||
    filename === "docs/implementation/implementation-plan.md"
  );
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
    .filter((record) => record && ownedByMilestone(milestone, record.filename))
    .sort((left, right) => left.filename.localeCompare(right.filename));
  const digest = createHash("sha256");
  for (const record of records)
    digest.update(record.filename).update("\0").update(record.hash).update("\n");
  return digest.digest("hex");
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
  return { artifact, exit, candidate };
}

const m0 = await validateMilestone("m0", [
  "operatingSystems",
  "nodeVersions",
  "matrixRuns",
  "coreSourceFiles",
  "negativeArchitectureFixtures",
  "publishablePackages",
  "publicEntrypoints",
  "exportedSymbols",
  "cleanDistFiles",
  "packedTarballs",
  "packedFiles",
]);
if (m0.artifact.passed !== m0.artifact.metrics.matrixRuns * 4)
  throw new Error("m0 passed count differs from four tests per matrix cell");

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

console.log(
  `evidence: M0/M1 schemas, zero-failure results, candidate parents, sequential predecessor, and required metrics are internally consistent`,
);

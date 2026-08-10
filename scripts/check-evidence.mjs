import { execFile } from "node:child_process";
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

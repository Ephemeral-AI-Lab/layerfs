import { spawnSync } from "node:child_process";
import { readdirSync, statSync } from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const argumentsList = process.argv.slice(2);
const requested = argumentsList.filter((argument) => !argument.startsWith("--"));
const excludeArgument = argumentsList.find((argument) =>
  argument.startsWith("--exclude="),
);
const profileArgument = argumentsList.find((argument) =>
  argument.startsWith("--profile="),
);
const concurrencyArgument = argumentsList.find((argument) =>
  argument.startsWith("--concurrency="),
);
const reporterArgument = argumentsList.find((argument) =>
  argument.startsWith("--reporter="),
);
const timeoutArgument = argumentsList.find((argument) =>
  argument.startsWith("--timeout="),
);
const namePatternArgument = argumentsList.find((argument) =>
  argument.startsWith("--test-name-pattern="),
);
const failFast = argumentsList.includes("--fail-fast");
const excluded = new Set(
  (excludeArgument?.slice("--exclude=".length) ?? "").split(",").filter(Boolean),
);
const profile = profileArgument?.slice("--profile=".length) ?? "full";
const hasConcurrencyArgument = Boolean(concurrencyArgument);
const concurrency = Number(
  hasConcurrencyArgument
    ? concurrencyArgument.slice("--concurrency=".length)
    : failFast
      ? "1"
      : profile === "quick"
        ? "4"
        : "1",
);
const timeout =
  timeoutArgument?.slice("--timeout=".length) ??
  (profile === "quick" ? "120000" : undefined);
const reporter =
  reporterArgument?.slice("--reporter=".length) ??
  (profile === "quick" ? "spec" : undefined);

if (!Number.isInteger(concurrency) || concurrency < 1)
  throw new Error("--concurrency must be a positive integer");
if (profile !== "full" && profile !== "quick")
  throw new Error(`unknown test profile: ${profile}`);

const testNamePattern = namePatternArgument?.slice("--test-name-pattern=".length);
if (requested.length === 0)
  throw new Error("run-test-suite requires at least one file or directory");

const files = [];
function collect(target) {
  const absolute = path.resolve(root, target);
  const relative = path.relative(root, absolute).split(path.sep);
  if (relative.some((part) => excluded.has(part))) return;
  const stat = statSync(absolute, { throwIfNoEntry: false });
  if (!stat) return;
  if (stat.isDirectory()) {
    for (const name of readdirSync(absolute).sort()) collect(path.join(target, name));
  } else if (absolute.endsWith(".test.mjs")) files.push(absolute);
}
for (const target of requested) collect(target);
if (files.length === 0) {
  console.error(`required test suite has zero tests: ${requested.join(", ")}`);
  process.exit(2);
}

const nodeArguments = ["--test", `--test-concurrency=${concurrency}`];
if (reporter) nodeArguments.push(`--test-reporter=${reporter}`);
if (timeout) nodeArguments.push(`--test-timeout=${timeout}`);
if (testNamePattern) nodeArguments.push(`--test-name-pattern=${testNamePattern}`);

function run(filesToRun) {
  const result = spawnSync(process.execPath, [...nodeArguments, ...filesToRun], {
    cwd: root,
    stdio: "inherit",
  });
  if (result.error) throw result.error;
  return result.status ?? 1;
}

const packageIntegrationFiles = files.filter((file) =>
  file.endsWith(`${path.sep}architecture${path.sep}package-integration.test.mjs`),
);
const parallelFiles = files.filter((file) => !packageIntegrationFiles.includes(file));

if (failFast) {
  for (const file of files) {
    const status = run([file]);
    if (status !== 0) process.exit(status);
  }
  process.exit(0);
}

// check:exports intentionally removes and rebuilds every package dist tree.
// When this M0 test is launched beside import-time Node suites, those suites
// can observe the deliberate clean window. Serialize that one repository-wide
// artifact gate before the otherwise parallel suite so the test remains
// covered without creating a false missing-dist failure.
if (concurrency > 1 && packageIntegrationFiles.length > 0) {
  for (const file of packageIntegrationFiles) {
    const status = run([file]);
    if (status !== 0) process.exit(status);
  }
}

process.exit(run(concurrency > 1 ? parallelFiles : files));

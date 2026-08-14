import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const argumentsList = process.argv.slice(2);
const baseArgument = argumentsList.find((argument) => argument.startsWith("--base="));
const base = baseArgument?.slice("--base=".length) ?? "HEAD";
const dryRun = argumentsList.includes("--dry-run");
const parallel = argumentsList.includes("--parallel");

function run(command, commandArguments, options = {}) {
  const executable =
    process.platform === "win32" && command === "pnpm" ? "pnpm.cmd" : command;
  const result = spawnSync(executable, commandArguments, {
    cwd: root,
    stdio: "inherit",
    windowsHide: true,
    shell: process.platform === "win32" && command === "pnpm",
    ...options,
  });
  if (result.error) throw result.error;
  return result.status ?? 1;
}

function gitLines(commandArguments) {
  const result = spawnSync("git", commandArguments, {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(
      `git ${commandArguments.join(" ")} failed with status ${result.status}`,
    );
  }
  return result.stdout.split(/\r?\n/u).filter(Boolean);
}

const changedFiles = [
  ...gitLines(["diff", "--name-only", "--diff-filter=ACDMRTUXB", base]),
  ...gitLines(["ls-files", "--others", "--exclude-standard"]),
].map((file) => file.replaceAll("\\", "/"));
const uniqueChangedFiles = [...new Set(changedFiles)].sort();

const quickTargets = [
  "tests/algorithms",
  "tests/architecture/foundation.test.mjs",
  "tests/branches",
  "tests/conformance",
  "tests/replication",
  "tests/storage",
];
const targets = new Set();
let broadFallback = false;
let needsBuild = false;
let needsApiCheck = false;

function addTarget(target) {
  if (existsSync(path.resolve(root, target))) targets.add(target);
}

function addQuickFallback() {
  broadFallback = true;
}

function classifyFsSource(relativePath) {
  if (relativePath.startsWith("src/branches/")) {
    addTarget("tests/branches");
    return;
  }
  if (
    relativePath.startsWith("src/cas/") ||
    relativePath.startsWith("src/cdc/") ||
    relativePath.startsWith("src/cow/") ||
    relativePath.startsWith("src/manifests/") ||
    relativePath.startsWith("src/patches/")
  ) {
    addTarget("tests/algorithms");
    return;
  }
  if (
    relativePath.startsWith("src/integrations/node-vfs") ||
    relativePath.startsWith("src/operations/node-vfs-bridge")
  ) {
    addTarget("tests/node-vfs");
    return;
  }
  if (
    relativePath.startsWith("src/integrations/replication") ||
    relativePath.startsWith("src/operations/replication-bridge") ||
    relativePath.startsWith("src/replication/")
  ) {
    addTarget("tests/replication");
    return;
  }
  if (relativePath === "src/index.ts") {
    addQuickFallback();
    return;
  }
  if (
    relativePath.startsWith("src/filesystem/") ||
    relativePath.startsWith("src/operations/") ||
    relativePath.startsWith("src/sqlite/") ||
    relativePath.startsWith("src/streams/") ||
    relativePath.startsWith("src/namespace/") ||
    relativePath.startsWith("src/maintenance/") ||
    relativePath.startsWith("src/cache/") ||
    relativePath.startsWith("src/resources/") ||
    relativePath.startsWith("src/revisions/")
  ) {
    addTarget("tests/storage");
    return;
  }
  addQuickFallback();
}

for (const file of uniqueChangedFiles) {
  if (file.startsWith("tests/") && file.endsWith(".test.mjs")) {
    addTarget(file);
    continue;
  }
  if (file.startsWith("tests/")) {
    addQuickFallback();
    continue;
  }
  if (file.startsWith("packages/") && file.includes("/api-snapshots/")) {
    needsApiCheck = true;
    continue;
  }
  if (
    file.startsWith("packages/") &&
    (file.includes("/src/") ||
      file.endsWith("/package.json") ||
      file.endsWith("/tsconfig.json"))
  ) {
    needsBuild = true;
  }
  if (file.endsWith("/src/index.ts")) needsApiCheck = true;
  if (file.startsWith("packages/fs/")) {
    classifyFsSource(file.slice("packages/fs/".length));
    if (file.endsWith("/package.json")) needsApiCheck = true;
    continue;
  }
  if (file.startsWith("packages/replication/")) {
    addTarget("tests/replication");
    if (file.endsWith("/package.json")) needsApiCheck = true;
    continue;
  }
  if (file.startsWith("packages/node-vfs/")) {
    addTarget("tests/node-vfs");
    if (file.endsWith("/package.json")) needsApiCheck = true;
    continue;
  }
  if (file.startsWith("packages/sqlite-node/")) {
    addTarget("tests/node-integration");
    addTarget("tests/storage");
    continue;
  }
  if (file.startsWith("packages/testkit/")) {
    addQuickFallback();
    continue;
  }
  if (
    file === "package.json" ||
    file === "pnpm-lock.yaml" ||
    file.startsWith("scripts/") ||
    file.endsWith("/tsconfig.json") ||
    file.startsWith(".github/")
  ) {
    addQuickFallback();
    if (file.includes("check-api") || file.includes("check-exports"))
      needsApiCheck = true;
    continue;
  }
}

if (broadFallback) {
  targets.clear();
  for (const target of quickTargets) addTarget(target);
}

const targetList = [...targets];
console.log(`affected tests: ${targetList.length ? targetList.join(", ") : "none"}`);
console.log(
  `changed files: ${uniqueChangedFiles.length} (base ${base}); mode: ${parallel ? "parallel" : "fail-fast"}`,
);
if (needsBuild) console.log("preflight: build required");
if (needsApiCheck) console.log("preflight: API snapshot check required");

if (dryRun || uniqueChangedFiles.length === 0) {
  if (uniqueChangedFiles.length === 0)
    console.log("no changes found; use pnpm test:quick for a baseline run");
  process.exit(0);
}

if (needsApiCheck) {
  const status = run(process.execPath, ["scripts/check-api-snapshots.mjs"]);
  if (status !== 0) process.exit(status);
}
if (needsBuild) {
  const status = run("pnpm", ["build"]);
  if (status !== 0) process.exit(status);
}

if (targetList.length === 0) {
  console.log("no executable test targets affected");
  process.exit(0);
}

const runnerArguments = [
  "scripts/run-test-suite.mjs",
  ...targetList,
  "--profile=quick",
];
if (!parallel) runnerArguments.push("--fail-fast");
process.exit(run(process.execPath, runnerArguments));

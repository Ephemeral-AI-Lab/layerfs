import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { performance } from "node:perf_hooks";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const deadlineMs = 600_000;
const started = performance.now();
const skipBuild = process.argv.slice(2).includes("--skip-build");
if (process.argv.slice(2).some((argument) => argument !== "--skip-build"))
  throw new Error("usage: node scripts/run-m6-local-gate.mjs [--skip-build]");
const pnpmExecutable =
  process.platform === "win32"
    ? path.join(
        process.env.PNPM_HOME ?? "",
        ".tools",
        "pnpm-exe",
        "10.32.1",
        "pnpm.exe",
      )
    : "pnpm";
const credentialPattern =
  /^(?:CLOUDFLARE_|CF_(?:API|ACCOUNT|ZONE)|WRANGLER_(?:API|ACCOUNT|OAUTH))/u;
const proxyPattern = /^(?:HTTP_PROXY|HTTPS_PROXY|ALL_PROXY|NO_PROXY)$/u;
const localEnvironment = Object.fromEntries(
  Object.entries(process.env).filter(
    ([name]) =>
      !credentialPattern.test(name.toUpperCase()) &&
      !proxyPattern.test(name.toUpperCase()),
  ),
);

function runCommand(name, command, args, environment = localEnvironment) {
  const taskStarted = performance.now();
  console.log(`m6-local-gate: START ${name}`);
  return new Promise((resolve, reject) => {
    let settled = false;
    const child = spawn(command, args, {
      cwd: root,
      env: environment,
      stdio: "inherit",
      windowsHide: true,
    });
    const remainingMs = Math.max(1, deadlineMs - (performance.now() - started));
    const deadline = setTimeout(() => {
      if (settled) return;
      settled = true;
      child.kill();
      reject(
        new Error(
          `m6-local-gate: ${name} exceeded the remaining ${Math.round(remainingMs)} ms target budget`,
        ),
      );
    }, remainingMs);
    child.on("error", (error) => {
      if (settled) return;
      settled = true;
      clearTimeout(deadline);
      reject(error);
    });
    child.on("exit", (code, signal) => {
      if (settled) return;
      settled = true;
      clearTimeout(deadline);
      const elapsedMs = Math.round(performance.now() - taskStarted);
      if (code === 0) {
        console.log(`m6-local-gate: PASS ${name} (${elapsedMs} ms)`);
        resolve({ name, elapsedMs });
        return;
      }
      reject(
        new Error(
          `m6-local-gate: ${name} failed (${code ?? signal ?? "unknown"}) after ${elapsedMs} ms`,
        ),
      );
    });
  });
}

function run(name, args, environment) {
  return runCommand(name, process.execPath, args, environment);
}

function runPnpm(name, args, environment) {
  if (process.platform === "win32" && !process.env.PNPM_HOME)
    throw new Error("m6-local-gate requires PNPM_HOME on Windows");
  return runCommand(name, pnpmExecutable, args, environment);
}

async function requireAll(promises) {
  const settled = await Promise.allSettled(promises);
  const failure = settled.find((result) => result.status === "rejected");
  if (failure) throw failure.reason;
  return settled.map((result) => result.value);
}

const build = skipBuild ? null : await runPnpm("workspace-build", ["build"]);
const previewDirectory = await mkdtemp(path.join(tmpdir(), "efs-m6-preview-gate-"));
let results;
try {
  const preview = await run("preview-bundle", [
    "scripts/check-cloudflare-preview.mjs",
    `--outdir=${previewDirectory}`,
  ]);
  const previewEnvironment = {
    ...localEnvironment,
    EFS_M6_PREVIEW_BUNDLE: path.join(previewDirectory, "index.js"),
  };
  const workerdAlgorithms = await run("workerd-algorithms", [
    "scripts/check-workerd-algorithms.mjs",
  ]);
  const parallelResults = await requireAll([
    runPnpm("node-portable", [
      "exec",
      "vitest",
      "run",
      "--config",
      "tests/durable-object-integration/vitest.node.config.ts",
    ]),
    run(
      "durable-object-scale-resource",
      ["scripts/run-m6-cloudflare-scale-resource.mjs"],
      previewEnvironment,
    ),
  ]);
  const [serialFaultResults, maintenanceFaults] = await requireAll([
    (async () => {
      const durableObjectPortable = await runPnpm(
        "durable-object-portable",
        [
          "exec",
          "vitest",
          "run",
          "--config",
          "tests/durable-object-integration/vitest.config.ts",
          "--exclude",
          "tests/durable-object-integration/cloudflare-scale.test.ts",
        ],
        previewEnvironment,
      );
      const migrations = await run(
        "durable-object-migration-faults",
        ["scripts/run-m6-cloudflare-migrations.mjs"],
        previewEnvironment,
      );
      const filesystemFaults = await run(
        "durable-object-filesystem-faults",
        ["scripts/run-m6-cloudflare-filesystem-faults.mjs"],
        previewEnvironment,
      );
      const publicationFaults = await run(
        "durable-object-publication-faults",
        ["scripts/run-m6-cloudflare-publication-faults.mjs"],
        previewEnvironment,
      );
      return {
        durableObjectPortable,
        migrations,
        filesystemFaults,
        publicationFaults,
      };
    })(),
    run(
      "node-and-durable-object-maintenance-faults",
      ["scripts/run-m6-maintenance-faults.mjs"],
      previewEnvironment,
    ),
  ]);
  const { durableObjectPortable, migrations, filesystemFaults, publicationFaults } =
    serialFaultResults;
  results = [
    preview,
    workerdAlgorithms,
    ...parallelResults,
    durableObjectPortable,
    migrations,
    filesystemFaults,
    publicationFaults,
    maintenanceFaults,
  ];
} finally {
  await rm(previewDirectory, { recursive: true, force: true });
}

const elapsedMs = Math.round(performance.now() - started);
console.log(
  `m6-local-gate: PASS (${elapsedMs} ms) ${JSON.stringify({ build, results })}`,
);
if (elapsedMs >= deadlineMs)
  throw new Error(`M6 faithful-local selection exceeded ${deadlineMs} ms`);

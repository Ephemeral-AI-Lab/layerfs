import { spawn } from "node:child_process";
import { performance } from "node:perf_hooks";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(import.meta.dirname, "..");
if (!process.env.EFS_M6_PREVIEW_BUNDLE)
  throw new Error("M6 Workerd scale resource gate requires EFS_M6_PREVIEW_BUNDLE");

const MIB = 1024 * 1024;
const MIN_REPRODUCED_RUNTIME_EFFECT_BYTES = 32 * MIB;
const MAX_WORKERD_PROCESS_RSS_BYTES = 768 * MIB;
const vitest = path.join(root, "node_modules", "vitest", "vitest.mjs");
const controlMode = process.argv.slice(2).includes("--control");
if (process.argv.slice(2).some((argument) => argument !== "--control"))
  throw new Error(
    "usage: node scripts/run-m6-cloudflare-scale-resource.mjs [--control]",
  );

function runCapture(command, args) {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: root,
      env: process.env,
      windowsHide: true,
      shell: false,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let stdout = "";
    let stderr = "";
    child.stdout.setEncoding("utf8");
    child.stderr.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (code === 0) resolve({ stdout, stderr });
      else
        reject(
          new Error(
            `${command} failed (${code ?? signal ?? "unknown"}):\n${stdout}\n${stderr}`,
          ),
        );
    });
  });
}

async function workerdProcesses() {
  if (process.platform === "win32") {
    const tasklist = path.join(
      process.env.SystemRoot ?? "C:\\Windows",
      "System32",
      "tasklist.exe",
    );
    const { stdout } = await runCapture(tasklist, [
      "/FI",
      "IMAGENAME eq workerd.exe",
      "/FO",
      "CSV",
      "/NH",
    ]);
    const rows = [];
    for (const line of stdout.split(/\r?\n/u)) {
      const fields = [...line.matchAll(/"([^"]*)"/gu)].map((match) => match[1]);
      if (fields.length < 5 || fields[0]?.toLowerCase() !== "workerd.exe") continue;
      const pid = Number(fields[1]);
      const rssKiB = Number(fields[4].replace(/[^0-9]/gu, ""));
      if (Number.isSafeInteger(pid) && Number.isSafeInteger(rssKiB))
        rows.push({ pid, rssBytes: rssKiB * 1024 });
    }
    return rows;
  }
  const { stdout } = await runCapture("ps", ["-eo", "pid=,rss=,comm="]);
  return stdout
    .trim()
    .split(/\n/u)
    .map((line) => line.trim().split(/\s+/u))
    .filter((fields) => fields.length === 3 && fields[2] === "workerd")
    .map(([pid, rssKiB]) => ({
      pid: Number(pid),
      rssBytes: Number(rssKiB) * 1024,
    }))
    .filter(
      (row) => Number.isSafeInteger(row.pid) && Number.isSafeInteger(row.rssBytes),
    );
}

let controlResourceEvidence;
if (!controlMode) {
  const control = await runCapture(process.execPath, [
    fileURLToPath(import.meta.url),
    "--control",
  ]);
  process.stdout.write(control.stdout);
  process.stderr.write(control.stderr);
  const marker = "m6-workerd-control-resource-evidence ";
  const line = control.stdout.split(/\r?\n/u).find((value) => value.includes(marker));
  if (!line) throw new Error("raw Workerd resource control did not emit evidence");
  controlResourceEvidence = JSON.parse(
    line.slice(line.indexOf(marker) + marker.length),
  );
}

const preexisting = await workerdProcesses();
if (preexisting.length)
  throw new Error(
    `M6 Workerd RSS gate requires a clean runner; found ${JSON.stringify(preexisting)}`,
  );

let phase = "ignore";
let baselinePeakRssBytes = 0;
let fullPeakRssBytes = 0;
let baselineMinimumRssBytes = Number.POSITIVE_INFINITY;
let fullMinimumRssBytes = Number.POSITIVE_INFINITY;
let peakWorkerdProcessRssBytes = 0;
let sampleCount = 0;
const observedPids = new Set();
let sampleQueue = Promise.resolve();
const sample = (samplePhase = phase) => {
  sampleQueue = sampleQueue.then(async () => {
    const rows = await workerdProcesses();
    const rssBytes = rows.reduce((total, row) => total + row.rssBytes, 0);
    for (const row of rows) observedPids.add(row.pid);
    if (rssBytes > 0 && (samplePhase === "baseline" || samplePhase === "full")) {
      sampleCount += 1;
      peakWorkerdProcessRssBytes = Math.max(peakWorkerdProcessRssBytes, rssBytes);
      if (samplePhase === "baseline") {
        baselinePeakRssBytes = Math.max(baselinePeakRssBytes, rssBytes);
        baselineMinimumRssBytes = Math.min(baselineMinimumRssBytes, rssBytes);
      } else {
        fullPeakRssBytes = Math.max(fullPeakRssBytes, rssBytes);
        fullMinimumRssBytes = Math.min(fullMinimumRssBytes, rssBytes);
      }
    }
  });
  return sampleQueue;
};

const started = performance.now();
const child = spawn(
  process.execPath,
  [
    vitest,
    "run",
    "--config",
    "tests/durable-object-integration/vitest.config.ts",
    controlMode
      ? "tests/durable-object-integration/cloudflare-resource-control.test.ts"
      : "tests/durable-object-integration/cloudflare-scale.test.ts",
    "--reporter=verbose",
  ],
  {
    cwd: root,
    env: {
      ...process.env,
      ...(controlMode ? { EFS_M6_RESOURCE_CONTROL: "1" } : {}),
    },
    windowsHide: true,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
  },
);
let stdoutBuffer = "";
let testEvidence;
const inspectLine = (line) => {
  const windowMarker = "m6-workerd-resource-window ";
  const windowOffset = line.indexOf(windowMarker);
  if (windowOffset >= 0) {
    const window = JSON.parse(line.slice(windowOffset + windowMarker.length));
    if (
      (window.phase !== "baseline" && window.phase !== "full") ||
      (window.edge !== "start" && window.edge !== "end")
    )
      throw new Error(`invalid Workerd resource window ${JSON.stringify(window)}`);
    phase = window.phase;
    void sample(window.phase);
    if (window.edge === "end") phase = "ignore";
  }
  const marker = controlMode ? "m6-workerd-control-evidence " : "m6-scale-evidence ";
  const offset = line.indexOf(marker);
  if (offset >= 0) testEvidence = JSON.parse(line.slice(offset + marker.length));
};
child.stdout.setEncoding("utf8");
child.stderr.setEncoding("utf8");
child.stdout.on("data", (chunk) => {
  process.stdout.write(chunk);
  stdoutBuffer += chunk;
  const lines = stdoutBuffer.split(/\r?\n/u);
  stdoutBuffer = lines.pop() ?? "";
  for (const line of lines) inspectLine(line);
  void sample();
});
child.stderr.on("data", (chunk) => process.stderr.write(chunk));
const interval = setInterval(() => void sample(), 500);
const exit = await new Promise((resolve, reject) => {
  child.on("error", reject);
  child.on("exit", (code, signal) => resolve({ code, signal }));
});
clearInterval(interval);
if (stdoutBuffer) inspectLine(stdoutBuffer);
await sample();

if (exit.code !== 0)
  throw new Error(
    `M6 Workerd scale resource test failed (${exit.code ?? exit.signal ?? "unknown"})`,
  );
if (
  !testEvidence ||
  (controlMode
    ? testEvidence.fullRows !== 100_000 ||
      testEvidence.filesystemCachesInstantiated !== false
    : testEvidence.rows !== 100_000)
)
  throw new Error("M6 Workerd resource test did not emit exact scale evidence");
if (
  sampleCount < (controlMode ? 2 : 10) ||
  baselinePeakRssBytes <= 0 ||
  fullPeakRssBytes <= 0
)
  throw new Error("M6 Workerd RSS sampling did not cover baseline and full phases");
if (peakWorkerdProcessRssBytes > MAX_WORKERD_PROCESS_RSS_BYTES)
  throw new Error(
    `Workerd process RSS exceeded ${MAX_WORKERD_PROCESS_RSS_BYTES}: ${peakWorkerdProcessRssBytes}`,
  );

const rssGrowthBytes = fullPeakRssBytes - baselinePeakRssBytes;
const baselineWindowGrowthBytes = baselinePeakRssBytes - baselineMinimumRssBytes;
const fullWindowGrowthBytes = fullPeakRssBytes - fullMinimumRssBytes;
if (
  controlMode &&
  (rssGrowthBytes < MIN_REPRODUCED_RUNTIME_EFFECT_BYTES ||
    fullWindowGrowthBytes < MIN_REPRODUCED_RUNTIME_EFFECT_BYTES)
)
  throw new Error(
    `raw Workerd control did not reproduce row-count-dependent resident growth: ${JSON.stringify({ rssGrowthBytes, fullWindowGrowthBytes, minimumBytes: MIN_REPRODUCED_RUNTIME_EFFECT_BYTES })}`,
  );

const resourceEvidence = {
  schema: controlMode
    ? "efs-m6-workerd-control-resource-v1"
    : "efs-m6-workerd-resource-v1",
  exactProcessBoundAvailable: false,
  platformIsolateLimitBytes: 128 * MIB,
  baselineRows: 10_240,
  fullRows: 100_000,
  baselinePeakRssBytes,
  fullPeakRssBytes,
  baselineMinimumRssBytes,
  fullMinimumRssBytes,
  rssGrowthBytes,
  baselineWindowGrowthBytes,
  fullWindowGrowthBytes,
  ...(controlMode
    ? {}
    : {
        rawRuntimeControlGrowthBytes: controlResourceEvidence.rssGrowthBytes,
        rawRuntimeControlWindowGrowthBytes:
          controlResourceEvidence.fullWindowGrowthBytes,
        reproducedRuntimeEffect: true,
        minimumReproducedRuntimeEffectBytes: MIN_REPRODUCED_RUNTIME_EFFECT_BYTES,
      }),
  peakWorkerdProcessRssBytes,
  maxWorkerdProcessRssBytes: MAX_WORKERD_PROCESS_RSS_BYTES,
  observedPids: [...observedPids].sort((left, right) => left - right),
  sampleCount,
  elapsedMs: Math.round(performance.now() - started),
  ...(controlMode
    ? {
        controlBaselineDatabaseBytes: testEvidence.baselineDatabaseBytes,
        controlFullDatabaseBytes: testEvidence.fullDatabaseBytes,
      }
    : {
        scaleFixtureDigest: testEvidence.fixtureDigest,
        scaleMainFileBytes: testEvidence.mainFileBytes,
        scalePhysicalRestarts: testEvidence.physicalRestarts,
      }),
};
console.log(
  `${
    controlMode
      ? "m6-workerd-control-resource-evidence"
      : "m6-workerd-resource-evidence"
  } ${JSON.stringify(resourceEvidence)}`,
);

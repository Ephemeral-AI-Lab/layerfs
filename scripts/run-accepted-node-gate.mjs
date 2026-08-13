import { spawn } from "node:child_process";
import { performance } from "node:perf_hooks";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const deadlineMs = 600_000;
const started = performance.now();
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

function runCommand(name, command, args) {
  const taskStarted = performance.now();
  console.log(`accepted-node-gate: START ${name}`);
  return new Promise((resolve, reject) => {
    let settled = false;
    const child = spawn(command, args, {
      cwd: root,
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
          `accepted-node-gate: ${name} exceeded the remaining ${Math.round(remainingMs)} ms target budget`,
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
        console.log(`accepted-node-gate: PASS ${name} (${elapsedMs} ms)`);
        resolve({ name, elapsedMs });
        return;
      }
      reject(
        new Error(
          `accepted-node-gate: ${name} failed (${code ?? signal ?? "unknown"}) after ${elapsedMs} ms`,
        ),
      );
    });
  });
}

function run(name, args) {
  return runCommand(name, process.execPath, args);
}

function runPnpm(name, args) {
  if (process.platform === "win32" && !process.env.PNPM_HOME)
    throw new Error("accepted-node-gate requires PNPM_HOME on Windows");
  return runCommand(name, pnpmExecutable, args);
}

async function requireAll(promises) {
  const settled = await Promise.allSettled(promises);
  const failure = settled.find((result) => result.status === "rejected");
  if (failure) throw failure.reason;
  return settled.map((result) => result.value);
}

const buildResult = await runPnpm("workspace-build", ["build"]);
// The operation-count smoke has an independent 60-second correctness ceiling and an
// isolated database. Let it overlap the read-only static checks while preserving fully
// uncontended execution for the latency-sensitive benchmark cells below.
const [staticResults, smokeResult] = await Promise.all([
  requireAll([
    runPnpm("fixtures-check", ["fixtures:check"]),
    runPnpm("docs-check", ["check:docs"]),
    runPnpm("style-check", ["check:style"]),
    runPnpm("architecture-check", ["check:architecture"]),
    runPnpm("exports-check", ["check:exports"]),
  ]),
  run("node-smoke", ["scripts/run-test-suite.mjs", "tests/smoke"]),
]);

// Performance cells run without competing test I/O so their latency and throughput
// measurements remain meaningful on the reference runner.
const benchmarkResults = [];
benchmarkResults.push(
  await run("m3-benchmarks", [
    "tests/performance/mini-bench.mjs",
    "--cell=A1",
    "--trials=5",
  ]),
);
benchmarkResults.push(
  await run("m4-branch-benchmarks", ["tests/performance/branch-bench.mjs"]),
);

// The independent correctness suites use isolated databases. Running them together
// removes predecessor duplication while keeping every mandatory Node check selected.
const correctnessResults = await requireAll([
  run("node-core-correctness", [
    "scripts/run-test-suite.mjs",
    "tests/architecture",
    "tests/algorithms",
    "tests/storage",
    "tests/node-integration",
    "tests/conformance",
    "tests/branches",
  ]),
  run("node-maintenance", ["scripts/run-test-suite.mjs", "tests/maintenance"]),
  run("node-fault", ["scripts/run-test-suite.mjs", "tests/fault"]),
  run("workerd-algorithms", ["scripts/check-workerd-algorithms.mjs"]),
]);

const elapsedMs = Math.round(performance.now() - started);
console.log(
  `accepted-node-gate: PASS (${elapsedMs} ms) ${JSON.stringify({
    build: buildResult,
    static: staticResults,
    benchmarks: benchmarkResults,
    smoke: smokeResult,
    correctness: correctnessResults,
  })}`,
);
if (elapsedMs >= 600_000)
  throw new Error(`accepted Node correctness and benchmark gate exceeded 600000 ms`);

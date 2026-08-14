import { spawn } from "node:child_process";
import path from "node:path";
import { performance } from "node:perf_hooks";

const root = path.resolve(import.meta.dirname, "..");
const deadlineMs = 600_000;
const started = performance.now();
const pnpmScript = process.env.npm_execpath;

function run(name, command, args) {
  console.log(`m7-local-gate: START ${name}`);
  const taskStarted = performance.now();
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd: root,
      stdio: "inherit",
      windowsHide: true,
    });
    const remaining = Math.max(1, deadlineMs - (performance.now() - started));
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      reject(new Error(`m7-local-gate: ${name} exceeded the remaining time budget`));
    }, remaining);
    child.once("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.once("exit", (code, signal) => {
      clearTimeout(timer);
      const elapsedMs = Math.round(performance.now() - taskStarted);
      if (code === 0) {
        console.log(`m7-local-gate: PASS ${name} (${elapsedMs} ms)`);
        resolve();
      } else {
        reject(
          new Error(
            `m7-local-gate: ${name} failed (${code ?? signal ?? "unknown"}) after ${elapsedMs} ms`,
          ),
        );
      }
    });
  });
}

if (pnpmScript && /\.[cm]?js$/u.test(pnpmScript))
  await run("build", process.execPath, [pnpmScript, "build"]);
else if (pnpmScript) await run("build", pnpmScript, ["build"]);
else await run("build", process.platform === "win32" ? "pnpm.cmd" : "pnpm", ["build"]);
await run("node-vfs-correctness-fault-resource", process.execPath, [
  "scripts/run-test-suite.mjs",
  "tests/node-vfs",
  "--exclude=real-fuse",
]);
const elapsedMs = Math.round(performance.now() - started);
if (elapsedMs >= deadlineMs)
  throw new Error(`M7 local selection exceeded ${deadlineMs} ms`);
console.log(`m7-local-gate: PASS (${elapsedMs} ms)`);

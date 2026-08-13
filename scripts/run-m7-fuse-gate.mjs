import { spawn } from "node:child_process";
import path from "node:path";
import { performance } from "node:perf_hooks";

const root = path.resolve(import.meta.dirname, "..");
const deadlineMs = 60_000;
const started = performance.now();
const child = spawn(
  process.execPath,
  [path.join(root, "tests/node-vfs/real-fuse-smoke.mjs")],
  { cwd: root, stdio: "inherit", windowsHide: true },
);
const deadline = setTimeout(() => child.kill("SIGTERM"), deadlineMs);
child.once("error", (error) => {
  clearTimeout(deadline);
  throw error;
});
child.once("exit", (code, signal) => {
  clearTimeout(deadline);
  const elapsedMs = Math.round(performance.now() - started);
  if (code === 0 && elapsedMs < deadlineMs) {
    console.log(`m7-real-fuse-gate: PASS (${elapsedMs} ms)`);
    return;
  }
  console.error(
    `m7-real-fuse-gate: ${code === 2 ? "BLOCKED" : "FAIL"} (${code ?? signal ?? "unknown"}, ${elapsedMs} ms)`,
  );
  process.exitCode = code ?? 1;
});

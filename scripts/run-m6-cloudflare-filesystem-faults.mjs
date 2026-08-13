import { spawn } from "node:child_process";
import { performance } from "node:perf_hooks";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
if (!process.env.EFS_M6_PREVIEW_BUNDLE)
  throw new Error("run-m6-cloudflare-filesystem-faults requires EFS_M6_PREVIEW_BUNDLE");
const positions = Object.freeze({
  "writeFile-create": 214,
  "writeFile-stream": 214,
  writeRange: 78,
  replaceRange: 78,
  truncate: 78,
  mkdir: 175,
  chmod: 29,
  link: 70,
  symlink: 59,
  rename: 60,
  unlink: 49,
  "rm-recursive": 114,
});
const tasks = Object.keys(positions);
const vitest = path.join(root, "node_modules", "vitest", "vitest.mjs");
const started = performance.now();
let next = 0;

function runOperation(operation) {
  return new Promise((resolve, reject) => {
    console.log(`m6-cloudflare-filesystem-faults: START ${operation}`);
    const child = spawn(
      process.execPath,
      [
        vitest,
        "run",
        "--config",
        "tests/durable-object-integration/vitest.config.ts",
        "tests/durable-object-integration/cloudflare-fault.test.ts",
        "--reporter=dot",
      ],
      {
        cwd: root,
        windowsHide: true,
        stdio: "inherit",
        env: {
          ...process.env,
          EFS_M6_FILESYSTEM_FAULT_OPERATION: operation,
        },
      },
    );
    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (code === 0) {
        console.log(`m6-cloudflare-filesystem-faults: PASS ${operation}`);
        resolve();
      } else
        reject(
          new Error(
            `m6-cloudflare-filesystem-faults: ${operation} failed (${code ?? signal ?? "unknown"})`,
          ),
        );
    });
  });
}

async function worker() {
  while (next < tasks.length) {
    if (performance.now() - started >= 300_000)
      throw new Error("M6 filesystem fault matrix exceeded 300,000 ms");
    await runOperation(tasks[next++]);
  }
}

await Promise.all(Array.from({ length: 4 }, () => worker()));
const elapsedMs = Math.round(performance.now() - started);
console.log(
  `m6-cloudflare-filesystem-faults: PASS ${JSON.stringify({
    statementPositions: Object.values(positions).reduce((sum, value) => sum + value, 0),
    operations: positions,
    restart: "evictDurableObject",
    elapsedMs,
  })}`,
);

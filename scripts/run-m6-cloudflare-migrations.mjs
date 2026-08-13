import { spawn } from "node:child_process";
import { performance } from "node:perf_hooks";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
if (!process.env.EFS_M6_PREVIEW_BUNDLE)
  throw new Error(
    "run-m6-cloudflare-migrations requires the exact EFS_M6_PREVIEW_BUNDLE",
  );

const statementCounts = Object.freeze({ 1: 365, 2: 339, 3: 292 });
const chunkSize = 48;
const concurrency = 4;
const deadlineMs = 300_000;
const started = performance.now();
const vitest = path.join(root, "node_modules", "vitest", "vitest.mjs");
const tasks = [];
for (const [version, count] of Object.entries(statementCounts))
  for (let start = 1; start <= count; start += chunkSize)
    tasks.push({
      version,
      start,
      end: Math.min(count, start + chunkSize - 1),
    });

function runChunk(task) {
  return new Promise((resolve, reject) => {
    const label = `v${task.version}:${task.start}-${task.end}`;
    console.log(`m6-cloudflare-migrations: START ${label}`);
    const child = spawn(
      process.execPath,
      [
        vitest,
        "run",
        "--config",
        "tests/durable-object-integration/vitest.config.ts",
        "tests/durable-object-integration/cloudflare-schema.test.ts",
        "--reporter=dot",
      ],
      {
        cwd: root,
        windowsHide: true,
        stdio: "inherit",
        env: {
          ...process.env,
          EFS_M6_MIGRATION_VERSION: task.version,
          EFS_M6_MIGRATION_START: String(task.start),
          EFS_M6_MIGRATION_END: String(task.end),
        },
      },
    );
    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (code === 0) {
        console.log(`m6-cloudflare-migrations: PASS ${label}`);
        resolve();
      } else {
        reject(
          new Error(
            `m6-cloudflare-migrations: ${label} failed (${code ?? signal ?? "unknown"})`,
          ),
        );
      }
    });
  });
}

let next = 0;
let passed = 0;
async function worker() {
  while (next < tasks.length) {
    if (performance.now() - started >= deadlineMs)
      throw new Error("M6 Cloudflare migration matrix exceeded 300,000 ms");
    const task = tasks[next++];
    await runChunk(task);
    passed += 1;
  }
}

await Promise.all(Array.from({ length: concurrency }, () => worker()));
const elapsedMs = Math.round(performance.now() - started);
if (elapsedMs >= deadlineMs)
  throw new Error("M6 Cloudflare migration matrix exceeded 300,000 ms");
console.log(
  `m6-cloudflare-migrations: PASS ${JSON.stringify({
    chunks: passed,
    statementPositions: Object.values(statementCounts).reduce(
      (total, value) => total + value,
      0,
    ),
    sourceVersions: Object.keys(statementCounts).map(Number),
    elapsedMs,
  })}`,
);

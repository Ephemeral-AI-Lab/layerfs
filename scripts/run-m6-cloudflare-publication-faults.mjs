import { spawn } from "node:child_process";
import { performance } from "node:perf_hooks";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
if (!process.env.EFS_M6_PREVIEW_BUNDLE)
  throw new Error(
    "run-m6-cloudflare-publication-faults requires EFS_M6_PREVIEW_BUNDLE",
  );
const counts = Object.freeze({ direct: 95, prepared: 91 });
const chunkSize = 32;
const tasks = [];
for (const [variant, count] of Object.entries(counts))
  for (let start = 1; start <= count; start += chunkSize)
    tasks.push({ variant, start, end: Math.min(count, start + chunkSize - 1) });
const vitest = path.join(root, "node_modules", "vitest", "vitest.mjs");
const started = performance.now();
let next = 0;

function runChunk(task) {
  return new Promise((resolve, reject) => {
    const label = `${task.variant}:${task.start}-${task.end}`;
    console.log(`m6-cloudflare-publication-faults: START ${label}`);
    const child = spawn(
      process.execPath,
      [
        vitest,
        "run",
        "--config",
        "tests/durable-object-integration/vitest.config.ts",
        "tests/durable-object-integration/cloudflare-publication-fault.test.ts",
        "--reporter=dot",
      ],
      {
        cwd: root,
        windowsHide: true,
        stdio: "inherit",
        env: {
          ...process.env,
          EFS_M6_PUBLICATION_VARIANT: task.variant,
          EFS_M6_PUBLICATION_START: String(task.start),
          EFS_M6_PUBLICATION_END: String(task.end),
        },
      },
    );
    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (code === 0) {
        console.log(`m6-cloudflare-publication-faults: PASS ${label}`);
        resolve();
      } else
        reject(
          new Error(
            `m6-cloudflare-publication-faults: ${label} failed (${code ?? signal ?? "unknown"})`,
          ),
        );
    });
  });
}

async function worker() {
  while (next < tasks.length) {
    if (performance.now() - started >= 180_000)
      throw new Error("M6 publication fault matrix exceeded 180,000 ms");
    await runChunk(tasks[next++]);
  }
}
await Promise.all(Array.from({ length: 3 }, () => worker()));
const elapsedMs = Math.round(performance.now() - started);
console.log(
  `m6-cloudflare-publication-faults: PASS ${JSON.stringify({
    statementPositions: Object.values(counts).reduce((sum, value) => sum + value, 0),
    variants: counts,
    elapsedMs,
  })}`,
);

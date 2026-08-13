import { spawn } from "node:child_process";
import { performance } from "node:perf_hooks";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
if (!process.env.EFS_M6_PREVIEW_BUNDLE)
  throw new Error("run-m6-maintenance-faults requires EFS_M6_PREVIEW_BUNDLE");
const topology = Object.freeze({
  snapshot: Object.freeze({ statement: 110, batch: 42 }),
  collection: Object.freeze({ statement: 259, batch: 128 }),
  abandoned: Object.freeze({ statement: 61, batch: 33 }),
});
const chunkSize = 32;
const tasks = [];
for (const target of ["node", "cloudflare"])
  for (const [variant, kinds] of Object.entries(topology))
    for (const [kind, count] of Object.entries(kinds))
      for (let start = 1; start <= count; start += chunkSize)
        tasks.push({
          target,
          variant,
          kind,
          start,
          end: Math.min(count, start + chunkSize - 1),
        });

const vitest = path.join(root, "node_modules", "vitest", "vitest.mjs");
const started = performance.now();
let next = 0;
function runChunk(task) {
  return new Promise((resolve, reject) => {
    const label = `${task.target}:${task.variant}:${task.kind}:${task.start}-${task.end}`;
    console.log(`m6-maintenance-faults: START ${label}`);
    const cloudflare = task.target === "cloudflare";
    const child = spawn(
      process.execPath,
      [
        vitest,
        "run",
        "--config",
        cloudflare
          ? "tests/durable-object-integration/vitest.config.ts"
          : "tests/durable-object-integration/vitest.node.config.ts",
        cloudflare
          ? "tests/durable-object-integration/cloudflare-maintenance-fault.test.ts"
          : "tests/durable-object-integration/node-maintenance-fault.test.ts",
        "--reporter=dot",
      ],
      {
        cwd: root,
        windowsHide: true,
        stdio: "inherit",
        env: {
          ...process.env,
          EFS_M6_MAINTENANCE_VARIANT: task.variant,
          EFS_M6_MAINTENANCE_KIND: task.kind,
          EFS_M6_MAINTENANCE_START: String(task.start),
          EFS_M6_MAINTENANCE_END: String(task.end),
        },
      },
    );
    child.on("error", reject);
    child.on("exit", (code, signal) => {
      if (code === 0) {
        console.log(`m6-maintenance-faults: PASS ${label}`);
        resolve();
      } else
        reject(
          new Error(
            `m6-maintenance-faults: ${label} failed (${code ?? signal ?? "unknown"})`,
          ),
        );
    });
  });
}
async function worker() {
  while (next < tasks.length) {
    if (performance.now() - started >= 360_000)
      throw new Error("M6 maintenance fault matrices exceeded 360,000 ms");
    await runChunk(tasks[next++]);
  }
}
await Promise.all(Array.from({ length: 4 }, () => worker()));
const elapsedMs = Math.round(performance.now() - started);
console.log(
  `m6-maintenance-faults: PASS ${JSON.stringify({
    targets: ["node", "cloudflare"],
    topology,
    positionsPerTarget: Object.values(topology).reduce(
      (total, kinds) =>
        total + Object.values(kinds).reduce((subtotal, value) => subtotal + value, 0),
      0,
    ),
    elapsedMs,
  })}`,
);

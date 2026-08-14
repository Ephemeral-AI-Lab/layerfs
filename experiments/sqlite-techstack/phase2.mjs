import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";

const ROOT = fileURLToPath(new URL(".", import.meta.url));
const BINARY = join(ROOT, "..", "m8-rust-port", "target", "release", "layerfs-m8-rust");
const RESULTS = join(ROOT, "results");
const repetitions = Number.parseInt(process.env.PHASE2_REPS ?? "6", 10);
if (!Number.isInteger(repetitions) || repetitions < 2) throw new Error("PHASE2_REPS must be at least 2");

const jobs = [
  ["create", [1, 10, 100]],
  ["edit1", [1, 10, 100]],
  ["edit3", [100]],
  ["read", [100]],
  ["materialize", [100]],
  ["materialize-100x1m", [1]],
];

function run(args) {
  const result = spawnSync(BINARY, args, {
    cwd: ROOT,
    encoding: "utf8",
    timeout: 15 * 60 * 1000,
  });
  if (result.error || result.status !== 0) {
    throw new Error(`${BINARY} ${args.join(" ")} failed: ${result.error?.message ?? result.stderr}`);
  }
  const output = result.stdout.trim();
  if (!output) throw new Error(`empty result for ${args.join(" ")}`);
  return JSON.parse(output);
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

function summarize(rows) {
  const groups = new Map();
  for (const row of rows.filter((item) => item.retained)) {
    const key = `${row.workload}|${row.size_mib}|${row.candidate}`;
    const group = groups.get(key) ?? [];
    group.push(row);
    groups.set(key, group);
  }
  return [...groups.entries()].map(([key, group]) => {
    const [workload, size_mib, candidate] = key.split("|");
    const metrics = {};
    for (const field of ["elapsed_ms", "cold_read_ms", "warm_read_ms", "random_4k_us", "materialize_ms", "median_ms"]) {
      const values = group.map((row) => row[field]).filter((value) => typeof value === "number");
      if (values.length) metrics[field] = median(values);
    }
    const timing = {};
    for (const field of [
      "m7_bounded_local_edit_ms",
      "payload_persistence_ms",
      "sqlite_commit_ms",
      "close_reopen_ms",
      "full_verification_ms",
      "total_end_to_end_ms",
    ]) {
      const values = group
        .map((row) => row.operation_timing?.[field])
        .filter((value) => typeof value === "number");
      if (values.length) timing[field] = median(values);
    }
    return { workload, size_mib: Number(size_mib), candidate, retained_samples: group.length, metrics, timing };
  });
}

const correctness = {
  carrier: run(["phase2-carrier-check"]),
  recovery_sqlite: run(["phase2-recovery", "sqlite", "1"]),
  recovery_hybrid: run(["phase2-recovery", "hybrid", "1"]),
};
for (const [name, result] of Object.entries(correctness)) {
  if (result.phase2 === "pass" || result.carrier_format_check === "pass") continue;
  throw new Error(`${name} correctness gate failed: ${JSON.stringify(result)}`);
}

const rows = [];
let sequence = 0;
for (const [workload, sizes] of jobs) {
  for (const size of sizes) {
    for (let sample = 0; sample < repetitions; sample += 1) {
      const modes = sample % 2 === 0 ? ["sqlite", "hybrid"] : ["hybrid", "sqlite"];
      for (const mode of modes) {
        process.stderr.write(`phase2 ${workload} ${size}MiB ${mode} sample ${sample + 1}/${repetitions}\n`);
        const result = run(["phase2", mode, workload, String(size)]);
        if (result.phase2 !== "pass") throw new Error(`invalid result: ${JSON.stringify(result)}`);
        rows.push({
          run_id: `phase2-${String(sequence++).padStart(3, "0")}`,
          workload,
          size_mib: size,
          mode,
          retained: sample > 0,
          sample,
          ...result,
        });
      }
    }
  }
}

mkdirSync(RESULTS, { recursive: true });
writeFileSync(join(RESULTS, "phase2.jsonl"), `${rows.map((row) => JSON.stringify(row)).join("\n")}\n`);
const summary = {
  campaign: "phase2",
  repetitions,
  warmups_discarded_per_cell: 1,
  retained_samples_per_cell: repetitions - 1,
  correctness,
  summary: summarize(rows),
  raw_path: "results/phase2.jsonl",
};
writeFileSync(join(RESULTS, "phase2-summary.json"), `${JSON.stringify(summary, null, 2)}\n`);
console.log(JSON.stringify({ campaign: "phase2", samples: rows.length, ...summary, raw_path: "results/phase2.jsonl" }));

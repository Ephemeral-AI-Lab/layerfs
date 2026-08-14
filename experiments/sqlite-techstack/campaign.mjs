import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { ROOT } from "./common.mjs";

const rust = join(ROOT, "rust", "target", "release", "sqlite-techstack-rsql");
const finalist100m = process.argv.includes("--100m");
const jobs = finalist100m ? [
  ["create", "random-100m", ["R-SQL"]],
  ["edit", "edit-small-100m", ["R-SQL"]],
] : [
  ["create", "random-10m", ["T-SQL", "R-SQL"]],
  ["create", "duplicate-10m", ["R-SQL", "T-SQL"]],
  ["read", "random-10m", ["R-SQL", "T-SQL"]],
  ["edit", "edit-small-10m", ["T-SQL", "R-SQL"]],
  ["storage_only", "random-10m", ["R-SQL", "T-SQL"]],
];
const env = {
  ...process.env,
  SQLITE3_LIB_DIR: "/Library/Developer/CommandLineTools/SDKs/MacOSX15.2.sdk/usr",
  SQLITE3_INCLUDE_DIR: "/Library/Developer/CommandLineTools/SDKs/MacOSX15.2.sdk/usr/include",
};
const raw = [];
let order = 0;
for (const [operation, fixture, candidates] of jobs) {
  for (const candidate of candidates) {
    const args = candidate === "T-SQL"
      ? ["--experimental-strip-types", join(ROOT, "tsql.ts"), "campaign", fixture, operation]
      : [rust, "campaign", fixture, operation];
    const result = spawnSync(candidate === "T-SQL" ? process.execPath : rust, args.slice(candidate === "T-SQL" ? 0 : 1), { cwd: ROOT, env, encoding: "utf8" });
    if (result.status !== 0) throw new Error(`${candidate} ${operation} failed: ${result.stderr}`);
    for (const line of result.stdout.trim().split("\n").filter(Boolean)) raw.push({ run_id: `exp1-${String(order++).padStart(2, "0")}`, order, ...JSON.parse(line) });
  }
}
mkdirSync(join(ROOT, "results"), { recursive: true });
const rawName = finalist100m ? "campaign-100m.jsonl" : "campaign.jsonl";
writeFileSync(join(ROOT, "results", rawName), `${raw.map((row) => JSON.stringify(row)).join("\n")}\n`);
console.log(JSON.stringify({ campaign: "complete", retained_samples: raw.length, raw_path: `results/${rawName}` }));

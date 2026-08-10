import { spawnSync } from "node:child_process";
import { readdirSync, statSync } from "node:fs";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const requested = process.argv.slice(2).filter((argument) => !argument.startsWith("--"));
const excludeArgument = process.argv.find((argument) => argument.startsWith("--exclude="));
const excluded = new Set((excludeArgument?.slice("--exclude=".length) ?? "").split(",").filter(Boolean));
if (requested.length === 0) throw new Error("run-test-suite requires at least one file or directory");

const files = [];
function collect(target) {
  const absolute = path.resolve(root, target);
  const relative = path.relative(root, absolute).split(path.sep);
  if (relative.some((part) => excluded.has(part))) return;
  const stat = statSync(absolute, { throwIfNoEntry: false });
  if (!stat) return;
  if (stat.isDirectory()) {
    for (const name of readdirSync(absolute).sort()) collect(path.join(target, name));
  } else if (absolute.endsWith(".test.mjs")) files.push(absolute);
}
for (const target of requested) collect(target);
if (files.length === 0) {
  console.error(`required test suite has zero tests: ${requested.join(", ")}`);
  process.exit(2);
}

const result = spawnSync(process.execPath, ["--test", "--test-concurrency=1", ...files], { cwd: root, stdio: "inherit" });
if (result.error) throw result.error;
process.exit(result.status ?? 1);

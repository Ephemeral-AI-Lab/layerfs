import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { ROOT } from "./common.mjs";

const inputName = process.argv[2] || "campaign.jsonl";
const rows = readFileSync(join(ROOT, "results", inputName), "utf8").trim().split("\n").map(JSON.parse);
const quantile = (values, p) => { const a = [...values].sort((x, y) => x - y); const i = (a.length - 1) * p; const lo = Math.floor(i); return a[lo] + (a[Math.ceil(i)] - a[lo]) * (i - lo); };
const fixtureMiB = (id) => id.endsWith("100m") ? 100 : id.endsWith("10m") ? 10 : null;
const groups = [];
for (const key of new Set(rows.map((row) => `${row.candidate}/${row.operation}/${row.fixture_id}`))) {
  const [candidate, operation, fixture_id] = key.split("/"); const group = rows.filter((row) => row.candidate === candidate && row.operation === operation && row.fixture_id === fixture_id);
  const numeric = (field) => group.map((row) => row[field]).filter((value) => typeof value === "number").map((value) => value / 1e6);
  const duration = numeric("duration_ns");
  groups.push({
    candidate, operation, fixture_id, samples: group.length,
    duration_ms: { min: Math.min(...duration), p25: quantile(duration, .25), median: quantile(duration, .5), p75: quantile(duration, .75), max: Math.max(...duration) },
    throughput_mib_s: fixtureMiB(fixture_id) === null ? null : fixtureMiB(fixture_id) / (quantile(duration, .5) / 1000),
    phase_source_cdc_ms: numeric("phase_source_cdc_ns"),
    phase_sql_commit_ms: numeric("phase_sql_commit_ns"),
    phase_close_reopen_verify_ms: numeric("phase_close_reopen_verify_ns"),
    checkpoint_ms: numeric("checkpoint_ns"),
    setup_ms: numeric("setup_ns"),
    statement_calls: [...new Set(group.map((row) => row.sqlite_statement_calls))],
    rows_read: [...new Set(group.map((row) => row.sqlite_rows_read))],
    rows_inserted: [...new Set(group.map((row) => row.sqlite_rows_inserted))],
    new_objects: [...new Set(group.map((row) => row.new_objects))],
    reused_objects: [...new Set(group.map((row) => row.reused_objects))],
    db_growth_bytes: [...new Set(group.map((row) => row.db_bytes_after - row.db_bytes_before))],
    peak_rss_mib: [...new Set(group.map((row) => typeof row.peak_rss_bytes === "number" ? row.peak_rss_bytes / 1048576 : "unavailable"))],
  });
}
const outputName = inputName === "campaign.jsonl" ? "summary.json" : inputName.replace(/\.jsonl$/, "-summary.json");
writeFileSync(join(ROOT, "results", outputName), `${JSON.stringify(groups, null, 2)}\n`);
console.log(JSON.stringify(groups, null, 2));

// Generates the per-call profile table of docs/architecture/api/front-ends/mcp.md
// from api/mcp/src/profiles.js and the Node package's `table` (it needs the
// built addon):
//   node api/mcp/scripts/gen-profiles.mjs          rewrite the generated block
//   node api/mcp/scripts/gen-profiles.mjs --check  exit 1 when it is stale
// A call without a profile bucket is an error, never a row with a blank cell.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { bucketOf, check, renderRows } from "../src/profiles.js";

export const document = fileURLToPath(new URL("../../../docs/architecture/api/front-ends/mcp.md", import.meta.url));
const BEGIN = "<!-- BEGIN GENERATED: api/mcp/scripts/gen-profiles.mjs -->";
const END = "<!-- END GENERATED -->";


/** The generated block for `table`, and the problems that make it wrong. */
export function render(table) {
  const problems = check(table);
  const rows = renderRows(table);
  const work = table.filter((spec) => bucketOf(spec) === "work").length;
  const admin = table.filter((spec) => bucketOf(spec) === "admin").length;
  const blocks = [
    BEGIN,
    "",
    `${table.length} calls: ${work} in \`work\`, ${admin} in \`admin\` only; \`--profile admin\` lists all ${table.length}.`,
    "",
    "| Call | Bucket | Profiles that list it | Why |",
    "| --- | --- | --- | --- |",
    ...rows,
    "",
    END,
  ];
  return { block: blocks.join("\n"), problems };
}

/** `text` (the page) with its generated block replaced by `block`. */
export function splice(text, block) {
  const start = text.indexOf(BEGIN);
  const end = text.indexOf(END);
  if (start < 0 || end < start) throw new Error(`${document} has no "${BEGIN}" ... "${END}" block`);
  return text.slice(0, start) + block + text.slice(end + END.length);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const { table } = await import("layerfs");
  const { block, problems } = render(table);
  if (problems.length > 0) {
    console.error(`${problems.length} profile problem(s):\n${problems.join("\n")}`);
    process.exit(1);
  }
  const current = readFileSync(document, "utf8");
  const next = splice(current, block);
  if (process.argv.includes("--check")) {
    if (next !== current) {
      console.error(`${document} differs from the table and the profile buckets; run node api/mcp/scripts/gen-profiles.mjs`);
      process.exit(1);
    }
  } else {
    writeFileSync(document, next);
    console.log(`Wrote ${document}`);
  }
}

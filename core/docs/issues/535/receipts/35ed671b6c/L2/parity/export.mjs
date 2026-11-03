// Writes the cell scripts of the frozen runner's workloads.mjs as JSON, so the
// CLI and Python drivers run byte-identical bodies. usage: node export.mjs <workloads.mjs> <out.json>
import { writeFileSync } from "node:fs";
const [src, out] = process.argv.slice(2);
const w = await import(src);
const cells = {};
for (const cell of w.CELLS) {
  cells[cell] = { setup: w.CF[cell][1] ?? null, timed: w.timedLine(w.command(cell)), verifier: w.verifier(cell), body_sha256: w.sha256(w.CF[cell][0]) };
}
writeFileSync(out, JSON.stringify({ identity: w.IDENTITY, cells }, null, 2) + "\n");

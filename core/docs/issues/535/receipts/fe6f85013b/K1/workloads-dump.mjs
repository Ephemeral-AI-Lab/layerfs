// K1 item 7: the cell workloads exactly as the Node runner uses them, as JSON,
// so the Python and CLI drivers run the same bytes.
//   node workloads-dump.mjs <path to cells/workloads.mjs>
import { pathToFileURL } from "node:url";
const w = await import(pathToFileURL(process.argv[2]).href);
const out = {};
for (const cell of w.CELLS) {
  out[cell] = {
    bodySha256: w.sha256(w.CF[cell][0]),
    setup: w.CF[cell][1],
    line: w.timedLine(w.command(cell)),
    verifier: w.verifier(cell),
  };
}
out.IDENTITY = w.IDENTITY;
console.log(JSON.stringify(out));

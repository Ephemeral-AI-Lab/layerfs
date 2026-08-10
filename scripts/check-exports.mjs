import { readFile } from "node:fs/promises";
import path from "node:path";

const packageFile = path.resolve(import.meta.dirname, "../packages/fs/package.json");
const manifest = JSON.parse(await readFile(packageFile, "utf8"));
const actual = Object.keys(manifest.exports).sort();
const expected = [
  ".",
  "./integrations/node-vfs",
  "./integrations/replication",
  "./sqlite-driver",
].sort();
if (JSON.stringify(actual) !== JSON.stringify(expected)) {
  throw new Error(`core exports changed: ${actual.join(", ")}`);
}
console.log("exports: approved public subpaths only");


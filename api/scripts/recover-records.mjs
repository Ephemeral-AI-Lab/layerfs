// One-off recovery of lost project records (layerfs-home.md, open decision 3).
//
// On 2026-10-23 a Mac restart emptied $TMPDIR, where dsh's LayerFS home lived,
// so every project lost <home>/<project>/project.record. Each project's Store
// is untouched in its Docker volume, and the record can be rebuilt from it:
// the catalog binding, LayerStack, Init's `main` Branch and the first Layer's
// root are rows of the Store; the cursor key only signs page cursors and is new.
// engine/crates/layerfs-node/examples/restore_record.rs writes the record (#347).
//
//   node api/scripts/recover-records.mjs --home <home> --image <sha256:…>
//        --restore <restore_record binary> [--apply] [project …]
//
// Projects come from the runtime's records, <home>/.runtime/projects/<name>.json,
// which name each volume. A project that still has a record is left alone. Each
// Store is read inside a container on its volume mounted read-only (immutable:
// the rows read are written at Init and sealed into the main file). Without
// --apply it prints what it would write and writes nothing.
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

const args = process.argv.slice(2);
const option = (name) => { const i = args.indexOf(name); return i >= 0 ? args.splice(i, 2)[1] : undefined; };
const home = option("--home"), image = option("--image"), restore = option("--restore");
const apply = args.includes("--apply");
const only = args.filter((a) => a !== "--apply");
if (!home || !image || !restore) {
  console.error("usage: recover-records.mjs --home <home> --image <sha256:…> --restore <restore_record> [--apply] [project …]");
  process.exit(2);
}

const FACTS = `
import sqlite3, json
c = sqlite3.connect("file:/v/global/store.sqlite?mode=ro&immutable=1", uri=True)
rows = lambda q: [r[0] for r in c.execute(q)]
print(json.dumps({
  "binding": rows("select hex(binding_key) from history_meta"),
  "stack": rows("select hex(layer_stack_id) from layer_stack"),
  "branch": rows("select hex(branch_id) from branch where name='main'"),
  "root": rows("select hex(root_id) from layer where parent_layer_id is null"),
  "sqlite": [sqlite3.sqlite_version],
}))`;

const records = join(home, ".runtime", "projects");
const names = only.length ? only : readdirSync(records).filter((f) => f.endsWith(".json")).map((f) => f.slice(0, -5)).sort();
let failed = 0;
for (const name of names) {
  const say = (text) => console.log(`${name}: ${text}`);
  const directory = join(home, name);
  if (existsSync(join(directory, "project.record"))) { say("has a record; left alone"); continue; }
  let volume;
  try { ({ volume } = JSON.parse(readFileSync(join(records, `${name}.json`), "utf8"))); } catch (error) { say(`no runtime record: ${error.message}`); failed++; continue; }
  let facts;
  try {
    facts = JSON.parse(execFileSync("docker", ["run", "--rm", "--network", "none", "-v", `${volume}:/v:ro`, "--entrypoint", "python3", image, "-c", FACTS], { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }));
  } catch (error) { say(`Store of ${volume} unreadable: ${String(error.stderr || error.message).trim().split("\n").pop()}`); failed++; continue; }
  const many = Object.entries(facts).filter(([, v]) => v.length !== 1).map(([k, v]) => `${k} ×${v.length}`);
  if (many.length) { say(`not exactly one of each: ${many.join(", ")}`); failed++; continue; }
  const [binding, stack, branch, root, sqlite] = ["binding", "stack", "branch", "root", "sqlite"].map((k) => facts[k][0]);
  say(`${volume}: binding ${binding.slice(0, 12)}…, stack ${stack.slice(0, 12)}…, main ${branch.slice(0, 12)}…, root ${root.slice(0, 12)}…, SQLite ${sqlite}`);
  if (!apply) continue;
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  // the record says the Store is installed, so its sealed size (1) is never read
  const path = execFileSync(restore, [home, name, binding, stack, branch, root, "1", sqlite, sqlite], { encoding: "utf8" }).trim();
  say(`wrote ${path}`);
}
process.exit(failed ? 1 : 0);

// The `layerfs` package over the real addon (#533 step 14): every call of
// the table exists and is checked by the library; a Store, an Init with
// progress, file and Branch calls, bytes both ways, and a folder scope.
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, chmodSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import layerfs, { client, close, table, families, LayerFSError } from "../src/index.js";

const temp = (tag) => {
  const dir = mkdtempSync(join(tmpdir(), `layerfs-node-${tag}-`));
  chmodSync(dir, 0o700);
  return dir;
};

test("the table: ten families, 70 calls, each a function", () => {
  assert.deepEqual(families, ["project", "branch", "commit", "file", "sandbox", "workspace", "machine", "store", "backup", "logs"]);
  assert.equal(table.length, 70);
  for (const spec of table) assert.equal(typeof layerfs[spec.family][spec.call], "function", `${spec.family}.${spec.call}`);
});

test("every call reaches the library, which checks its arguments first", async () => {
  const home = temp("check");
  const lfs = client({ home });
  for (const spec of table) {
    const required = spec.args.filter((a) => !a.optional);
    if (required.length === 0) continue;
    await assert.rejects(lfs[spec.family][spec.call](), (error) => {
      assert.ok(error instanceof LayerFSError, `${spec.family}.${spec.call}`);
      assert.equal(error.code, "InvalidInput", `${spec.family}.${spec.call}: ${error.message}`);
      assert.match(error.message, /is required/);
      return true;
    });
  }
  // the calls with no required argument answer
  assert.deepEqual((await lfs.store.list()).items, []);
  assert.deepEqual((await lfs.project.list()).items, []);
  assert.deepEqual((await lfs.sandbox.list()).items, []);
  assert.deepEqual((await lfs.backup.list()).items, []);
  const machines = await lfs.machine.list();
  assert.deepEqual(machines.machines.map((m) => m.name), ["docker", "native"]);
  await assert.rejects(lfs.store.get("shop", { bogus: 1 }), /unknown option "bogus"/);
  rmSync(home, { recursive: true, force: true });
});

test("a Store, an Init with progress, files, Branches and a folder scope", async () => {
  const home = temp("home");
  const folder = temp("folder");
  writeFileSync(join(folder, "README.md"), "hello node\n");
  const lfs = client({ home });
  await lfs.store.create("shop");
  const progress = [];
  const made = await lfs.project.init("shop/api", folder, { on_progress: (p) => progress.push(p) });
  assert.equal(made.project, "shop/api");
  const read = await lfs.file.read("shop/api", "main", "/README.md");
  assert.equal(read.text, "hello node\n");
  assert.equal(read.start_line, 1, "results are snake_case");
  await lfs.branch.fork("shop/api", "agent.main", "main");
  await lfs.file.mkdir("shop/api", "agent.main", "/web");
  await lfs.file.write("shop/api", "agent.main", "/web/a.bin", Buffer.from([0, 1, 255]), { message: "bytes" });
  const stat = await lfs.file.stat("shop/api", "agent.main", "/web/a.bin");
  assert.equal(stat.size, 3);

  // scoped: paths beneath /web, Branch names under agent., no history
  const scoped = client({ home, scope: { project: "shop/api", folder: "/web", branch: "agent" } });
  const inside = await scoped.file.stat("shop/api", "main", "/web/a.bin");
  assert.equal(inside.size, 3);
  await assert.rejects(scoped.file.read("shop/api", "main", "/README.md"), /out of scope: path/);
  await assert.rejects(scoped.commit.list("shop/api", "main"), /out of scope: history/);
  const branches = await scoped.branch.list("shop/api");
  assert.deepEqual(branches.items.map((b) => b.name), ["main"]);
  await assert.rejects(scoped.store.list(), /out of scope: store.list/);
  await assert.rejects(scoped.file.stat("shop/other", "main", "/web/a.bin"), /out of scope: project/);
  await assert.rejects(client({ home, scope: { folder: "/web" } }).store.list(), /scope.project/);
  await assert.rejects(client({ home, scope: { project: "shop/api", folder: "web" } }).file.stat("shop/api", "main", "/web"), /canonical/);
  // Windows cannot remove a Store its process still holds open; close first
  await close();
  rmSync(home, { recursive: true, force: true });
  rmSync(folder, { recursive: true, force: true });
});

test("the call log names the client node and takes log_tag with any call (#659)", { skip: process.platform === "win32" && "the call log is not ported to Windows yet (#659)" }, async () => {
  const home = temp("logs");
  const lfs = client({ home });
  await lfs.store.create("shop");
  await lfs.store.list({ log_tag: "dsh:s1:r1" });
  await assert.rejects(lfs.store.get("nope", { log_tag: "dsh:s1:r2" }), LayerFSError);
  const page = await lfs.logs.list({ tag: "dsh:s1:" });
  assert.equal(page.has_more, false);
  assert.deepEqual(page.lines.map((l) => [l.client, l.argv, l.outcome, l.tag]), [
    ["node", ["store", "get", "nope"], "NotFound", "dsh:s1:r2"],
    ["node", ["store", "list"], "ok", "dsh:s1:r1"],
  ]);
  const reset = await lfs.logs.reset();
  assert.deepEqual(reset, { kind: "Reset", removed: 3 });
  assert.deepEqual((await lfs.logs.list()).lines, []);
  rmSync(home, { recursive: true, force: true });
});

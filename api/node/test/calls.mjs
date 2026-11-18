// The `layerfs` package over the real addon (#533 step 14): every call of
// the table exists and is checked by the library; a Store, an Init with
// progress, file and Branch calls, bytes both ways, and a folder scope.
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, chmodSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import layerfs, { client, table, families, LayerFSError } from "../src/index.js";

const temp = (tag) => {
  const dir = mkdtempSync(join(tmpdir(), `layerfs-node-${tag}-`));
  chmodSync(dir, 0o700);
  return dir;
};

test("the table: twelve families, 87 calls, each a function", () => {
  assert.deepEqual(families, ["home", "project", "branch", "commit", "file", "sandbox", "workspace", "machine", "store", "backup", "logs", "doctor"]);
  assert.equal(table.length, 87);
  for (const spec of table) assert.equal(typeof layerfs[spec.family][spec.call], "function", `${spec.family}.${spec.call}`);
});

test("client.home is the home family; the client's home is where it calls", async () => {
  const parent = temp("home");
  const home = join(parent, "made");
  try {
    const lfs = client({ home });
    assert.equal(typeof lfs.home.init, "function");
    // the short form: what is wrong and its fix; nothing else
    const missing = await lfs.home.get();
    assert.deepEqual(Object.keys(missing), ["home", "state", "version", "reason", "fix"]);
    assert.deepEqual([missing.state, missing.reason, missing.fix], ["broken", "missing", "layerfs home init"]);
    const made = await lfs.home.init();
    assert.deepEqual(made, { home });
    assert.equal((await lfs.home.init({ detail: true })).outcome, "unchanged");
    const ready = await lfs.home.get();
    assert.deepEqual(Object.keys(ready), ["home", "state", "version", "generation"]);
    assert.equal(ready.state, "ready");
    // `detail` is the long form: every key, null when it has no value
    const long = await lfs.home.get({ detail: true });
    assert.deepEqual(Object.keys(long), ["home", "state", "version", "generation", "reason", "fix", "message", "layout", "mode", "bundle", "daemon", "cache"]);
    assert.deepEqual([long.reason, long.fix, long.layout, long.mode], [null, null, 3n, "0700"].map((v) => (typeof v === "bigint" ? Number(v) : v)));
    // `with` adds the sections asked for, in either form
    const tree = await lfs.home.get({ with: ["tree"] });
    assert.deepEqual(Object.keys(tree.tree), ["machines", "stores", "buckets"]);
    const both = await lfs.home.get({ detail: true, with: ["platforms", "tree"] });
    assert.equal(typeof both.platforms.host, "string");
    assert.ok(both.platforms.items.every((row) => "candidates" in row && typeof row.ready === "boolean"));
    // a section the call does not declare, and a `with` that is no list
    await assert.rejects(lfs.home.get({ with: ["live"] }), (error) =>
      error instanceof LayerFSError && error.error === "InvalidInput" && /takes platforms, tree/.test(error.message));
    await assert.rejects(lfs.home.init({ with: ["tree"] }), /takes no section/);
    await assert.rejects(lfs.home.get({ detail: "yes" }), /`detail` must be true or false/);
  } finally {
    rmSync(parent, { recursive: true, force: true });
  }
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
  assert.deepEqual(machines.items.map((m) => m.machine), ["docker", "native"]);
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
  rmSync(home, { recursive: true, force: true });
  rmSync(folder, { recursive: true, force: true });
});

test("the call log names the client node and takes log_tag with any call (#659)", async () => {
  const home = temp("logs");
  const lfs = client({ home });
  await lfs.store.create("shop");
  await lfs.store.list({ log_tag: "dsh:s1:r1" });
  await assert.rejects(lfs.store.get("nope", { log_tag: "dsh:s1:r2" }), LayerFSError);
  // the call log is `home.log` (`logs.list` until 2.0): a row's short form
  // is its call, its outcome, when, and what went wrong
  assert.equal(lfs.logs.list, undefined);
  const short = await lfs.home.log({ tag: "dsh:s1:" });
  assert.deepEqual(Object.keys(short), ["items"]);
  assert.deepEqual(short.items.map((row) => Object.keys(row)), [["call", "outcome", "logged_at", "error"], ["call", "outcome", "logged_at"]]);
  const page = await lfs.home.log({ tag: "dsh:s1:", detail: true });
  assert.equal(page.next_cursor, null);
  assert.deepEqual(page.items.map((l) => [l.call, l.client, l.argv, l.outcome, l.error, l.tag]), [
    ["store.get", "node", ["store", "get", "nope"], "refused", "NotFound", "dsh:s1:r2"],
    ["store.list", "node", ["store", "list"], "ok", null, "dsh:s1:r1"],
  ]);
  // pages: a cursor carries its filters, and answers no other call's
  const first = await lfs.home.log({ limit: 1 });
  assert.equal(first.items.length, 1);
  const rest = await lfs.home.log({ cursor: first.next_cursor });
  assert.deepEqual([rest.items.length, rest.next_cursor], [2, undefined]);
  await assert.rejects(lfs.home.log({ cursor: first.next_cursor, call: "store.get" }), (error) => error.error === "InvalidInput");
  assert.deepEqual(await lfs.home.clear_log(), { ok: true });
  assert.deepEqual(await lfs.home.clear_log({ detail: true }), { ok: true, outcome: "cleared", line_count: 0 });
  assert.deepEqual((await lfs.home.log()).items, []);
  rmSync(home, { recursive: true, force: true });
});

test("a refusal carries its public code, its fix and, in the long form, its detail", async () => {
  const home = temp("refusal");
  const lfs = client({ home });
  await lfs.store.create("shop");
  const short = await lfs.file.read("shop/nope", "main", "/a").catch((error) => error);
  assert.ok(short instanceof LayerFSError);
  // `code` stays, and is the public code; `error` is the same word
  assert.deepEqual([short.code, short.error, short.fix, short.detail], ["NotFound", "NotFound", null, null]);
  const long = await lfs.file.read("shop/nope", "main", "/a", { detail: true }).catch((error) => error);
  assert.equal(long.error, "NotFound");
  assert.deepEqual(long.detail, { code: "NoSuchProject" });
  // the fields the next call needs are properties of the error
  const made = LayerFSError.from({ error: "Conflict", message: "the head moved", fix: "layerfs branch get shop/api main", commit: "12d1" });
  assert.deepEqual([made.error, made.fix, made.commit, made.message], ["Conflict", "layerfs branch get shop/api main", "12d1", "the head moved"]);
  rmSync(home, { recursive: true, force: true });
});

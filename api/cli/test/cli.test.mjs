// The layerfs command end to end: each test spawns `node api/cli/src/layerfs.js`
// with the real @layerfs/agent JavaScript over a stand-in native binding
// (api/agent/test/fixture/fake-native.cjs, shared with the SDK
// tests), which logs every binding call. No Docker, Store
// or daemon; runs anywhere:
//   node --test api/cli/test/cli.test.mjs
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { after, before, test } from "node:test";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { CALLS, INIT } from "../src/calls.js";
import { usage } from "../src/help.js";
import { tables, written } from "./doc-tables.mjs";
import { CASES, ENV, contains, made, plain } from "../../agent/test/fixture/cases.mjs";

const CLI = fileURLToPath(new URL("../src/layerfs.js", import.meta.url));
const FAKE = fileURLToPath(new URL("../../agent/test/fixture/fake-native.cjs", import.meta.url));
const work = realpathSync(mkdtempSync(join(tmpdir(), "layerfs-cli-")));
after(() => rmSync(work, { recursive: true, force: true }));

const BASE = { PATH: process.env.PATH, HOME: process.env.HOME, ...ENV, LAYERFS_NATIVE: FAKE };
let runs = 0;

/** One layerfs run: exit code, stdout, stderr and the binding calls it made. */
function cli(args, { input = "", env = {}, cwd } = {}) {
  const log = join(work, `calls-${++runs}.jsonl`);
  const r = spawnSync(process.execPath, [CLI, ...args], { input, cwd, env: { ...BASE, CLI_FAKE_LOG: log, ...env }, timeout: 20_000 });
  assert.equal(r.error, undefined, `spawn failed: ${r.error}`);
  const calls = existsSync(log) ? readFileSync(log, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l)) : [];
  // the calls the command made: no Store open/close, no LayerFS.open probe (one Branch)
  const own = made(calls);
  return { code: r.status, out: r.stdout.toString(), err: r.stderr.toString(), calls, made: own, ops: own.map(([op]) => op) };
}


const source = join(work, "src-folder");
mkdirSync(source);
writeFileSync(join(source, "README.md"), "hi\n");
const WS = "1:/workspaces/w1";

// Consistency with the SDK: each shared case (api/agent/test/
// fixture/cases.mjs, whose expected results the SDK's own calls.mjs checks)
// runs through the SDK in this process and through the command, over the
// same stand-in binding and settings. The command must make the same binding
// call with the same arguments, print the SDK's result as JSON (mount adds
// `ws`) and exit 0, or exit 125 with the SDK's refusal on stderr.
const key = (e) => (e.family ? `${e.family} ${e.call}` : e.call);
const command = (e) => (e.family ? [e.family, e.call] : [e.call]);
const native = createRequire(import.meta.url)(FAKE);
let sdk;
before(async () => {
  Object.assign(process.env, ENV, { LAYERFS_NATIVE: FAKE });
  delete process.env.CLI_FAKE_LOG;
  const { LayerFS } = await import("@layerfs/agent");
  sdk = { LayerFS, lf: await LayerFS.open("site") };
});
after(() => sdk?.lf.close());

/** One case through the SDK in this process: the binding calls it made, and its result or refusal. */
async function throughSdk(c) {
  const first = native.args.length;
  process.env.CLI_FAKE_REFUSE = (c.refuse ?? []).join(",");
  try {
    return { result: plain(await c.sdk(sdk.lf, sdk.LayerFS)) };
  } catch (error) {
    return { error };
  } finally {
    delete process.env.CLI_FAKE_REFUSE;
    // as JSON, the way the command's calls are logged: a key whose value is undefined is not sent
    sdk.calls = JSON.parse(JSON.stringify(made(native.calls.slice(first).map((op, i) => [op, plain(native.args[first + i])]))));
  }
}

test("every table entry has a case", () => {
  const covered = new Set(CASES.map((c) => c.argv.slice(0, c.argv[0] === "init" ? 1 : 2).join(" ")));
  for (const entry of [INIT, ...CALLS]) assert.ok(covered.has(command(entry).join(" ")), key(entry));
});

for (const c of CASES) {
  test(`${c.name}: the command matches the SDK`, async () => {
    const want = await throughSdk(c);
    const r = cli(c.argv, { input: c.input, env: { CLI_FAKE_REFUSE: (c.refuse ?? []).join(",") } });
    assert.deepEqual(r.made, sdk.calls, "the same binding calls and arguments");
    if (want.error) {
      const { code, message, detail } = want.error;
      assert.deepEqual([r.code, r.out], [125, ""], r.err);
      assert.deepEqual(JSON.parse(r.err), { code, message: message.replace(`${code}: `, ""), detail: plain(detail ?? null) });
      return;
    }
    assert.equal(r.code, 0, r.err);
    assert.equal(r.err, "");
    assert.ok(r.out.endsWith("\n") && !r.out.slice(0, -1).includes("\n"), "one line");
    const printed = c.op === "workspace.mount" ? { ...want.result, ws: `${want.result.sandbox}:${want.result.path}` } : want.result;
    assert.deepEqual(JSON.parse(r.out), printed);
  });
}

test("defaults: unset options are not sent, SDK defaults apply", () => {
  const diff = cli(["branch", "diff_branches", "a", "b"]).made[0][1];
  assert.ok(!("hunks" in diff) && !("cursor" in diff));
  assert.ok(contains(cli(["fs", "read", "main", "/a"]).made[0][1], { startLine: 1 }));
  assert.ok(contains(cli(["init", "site", source]).made[0][1], { timeoutMs: 300000, path: source }));
  assert.ok(contains(cli(["workspace", "commit", WS]).made[0][1], { force: false }));
});

test("the client is opened once and closed once", () => {
  const r = cli(["fs", "stat", "main", "/a"]);
  assert.deepEqual(r.calls.map(([op]) => op), ["store.open", "project.list_branches", "fs.stat", "store.close"]);
});

test("dashed call names are the same calls", () => {
  assert.deepEqual(cli(["branch", "list-commits", "cart"]).ops, ["branch.list_commits"]);
});

test("init resolves a relative source folder against the working directory", () => {
  assert.ok(contains(cli(["init", "site", "src-folder"], { cwd: work }).made[0][1], { path: source }));
});

for (const [why, argv, message] of [
  ["missing positional", ["fs", "read", "main"], "missing <path>"],
  ["extra positional", ["fs", "stat", "main", "/a", "/b"], 'unexpected argument "/b"'],
  ["unknown option", ["fs", "stat", "main", "/a", "--nope"], "unknown option --nope"],
  ["unknown short option", ["fs", "stat", "main", "/a", "-x"], "unknown option -x"],
  ["duplicate option", ["fs", "mkdir", "cart", "/d", "-m", "a", "--message", "b"], "--message is given twice"],
  ["bool with a value", ["branch", "diff_branches", "a", "b", "--no-hunks=yes"], "--no-hunks takes no value"],
  ["int that is not a whole number", ["project", "list_branches", "--page-size", "x"], "--page-size takes a whole number"],
  ["bad JSON option", ["branch", "merge", "a", "b", "ab", "--units", "{"], "--units is not JSON"],
  ["option with no value", ["fs", "mkdir", "cart", "/d", "-m"], "-m needs a value"],
  ["bad Workspace handle", ["workspace", "commit", "1/workspaces/w1"], "a Workspace is <sandbox>:<path>"],
  ["Workspace handle with no path", ["workspace", "commit", "1:"], "a Workspace is <sandbox>:<path>"],
  ["unknown command", ["nope"], 'unknown command "nope"'],
  ["unknown call", ["fs", "nope"], 'unknown fs call "nope"'],
  ["both --old and --old-from", ["fs", "edit", "cart", "/a", "--old", "x", "--old-from", "/x", "--new", "y"], "give exactly one of --old and --old-from"],
  ["neither --new nor --new-from", ["fs", "edit", "cart", "/a", "--old", "x"], "give exactly one of --new and --new-from"],
  ["unreadable --from", ["fs", "write", "cart", "/a", "--from", "/no/such/file"], "cannot read /no/such/file"],
  ["--mode that is not octal", ["fs", "mkdir", "cart", "/d", "--mode", "rwx"], '--mode takes octal permission bits such as 755, not "rwx"'],
  ["--mode with a digit past 7", ["fs", "write", "cart", "/a", "--mode", "0789"], '--mode takes octal permission bits such as 755, not "0789"'],
]) {
  test(`usage error, nothing called: ${why}`, () => {
    const r = cli(argv);
    assert.equal(r.code, 2);
    assert.match(r.err, new RegExp(`^layerfs: ${message.replace(/[[\]{}()*+?.\\^$|]/g, "\\$&")}`));
    assert.deepEqual(r.calls, []);
    assert.equal(r.out, "");
  });
}

test("usage errors print the command's usage line", () => {
  assert.ok(cli(["fs", "read", "main"]).err.includes(`usage: ${usage(CALLS.find((c) => c.call === "read"))}\n`));
});

test("apply with input that is not JSON exits 2 before anything opens", () => {
  const r = cli(["fs", "apply", "cart"], { input: "[{" });
  assert.equal(r.code, 2);
  assert.match(r.err, /^layerfs: the changes are not JSON/);
  assert.deepEqual(r.calls, []);
});

test("a missing project exits 2 before anything opens", () => {
  const r = cli(["fs", "stat", "main", "/a"], { env: { LAYERFS_PROJECT: "" } });
  assert.equal(r.code, 2);
  assert.match(r.err, /no LayerFS project: pass --project <name> or set LAYERFS_PROJECT/);
  assert.deepEqual(r.calls, []);
});

test("--project wins over LAYERFS_PROJECT; an integration's prefixed name is not read", () => {
  assert.equal(cli(["fs", "stat", "main", "/a", "--project", "other"]).made[0][1].project, "other");
  assert.equal(cli(["--project=other", "fs", "stat", "main", "/a"]).made[0][1].project, "other");
  const none = cli(["fs", "stat", "main", "/a"], { env: { LAYERFS_PROJECT: "", DSH_LAYERFS_PROJECT: "dshp" } });
  assert.equal(none.code, 2, none.err);
  assert.match(none.err, /no LayerFS project/);
});

test("after --, every token is positional, even global option names", () => {
  const r = cli(["workspace", "exec", WS, "--", "--version"]);
  assert.equal(r.code, 0, r.err);
  assert.equal(r.made[0][1].command, "--version");
});

test("bodies: --from bytes, --old-from and --new-from text", () => {
  const bin = join(work, "body.bin");
  writeFileSync(bin, Buffer.from([1, 2, 3, 250]));
  assert.ok(contains(cli(["fs", "write", "cart", "/b", "--from", bin]).made[0][1], { bytes: { base64: "AQID+g==" } }));
  const changes = join(work, "changes.json");
  writeFileSync(changes, '[{"op":"rm","path":"/x"}]');
  assert.ok(contains(cli(["fs", "apply", "cart", "--from", changes]).made[0][1], { changes: [{ op: "rm", path: "/x" }] }));
  const [o, n] = [join(work, "old.txt"), join(work, "new.txt")];
  writeFileSync(o, "old\nline\n");
  writeFileSync(n, "new\n");
  assert.ok(contains(cli(["fs", "edit", "cart", "/a", "--old-from", o, "--new-from", n]).made[0][1], { oldText: "old\nline\n", newText: "new\n" }));
});

test("a refusal prints as text with --text", () => {
  const t = cli(["--text", "fs", "read", "main", "/missing"]);
  assert.deepEqual([t.code, t.out, t.err], [125, "", "NoSuchPath: /missing does not exist\n"]);
});

test("exec --stream passes the command's output and exit code through", () => {
  const stream = cli(["workspace", "exec", WS, "exit 7", "--stream"]);
  assert.deepEqual([stream.code, stream.out, stream.err], [7, "out:exit 7", "err"]);
  assert.equal(stream.made[0][1].spool, true);
});

test("--text prints readable results", () => {
  const r = cli(["--text", "workspace", "commit", WS]);
  assert.equal(r.out, "kind: Committed\nhead: c0ffee\ncommit:\n  id: c0ffee\n  logical: 120\n  newCanonical: 40\n  newStored: 30\n  newMetadata: 8\n");
  assert.equal(cli(["--text", "fs", "read", "main", "/a"]).out, "text: |\n  a\nnextLine: 2\n");
});

test("help, --help and --version", () => {
  const all = cli(["help"]);
  assert.equal(all.code, 0);
  for (const e of [INIT, ...CALLS]) assert.ok(all.out.includes(usage(e).replace(/^layerfs /, "")), key(e));
  assert.equal(cli(["--help"]).out, all.out);
  const fs = cli(["help", "fs"]);
  assert.ok(fs.out.includes("fs read <ref> <path>") && !fs.out.includes("workspace mount"));
  assert.equal(cli(["fs", "--help"]).out, fs.out);
  const one = cli(["fs", "read", "--help"]);
  assert.equal(one.out, `usage: ${usage(CALLS.find((c) => c.call === "read"))}\n  ${CALLS.find((c) => c.call === "read").summary}\n`);
  assert.deepEqual([cli(["--version"]).out, cli(["--version"]).code], ["1.1.0\n", 0]);
  const bare = cli([]);
  assert.deepEqual([bare.code, bare.out], [2, ""]);
  assert.equal(bare.err, all.out);
  assert.equal(cli(["help", "nope"]).code, 2);
  for (const r of [all, one, bare]) assert.deepEqual(r.calls, []);
});

test("there are no short forms: a family with no call, or an old lfs name, exits 2 with nothing called", () => {
  for (const [argv, said] of [[["sandbox"], 'unknown sandbox call ""'], [["status"], 'unknown command "status"'],
    [["run", "1", "cart", "true"], 'unknown command "run"'], [["cat", "main", "/a"], 'unknown command "cat"']]) {
    const r = cli(argv);
    assert.equal(r.code, 2, argv.join(" "));
    assert.ok(r.err.startsWith(`layerfs: ${said}`), r.err);
    assert.deepEqual(r.calls, [], argv.join(" "));
  }
  assert.deepEqual(cli(["sandbox", "create"]).ops, ["sandbox.create"]);
  assert.deepEqual(cli(["sandbox", "list-workspaces", "2"]).ops, ["sandbox.list_workspaces"]);
});

test("docs/agent.md holds the tables the call table writes", () => {
  assert.deepEqual(written(), tables());
});

// The CLI's files mirror the agent SDK's: every SDK call file has a command
// file of the same family and name, and nothing else (index.js and the SDK's
// fs/body.js helper aside). A new SDK call without its command fails here.
test("api/cli/src/<family>/<call>.js mirrors api/agent/src/<family>/<call>.js", () => {
  const SDK = fileURLToPath(new URL("../../agent/src/", import.meta.url));
  const SRC = fileURLToPath(new URL("../src/", import.meta.url));
  const calls = (root, family, skip) => readdirSync(join(root, family)).filter((f) => f.endsWith(".js") && !skip.includes(f)).sort();
  for (const family of ["project", "branch", "fs", "workspace", "sandbox"]) {
    assert.deepEqual(calls(SRC, family, ["index.js"]), calls(SDK, family, ["index.js", "body.js"]), family);
  }
  const files = [...CALLS.map((c) => `${c.family}/${c.call}.js`), "project/init.js"].sort();
  const onDisk = ["project", "branch", "fs", "workspace", "sandbox"].flatMap((f) => calls(SRC, f, ["index.js"]).map((c) => `${f}/${c}`)).sort();
  assert.deepEqual(files, onDisk, "every command file is in its family's index.js");
});

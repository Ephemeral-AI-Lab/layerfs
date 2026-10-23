// The layerfs command end to end: each test spawns `node cli/src/layerfs.js`
// with the real @layerfs/agent JavaScript over a stand-in native binding
// (sdk/js/packages/agent/test/fixture/fake-native.cjs, shared with the SDK
// tests), which logs every binding call. No Docker, Store
// or daemon; runs anywhere:
//   node --test cli/test/cli.test.mjs
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { after, test } from "node:test";
import { fileURLToPath } from "node:url";
import { CALLS, INIT } from "../src/calls.js";
import { usage } from "../src/help.js";
import { tables, written } from "./doc-tables.mjs";

const CLI = fileURLToPath(new URL("../src/layerfs.js", import.meta.url));
const FAKE = fileURLToPath(new URL("../../sdk/js/packages/agent/test/fixture/fake-native.cjs", import.meta.url));
const work = realpathSync(mkdtempSync(join(tmpdir(), "layerfs-cli-")));
after(() => rmSync(work, { recursive: true, force: true }));

const BASE = {
  PATH: process.env.PATH, HOME: process.env.HOME,
  LAYERFS_NATIVE: FAKE, LAYERFS_HOME: "/private/cli-home", LAYERFS_DOCKER: "/fake.sock",
  LAYERFS_IMAGE: "fake-image", LAYERFS_VOLUME: "fake-volume", LAYERFS_DAEMON: "/fake/daemon",
  LAYERFS_STORE_PATH: "/fake/store.sqlite", LAYERFS_PROJECT: "site",
};
let runs = 0;

/** One layerfs run: exit code, stdout, stderr and the binding calls it made. */
function cli(args, { input = "", env = {}, cwd } = {}) {
  const log = join(work, `calls-${++runs}.jsonl`);
  const r = spawnSync(process.execPath, [CLI, ...args], { input, cwd, env: { ...BASE, CLI_FAKE_LOG: log, ...env }, timeout: 20_000 });
  assert.equal(r.error, undefined, `spawn failed: ${r.error}`);
  const calls = existsSync(log) ? readFileSync(log, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l)) : [];
  // the calls the command made: no Store open/close, no LayerFS.open probe (one Branch)
  const made = calls.filter(([op, a]) => !op.startsWith("store.") && !(op === "project.list_branches" && a.pageSize === 1));
  return { code: r.status, out: r.stdout.toString(), err: r.stderr.toString(), calls, made, ops: made.map(([op]) => op) };
}

/** True when every field of `want` is in `have`, recursively. */
function contains(have, want) {
  if (want === null || typeof want !== "object") return Object.is(have, want);
  if (have === null || typeof have !== "object") return false;
  return Object.entries(want).every(([k, v]) => contains(have[k], v));
}

const source = join(work, "src-folder");
mkdirSync(source);
writeFileSync(join(source, "README.md"), "hi\n");
const WS = "1:/workspaces/w1";
const OF_WS = { sandbox: "1", path: "/workspaces/w1" };

// One case per table entry: argv after the command, then the binding op and
// the args (a subset) the SDK must send. The test below fails when a call in
// the table has no case here.
const CASES = {
  "init": { argv: ["site", "src-folder", "--timeout", "5000"], cwd: work, op: "project.init", args: { name: "site", path: resolve(work, "src-folder"), timeoutMs: 5000 } },
  "project list_branches": { argv: ["--page-size", "5", "--page-pos", "10"], op: "project.list_branches", args: { pageSize: 5, pagePos: 10 } },
  "project list_sandboxes": { argv: [], op: "project.list_sandboxes", args: { project: "site" } },
  "branch fork": { argv: ["cart"], op: "branch.fork", args: { name: "cart", from: "main" } },
  "branch list_commits": { argv: ["cart"], op: "branch.list_commits", args: { branch: "cart", pageSize: 50, pagePos: 0 } },
  "branch diff_commits": { argv: ["c1", "c2", "--hunks", "--cursor", "/a"], op: "branch.diff_commits", args: { from: "c1", to: "c2", hunks: true, cursor: "/a" } },
  "branch diff_branches": { argv: ["a", "b"], op: "branch.diff_branches", args: { from: "a", to: "b", pageSize: 50 } },
  "branch merge": { argv: ["a", "b", "ab", "-m", "merge it", "--units", '["dist/**"]'], op: "branch.merge", args: { branch: "a", with: "b", name: "ab", message: "merge it", units: ["dist/**"] } },
  "branch publish": { argv: ["cart", "c0ffee"], op: "branch.publish", args: { branch: "cart", commit: "c0ffee" } },
  "fs read": { argv: ["main", "/a.txt", "--start-line=3", "--end-line", "9"], op: "fs.read", args: { branch: "main", path: "/a.txt", startLine: 3, endLine: 9 } },
  "fs list": { argv: ["main"], op: "fs.list", args: { branch: "main", path: "/", pageSize: 50, pagePos: 0 } },
  "fs stat": { argv: ["main", "/a"], op: "fs.stat", args: { branch: "main", path: "/a" } },
  "fs grep": { argv: ["main", "TODO", "/src"], op: "fs.grep", args: { branch: "main", pattern: "TODO", path: "/src" } },
  "fs glob": { argv: ["main", "**/*.js"], op: "fs.glob", args: { branch: "main", pattern: "**/*.js", path: "/" } },
  "fs diff_files": { argv: ["main", "/a", "cart", "/b"], op: "fs.diff_files", args: { left: { commit: "main", path: "/a" }, right: { commit: "cart", path: "/b" } } },
  "fs blame": { argv: ["main", "/a", "--from-line", "4"], op: "fs.blame", args: { branch: "main", path: "/a", fromLine: 4 } },
  "fs write": { argv: ["cart", "/h.bin", "-m", "bytes"], input: Buffer.from([0, 255, 1, 2]), op: "fs.write", args: { branch: "cart", path: "/h.bin", bytes: { base64: "AP8BAg==" }, message: "bytes" } },
  "fs edit": { argv: ["cart", "/a", "--old", "x", "--new", "y"], op: "fs.edit", args: { branch: "cart", path: "/a", oldText: "x", newText: "y" } },
  "fs apply": { argv: ["cart", "-m", "two"], input: '[{"op":"write","path":"/a","content":"hi"}]', op: "fs.apply", args: { branch: "cart", changes: [{ op: "write", path: "/a", content: "hi" }], message: "two" } },
  "fs mkdir": { argv: ["cart", "/d"], op: "fs.mkdir", args: { branch: "cart", path: "/d", recursive: true } },
  "fs rm": { argv: ["cart", "/d"], op: "fs.rm", args: { branch: "cart", path: "/d" } },
  "fs cp": { argv: ["cart", "/a", "/b"], op: "fs.cp", args: { branch: "cart", from: "/a", to: "/b" } },
  "fs mv": { argv: ["cart", "/a", "/b", "-m", "move"], op: "fs.mv", args: { branch: "cart", from: "/a", to: "/b", message: "move" } },
  "workspace mount": { argv: ["1", "cart"], op: "workspace.mount", args: { sandbox: "1", branch: "cart" } },
  "workspace exec": { argv: [WS, "npm test", "-m", "step"], op: "workspace.exec", args: { ...OF_WS, command: "npm test", message: "step" } },
  "workspace commit": { argv: [WS, "--force"], op: "workspace.commit", args: { ...OF_WS, force: true } },
  "workspace unmount": { argv: [WS, "--discard"], op: "workspace.unmount", args: { ...OF_WS, discard: true } },
  "sandbox create": { argv: ["--image", "sha256:abc"], op: "sandbox.create", args: { deploy: { image: "sha256:abc" } } },
  "sandbox list_workspaces": { argv: ["2"], op: "sandbox.list_workspaces", args: { sandbox: "2" } },
  "sandbox close": { argv: ["2"], op: "sandbox.close", args: { sandbox: "2" } },
};

const key = (e) => (e.family ? `${e.family} ${e.call}` : e.call);
const command = (e) => (e.family ? [e.family, e.call] : [e.call]);

test("every table entry has a case", () => {
  assert.deepEqual([INIT, ...CALLS].map(key).sort(), Object.keys(CASES).sort());
});

for (const entry of [INIT, ...CALLS]) {
  test(`${key(entry)} makes ${CASES[key(entry)]?.op}`, () => {
    const c = CASES[key(entry)];
    const r = cli([...command(entry), ...c.argv], { input: c.input, cwd: c.cwd });
    assert.equal(r.code, 0, r.err);
    assert.equal(r.err, "");
    assert.deepEqual(r.ops, [c.op]);
    assert.ok(contains(r.made[0][1], c.args), `${c.op} got ${JSON.stringify(r.made[0][1])}`);
    JSON.parse(r.out); // one JSON value
    assert.ok(r.out.endsWith("\n") && !r.out.slice(0, -1).includes("\n"), "one line");
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

test("init prints Init's result", () => {
  const r = cli(["init", "site", source]);
  assert.deepEqual(JSON.parse(r.out), { kind: "Initialized", project: "site", layer: "lay0", root: "r00t", store: "/fake/store.sqlite" });
});

test("mount prints the Workspace handle", () => {
  assert.deepEqual(JSON.parse(cli(["workspace", "mount", "1", "cart"]).out), { sandbox: "1", path: "/workspaces/w1", image: "fake-image", ws: WS });
});

for (const [why, argv, message] of [
  ["missing positional", ["fs", "read", "main"], "missing <path>"],
  ["extra positional", ["fs", "stat", "main", "/a", "/b"], 'unexpected argument "/b"'],
  ["unknown option", ["fs", "stat", "main", "/a", "--nope"], "unknown option --nope"],
  ["unknown short option", ["fs", "stat", "main", "/a", "-x"], "unknown option -x"],
  ["duplicate option", ["fs", "mkdir", "cart", "/d", "-m", "a", "--message", "b"], "--message is given twice"],
  ["bool with a value", ["branch", "diff_branches", "a", "b", "--hunks=yes"], "--hunks takes no value"],
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

test("--project wins over LAYERFS_PROJECT; DSH_LAYERFS_PROJECT is the fallback", () => {
  assert.equal(cli(["fs", "stat", "main", "/a", "--project", "other"]).made[0][1].project, "other");
  assert.equal(cli(["--project=other", "fs", "stat", "main", "/a"]).made[0][1].project, "other");
  assert.equal(cli(["fs", "stat", "main", "/a"], { env: { LAYERFS_PROJECT: "", DSH_LAYERFS_PROJECT: "dshp" } }).made[0][1].project, "dshp");
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

test("a refusal exits 125 with JSON on stderr, or text with --text", () => {
  const r = cli(["fs", "read", "main", "/missing"]);
  assert.equal(r.code, 125);
  assert.equal(r.out, "");
  assert.deepEqual(JSON.parse(r.err), { code: "NoSuchPath", message: "/missing does not exist", detail: null });
  const t = cli(["--text", "fs", "read", "main", "/missing"]);
  assert.deepEqual([t.code, t.err], [125, "NoSuchPath: /missing does not exist\n"]);
  const d = cli(["branch", "merge", "a", "b", "ab"], { env: { CLI_FAKE_REFUSE: "branch.merge" } });
  assert.deepEqual(JSON.parse(d.err), { code: "Conflict", message: "branch.merge refused by the fake", detail: { op: "branch.merge" } });
});

test("an SDK input refusal is 125, not a usage error", () => {
  const r = cli(["workspace", "mount", "0", "cart"]);
  assert.equal(r.code, 125);
  assert.equal(JSON.parse(r.err).code, "InvalidInput");
});

test("exec returns the command's exit code in JSON; --stream passes output and code through", () => {
  const plain = cli(["workspace", "exec", WS, "exit 7"]);
  assert.equal(plain.code, 0);
  assert.ok(contains(JSON.parse(plain.out), { exitCode: 7, stdout: "out:exit 7", stderr: "err" }));
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
  assert.deepEqual([cli(["--version"]).out, cli(["--version"]).code], ["1.0.0\n", 0]);
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

test("sdk-to-cli.md holds the tables the call table writes", () => {
  assert.deepEqual(written(), tables());
});

// The CLI's files mirror the agent SDK's: every SDK call file has a command
// file of the same family and name, and nothing else (index.js and the SDK's
// fs/body.js helper aside). A new SDK call without its command fails here.
test("cli/src/<family>/<call>.js mirrors sdk/js/packages/agent/src/<family>/<call>.js", () => {
  const SDK = fileURLToPath(new URL("../../sdk/js/packages/agent/src/", import.meta.url));
  const SRC = fileURLToPath(new URL("../src/", import.meta.url));
  const calls = (root, family, skip) => readdirSync(join(root, family)).filter((f) => f.endsWith(".js") && !skip.includes(f)).sort();
  for (const family of ["project", "branch", "fs", "workspace", "sandbox"]) {
    assert.deepEqual(calls(SRC, family, ["index.js"]), calls(SDK, family, ["index.js", "body.js"]), family);
  }
  const files = [...CALLS.map((c) => `${c.family}/${c.call}.js`), "project/init.js"].sort();
  const onDisk = ["project", "branch", "fs", "workspace", "sandbox"].flatMap((f) => calls(SRC, f, ["index.js"]).map((c) => `${f}/${c}`)).sort();
  assert.deepEqual(files, onDisk, "every command file is in its family's index.js");
});

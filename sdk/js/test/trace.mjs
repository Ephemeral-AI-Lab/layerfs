// Per-call trace checks over a stand-in binding; each case is one command:
//   node sdk/js/test/trace.mjs on|off|failure|dsh-env|start|ids|no-content|store|daemon-host
// No Docker, Store or daemon is used.
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const FAKE = fileURLToPath(new URL("./fixture/fake-native.cjs", import.meta.url));
Object.assign(process.env, {
  LAYERFS_NATIVE: FAKE, LAYERFS_HOME: "/private/trace-home", LAYERFS_DOCKER: "/fake.sock",
  LAYERFS_IMAGE: "fake-image", LAYERFS_VOLUME: "fake-volume", LAYERFS_DAEMON: "/fake/daemon",
  LAYERFS_STORE_PATH: "/fake/store.sqlite",
});
for (const name of ["TRACE", "TRACE_STEP", "TRACE_START", "DAEMON_HOST"]) {
  delete process.env[`LAYERFS_${name}`];
  delete process.env[`DSH_LAYERFS_${name}`];
}
const { LayerFS } = await import("@layerfs/sdk");
const { LayerFSOperator } = await import("@layerfs/sdk/operator");

const BIG = "x".repeat(10000);
const WS = { sandbox: "1", path: "/workspaces/w1" };
/** Every SDK group once, plus one refusal; returns each call's value or code. */
const pick = (line, fields) => Object.fromEntries(fields.filter((field) => field in line).map((field) => [field, line[field]]));

async function script(client) {
  const op = new LayerFSOperator(client);
  const steps = [
    () => client.project.init("/src/website"),
    () => client.project.list_branches(),
    () => client.fs.read("main", "/README.md", 1, 20),
    () => client.fs.list("main", "/"),
    () => client.fs.stat("main", "/README.md"),
    () => client.fs.grep("main", "TODO"),
    () => client.fs.glob("main", "**/*.js"),
    () => client.fs.diff_files("main", "/a", "feature", "/a"),
    () => client.fs.blame("main", "/README.md"),
    () => client.fs.write("feature", "/big.txt", BIG, "Write big"),
    () => client.fs.edit("feature", "/big.txt", "xx", "yy"),
    () => client.fs.apply("feature", [{ op: "write", path: "/a", content: BIG }, { op: "rm", path: "/b" }]),
    () => client.fs.mkdir("feature", "/dir"),
    () => client.fs.rm("feature", "/dir"),
    () => client.fs.cp("feature", "/a", "/c"),
    () => client.fs.mv("feature", "/c", "/d"),
    () => client.branch.fork("feature"),
    () => client.branch.list_commits("feature"),
    () => client.branch.diff_commits("a", "b"),
    () => client.branch.diff_branches("main", "feature"),
    () => client.branch.merge("main", "feature", "merged"),
    () => client.branch.publish("feature"),
    () => client.workspace.mount("feature"),
    () => client.workspace.exec(WS, "npm test"),
    () => client.workspace.status(WS),
    () => client.workspace.commit(WS, "Save"),
    () => client.workspace.unmount(WS),
    () => client.fs.read("main", "/missing"),
    () => op.store.stats(),
    () => op.store.commits("feature"),
    () => op.files.stats("main", "/README.md"),
    () => op.files.history("main", "/README.md"),
    () => op.workspace.overlay(WS),
    () => op.workspace.changes(WS),
    () => op.inspect.snapshot(),
    () => op.process.start(WS, "npm run dev"),
    () => op.process.list(),
    () => op.process.cancel("proc_1_ab"),
  ];
  const results = [];
  for (const step of steps) results.push(await step().then((value) => ({ value }), (error) => ({ code: error.code })));
  return results;
}

const read = (path) => readFileSync(path, "utf8").trimEnd().split("\n").map((line) => JSON.parse(line));

const dir = mkdtempSync(join(tmpdir(), "layerfs-trace-"));
const file = join(dir, "trace.jsonl");
const selected = process.argv[2];
try {
  if (selected === "on") {
    const reference = await script(new LayerFS("website"));
    const native = (await import(FAKE)).default;
    const before = native.calls.length;
    assert.ok(native.observed.every((scope) => scope === null), "untraced calls carry no scope");
    process.env.LAYERFS_TRACE = file;
    const traced = await script(new LayerFS("website"));
    assert.deepEqual(traced, reference, "tracing changes no value or refusal");
    const lines = readFileSync(file, "utf8").trimEnd().split("\n").map((line) => JSON.parse(line));
    assert.equal(lines.length, native.calls.length - before, "one line per native call");
    assert.deepEqual(lines.map((line) => line.method), native.calls.slice(before));
    for (const line of lines) {
      assert.equal(line.layerfs_trace, 1);
      assert.match(line.id, /^[0-9a-f]{8}-\d+$/);
      assert.ok(Number.isFinite(line.ms) && line.ms >= 0);
      assert.ok(!Number.isNaN(Date.parse(line.start)));
      assert.equal(typeof line.ok, "boolean");
      assert.equal(line.args.deploy, undefined);
      assert.equal(line.args.home, undefined);
      assert.match(line.scope, /^[0-9a-f]{64}$/);
      assert.equal(line.args.observe, undefined);
    }
    assert.deepEqual(lines.map((line) => line.scope), native.observed.slice(before), "the binding got each line's scope");
    assert.equal(new Set(lines.map((line) => line.id)).size, lines.length);
    const groups = new Set(lines.map((line) => line.method.split(".").slice(0, line.method.startsWith("backend.") ? 3 : 1).join(".")));
    for (const group of ["project", "fs", "branch", "workspace", "sandbox", "backend.store.project", "backend.store.branch", "backend.store.fs", "backend.daemon.workspace", "backend.daemon.process"]) assert.ok(groups.has(group), group);
    const write = lines.find((line) => line.method === "fs.write");
    assert.deepEqual(write.args, { branch: "feature", path: "/big.txt", contentBytes: 10000, message: "Write big" });
    assert.equal(write.commit, "c0ffee");
    assert.deepEqual(write.sizes, { logical: 120, newCanonical: 40, newStored: 30, newMetadata: 8 });
    const apply = lines.find((line) => line.method === "fs.apply");
    assert.equal(apply.args.changes, 2);
    assert.equal(apply.args.changeBytes, 10000);
    assert.ok(!readFileSync(file, "utf8").includes("xxxx"), "no file contents in the trace");
    const commit = lines.find((line) => line.method === "workspace.commit");
    assert.equal(commit.kind, "Committed");
    assert.equal(commit.commit, "c0ffee");
    const [tree] = commit.daemon[0].timing;
    assert.equal(tree.name, "commit");
    assert.equal(tree.ms, 2.5);
    assert.deepEqual(tree.children.map((child) => child.name), ["capture", "construct"]);
    assert.equal(tree.children[1].children[0].name, "captured.namespace");
    assert.equal(commit.daemon[0].storage.pack_write_bytes, 4096);
    assert.equal(commit.daemon[0].construction.files_constructed, 1);
    // SQL work passes through on every exchange; opcode counts on status and unmount.
    assert.deepEqual(commit.daemon[0].sql, { writer: { statements: 3, fullscan_steps: 0 }, readers: { statements: 2, vm_steps: 40 } });
    for (const line of lines.filter((line) => line.daemon)) {
      assert.equal("fuse" in line.daemon[0], ["backend.daemon.workspace.status", "workspace.unmount"].includes(line.method), line.method);
    }
    assert.equal(lines.find((line) => line.method === "backend.daemon.workspace.status").daemon[0].fuse.getattr, 7);
    const refused = lines.find((line) => line.args.path === "/missing");
    assert.equal(refused.ok, false);
    assert.equal(refused.code, "NoSuchPath");
    assert.match(refused.message, /does not exist/);
    // Every publishing call names its Commit; other calls name none.
    assert.equal(lines.find((line) => line.method === "branch.merge").commit, "m3rge");
    const publish = lines.find((line) => line.method === "branch.publish");
    assert.deepEqual([publish.commit, publish.layer], ["pub1", "lay1"]);
    const rm = lines.find((line) => line.method === "fs.rm");
    assert.deepEqual([rm.kind, rm.commit], ["UpToDate", "h3ad"]);
    const PUBLISHING = ["fs.write", "fs.edit", "fs.apply", "fs.mkdir", "fs.rm", "fs.cp", "fs.mv", "workspace.commit", "branch.merge", "branch.publish"];
    for (const line of lines) assert.equal("commit" in line, PUBLISHING.includes(line.method) && line.ok, line.method);
    // Unset, no line has a step.
    assert.ok(lines.every((line) => !("step" in line)));
    // The step label is read at each call, bounded, and the option wins.
    const stepped = join(dir, "step.jsonl");
    const labelled = new LayerFS("website", { trace: stepped });
    for (const label of ["install deps", "s".repeat(300), undefined, ""]) {
      if (label === undefined) delete process.env.LAYERFS_TRACE_STEP; else process.env.LAYERFS_TRACE_STEP = label;
      await labelled.project.list_branches();
    }
    labelled.options.traceStep = "option step";
    process.env.LAYERFS_TRACE_STEP = "environment step";
    await labelled.project.list_branches();
    delete process.env.LAYERFS_TRACE_STEP;
    const steps = readFileSync(stepped, "utf8").trimEnd().split("\n").map((line) => JSON.parse(line)).filter((line) => line.method === "project.list_branches").map((line) => line.step);
    assert.deepEqual(steps, ["install deps", `${"s".repeat(200)}…`, undefined, undefined, "option step"]);
    // The client option overrides the environment.
    const other = join(dir, "option.jsonl");
    await new LayerFS("website", { trace: other }).project.list_branches();
    assert.equal(readFileSync(other, "utf8").trimEnd().split("\n").length, 1, "a Store call needs no Sandbox");
  } else if (selected === "off") {
    // A step label alone traces nothing.
    process.env.LAYERFS_TRACE_STEP = "unused step";
    const results = await script(new LayerFS("website"));
    assert.equal(results.at(27).code, "NoSuchPath");
    assert.equal(existsSync(file), false);
    assert.deepEqual(readdirSync(dir), []);
  } else if (selected === "failure") {
    // A trace path that cannot be written leaves every result unchanged.
    const reference = await script(new LayerFS("website"));
    const traced = await script(new LayerFS("website", { trace: join(dir, "absent/dir/trace.jsonl") }));
    assert.deepEqual(traced, reference);
  } else if (selected === "dsh-env") {
    // With only the DSH_ names set (as dsh sets them in a bash call), the
    // same lines appear, labelled by DSH_LAYERFS_TRACE_STEP.
    process.env.DSH_LAYERFS_TRACE = file;
    process.env.DSH_LAYERFS_TRACE_STEP = "call_7";
    const results = await script(new LayerFS("website"));
    const lines = read(file);
    assert.equal(lines.length, results.length + 2, "one line per call; the read model makes three: history, the Sandbox listing and the one Sandbox's Workspaces");
    assert.ok(lines.every((line) => line.step === "call_7" && line.phase === "end"));
    // The LAYERFS_ names win over their DSH_ twins.
    const own = join(dir, "own.jsonl");
    process.env.LAYERFS_TRACE = own;
    process.env.LAYERFS_TRACE_STEP = "own step";
    await new LayerFS("website").project.list_branches();
    assert.deepEqual(read(own).map((line) => line.step), ["own step"]);
    assert.equal(read(file).length, lines.length);
    // Deployment settings fall back to their DSH_ twins the same way.
    const saved = {};
    for (const name of ["HOME", "DOCKER", "IMAGE", "VOLUME", "DAEMON"]) {
      saved[name] = process.env[`LAYERFS_${name}`];
      delete process.env[`LAYERFS_${name}`];
      process.env[`DSH_LAYERFS_${name}`] = `dsh-${name.toLowerCase()}`;
    }
    const twin = new LayerFS("website");
    assert.equal(twin.options.home, "dsh-home");
    assert.deepEqual(twin.options.deploy, { docker: "dsh-docker", image: "dsh-image", volume: "dsh-volume", daemon: "dsh-daemon" });
    for (const [name, value] of Object.entries(saved)) { process.env[`LAYERFS_${name}`] = value; delete process.env[`DSH_LAYERFS_${name}`]; }
    assert.equal(new LayerFS("website").options.deploy.image, "fake-image");
  } else if (selected === "start") {
    // Start lines precede their end lines and share id, step, method, args, start and scope.
    process.env.LAYERFS_TRACE = file;
    process.env.DSH_LAYERFS_TRACE_START = "1";
    process.env.LAYERFS_TRACE_STEP = "with starts";
    const results = await script(new LayerFS("website"));
    const lines = read(file);
    assert.equal(lines.length, 2 * (results.length + 2));
    const starts = lines.filter((line) => line.phase === "start");
    const ends = lines.filter((line) => line.phase === "end");
    assert.equal(starts.length, ends.length);
    for (const begin of starts) {
      const end = ends.find((line) => line.id === begin.id);
      assert.ok(lines.indexOf(begin) < lines.indexOf(end), begin.method);
      for (const field of ["layerfs_trace", "step", "method", "args", "start", "scope"]) assert.deepEqual(begin[field], end[field], field);
      for (const field of ["ms", "ok", "code", "commit", "daemon"]) assert.ok(!(field in begin), field);
    }
    // Only "1" turns them on.
    const other = join(dir, "other.jsonl");
    process.env.DSH_LAYERFS_TRACE_START = "true";
    await new LayerFS("website", { trace: other }).project.list_branches();
    assert.ok(read(other).every((line) => line.phase === "end"));
  } else if (selected === "ids") {
    // Lines name what a call made: Branches, heads, Workspaces and their Sandboxes, processes, exit codes.
    process.env.LAYERFS_TRACE = file;
    await script(new LayerFS("website"));
    const lines = read(file);
    const of = (method) => lines.find((line) => line.method === method);
    assert.equal(lines.filter((line) => line.method === "workspace.mount").length, 1, "a snapshot mounts nothing");
    assert.deepEqual(pick(of("branch.fork"), ["branch", "base", "head"]), { branch: "feature", base: "lay0", head: "not-a-publication" });
    assert.deepEqual(pick(of("branch.merge"), ["branch", "head", "commit"]), { branch: "merged", head: "m3rge", commit: "m3rge" });
    assert.deepEqual(pick(of("workspace.mount"), ["sandbox", "path", "branch", "base", "baseLayer"]), { sandbox: "1", path: "/workspaces/w1", branch: "feature", base: "b4se", baseLayer: "lay0" });
    assert.equal(of("workspace.exec").exitCode, 3);
    assert.equal(of("backend.daemon.process.start").process, "proc_1_ab");
    assert.equal(of("backend.daemon.process.cancel").process, undefined, "a cancel names its process in args only");
    // Calls that make nothing carry no result ids.
    const IDS = ["branch", "base", "head", "sandbox", "path", "baseLayer", "process", "exitCode"];
    for (const method of ["fs.read", "fs.list", "backend.daemon.workspace.status", "workspace.unmount", "backend.store.project.get_storage"]) {
      assert.deepEqual(pick(of(method), IDS), {}, method);
    }
  } else if (selected === "no-content") {
    // File content, exec output and the root hash never reach a line, with or without start lines.
    process.env.LAYERFS_TRACE = file;
    process.env.LAYERFS_TRACE_START = "1";
    await script(new LayerFS("website"));
    const text = readFileSync(file, "utf8");
    assert.ok(!text.includes("xxxx"), "no file content or exec output");
    assert.ok(!text.includes("r00t"), "no root hash");
    for (const line of read(file)) {
      for (const field of ["stdout", "stderr", "content", "root", "value"]) assert.ok(!(field in line), `${line.method} ${field}`);
    }
  } else if (selected === "store") {
    // Store calls open the client's Store in this process once, and close()
    // releases it; no Sandbox is started for them (#352 F-020).
    const native = (await import(FAKE)).default;
    const client = new LayerFS("website");
    await client.project.list_branches();
    await client.fs.read("main", "/README.md");
    assert.deepEqual(native.stores, [["open", "/fake/store.sqlite"]]);
    assert.deepEqual(native.calls, ["project.list_branches", "fs.read"]);
    assert.ok(native.args.every((args) => args.storePath === "/fake/store.sqlite" && !("sandbox" in args)));
    await client.close();
    assert.deepEqual(native.stores.at(-1), ["close", "/fake/store.sqlite"]);
    // Without a Store path a Store call is refused before the binding.
    delete process.env.LAYERFS_STORE_PATH;
    const calls = native.calls.length;
    await assert.rejects(new LayerFS("website").fs.read("main", "/README.md"), { code: "InvalidInput", message: /storePath/ });
    assert.equal(native.calls.length, calls);
    // A client's Store policy goes with its Init only.
    process.env.LAYERFS_STORE_PATH = "/fake/store.sqlite";
    await new LayerFS("website3", { storePolicy: { wholeFileDepth: 8, chunkDepth: 4 } }).project.init("/src/website");
    assert.deepEqual(native.args.at(-1).policy, { wholeFileDepth: 8, chunkDepth: 4 });
    await new LayerFS("website3").project.init("/src/website");
    assert.equal("policy" in native.args.at(-1), false);
    // A snapshot reads history from the Store in this process and starts no
    // Sandbox: with none recorded it asks no daemon.
    native.sandboxList = [];
    const reader = new LayerFS("website");
    const before = native.calls.length;
    const snap = await new LayerFSOperator(reader).inspect.snapshot();
    assert.deepEqual(native.calls.slice(before), ["backend.store.project.get_history", "project.list_sandboxes"]);
    assert.equal(native.args[before].storePath, "/fake/store.sqlite");
    assert.deepEqual([snap.kind, snap.sandboxes, snap.gone], ["ok", [], []]);
  } else if (selected === "daemon-host") {
    // A client in a container dials its daemonHost on every call that
    // reaches a Sandbox, operator calls included: the binding reopens or
    // starts the Sandbox with the call's deploy.
    const native = (await import(FAKE)).default;
    await script(new LayerFS("website", { daemonHost: "host.docker.internal" }));
    const named = native.calls.filter((_, index) => "sandbox" in native.args[index]);
    for (const method of ["workspace.exec", "backend.daemon.workspace.status", "workspace.commit", "workspace.unmount", "backend.daemon.workspace.get_storage", "backend.daemon.workspace.list_changes", "sandbox.list_workspaces", "backend.daemon.process.start"]) {
      assert.ok(named.includes(method), method);
    }
    for (const [index, args] of native.args.entries()) {
      const method = native.calls[index];
      if ("sandbox" in args || method.startsWith("workspace.") || method.startsWith("backend.daemon.") || method === "project.list_sandboxes") {
        assert.equal(args.deploy?.daemonHost, "host.docker.internal", method);
      }
    }
  } else {
    console.error("usage: trace.mjs on|off|failure|dsh-env|start|ids|no-content|store|daemon-host");
    process.exit(2);
  }
  console.log(`PASS trace ${selected}`);
} finally {
  rmSync(dir, { recursive: true, force: true });
}


// Host plugin checks without dsh, Docker or a Store:
//   node dsh/packages/host/tests/host.mjs
// dsh's appendPluginRecord is replaced by a stand-in that keeps each record
// on the fake Session; Sessions, the web server and Connection are fakes.
import assert from "node:assert/strict";
import { appendFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { register } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";

const STAND_IN = "export function appendPluginRecord(session, type, data) { session.records.push({ type, data }); }";
register(`data:text/javascript,${encodeURIComponent(`
export async function resolve(specifier, context, next) {
  if (specifier === "@deepseek-ai/dsh-session") return { url: ${JSON.stringify(`data:text/javascript,${encodeURIComponent(STAND_IN)}`)}, shortCircuit: true };
  return next(specifier, context);
}`)}`);
const { resolveConfig } = await import("../src/config.js");
const { BindingStore } = await import("../src/binding.js");
const { Collector } = await import("../src/collector.js");
const { contributor, projectContext } = await import("../src/env.js");
const { registerRoutes } = await import("../src/route.js");
const { TreeReader } = await import("../src/trees.js");
const { removeProject } = await import("../src/remove.js");
const { migrate } = await import("../src/migrate.js");
const { UNREACHABLE } = await import("@layerfs/port");
const { hostAccess, prepareInit, randomName, scanSource, watching } = await import("../src/init.js");
const { InitJobs } = await import("../src/jobs.js");
const { CALL, PROJECT } = await import("@dsh-layerfs/model/sources/records.js");

const dir = mkdtempSync(join(tmpdir(), "dsh-layerfs-host-"));
const FOLDER = "/work/website";
const tick = () => new Promise((resolve) => setImmediate(resolve));

function session(id, cwd, parentSession) {
  return { header: parentSession ? { id, cwd, parentSession, origin: "subagent" } : { id, cwd }, records: [] };
}

function host() {
  const spec = resolveConfig({ image: "sha256:img", daemon: "/d/layerfs-daemon", bundle: "/b", home: join(dir, "home"), traceDir: join(dir, "traces"), volumes: join(dir, "volumes") }, { dir: join(dir, "profile") });
  mkdirSync(spec.volumes, { recursive: true });
  const live = new Map();
  const ctx = { sessions: { get: (id) => live.get(id) } };
  const bindings = new BindingStore(spec.stateDir);
  const collector = new Collector(ctx, spec, bindings);
  return { spec, live, ctx, bindings, collector, env: contributor(ctx, spec, bindings, collector) };
}

const trace = (id, method, phase, extra = {}) => JSON.stringify({ layerfs_trace: 1, id, step: "call_1", method, args: {}, start: "2026-10-16T00:00:00.000Z", phase, ...(phase === "end" ? { ms: 1, ok: true, daemon: { t: 1 }, scope: "x" } : {}), ...extra }) + "\n";

try {
  const h = host();
  const root = session("session-root", FOLDER), child = session("child-1", FOLDER, "session-root");
  h.live.set(root.header.id, root).set(child.header.id, child);

  // a session without a project gets no LayerFS environment
  assert.deepEqual(h.env.resolve({ agent: { session: root }, callId: "c0" }), {});
  assert.equal(h.bindings.session("session-root"), null);

  // the folder's only project is used, locked at the first bash call and recorded on the root
  h.bindings.addProject(FOLDER, { name: "website", source: FOLDER, volume: "dsh-layerfs-website-1", created: 1 });
  // the runtime context names it (a subagent sees its root's), before any bash call
  assert.match(projectContext(h.bindings, h.collector, child), /^LayerFS project of this conversation: website \(live\)\./);
  assert.equal(projectContext(h.bindings, h.collector, undefined), "");
  const vars = h.env.resolve({ agent: { session: child }, callId: "c1", rootCallId: "r1" });
  assert.equal(vars.DSH_LAYERFS_PROJECT, "website");
  assert.equal(vars.DSH_LAYERFS_VOLUME, "dsh-layerfs-website-1");
  assert.equal(vars.DSH_LAYERFS_STORE_PATH, "/var/lib/docker/volumes/dsh-layerfs-website-1/_data/global/store.sqlite");
  if (process.platform === "darwin") {
    // macOS: the SDK is ported, its calls run in the Store container (@layerfs/port)
    assert.match(vars.DSH_LAYERFS_SDK, /\/port\/src\/sdk\.js$/);
    assert.equal(vars.DSH_LAYERFS_DAEMON_HOST, "host.docker.internal");
    assert.equal(vars.DSH_LAYERFS_STORE_CONTAINER, "layerfs-store", "one per home, named by its prefix");
    assert.equal(vars.DSH_LAYERFS_BUNDLE, "/b");
  }
  assert.equal(vars.DSH_LAYERFS_IMAGE, "sha256:img");
  assert.equal(vars.DSH_LAYERFS_CONTAINER_PREFIX, "layerfs-", "Sandboxes are layerfs-<project>-<n>");
  assert.match(vars.DSH_LAYERFS_OPERATOR, process.platform === "darwin" ? /^file:.*\/port\/src\/operator\.js$/ : /^file:.*\/operator\/index\.js$/);
  assert.equal(vars.DSH_LAYERFS_TRACE_STEP, "r1");
  assert.equal(vars.DSH_LAYERFS_TRACE, join(h.spec.traceDir, "child-1.jsonl"));
  assert.ok(Object.keys(vars).every((key) => key in h.env.variables), "every value is a declared variable");
  assert.deepEqual(h.bindings.session("session-root"), { project: "website", locked: true, folder: FOLDER });
  assert.equal(h.bindings.choose("session-root", FOLDER, null), false, "a locked choice is refused");
  assert.deepEqual(root.records.map((r) => r.type), [PROJECT]);
  h.env.resolve({ agent: { session: root }, callId: "c2" });
  assert.equal(root.records.length, 1, "the project is recorded once");

  // the parent's catalog names the child; the child's files land in its own Session
  h.collector.observe(root, { type: "subagent/catalog", data: { childId: "child-1", label: " tester " } });
  assert.equal(h.collector.agentOf(child).name, "tester");
  assert.equal(h.collector.agentOf(root).name, "main");
  appendFileSync(vars.DSH_LAYERFS_TRACE, trace("a-1", "workspace.mount", "start"));
  h.collector.poll();
  assert.equal(h.collector.snapshot("session-root").running.length, 1);
  appendFileSync(vars.DSH_LAYERFS_TRACE, trace("a-1", "workspace.mount", "end", { sandbox: "1" }) + trace("a-2", "workspace.exec", "end").slice(0, 20));
  h.collector.poll();
  const snap = h.collector.snapshot("session-root");
  assert.deepEqual([snap.running.length, snap.lines.length, snap.lines[0].agent], [0, 1, "tester"]);
  assert.deepEqual(snap.agents, { "session-root": "main", "child-1": "tester" });
  const [record] = child.records;
  assert.equal(record.type, CALL);
  assert.deepEqual(record.data.agent, { id: "child-1", name: "tester" });
  assert.equal(record.data.project, "website");
  assert.ok(!("daemon" in record.data.line) && !("scope" in record.data.line), "telemetry stays out of records");

  // a partial line waits for its newline; a Session that is not live gets its records later
  h.live.delete("child-1");
  appendFileSync(vars.DSH_LAYERFS_TRACE, trace("a-2", "workspace.exec", "end").slice(20));
  h.collector.afterEvent("child-1");
  await tick();
  assert.equal(child.records.length, 1);
  assert.equal(h.collector.snapshot("session-root").lines.length, 2);
  h.live.set("child-1", child);
  h.collector.observe(child, { type: "turn/start", data: {} });
  await tick();
  assert.deepEqual(child.records.map((r) => r.data.line.method), ["workspace.mount", "workspace.exec"]);

  // a new Host loads earlier lines for the views without recording them again
  const again = host();
  again.live.set("child-1", child);
  assert.equal(again.collector.snapshot("session-root").lines.length, 2);
  again.collector.track({ id: "child-1", root: "session-root" }, { name: "website" });
  again.collector.poll();
  assert.equal(child.records.length, 2);
  assert.equal(JSON.parse(readFileSync(join(h.spec.stateDir, "bindings.json"), "utf8")).agents["child-1"].name, "tester");

  // finished lines outlive the temporary trace files through the journal
  rmSync(h.spec.traceDir, { recursive: true, force: true });
  assert.deepEqual(host().collector.snapshot("session-root").lines.map((l) => l.method), ["workspace.mount", "workspace.exec"]);

  // Files reads (reads.js) on a fake client: a folder page, a file window, a binary
  // file's size (the SDK refuses its text), changed paths and one file's diff
  const REF = "a".repeat(64), reads = [], closed = [];
  const fakeFs = {
    async list(ref, dir) {
      const items = { "/": [{ path: "/node_modules", type: "dir", inode: 2 }, { path: "/src", type: "dir", inode: 3 }, { path: "/big.txt", type: "file", inode: 4 }, { path: "/.DS_Store", type: "file", inode: 7 }], "/src": [{ path: "/src/a.js", type: "file", inode: 5 }, { path: "/src/link", type: "symlink", inode: 6 }], "/node_modules": [{ path: "/node_modules/dep.js", type: "file", inode: 8 }] }[dir] ?? [];
      return { items, hasMore: false };
    },
    async read(ref, path, line) {
      reads.push(path);
      if (path === "/.DS_Store") throw Object.assign(new Error("Binary: text contains NUL or invalid UTF-8"), { code: "Binary" });
      return path === "/big.txt" ? { text: "x".repeat(100_000), nextLine: line + 1 } : { text: "ok\n", nextLine: null };
    },
    async stat() { return { size: 6148 }; },
    async blame(ref, path, from) { return { lines: [{ n: from, text: "ok", commit: ref, message: "m", lineTruncated: false }], versions: 1, nextLine: null }; },
    async diff_files(from, path, to) { return { kind: "Text", hunks: [{ op: "+", text: "ok" }], hunksOmitted: false }; },
  };
  const diffed = [];
  // the root each Commit's diff answer reports: "f…" and "g…" are Commits of retired Branches, not in the snapshot
  const rootOf = { ["a".repeat(64)]: "r1", ["b".repeat(64)]: "r0", ["f".repeat(64)]: "r7", ["0".repeat(64)]: "r6" };
  const fakeBranch = { async diff_commits(from, to, size, position, options) { diffed.push([size, position, options]); return { from: { root: rootOf[from] }, to: { root: rootOf[to] }, items: [{ path: "/src/a.js", kind: "Modified", type: "file", hunks: [] }], hasMore: true, nextPagePos: 1000 }; } };
  const trees = new TreeReader(h.spec, async () => ({ layerfs: { fs: fakeFs, branch: fakeBranch, close: async () => closed.push("store") },
    operator: { inspect: { snapshot: async () => ({ commits: [{ id: "a".repeat(64), root: "r1" }, { id: "b".repeat(64), root: "r0" }], layers: [{ id: "c".repeat(64), root: "r1" }, { id: "d".repeat(64), root: "r9" }, { id: "e".repeat(64), root: "r7", sourceCommit: "f".repeat(64) }, { id: "9".repeat(64), root: "r8", sourceCommit: "0".repeat(64) }] }) } } }));

  // the route answers only requests Connection accepts
  let handler;
  const routeCtx = { ...h.ctx, effect: (fn) => fn(), webServer: { register: (route) => { handler = route.handler; return () => {}; } }, connection: { requestRejection: (req) => (req.headers.cookie ? undefined : 401) } };
  const runs = [];
  const jobs = new InitJobs((prepared, report) => new Promise((resolve, reject) => runs.push({ prepared, report, resolve, reject })));
  registerRoutes(routeCtx, { spec: h.spec, bindings: h.bindings, collector: h.collector, trees, jobs });
  const call = async (method, url, body, cookie = "dsh=1") => {
    const res = { statusCode: 0, headers: {}, text: "", setHeader(k, v) { this.headers[k] = v; }, end(text = "") { this.text = text; } };
    const req = { method, url, headers: cookie ? { cookie } : {}, async *[Symbol.asyncIterator]() { if (body) yield Buffer.from(JSON.stringify(body)); } };
    await handler(req, res);
    return { status: res.statusCode, body: res.text ? JSON.parse(res.text) : null };
  };
  assert.equal((await call("GET", "/layerfs/status", null, null)).status, 401);
  assert.deepEqual((await call("GET", "/layerfs/status")).body.missing, []);
  assert.equal((await call("GET", `/layerfs/workspace?folder=${encodeURIComponent(FOLDER)}`)).body.lastUsed, "website");
  const view = await call("GET", "/layerfs/session/child-1");
  assert.deepEqual([view.status, view.body.root, view.body.project.name, view.body.lines.length], [200, "session-root", "website", 2]);
  assert.equal((await call("POST", "/layerfs/session/session-root/project", { project: null })).status, 409);
  const fresh = session("session-new", FOLDER);
  h.live.set(fresh.header.id, fresh);
  const chose = await call("POST", "/layerfs/session/session-new/project", { project: null });
  assert.deepEqual([chose.status, chose.body.project, chose.body.lines], [200, null, []]);
  h.live.clear(); // Sessions that are not loaded: the binding keeps the folder
  const cold = await call("GET", "/layerfs/session/child-1");
  assert.deepEqual([cold.body.root, cold.body.folder, cold.body.project.name, cold.body.agents["child-1"]], ["session-root", FOLDER, "website", "tester"]);
  assert.equal((await call("GET", "/layerfs/nope")).status, 404);
  // Files reads: one SDK call each and kept; bad arguments are refused before any call
  const B = "b".repeat(64), files = (q) => call("GET", `/layerfs/session/child-1/${q}`);
  const page = await files(`list?ref=${REF}&dir=/src`);
  assert.deepEqual([page.status, page.body.items.map((i) => i.path), page.body.next], [200, ["/src/a.js", "/src/link"], null]);
  const before = reads.length;
  assert.deepEqual((await files(`read?ref=${REF}&path=/src/a.js`)).body, { text: "ok\n", startLine: 1, nextLine: null, lineTruncated: false, size: null });
  await files(`read?ref=${REF}&path=/src/a.js`);
  assert.equal(reads.length, before + 1, "a window is read once");
  assert.deepEqual((await files(`read?ref=${REF}&path=/.DS_Store`)).body, { binary: true, size: 6148 });
  assert.deepEqual((await files(`blame?ref=${REF}&path=/src/a.js&line=3`)).body, { lines: [{ n: 3, text: "ok", lineTruncated: false, commit: REF, message: "m" }], versions: 1, nextLine: null });
  const changes = await files(`changes?from=${B}&to=${REF}`);
  assert.deepEqual([changes.body, diffed], [{ items: [{ path: "/src/a.js", kind: "Modified", type: "file" }], next: 1000 }, [[1000, 0, { hunks: false }]]]);
  // a Layer is compared as the Commit with its files; one with none is refused
  assert.deepEqual((await files(`changes?from=${"c".repeat(64)}&to=${REF}`)).body, { items: [], next: null });
  assert.equal(diffed.length, 1, "same files: no diff call");
  assert.deepEqual([(await files(`changes?from=${"d".repeat(64)}&to=${REF}`)).status, (await files(`changes?from=${"d".repeat(64)}&to=${REF}`)).body.code], [409, "NotComparable"]);
  // a Layer whose source Commit is not listed is compared as that Commit when the diff shows its root; a merged publish is refused
  assert.equal((await files(`changes?from=${"e".repeat(64)}&to=${REF}`)).body.items.length, 1);
  assert.equal((await files(`changes?from=${"9".repeat(64)}&to=${REF}`)).body.code, "NotComparable");
  assert.equal((await call("GET", `/layerfs/project/website/diff?from=${B}&to=${REF}&path=/src/a.js`)).body.kind, "Text");
  for (const q of ["list?ref=nope", `read?ref=${REF}&path=/a/../b`, `read?ref=${REF}&path=/a&line=0`, `changes?from=${B}`])
    assert.deepEqual([(await files(q)).status, (await files(q)).body.code], [400, "invalid-input"], q);
  await trees.dispose();
  assert.deepEqual(closed, ["store"], "releasing a client closes its Store; Sandboxes close themselves when idle");

  // state reads go through the same per-project client: one client, not one per read
  let opened = 0, snapshots = 0;
  const empty = { project: "p", branches: [], commits: [], layers: [], sandboxes: [], store: {} };
  const shared = new TreeReader(h.spec, async () => { opened++; return { layerfs: { fs: fakeFs, close: async () => {} }, operator: { inspect: { snapshot: async () => (snapshots++, empty) } } }; });
  const P = { name: "p" };
  const [stateA, stateB] = await Promise.all([shared.state(P), shared.state(P), shared.files(P, "list", new URLSearchParams({ ref: REF }))]);
  assert.deepEqual([opened, snapshots, typeof stateA.s, typeof stateB.labels], [1, 1, "object", "object"], "one client, and one snapshot for reads made while it runs");
  await shared.dispose();
  // a snapshot whose Sandbox does not answer: History is drawn from the Store
  // alone after sandboxMs, and later reads share the running snapshot
  {
    let asked = 0, answer;
    const stuck = new Promise((resolve) => { answer = resolve; });
    const history = { project: "p", branches: [{ name: "main", base: null, head: null }], commits: [], layers: [], more: {} };
    const slow = new TreeReader(h.spec, async () => ({ layerfs: { close: async () => {} }, history: async () => history, operator: { inspect: { snapshot: () => (asked++, stuck) } } }));
    slow.sandboxMs = 50;
    const first = await slow.state(P), second = await slow.state(P);
    assert.deepEqual([first.late, second.late, asked], [true, true, 1], "drawn without Workspaces; the stuck snapshot is not asked again");
    assert.deepEqual(first.s.sandboxOrder, []);
    // the History-only read (a chat's History) asks no Sandbox and does not wait for one
    const started = Date.now(), only = await slow.history(P);
    assert.deepEqual([typeof only.labels, only.late, asked, Date.now() - started < 50], ["object", undefined, 1, true]);
    answer(empty);
    await new Promise((resolve) => setTimeout(resolve, 0));
    assert.equal((await slow.state(P)).late, undefined, "once it answers, the next read is a new snapshot");
    assert.equal(asked, 2);
    await slow.dispose();
  }
  // the project's running Sandboxes are drawn under their container names; one
  // the snapshot found gone after the listing (#373: idle close) is drawn closed
  const placed = { ...empty, sandboxes: [{ id: "1", name: "layerfs-p-1", workspaces: [{ directory: "/workspaces/1", mounted: true, branch: "f", base: null, baseLayer: null, root: null }] }], gone: [{ id: "2", name: "layerfs-p-2", detail: { containerGone: true } }] };
  const drawn = new TreeReader(h.spec, async () => ({ layerfs: { close: async () => {} }, operator: { inspect: { snapshot: async () => placed } } }));
  const shown = (await drawn.state(P)).s;
  assert.deepEqual(shown.sandboxOrder.map((id) => [id, shown.sandboxes[id].closed]), [["layerfs-p-1", false], ["layerfs-p-2", true]]);
  assert.deepEqual(shown.sandboxes["layerfs-p-1"].wsOrder, ["/workspaces/1"]);
  await drawn.dispose();
  // forgetting a project releases its client now
  const forgotten = [];
  const owner = new TreeReader(h.spec, async () => ({ layerfs: { close: async () => forgotten.push("store") }, operator: { inspect: { snapshot: async () => empty } } }));
  await owner.state(P);
  await owner.forget("p");
  assert.deepEqual(forgotten, ["store"]);

  // storage: Store totals and Init apart; every Branch's Commits once, newest first; main lists none
  const C = (id, time) => ({ id, time, message: id, via: "fs", logical: 10, newCanonical: 5, newStored: 3, newMetadata: 1 });
  const commitsOf = { feature: [C("c2", 2), C("c1", 1)], fix: [C("c3", 3), C("c1", 1)] };
  const listed = [];
  const stores = new TreeReader(h.spec, async () => ({
    layerfs: {
      close: async () => {},
      project: { list_branches: async () => ({ items: ["main", "feature", "fix"].map((name) => ({ name })), hasMore: false }) },
      branch: { list_commits: async (b) => (listed.push(b), { items: commitsOf[b], hasMore: false }) },
    },
    operator: { store: { stats: async () => ({ file: 4096, logical: 30, init: { entries: 3 } }) } },
  }));
  const storage = await stores.storage(P);
  assert.deepEqual([storage.store, storage.init, storage.commits.map((c) => c.id), storage.branches, storage.partial, listed], [{ file: 4096, logical: 30 }, { entries: 3 }, ["c3", "c2", "c1"], 2, false, ["feature", "fix"]]);
  await stores.dispose();

  // projects: any session may use any project; a folder only remembers its default
  const OTHER = "/work/other";
  const listedProjects = (await call("GET", "/layerfs/projects")).body;
  assert.deepEqual([listedProjects.projects.map((p) => [p.name, "folder" in p]), listedProjects.defaults], [[["website", false]], { [FOLDER]: "website" }]);
  assert.deepEqual([h.bindings.workspace(OTHER).projects.map((p) => p.name), h.bindings.projectFor("session-other", OTHER)], [["website"], null], "listed everywhere, nobody's default yet");
  assert.equal((await call("POST", "/layerfs/workspace", { folder: OTHER, project: "website" })).body.lastUsed, "website");
  assert.equal(h.bindings.projectFor("session-other", OTHER).name, "website", "a new session in the folder uses its default");
  assert.equal(h.bindings.choose("session-third", "/work/third", "website"), true, "a session chooses a project imported elsewhere");
  assert.equal(h.bindings.workspace("/work/third").lastUsed, "website");
  assert.equal((await call("POST", "/layerfs/workspace", { folder: OTHER, project: 3 })).status, 400);
  assert.equal((await call("POST", "/layerfs/workspace", { project: "website" })).status, 400);
  assert.equal((await call("POST", "/layerfs/workspace", { folder: OTHER, project: "nope" })).status, 404);
  assert.equal((await call("GET", "/layerfs/project/nope/storage")).status, 404);

  // Init: the source is counted first; the route answers a job, and the job its steps
  const src = join(dir, "src-folder");
  mkdirSync(join(src, "lib"), { recursive: true });
  writeFileSync(join(src, "a.txt"), "hello");
  writeFileSync(join(src, "lib", "b.txt"), "abc");
  const seen = [];
  assert.deepEqual(await scanSource(src, (c) => seen.push(c.entries)), { entries: 4, files: 2, bytes: 8 });
  assert.equal(seen.at(-1), 4);
  const started = await call("POST", "/layerfs/init", { folder: dir, source: src });
  assert.deepEqual([started.status, started.body.job.name, started.body.job.step], [202, "src-folder", "scan"]);
  const second = await call("POST", "/layerfs/init", { folder: dir, source: src });
  assert.equal(second.body.job.name, "src-folder-1", "a running Init keeps its name");
  assert.equal(randomName(new Set(["amber-badger"]), join(dir, "no-home"), () => 0), "amber-badger-1", "the pairs run out: numbered");
  assert.equal((await call("POST", "/layerfs/init", { folder: "relative" })).status, 409);
  const job = (id) => call("GET", `/layerfs/init/${id}`).then((r) => r.body.job);
  runs[0].report({ step: "import", totals: { entries: 4, files: 2, bytes: 8 } });
  assert.deepEqual([(await job(started.body.job.id)).step, (await job(started.body.job.id)).totals.files], ["import", 2]);
  runs[0].resolve({ name: "src-folder" });
  runs[1].reject(Object.assign(new Error("no Docker"), { code: "docker" }));
  await tick();
  assert.deepEqual([(await job(started.body.job.id)).step, (await job(started.body.job.id)).project.name], ["done", "src-folder"]);
  assert.deepEqual((await job(second.body.job.id)).error, { code: "docker", message: "no Docker" });
  runs[1].report({ step: "import" });
  assert.equal((await job(second.body.job.id)).step, "failed", "an ended job takes no more reports");
  assert.equal((await call("GET", "/layerfs/init/nope")).status, 404);
  // LayerFS's counts are read while its Init runs; a failed read stops only the counts
  const later = (ms, value) => new Promise((resolve) => setTimeout(() => resolve(value), ms));
  let progressReads = 0;
  const counted = [], counting = { operator: { project: { progress: async () => ({ step: "files", entries: 3, files: ++progressReads, bytes: 10 }) } } };
  assert.equal(await watching(counting, (f) => counted.push(f.imported), () => later(600, "sealed")), "sealed");
  assert.ok(counted.length >= 2 && counted.every((c) => c.step === "files"), "counts while Init runs");
  const after = progressReads;
  await later(300);
  assert.equal(progressReads, after, "no reads after Init ends");
  const broken = { operator: { project: { progress: async () => { throw new Error("no binding"); } } } };
  assert.equal(await watching(broken, () => assert.fail("no counts"), () => later(300, "sealed")), "sealed");

  // who made each node: every journaled session's lines of that project
  const journal = (line) => appendFileSync(join(h.spec.stateDir, "lines", "session-root.jsonl"), JSON.stringify({ agentId: "child-1", agent: "tester", project: "website", ok: true, start: "2026-10-16T00:00:01.000Z", ms: 5, ...line }) + "\n");
  journal({ id: "x-1", method: "fs.write", kind: "Committed", commit: "c".repeat(64) });
  journal({ id: "x-2", method: "branch.publish", kind: "Added", commit: "c".repeat(64), layer: "l".repeat(66) });
  journal({ id: "x-3", method: "branch.fork", kind: "Forked", branch: "feature" });
  const known = h.collector.nodesOf("website"), at = Date.parse("2026-10-16T00:00:01.005Z");
  assert.deepEqual(known.authors, { ["c".repeat(64)]: "tester", ["l".repeat(66)]: "tester" });
  assert.deepEqual([known.times, known.forks], [{ ["l".repeat(66)]: at }, { feature: at }]);
  assert.deepEqual(h.collector.nodesOf("other"), { authors: {}, times: {}, forks: {} });

  // version 1 and 2 files are read as version 3: no attached folder, and a folder's one attached project stays its default
  const old = join(dir, "v1"), two = join(dir, "v2");
  mkdirSync(old); mkdirSync(two);
  writeFileSync(join(old, "bindings.json"), JSON.stringify({ version: 1, workspaces: { [FOLDER]: { projects: [{ name: "a", source: FOLDER, volume: "v", created: 1 }], lastUsed: "a" } }, sessions: {}, agents: {} }));
  const upgraded = new BindingStore(old);
  assert.deepEqual([upgraded.data.version, "folder" in upgraded.byName("a"), upgraded.workspace(FOLDER).lastUsed], [3, false, "a"]);
  const project = (name, folder) => ({ name, source: folder ?? "/s", volume: "v", created: 1, folder });
  writeFileSync(join(two, "bindings.json"), JSON.stringify({ version: 2, projects: { a: project("a", "/one"), b: project("b", "/two"), c: project("c", "/two"), d: project("d", null) }, workspaces: { "/two": { lastUsed: null } }, sessions: {}, agents: {} }));
  const v2 = new BindingStore(two);
  assert.deepEqual([v2.data.version, v2.defaults(), Object.values(v2.data.projects).some((p) => "folder" in p)], [3, { "/one": "a" }, false]);

  // removal: confirmed by name, refused while a running container uses the volume; stopped ones go with it, then volume, home entry and binding
  assert.equal((await call("POST", "/layerfs/project/website/remove", { confirm: "web" })).status, 400);
  const ran = [], dockerWith = (users) => async (file, args) => { ran.push(args.slice(2).join(" ")); return { stdout: args[2] === "ps" ? users : "" }; };
  const forgot = [], fakeTrees = { forget: async (name) => { forgot.push(name); } };
  mkdirSync(join(h.spec.home, "website"), { recursive: true });
  await assert.rejects(removeProject(h.spec, h.bindings, fakeTrees, "website", dockerWith("abc running\nold exited\n")), { code: "busy" });
  assert.deepEqual([ran, h.bindings.byName("website") !== null], [["ps -a --filter volume=dsh-layerfs-website-1 --format {{.ID}} {{.State}}"], true]);
  ran.length = 0;
  // idle-closed Sandboxes (exited) and a failed start (created) do not refuse removal; they go with it
  assert.deepEqual(await removeProject(h.spec, h.bindings, fakeTrees, "website", dockerWith("layerfs-website-1 exited\nlayerfs-website-2 exited\nnew created\n")), { removed: "website", volume: "dsh-layerfs-website-1" });
  assert.deepEqual(ran, ["ps -a --filter volume=dsh-layerfs-website-1 --format {{.ID}} {{.State}}", "rm layerfs-website-1 layerfs-website-2 new", "volume rm dsh-layerfs-website-1"]);
  assert.deepEqual(forgot, ["website", "website"]);
  mkdirSync(join(dir, "website"), { recursive: true });
  // Linux: a Host that cannot write Docker's volumes directory, or does not own the LayerFS home, is refused before a volume is made
  const uid = process.getuid?.() ?? 1000;
  assert.equal(hostAccess(h.spec, { platform: "linux", uid }), null);
  assert.equal(hostAccess({ ...h.spec, volumes: join(dir, "no-volumes") }, { platform: "linux", uid }), null, "an engine without that directory is not checked");
  assert.equal(hostAccess({ ...h.spec, volumes: "/nowhere" }, { platform: "darwin", uid }), null, "macOS imports in the Store container");
  writeFileSync(join(dir, "volumes-file"), "");
  assert.match(hostAccess({ ...h.spec, volumes: join(dir, "volumes-file", "x") }, { platform: "linux", uid }), /cannot write Docker's volumes directory .* \(ENOTDIR\)/);
  assert.match(hostAccess(h.spec, { platform: "linux", uid: uid + 1 }), /belongs to uid \d+, not this Host's uid/);
  if (process.platform === "linux") assert.throws(() => prepareInit({ ...h.spec, volumes: join(dir, "volumes-file", "x") }, h.bindings, { folder: dir }), { code: "host-access" });
  assert.equal(prepareInit({ ...h.spec, volumes: "/nowhere-file/x" }, h.bindings, { folder: dir, backend: "sim" }).project.backend, "sim", "a simulated project needs no Docker");
  assert.equal(prepareInit(h.spec, h.bindings, { folder: dir, source: join(dir, "website") }).project.name, "website", "a removed name is free again at once");
  assert.equal(prepareInit(h.spec, h.bindings, { folder: dir, name: "website" }).project.name, "website");
  // a new empty project: a random adjective-noun name, an empty folder of its own (runInit removes it)
  const blank = prepareInit(h.spec, h.bindings, { folder: dir, empty: true }).project;
  assert.match(blank.name, /^[a-z]+-[a-z]+$/);
  assert.deepEqual([blank.empty, readdirSync(blank.source)], [true, []]);
  rmSync(blank.source, { recursive: true });
  assert.equal(prepareInit(h.spec, h.bindings, { folder: dir, empty: true, name: "blank" }).project.name, "blank");
  assert.equal(existsSync(join(h.spec.home, "website")), false);
  assert.deepEqual([h.bindings.byName("website"), h.bindings.workspace(FOLDER).lastUsed, h.bindings.session("session-root").project], [null, null, null]);
  assert.equal(h.bindings.projectFor("session-root", FOLDER), null, "a session on a removed project has none");
  await assert.rejects(removeProject(h.spec, h.bindings, fakeTrees, "website", dockerWith("")), { code: "not-found" });
  // a demo's Replay asks to stop the running Sandboxes instead of refusing
  h.bindings.addProject(FOLDER, { name: "website", source: FOLDER, volume: "dsh-layerfs-website-2", created: 2 });
  ran.length = 0;
  // its Sandbox records of the older layout go too: the service Sandbox's (an empty claim, as a stopped call leaves it) and each naming its volume
  const records = join(h.spec.home, "_sandboxes");
  for (const [entry, volume] of [["_service.website", null], ["website-a", "dsh-layerfs-website-2"], ["other-b", "dsh-layerfs-other-1"]]) {
    mkdirSync(join(records, entry), { recursive: true });
    if (volume) writeFileSync(join(records, entry, "sandbox.record"), Buffer.concat([Buffer.from("LFSA\u0002\u0000volume\u0004"), Buffer.from(volume)]));
  }
  assert.deepEqual(await removeProject(h.spec, h.bindings, fakeTrees, "website", dockerWith("abc running\nold exited\n"), { stopSandboxes: true }), { removed: "website", volume: "dsh-layerfs-website-2" });
  assert.deepEqual(ran, ["ps -a --filter volume=dsh-layerfs-website-2 --format {{.ID}} {{.State}}", "rm -f abc old", "volume rm dsh-layerfs-website-2"]);
  assert.deepEqual(readdirSync(records), ["other-b"], "another project's Sandbox record stays");
  // the snapshot keeps a few Commits per Branch: History gets the rest from
  // branch.list_commits, so a cut-off Commit keeps its place as a parent
  {
    const { stateOf } = await import("../src/layerfs-client.js");
    const id = (n) => "12" + String(n).padStart(64, "0"), L0 = "32" + "0".repeat(64);
    const commit = (n, parent) => ({ id: id(n), branch: "feature", parent: parent && id(parent), base: L0, root: "a".repeat(63) + n, via: "fs", message: `c${n}`, time: n });
    const all = [commit(4, 3), commit(3, 2), commit(2, 1), commit(1, null)];
    const pages = [];
    const client = {
      operator: { inspect: { snapshot: async () => ({ project: "website", commits: all.slice(0, 3), more: { feature: "cursor" }, layers: [{ id: L0, parent: null, root: "b".repeat(64) }], branches: [{ name: "main", base: L0, head: null }, { name: "feature", base: L0, head: id(4) }], workspaces: [] }) } },
      layerfs: { branch: { list_commits: async (branch, size, pos) => { pages.push([branch, pos]); return pos === 0 ? { items: all.slice(0, 2), hasMore: true, nextPagePos: 2 } : { items: all.slice(2), hasMore: false, nextPagePos: null }; } } },
    };
    const state = await stateOf(client, await client.operator.inspect.snapshot());
    assert.deepEqual(pages, [["feature", 0], ["feature", 2]]);
    assert.equal(Object.keys(state.s.commits).length, 4, "the cut-off Commit is listed");
    assert.equal(state.s.commits[state.labels[id(2)]].parent, state.labels[id(1)], "and is its child's parent");
  }
  // the move to #373's layout: an older Sandbox is removed only when it has
  // nothing to lose; one running with a mounted Workspace is kept, with every
  // record, and reported; records go once every older container is gone
  {
    const home = join(dir, "migrate-home"), records = join(home, "_sandboxes");
    const older = (name, container, mounts = 0) => {
      mkdirSync(join(records, name), { recursive: true });
      if (container) writeFileSync(join(records, name, "container"), container);
      for (let i = 0; i < mounts; i++) writeFileSync(join(records, name, `mount-${i}.record`), "");
    };
    older("_service.a", "dead", 1); // a container Docker no longer has: removed whatever its records say
    older("_service.b", "idle"); // running, no Workspace
    older("named", "busy", 2); // running, two mounted Workspaces
    older("claimed", null); // a claim that never started
    mkdirSync(join(home, "a"), { recursive: true });
    writeFileSync(join(home, "a", "service.record"), "");
    const state = { dead: null, idle: "true", busy: "true" }, ran = [], warned = [];
    const docker = async (file, args) => {
      const [verb, ...rest] = args.slice(2);
      ran.push(`${verb} ${rest.at(-1)}`);
      const id = rest.at(-1);
      if (verb === "inspect" && state[id] === null) throw Object.assign(new Error("exit 1"), { stderr: `Error: No such object: ${id}` });
      if (verb === "rm" && id === "stuck") throw Object.assign(new Error("permission denied"), { stderr: "permission denied" });
      return { stdout: verb === "inspect" ? `${state[id]}\n` : "" };
    };
    const spec = { home, docker: "/var/run/docker.sock" };
    const first = await migrate(spec, docker, (text) => warned.push(text));
    assert.deepEqual(first.kept.map((k) => [k.sandbox, k.container, k.workspaces]), [["named", "busy", 2]]);
    assert.ok(!ran.includes("rm busy"), "a Sandbox with mounted Workspaces is never removed");
    assert.ok(ran.includes("rm idle"));
    assert.match(warned.join("\n"), /named .*left running: 2 mounted Workspaces.*docker rm -f busy/);
    assert.ok(existsSync(join(records, "_service.b")) && existsSync(join(home, "a", "service.record")), "records stay while one is kept");
    // the user removed it: the next start finds every older container gone and forgets the records
    state.busy = null;
    assert.deepEqual((await migrate(spec, docker, () => {})).kept, []);
    assert.deepEqual([existsSync(records), existsSync(join(home, "a", "service.record"))], [false, false]);
    // a removal Docker refuses keeps every record too, reported
    older("_service.c", "stuck");
    state.stuck = "false";
    const refused = await migrate(spec, docker, () => {});
    assert.deepEqual(refused.kept.map((k) => k.sandbox), ["_service.c"]);
    assert.match(refused.kept[0].reason, /could not remove/);
    assert.ok(existsSync(join(records, "_service.c")));
  }
  // an agent's call that found the Store container down tells the Host, which starts it again; other failures do not
  {
    const healing = host();
    let heals = 0;
    healing.collector.onUnreachable = () => heals++;
    const file = healing.collector.track({ id: "heal-1", root: "heal-root" }, { name: "website" });
    const failed = (id, message) => JSON.stringify({ layerfs_trace: 1, id, method: "fs.read", start: new Date().toISOString(), ms: 1, ok: false, code: "Failed", message }) + "\n";
    appendFileSync(file, failed("u-1", "the Store container layerfs-store's worker did not answer (connect ECONNREFUSED 127.0.0.1:55012): dsh starts it again; run the call again once it is up")
      + failed("u-2", "NoSuchPath: /nope.js"));
    healing.collector.poll();
    assert.equal(heals, 1);
  }
  // the ported binding's answers when the Store container is down, as trace lines carry them
  for (const message of ["the Store container layerfs-store has not started for /h: dsh starts it; run the call again once it is up",
    "the Store container layerfs-store's worker did not answer (connect ECONNREFUSED 127.0.0.1:1): dsh starts it again",
    "the connection to the Store container layerfs-store was lost (read ECONNRESET): check the project before going on",
    "the Store container layerfs-store is not there to run project.init"]) assert.match(message, UNREACHABLE);
  assert.doesNotMatch("NoSuchPath: the Store container docs is a folder", UNREACHABLE);
  console.log("host: ok");
} finally {
  rmSync(dir, { recursive: true, force: true });
}

// C12 per-tool-call cycle (#420): the retired fs-bench-pro timing cell C12
// (`git init`, 100 files, `git add -A`, `git commit` in one Workspace through
// FUSE) ported to the public JS SDK, as one sample of the full per-tool-call
// path: mount -> exec -> Workspace Commit -> unmount, each phase timed.
//
// Base: an empty project root (the R7 empty master: one root entry), forked
// to Branch `c12` (`main` is the read-only base view).
// Class B: one untimed identical command on an earlier mount of the same
// Branch, terminally unmounted with discard (no Commit), so the sample's
// mount finds a warm daemon while the Branch is still the empty root.
// Every call is traced (LAYERFS_TRACE, set by the launcher); between phases
// an untimed traced `workspace.status` gives the kernel and overlay counters.
// The daemon's numeric observation records are read from its container log
// through the Docker socket and saved beside the trace. Disposable Store only
// (the native SDK selects it). One sample; nothing is retried.
//
//   python3 tools/worktree.py run --resources <resources.json> --bundle <sealed linux bundle> \
//     --timeout 90 -- node sdk/js/test/verify/c12_cycle_real.mjs inner
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { request } from "node:http";
import { loadavg, tmpdir } from "node:os";
import { join } from "node:path";
import { performance } from "node:perf_hooks";

if (process.argv[2] !== "inner") throw new Error("usage: c12_cycle_real.mjs inner (under tools/worktree.py run)");
const OUT = process.argv[3] ?? "/receipts";
const C12 = "{ git init -q; for i in $(seq 1 100); do echo $i > f$i; done; git add -A; git -c user.email=a@b -c user.name=a commit -qm init; } >/dev/null";
// The same command, with the in-container clock around it and its status.
const TIMED = `s=$(date +%s%N); ${C12}; rc=$?; e=$(date +%s%N); echo "C12 $((e-s)) $rc"`;
const STORE = process.env.LAYERFS_STORE_PATH;
const PREFIX = process.env.LAYERFS_CONTAINER_PREFIX;
const BRANCH = "c12";
const SOCKET = process.env.LAYERFS_DOCKER ?? "/var/run/docker.sock";

const size = (path) => { try { return statSync(path).size; } catch { return null; } };
const storeBytes = () => ({ main: size(STORE), wal: size(`${STORE}-wal`), shm: size(`${STORE}-shm`) });
const step = (label) => { process.env.LAYERFS_TRACE_STEP = label; };
async function timed(fn) {
  const t = performance.now();
  const value = await fn();
  return [value, Math.round((performance.now() - t) * 1000) / 1000];
}

/** One Docker Engine GET over the socket; the raw body. */
function engine(path) {
  return new Promise((resolve, reject) => {
    const req = request({ socketPath: SOCKET, path, method: "GET" }, (res) => {
      const parts = [];
      res.on("data", (c) => parts.push(c));
      res.on("end", () => (res.statusCode === 200 ? resolve(Buffer.concat(parts)) : reject(new Error(`${path}: ${res.statusCode}`))));
    });
    req.on("error", reject);
    req.end();
  });
}
/** A non-TTY container log: 8-byte frame headers, stream 1 stdout, 2 stderr. */
function demux(body) {
  let text = "";
  for (let at = 0; at + 8 <= body.length; ) {
    const length = body.readUInt32BE(at + 4);
    text += body.subarray(at + 8, at + 8 + length).toString("utf8");
    at += 8 + length;
  }
  return text;
}

const toHex = (words) => words.map((w) => BigInt(w).toString(16).padStart(16, "0")).join("");
/** The daemon's records grouped by trace scope: section -> index -> values. */
function observations(log) {
  const byScope = new Map();
  for (const line of log.split("\n")) {
    const at = line.indexOf('{"layerfs_observation":');
    if (at < 0) continue;
    // Scope and daemon words reach 2^64: every array number is read as text.
    const row = JSON.parse(line.slice(at).replace(/([[,])(\d+)(?=[,\]])/g, '$1"$2"'));
    const scope = toHex(row.scope.map(String));
    const groups = byScope.get(scope) ?? new Map();
    byScope.set(scope, groups);
    const group = groups.get(row.call) ?? {};
    groups.set(row.call, group);
    (group[row.section] ??= {})[row.index] = row.available ? row.values.map(Number) : null;
  }
  return byScope;
}
/** The counters this report needs from one call's records (cumulative). */
function counters(group) {
  const sum = (section, field) => Object.values(group?.[section] ?? {}).reduce((t, v) => t + (v ? v[field] : 0), 0);
  return {
    ownerJobs: group?.[1]?.[0]?.[0] ?? null,
    // StatementWork.executions; the 2026-10-10 baseline counted foreground only.
    overlayForegroundStatements: sum(2, 1),
    overlayStatements: sum(2, 1) + sum(3, 1),
    overlayStatementAttempts: sum(2, 0) + sum(3, 0),
    overlayVmSteps: sum(2, 7) + sum(3, 7),
    storeWriterStatements: group?.[13]?.[-1]?.[0] ?? null,
    storeReaderStatements: sum(14, 0),
    kernelRequests: group?.[25]?.[0]?.[2] ?? null,
    // The 39 opcode counts, as the 2026-10-10 baseline summed them.
    kernelOpcodes: group?.[26]?.[0] ? group[26][0].slice(0, 39).reduce((t, v) => t + v, 0) : null,
    overlayLogicalBytes: group?.[28]?.[0]?.[14] ?? null,
    overlayAllocatedBytes: group?.[28]?.[0]?.[15] ?? null,
  };
}
const minus = (a, b) => Object.fromEntries(Object.keys(a).map((k) => [k, a[k] === null || b?.[k] === null || b?.[k] === undefined ? null : a[k] - b[k]]));

const report = { cell: "C12", class: "B", label: "exploratory, one sample", loadavgStartContainer: loadavg(), store: {}, phases: {} };
try {
  const m = JSON.parse(readFileSync("manifest.json", "utf8"));
  report.bundle = { build_id: m.build_id, profile: m.profile, source: m.source, toolchain: m.toolchain, images: m.images, daemon: m.files["layerfs-daemon"], addon: m.files["layerfs.node"], script: m.files["sdk/js/test/verify/c12_cycle_real.mjs"] };
} catch { report.bundle = null; }
const { LayerFS } = await import("../../src/index.js");
const work = mkdtempSync(join(tmpdir(), "layerfs-c12-"));
const source = join(work, "empty");
mkdirSync(source);
const project = `c12-${Date.now().toString(36)}`;
const lf = new LayerFS(project);
let failure = null;
try {
  step("setup");
  report.init = await lf.project.init(source);
  report.store.afterInit = storeBytes();
  // `main` is the read-only base view; Workspaces mount a forked Branch.
  report.fork = await lf.branch.fork(BRANCH);

  // Class B warm-up: identical command, earlier mount, terminal unmount.
  step("warmup");
  const warm = await lf.workspace.mount(BRANCH);
  const warmRan = await lf.workspace.exec(warm, TIMED);
  assert.equal(warmRan.exitCode, 0, JSON.stringify(warmRan));
  report.warmup = { stdout: warmRan.stdout.trim(), unmount: await lf.workspace.unmount(warm, true) };
  report.store.afterWarmup = storeBytes();

  // The sample.
  step("sample.mount");
  const [ws, mountMs] = await timed(() => lf.workspace.mount(BRANCH));
  assert.equal(ws.sandbox, warm.sandbox, "the sample mount reuses the warm-up's daemon");
  step("probe.before_exec");
  report.statusBeforeExec = await lf.workspace.status(ws);
  step("sample.exec");
  const [ran, execMs] = await timed(() => lf.workspace.exec(ws, TIMED));
  assert.equal(ran.exitCode, 0, JSON.stringify(ran));
  const [, ns, rc] = ran.stdout.trim().split(" ");
  assert.equal(rc, "0", ran.stdout);
  step("probe.after_exec");
  report.statusAfterExec = await lf.workspace.status(ws);
  report.store.afterExec = storeBytes();
  step("sample.commit");
  const [committed, commitMs] = await timed(() => lf.workspace.commit(ws, "C12"));
  report.store.afterCommit = storeBytes();
  step("sample.unmount");
  const [unmounted, unmountMs] = await timed(() => lf.workspace.unmount(ws));
  report.store.afterUnmount = storeBytes();
  report.phases = { mountMs, execMs, execInContainerMs: Number(ns) / 1e6, commitMs, unmountMs, cycleMs: mountMs + execMs + commitMs + unmountMs };
  report.commit = committed;
  report.unmount = unmounted;

  // Untimed check: the Branch now holds .git and f1..f100.
  step("check");
  const listed = (await lf.fs.list(BRANCH, "/", 1000)).items.map((i) => i.path ?? `/${i.name}`).sort();
  report.check = { entries: listed.length, git: listed.includes("/.git"), f100: listed.includes("/f100") };
  assert.equal(listed.length, 101, JSON.stringify(listed.slice(0, 5)));
  report.sandbox = ws.sandbox;
} catch (error) {
  failure = error;
  report.failure = { code: error?.code ?? null, message: String(error?.message ?? error) };
} finally {
  await lf.close?.().catch(() => {});
  rmSync(work, { recursive: true, force: true });
}

// The daemon's observation records, then per-phase deltas joined by trace scope.
try {
  const list = JSON.parse((await engine(`/containers/json?all=1&filters=${encodeURIComponent(JSON.stringify({ name: [PREFIX] }))}`)).toString());
  const daemons = list.filter((c) => c.Names.some((n) => n.startsWith(`/${PREFIX}${project}-`)));
  let log = "";
  for (const c of daemons) {
    const text = demux(await engine(`/containers/${c.Id}/logs?stdout=1&stderr=1`));
    writeFileSync(join(OUT, `daemon-${c.Names[0].slice(1)}.log`), text);
    log += text;
  }
  report.daemonContainers = daemons.map((c) => ({ id: c.Id, name: c.Names[0], image: c.ImageID }));
  const byScope = observations(log);
  const trace = readFileSync(process.env.LAYERFS_TRACE, "utf8").trimEnd().split("\n").map((l) => JSON.parse(l)).filter((l) => l.phase !== "start");
  // The last record group of a traced call (mount sends bind then Attach).
  const last = (line) => { const g = byScope.get(line?.scope); return g ? g.get(Math.max(...g.keys())) : null; };
  const lineOf = (stepName, method) => trace.find((l) => l.step === stepName && l.method === method);
  const at = {
    warmUnmount: counters(last(lineOf("warmup", "workspace.unmount"))),
    mount: counters(last(lineOf("sample.mount", "workspace.mount"))),
    beforeExec: counters(last(lineOf("probe.before_exec", "workspace.status"))),
    afterExec: counters(last(lineOf("probe.after_exec", "workspace.status"))),
    commit: counters(last(lineOf("sample.commit", "workspace.commit"))),
    unmount: counters(last(lineOf("sample.unmount", "workspace.unmount"))),
  };
  report.observed = {
    cumulative: at,
    // Daemon-wide cumulative counters, differenced between traced calls.
    delta: {
      mount: minus(at.mount, at.warmUnmount),
      exec: minus(at.afterExec, at.beforeExec),
      commit: minus(at.commit, at.afterExec),
      unmount: minus(at.unmount, at.commit),
      cycle: minus(at.unmount, at.warmUnmount),
    },
  };
  // Each traced call's own Store SQL (daemon, this call only) and FUSE counts.
  const brief = (l) => l && {
    ms: l.ms, ok: l.ok, kind: l.kind, sizes: l.sizes,
    daemonStore: (l.daemon ?? []).map((d) => Object.fromEntries(["writer", "readers", "call"].map((k) => [k, d.sql?.[k] ? { statements: d.sql[k].statements, transactions: d.sql[k].transactions, writeTransactions: d.sql[k].write_transactions, readSnapshots: d.sql[k].read_snapshots, statementMs: d.sql[k].statement_ns / 1e6 } : null]))),
    timing: (l.daemon ?? []).map((d) => d.timing),
    fuseServed: (l.daemon ?? []).map((d) => d.fuse?.served ?? null).filter((v) => v !== null),
  };
  report.traced = Object.fromEntries(trace.filter((l) => l.step?.startsWith("sample.") || l.step?.startsWith("probe.")).map((l) => [`${l.step}:${l.method}`, brief(l)]));
} catch (error) {
  report.observationFailure = String(error?.message ?? error);
}
report.loadavgEndContainer = loadavg();
writeFileSync(join(OUT, "c12-summary.json"), `${JSON.stringify(report, null, 1)}\n`);
console.log(JSON.stringify({ phases: report.phases, observed: report.observed?.delta, commit: report.commit?.kind, failure: report.failure ?? null }, null, 1));
if (failure) throw failure;
if (report.observationFailure) throw new Error(`observations: ${report.observationFailure}`);
console.log("C12 cycle: one sample taken");

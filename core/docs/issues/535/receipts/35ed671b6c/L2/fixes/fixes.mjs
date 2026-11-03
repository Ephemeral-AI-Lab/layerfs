// Lane L2 (final terminal pass at 35ed671b6c): checks of the fixes since
// fe6f85013b that this lane reaches, through the frozen Node package.
//   node fixes.mjs <machine> <receipt.json>
// Env: LAYERFS_HOME (holds <machine>), LAYERFS_K_NODE (package dir),
// LFS535_PG_LOOP (a loopback URL prefix "postgres://postgres@127.0.0.1:<port>/"),
// LFS535_PG_PASSWORD (`env:NAME`). One attempt per call, no retry.
import { mkdtempSync, readdirSync, rmSync, statSync, writeFileSync, existsSync } from "node:fs";
import { loadavg, tmpdir } from "node:os";
import { join, relative } from "node:path";

const [machine, receipt] = process.argv.slice(2);
const { client } = await import(join(process.env.LAYERFS_K_NODE, "src/index.js"));
const home = process.env.LAYERFS_HOME;
const lfs = client({ home });
const pg = process.env.LFS535_PG_LOOP;
const password = process.env.LFS535_PG_PASSWORD;
const out = { format: "lfs535-l2-fixes/1", machine, product: process.env.LFS533_PRODUCT, startedAt: new Date().toISOString(), load1: loadavg()[0], checks: {}, steps: [] };
const errorOf = (e) => ({ code: e?.code ?? null, message: String(e?.message ?? e).slice(0, 1500), detail: e?.detail ?? null });
const step = async (name, call) => {
  const at = performance.now();
  try {
    const value = await call();
    out.steps.push({ name, ms: Math.round(performance.now() - at), ok: true });
    return { ok: true, value };
  } catch (e) {
    out.steps.push({ name, ms: Math.round(performance.now() - at), ok: false, error: errorOf(e) });
    return { ok: false, error: errorOf(e) };
  }
};
const verdict = (name, pass, evidence) => { out.checks[name] = { verdict: pass ? "PASS" : "FAIL", ...evidence }; };
// every path under the home with its size and mtime (a read must change none)
const tree = () => {
  const items = {};
  const walk = (dir) => {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      const st = statSync(path, { throwIfNoEntry: false });
      if (!st) continue;
      items[relative(home, path)] = entry.isDirectory() ? "dir" : `${st.size}@${st.mtimeMs}`;
      if (entry.isDirectory()) walk(path);
    }
  };
  walk(home);
  return items;
};
const diffTree = (a, b) => {
  const changed = [];
  for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) if (a[k] !== b[k]) changed.push(`${k}: ${a[k] ?? "absent"} -> ${b[k] ?? "absent"}`);
  return changed;
};

const src = mkdtempSync(join(tmpdir(), "lfs535l2-fx-"));
writeFileSync(join(src, "a.txt"), "fixes check\n");
const cleanup = [];
try {
  // 1. a missing PostgreSQL database is Unavailable "the server refused the session"
  {
    const r = await step("fxmiss.store.connect (missing database)", () => lfs.store.connect("fxmiss", `${pg}nosuchdb`, { password }));
    const folder = existsSync(join(home, "store", "fxmiss"));
    verdict("missing-database", !r.ok && r.error.code === "Unavailable" && /the server refused the session/.test(r.error.message) && !folder, { answer: r.ok ? r.value : r.error, storeFolderLeft: folder });
    const w = await step("fxpw.store.connect (wrong password)", () => lfs.store.connect("fxpw", `${pg}fx6`, { password: "env:LFS535_WRONG_PASSWORD" }));
    verdict("wrong-password", !w.ok && w.error.code === "Unavailable" && /the server refused the session/.test(w.error.message), { answer: w.ok ? w.value : w.error, note: "LFS535_WRONG_PASSWORD is a fixed wrong value set by the driver" });
  }
  // 2. project.init {progress:{phase,done,total}} on every route
  {
    const routes = {};
    const run = async (label, store, make) => {
      const made = await step(`${store}.make`, make);
      if (!made.ok) { routes[label] = { error: made.error }; return; }
      const events = [];
      const r = await step(`${store}.project.init`, () => lfs.project.init(`${store}/p`, src, { on_progress: (p) => events.push(p) }));
      routes[label] = { ok: r.ok, error: r.ok ? null : r.error, phases: events.map((e) => e.phase), events: events.slice(0, 40), eventCount: events.length };
    };
    await run("native-sqlite", "fxn", () => lfs.store.create("fxn"));
    cleanup.push(["remove", "fxn"]);
    await run("postgresql", "fxp", () => lfs.store.connect("fxp", `${pg}fx1`, { password }));
    cleanup.push(["disconnect", "fxp"]);
    await run("worker-sqlite", "fxw", async () => { await lfs.store.create("fxw"); cleanup.push(["pull", "fxw"]); return lfs.store.push("fxw", machine); });
    const shape = (events) => events.every((e) => e && typeof e.phase === "string" && "done" in e && "total" in e);
    const uniq = (phases) => phases.filter((p, i) => phases.indexOf(p) === i);
    const nat = routes["native-sqlite"], pgr = routes.postgresql, wrk = routes["worker-sqlite"];
    const ok = nat?.ok && pgr?.ok && wrk?.ok
      && shape(nat.events) && shape(pgr.events) && shape(wrk.events)
      && JSON.stringify(uniq(nat.phases)) === JSON.stringify(["check", "install", "record"])
      && JSON.stringify(uniq(pgr.phases)) === JSON.stringify(["check", "install", "record"])
      && JSON.stringify(uniq(wrk.phases)) === JSON.stringify(["check", "send", "install", "record"])
      && wrk.events.filter((e) => e.phase !== "send").every((e) => e.done === null && e.total === null);
    verdict("init-progress-every-route", ok, { routes });
  }
  // 3. store.status / store.list change nothing in the home
  {
    const before = tree();
    const reads = {};
    for (const s of ["fxn", "fxp", "fxw"]) reads[`status ${s}`] = await step(`${s}.store.status`, () => lfs.store.status(s));
    reads.list = await step("store.list", () => lfs.store.list());
    const after = tree();
    const changed = diffTree(before, after);
    const locks = Object.keys(after).filter((k) => /(^|\/)lock$/.test(k) && !(k in before));
    verdict("store-reads-change-nothing", changed.length === 0 && Object.values(reads).every((r) => r.ok), { changed, newLockFiles: locks, leasesLog: after["log/leases.jsonl"] ?? "absent", answers: Object.fromEntries(Object.entries(reads).map(([k, r]) => [k, r.ok ? (r.value?.state ?? (r.value?.items ? `${r.value.items.length} items` : "ok")) : r.error])) });
  }
  // 4. workspace.mount(sandbox, "main") is refused InvalidInput; InUse fills detail.related
  {
    const fork = await step("fxw.branch.fork", () => lfs.branch.fork("fxw/p", "lane", "main"));
    const sb = await step("fxw.sandbox.create", () => lfs.sandbox.create("fxw/p", "fxwsb", machine));
    if (sb.ok) cleanup.unshift(["close", "fxwsb"]);
    const m = await step("fxwsb.workspace.mount main", () => lfs.workspace.mount("fxwsb", "main"));
    if (m.ok) cleanup.unshift(["unmount", m.value.ws]);
    verdict("mount-main-refused", fork.ok && sb.ok && !m.ok && m.error.code === "InvalidInput", { answer: m.ok ? m.value : m.error });
    const lane = await step("fxwsb.workspace.mount lane", () => lfs.workspace.mount("fxwsb", "lane"));
    if (lane.ok) await step("fxwsb.workspace.unmount lane", () => lfs.workspace.unmount(lane.value.ws, { discard: true }));
    out.checks["mount-main-refused"].sideBranchMount = lane.ok ? "mounted" : lane.error;
    const rm = await step("fxw/p project.remove (Sandbox recorded)", () => lfs.project.remove("fxw/p"));
    const related = rm.ok ? null : rm.error.detail?.related;
    verdict("project-remove-inuse-related", !rm.ok && rm.error.code === "InUse" && Array.isArray(related) && related.some((x) => x.kind === "sandbox" && x.name === "fxwsb"), { answer: rm.ok ? rm.value : rm.error });
    const psb = await step("fxp.sandbox.create", () => lfs.sandbox.create("fxp/p", "fxpsb", machine));
    if (psb.ok) cleanup.unshift(["close", "fxpsb"]);
    const dc = await step("fxp store.disconnect (Sandbox recorded)", () => lfs.store.disconnect("fxp"));
    const rel2 = dc.ok ? null : dc.error.detail?.related;
    verdict("store-disconnect-inuse-related", psb.ok && !dc.ok && dc.error.code === "InUse" && Array.isArray(rel2) && rel2.length > 0, { sandbox: psb.ok ? "created" : psb.error, answer: dc.ok ? dc.value : dc.error });
    if (dc.ok) cleanup.splice(cleanup.findIndex((c) => c[0] === "disconnect" && c[1] === "fxp"), 1);
  }
  // 5. the reach check's refusal when the server refuses the session: a
  // Store connected to database fx2, which is then dropped on the server
  // (`DROP DATABASE fx2 WITH (FORCE)` through `docker exec` on the lane's own
  // container); sandbox.create on <machine> runs the reach check.
  {
    const c = await step("fxr.store.connect", () => lfs.store.connect("fxr", `${pg}fx2`, { password }));
    if (c.ok) cleanup.push(["disconnect", "fxr"]);
    const i = await step("fxr.project.init", () => lfs.project.init("fxr/p", src));
    const { execFileSync } = await import("node:child_process");
    const dropped = await step("drop database fx2 on the server", async () => execFileSync("docker", ["exec", process.env.LFS535_PG_CONTAINER, "psql", "-U", "postgres", "-qc", "DROP DATABASE fx2 WITH (FORCE)"], { encoding: "utf8" }));
    const sb = await step("fxr.sandbox.create (database gone)", () => lfs.sandbox.create("fxr/p", "fxrsb", machine));
    if (sb.ok) cleanup.unshift(["close", "fxrsb"]);
    verdict("reach-refusal-server-refused-session", c.ok && i.ok && dropped.ok && !sb.ok && sb.error.code === "Unavailable" && /cannot reach PostgreSQL Store fxr's server at .*the server refused the session/.test(sb.error.message), { answer: sb.ok ? sb.value : sb.error });
  }
} finally {
  for (const [what, name] of cleanup) {
    if (what === "unmount") await step(`cleanup unmount ${name}`, () => lfs.workspace.unmount(name, { discard: true, force: true }));
    if (what === "close") await step(`cleanup sandbox.close ${name}`, () => lfs.sandbox.close(name));
  }
  for (const [what, name] of cleanup) {
    if (what === "pull") {
      await step(`cleanup store.pull ${name}`, () => lfs.store.pull(name));
      await step(`cleanup store.remove ${name}`, () => lfs.store.remove(name));
    }
    if (what === "remove") await step(`cleanup store.remove ${name}`, () => lfs.store.remove(name));
    if (what === "disconnect") await step(`cleanup store.disconnect ${name}`, () => lfs.store.disconnect(name));
  }
  rmSync(src, { recursive: true, force: true });
  out.endedAt = new Date().toISOString();
  writeFileSync(receipt, JSON.stringify(out, null, 2) + "\n");
}
for (const [k, v] of Object.entries(out.checks)) console.log(`${v.verdict} ${k}`);

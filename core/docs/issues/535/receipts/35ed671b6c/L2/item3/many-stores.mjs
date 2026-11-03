// #535 phase 8 item 3 (VM part), lane L2 (copied from K2): one VM machine holds two sqlite
// Stores (pushed) and uses two PostgreSQL Stores; Sandboxes of all four run at
// once (mount, exec, Commit, unmount together), and each Commit reads back
// from the caller (an in-process `file.read` on the Mac). One attempt per
// call, no retry.
//   node many-stores.mjs <machine> <receipt.json>
// Env: LAYERFS_HOME (holds <machine>), LFS535_PG_LOOP / LFS535_PG_REMOTE (URLs,
// no password), LFS535_PG_PASSWORD (`env:NAME`), LAYERFS_K_NODE (package dir).
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { loadavg, tmpdir } from "node:os";
import { join } from "node:path";

const [machine, receipt] = process.argv.slice(2);
const { client } = await import(join(process.env.LAYERFS_K_NODE, "src/index.js"));
const lfs = client({ home: process.env.LAYERFS_HOME });
const now = () => performance.now();
const out = { format: "lfs535-l2-item3/1", machine, product: process.env.LFS533_PRODUCT, startedAt: new Date().toISOString(), load1: loadavg()[0], ok: false, steps: [], stores: {} };
const step = async (name, call) => {
  const at = now();
  const startedAt = new Date().toISOString();
  try {
    const value = await call();
    out.steps.push({ name, startedAt, ms: Math.round((now() - at) * 1000) / 1000, ok: true });
    return value;
  } catch (error) {
    out.steps.push({ name, startedAt, ms: Math.round((now() - at) * 1000) / 1000, ok: false, error: { code: error?.code ?? null, message: String(error?.message ?? error).slice(0, 1500) } });
    throw error;
  }
};
const password = process.env.LFS535_PG_PASSWORD;
// LFS535_I3_PREFIX names the Stores (default i3). (K2: attempt 3 used j3, since a
// worker kept a reused PostgreSQL Store name's old service open; fixed by 2d607fc066)
const p = process.env.LFS535_I3_PREFIX ?? "i3";
const stores = [
  { name: `${p}s1`, kind: "sqlite" },
  { name: `${p}s2`, kind: "sqlite" },
  { name: `${p}p1`, kind: "postgresql", url: process.env.LFS535_PG_LOOP },
  { name: `${p}p2`, kind: "postgresql", url: process.env.LFS535_PG_REMOTE },
];
const empty = mkdtempSync(join(tmpdir(), "lfs535l2-i3-"));
const made = [];
const opened = [];
const pushed = [];
const mounted = new Set();
try {
  for (const s of stores) {
    if (s.kind === "sqlite") await step(`${s.name}.store.create`, () => lfs.store.create(s.name));
    else await step(`${s.name}.store.connect`, () => lfs.store.connect(s.name, s.url, { password }));
    made.push(s);
    out.stores[s.name] = { kind: s.kind, host: s.url ? new URL(s.url).hostname : null };
    await step(`${s.name}.project.init`, () => lfs.project.init(`${s.name}/p`, empty));
    // main takes no Workspace (attempt 1 mounted it: NoSuchBranch); fork a Branch, as the cells do
    await step(`${s.name}.branch.fork`, () => lfs.branch.fork(`${s.name}/p`, "lane", "main"));
    if (s.kind === "sqlite") {
      await step(`${s.name}.store.push`, () => lfs.store.push(s.name, machine));
      pushed.push(s);
    }
  }
  out.machineBefore = await step("machine.get", () => lfs.machine.get(machine));
  // the four Sandboxes, created together
  await Promise.all(stores.map(async (s) => {
    out.stores[s.name].sandbox = await step(`${s.name}.sandbox.create`, () => lfs.sandbox.create(`${s.name}/p`, s.name, machine));
    opened.push(s);
  }));
  // all four run at once: mount, exec, Commit, unmount
  const t0 = now();
  await Promise.all(stores.map(async (s) => {
    const { ws } = await step(`${s.name}.mount`, () => lfs.workspace.mount(s.name, "lane"));
    mounted.add(ws);
    const ran = await step(`${s.name}.exec`, () => lfs.workspace.exec(ws, `echo "written by ${s.name} on ${machine}" > i3-${s.name}.txt; pwd; sleep 2; cat i3-${s.name}.txt`, { message: `item3 ${s.name}` }));
    const committed = await step(`${s.name}.commit`, () => lfs.workspace.commit(ws, { message: `item3 ${s.name}` }));
    await step(`${s.name}.unmount`, () => lfs.workspace.unmount(ws, {}));
    mounted.delete(ws);
    Object.assign(out.stores[s.name], { exec: { exit: ran.exit_code, stdout: ran.stdout?.slice(0, 500), stderr: ran.stderr?.slice(0, 500) }, commit: committed });
  }));
  out.concurrentMs = Math.round(now() - t0);
  // read back from the caller
  for (const s of stores) {
    const read = await step(`${s.name}.file.read`, () => lfs.file.read(`${s.name}/p`, "lane", `/i3-${s.name}.txt`));
    out.stores[s.name].readBack = read;
  }
  const problems = [];
  for (const s of stores) {
    const r = out.stores[s.name];
    if (r.exec?.exit !== 0) problems.push(`${s.name} exec exit ${r.exec?.exit}`);
    if (!r.commit?.commit) problems.push(`${s.name} Commit answered ${JSON.stringify(r.commit)}`);
    const text = typeof r.readBack === "string" ? r.readBack : JSON.stringify(r.readBack);
    if (!text.includes(`written by ${s.name} on ${machine}`)) problems.push(`${s.name} read back ${text.slice(0, 200)}`);
  }
  // overlap: every exec started before any exec ended
  const execs = out.steps.filter((x) => x.name.endsWith(".exec"));
  const latestStart = Math.max(...execs.map((x) => Date.parse(x.startedAt)));
  const earliestEnd = Math.min(...execs.map((x) => Date.parse(x.startedAt) + x.ms));
  out.execOverlap = { latestStart: new Date(latestStart).toISOString(), earliestEnd: new Date(earliestEnd).toISOString(), allOverlap: latestStart < earliestEnd };
  if (!out.execOverlap.allOverlap) problems.push("the four execs did not overlap");
  out.ok = problems.length === 0;
  out.cause = problems.join("; ") || null;
} catch (error) {
  out.ok = false;
  out.cause = String(error?.message ?? error);
} finally {
  for (const ws of mounted) await step(`cleanup.unmount ${ws}`, () => lfs.workspace.unmount(ws, { discard: true, force: true })).catch(() => {});
  for (const s of opened) await step(`${s.name}.sandbox.close`, () => lfs.sandbox.close(s.name)).catch(() => {});
  for (const s of pushed) await step(`${s.name}.store.pull`, () => lfs.store.pull(s.name)).catch(() => { out.ok = false; });
  for (const s of made) {
    if (s.kind === "sqlite") await step(`${s.name}.store.remove`, () => lfs.store.remove(s.name)).catch(() => { out.ok = false; });
    else await step(`${s.name}.store.disconnect`, () => lfs.store.disconnect(s.name)).catch(() => { out.ok = false; });
  }
  rmSync(empty, { recursive: true, force: true });
  out.endedAt = new Date().toISOString();
  writeFileSync(receipt, JSON.stringify(out, null, 2) + "\n");
}
console.log(`item3 ${machine} ${out.ok ? "PASS" : "FAIL"} ${out.cause ?? ""}`);
process.exit(out.ok ? 0 : 1);

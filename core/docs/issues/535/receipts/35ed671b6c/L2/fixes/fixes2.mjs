// Lane L2, second attempt of fixes.mjs part 4 after a diagnosis: the first
// attempt's Sandbox on a project Init'd through the worker was refused
// NoSuchProject (worker-init-then-sandbox.log, product defect), so here the
// project is Init'd natively, forked, then pushed (as the cells do).
//   node fixes2.mjs <machine> <receipt.json>
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
const out = { format: "lfs535-l2-fixes2/1", machine, product: process.env.LFS533_PRODUCT, startedAt: new Date().toISOString(), load1: loadavg()[0], checks: {}, steps: [] };
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
  {
    await step("fxw.store.create", () => lfs.store.create("fxw"));
    cleanup.push(["remove", "fxw"]);
    await step("fxw.project.init (native)", () => lfs.project.init("fxw/p", src));
  }
  // 4. workspace.mount(sandbox, "main") is refused InvalidInput; InUse fills detail.related
  {
    const fork = await step("fxw.branch.fork", () => lfs.branch.fork("fxw/p", "lane", "main"));
    const pushed = await step("fxw.store.push", () => lfs.store.push("fxw", machine));
    if (pushed.ok) cleanup.splice(cleanup.findIndex((c) => c[0] === "remove"), 1, ["pull", "fxw"]);
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

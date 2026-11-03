// Lane R2: targeted rerun of L2-D1 (fixed in 17b61af46b) at product 54161c9e2d.
// A native sqlite Store is pushed to <machine>, then `project.init` runs through
// the machine's worker (H11a: the source folder is on this Mac), then
// sandbox.create, workspace.mount of a forked side Branch, workspace.exec
// writing a file, workspace.commit, unmount, and file.read of the file from
// the Mac; then project.remove is refused InUse with detail.related while the
// Sandbox lives, and allowed after sandbox.close. Cleanup pulls the Store back
// and removes it. One attempt per call, no retry.
//   node d1.mjs <machine> <store> <receipt.json>
// Env: LAYERFS_HOME (holds <machine>), LAYERFS_K_NODE (package dir).
// Script fix after attempt 1 (apple-container-d1-attempt1.json): file.read answers
// {kind, text, ...}; attempt 1 passed it to Buffer.from and threw after the read
// (the product calls up to file.read had succeeded), so its project.remove checks
// never ran. Attempt 2 reads .text. After attempt 2 (apple-container) the overall
// `verdict` counted 4 checks of 5 (all five PASS there); fixed to 5 for microsandbox.
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { loadavg, tmpdir } from "node:os";
import { join } from "node:path";

const [machine, store, receipt] = process.argv.slice(2);
const { client } = await import(join(process.env.LAYERFS_K_NODE, "src/index.js"));
const lfs = client({ home: process.env.LAYERFS_HOME });
const project = `${store}/p`;
const sandbox = `${store}sb`;
const out = { format: "lfs535-r2-d1/1", machine, store, product: process.env.LFS533_PRODUCT, startedAt: new Date().toISOString(), load1: loadavg()[0], checks: {}, steps: [] };
const errorOf = (e) => ({ code: e?.code ?? null, message: String(e?.message ?? e).slice(0, 1500), detail: e?.detail ?? null });
const step = async (name, call) => {
  const at = performance.now();
  const startedAt = new Date().toISOString();
  try {
    const value = await call();
    out.steps.push({ name, startedAt, ms: Math.round(performance.now() - at), ok: true, value });
    return { ok: true, value };
  } catch (e) {
    out.steps.push({ name, startedAt, ms: Math.round(performance.now() - at), ok: false, error: errorOf(e) });
    return { ok: false, error: errorOf(e) };
  }
};
const verdict = (name, pass, evidence) => { out.checks[name] = { verdict: pass ? "PASS" : "FAIL", ...evidence }; };

const src = mkdtempSync(join(tmpdir(), "lfs535r2-d1-"));
writeFileSync(join(src, "a.txt"), "r2 d1 source\n");
const text = `written in ${sandbox} on ${machine}`;
let created = false, pushed = false, closed = false, mountedWs = null;
try {
  created = (await step("store.create", () => lfs.store.create(store))).ok;
  pushed = (await step("store.push", () => lfs.store.push(store, machine))).ok;
  const events = [];
  const init = await step("project.init (worker route, folder on the Mac)", () => lfs.project.init(project, src, { on_progress: (p) => events.push(p) }));
  out.initProgress = events;
  verdict("init-through-worker", init.ok && init.value?.machine === machine && events.some((e) => (e.progress ?? e).phase === "send"), { machine: init.value?.machine ?? null, phases: events.map((e) => (e.progress ?? e).phase) });
  const fork = await step("branch.fork lane", () => lfs.branch.fork(project, "lane", "main"));
  const sb = await step("sandbox.create", () => lfs.sandbox.create(project, sandbox, machine));
  verdict("sandbox-create-after-worker-init", sb.ok, { answer: sb.ok ? sb.value : sb.error });
  let wrote = false, committed = null;
  if (sb.ok) {
    const m = await step("workspace.mount lane", () => lfs.workspace.mount(sandbox, "lane"));
    if (m.ok) {
      mountedWs = m.value.ws;
      const ran = await step("workspace.exec write d1.txt", () => lfs.workspace.exec(mountedWs, `printf '%s\\n' '${text}' > d1.txt; cat d1.txt`, { message: "r2 d1" }));
      wrote = ran.ok && ran.value?.exit_code === 0;
      const c = await step("workspace.commit", () => lfs.workspace.commit(mountedWs, { message: "r2 d1" }));
      committed = c.ok ? c.value : c.error;
      const u = await step("workspace.unmount", () => lfs.workspace.unmount(mountedWs, { discard: false }));
      if (u.ok) mountedWs = null;
    }
  }
  const read = await step("file.read lane /d1.txt (from the Mac)", () => lfs.file.read(project, "lane", "/d1.txt"));
  const readText = read.ok ? (typeof read.value === "string" ? read.value : read.value?.text ?? null) : null;
  verdict("mount-exec-commit-read", fork.ok && sb.ok && wrote && (committed?.kind ?? committed?.outcome) === "Committed" && !!committed?.commit && read.ok && String(readText).includes(text), { commit: committed, read: read.ok ? readText : read.error });
  const rm1 = await step("project.remove while the Sandbox lives", () => lfs.project.remove(project));
  const related = rm1.ok ? null : rm1.error.detail?.related;
  verdict("project-remove-inuse-related", sb.ok && !rm1.ok && rm1.error.code === "InUse" && Array.isArray(related) && related.some((x) => x.kind === "sandbox" && x.name === sandbox), { answer: rm1.ok ? rm1.value : rm1.error });
  if (sb.ok) closed = (await step("sandbox.close", () => lfs.sandbox.close(sandbox))).ok;
  const rm2 = await step("project.remove after sandbox.close", () => lfs.project.remove(project));
  verdict("project-remove-after-close", closed && rm2.ok, { answer: rm2.ok ? rm2.value : rm2.error });
} finally {
  if (mountedWs) await step("cleanup workspace.unmount", () => lfs.workspace.unmount(mountedWs, { discard: true, force: true }));
  if (!closed && out.steps.some((s) => s.name === "sandbox.create" && s.ok)) await step("cleanup sandbox.close", () => lfs.sandbox.close(sandbox));
  if (pushed) await step("cleanup store.pull", () => lfs.store.pull(store));
  if (created) await step("cleanup store.remove", () => lfs.store.remove(store));
  rmSync(src, { recursive: true, force: true });
  out.endedAt = new Date().toISOString();
  const all = Object.values(out.checks);
  out.verdict = all.length === 5 && all.every((c) => c.verdict === "PASS") ? "PASS" : "FAIL";
  writeFileSync(receipt, JSON.stringify(out, null, 2) + "\n");
}
for (const [k, v] of Object.entries(out.checks)) console.log(`${v.verdict} ${k}`);
console.log(`verdict ${out.verdict}`);

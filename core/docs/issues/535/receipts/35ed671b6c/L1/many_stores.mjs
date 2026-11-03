// L1 (from K1) item 2 (#535 phase 8 item 3, native part): one home; `native` holds two
// sqlite Stores (s1, s2) and uses two PostgreSQL Stores (p1, p2). Four
// Sandboxes, one per Store, run at once: each mounts its project's Branch `lane` (forked from `main`),
// writes a file, Commits, and each Commit is read back from the caller with
// `file.read`. Every step's start/end time is recorded so the overlap shows.
//   node many_stores.mjs <node package src/index.js> <home> <receipt.json>
// Env: LFS535_PG_URL with "{db}" (no password), LFS535_PG_PASSWORD (env:NAME),
// LFS533_DAEMON + LFS533_LAYERFS (import-env, as the runner does).
import { mkdtempSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { tmpdir, loadavg } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const [pkg, home, receipt] = process.argv.slice(2);
const { client } = await import(pathToFileURL(pkg).href);
const lfs = client({ home });
const out = { format: "l1-many-stores/1", home, startedAt: new Date().toISOString(), load1: loadavg()[0], steps: [], stores: {}, ok: false };
const t0 = performance.now();
const ms = () => Math.round((performance.now() - t0) * 10) / 10;
async function step(name, fn) {
  const at = ms();
  try {
    const value = await fn();
    out.steps.push({ name, start: at, end: ms(), ok: true });
    return value;
  } catch (error) {
    out.steps.push({ name, start: at, end: ms(), ok: false, error: { code: error?.code ?? null, message: String(error?.message ?? error).slice(0, 1500) } });
    throw error;
  }
}
const empty = mkdtempSync(join(tmpdir(), "lfs535k1-ms-src-"));
const stores = [
  { name: "s1", kind: "sqlite" },
  { name: "s2", kind: "sqlite" },
  { name: "p1", kind: "postgresql", db: process.env.K1_DB1 ?? "m1" },
  { name: "p2", kind: "postgresql", db: process.env.K1_DB2 ?? "m2" },
];
const sandboxes = [];
try {
  execFileSync(process.env.LFS533_LAYERFS, ["import-env"], {
    env: { PATH: process.env.PATH, HOME: process.env.HOME, LAYERFS_HOME: home, LAYERFS_DAEMON: process.env.LFS533_DAEMON },
  });
  for (const s of stores) {
    if (s.kind === "sqlite") await step(`${s.name}.store.create`, () => lfs.store.create(s.name));
    else await step(`${s.name}.store.connect`, () => lfs.store.connect(s.name, process.env.LFS535_PG_URL.replace("{db}", s.db), { password: process.env.LFS535_PG_PASSWORD }));
    await step(`${s.name}.project.init`, () => lfs.project.init(`${s.name}/p`, empty));
    // `main` is the published base and is not mounted: a Branch forked from it is (as the cell runner does)
    await step(`${s.name}.branch.fork`, () => lfs.branch.fork(`${s.name}/p`, "lane", "main"));
  }
  out.storeList = await step("store.list", () => lfs.store.list());
  // the four Sandboxes at once, each its whole life concurrently
  const results = await Promise.allSettled(stores.map(async (s) => {
    const project = `${s.name}/p`;
    const name = `sb-${s.name}`;
    const created = await step(`${s.name}.sandbox.create`, () => lfs.sandbox.create(project, name, "native"));
    sandboxes.push(name);
    const { ws } = await step(`${s.name}.workspace.mount`, () => lfs.workspace.mount(name, "lane"));
    const token = `${s.name}-${Date.now()}-${Math.random().toString(16).slice(2)}`;
    const ran = await step(`${s.name}.workspace.exec`, () => lfs.workspace.exec(ws, `printf '%s\\n' ${token} > k1.txt`, { message: `k1 ${s.name} write` }));
    const committed = await step(`${s.name}.workspace.commit`, () => lfs.workspace.commit(ws, { message: `k1 ${s.name}` }));
    await step(`${s.name}.workspace.unmount`, () => lfs.workspace.unmount(ws, {}));
    const read = await step(`${s.name}.file.read`, () => lfs.file.read(project, "lane", "/k1.txt"));
    const text = typeof read === "string" ? read : read?.text ?? read?.content ?? JSON.stringify(read);
    const matches = String(text).trim() === token;
    out.stores[s.name] = { kind: s.kind, sandbox: created, exitCode: ran.exit_code, commit: committed, token, read, matches };
    if (!matches || ran.exit_code !== 0 || !committed?.commit) throw new Error(`${s.name}: read back ${JSON.stringify(read)} for ${token}, exit ${ran.exit_code}, commit ${JSON.stringify(committed)}`);
  }));
  const failed = results.filter((r) => r.status === "rejected").map((r) => String(r.reason?.message ?? r.reason));
  // overlap: all four Sandboxes alive at once (every create ended before the first close below)
  const creates = out.steps.filter((s) => s.name.endsWith(".sandbox.create"));
  const commits = out.steps.filter((s) => s.name.endsWith(".workspace.commit"));
  out.overlap = {
    createStarts: creates.map((s) => s.start), createEnds: creates.map((s) => s.end),
    commitStarts: commits.map((s) => s.start), commitEnds: commits.map((s) => s.end),
    allCreatedBefore: Math.max(...creates.map((s) => s.end)),
  };
  out.sandboxList = await step("sandbox.list", () => lfs.sandbox.list());
  out.ok = failed.length === 0 && Object.keys(out.stores).length === 4;
  out.cause = failed.join("; ") || null;
} catch (error) {
  out.cause = String(error?.message ?? error);
} finally {
  for (const name of sandboxes) {
    await step(`${name}.sandbox.close`, () => lfs.sandbox.close(name)).catch(() => {});
  }
  out.endedAt = new Date().toISOString();
  writeFileSync(receipt, JSON.stringify(out, null, 2) + "\n");
}
console.log(`many-stores native ${out.ok ? "PASS" : "FAIL"} ${out.cause ?? ""}`);
process.exit(out.ok ? 0 : 1);

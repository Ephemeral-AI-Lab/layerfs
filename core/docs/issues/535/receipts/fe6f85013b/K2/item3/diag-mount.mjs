// Diagnosis for item 3 attempt 2 (lane K2): two PostgreSQL Stores, one
// Sandbox each on <machine>, mounted one at a time and then both at once, in
// one Node process. One attempt per call.
//   node diag-mount.mjs <machine> <url1> <url2>   (LFS535_PG_PASSWORD = env:NAME)
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const { client } = await import(join(process.env.LAYERFS_K_NODE, "src/index.js"));
const lfs = client({ home: process.env.LAYERFS_HOME });
const empty = mkdtempSync(join(tmpdir(), "lfs535k2-d-"));
const password = process.env.LFS535_PG_PASSWORD;
const t0 = Date.now();
const show = async (label, call) => {
  const at = Date.now() - t0;
  try { const v = await call(); console.log(`+${at}ms`, label, "OK", JSON.stringify(v).slice(0, 200)); return v; }
  catch (e) { console.log(`+${at}ms`, label, "REFUSED", e.code, e.message.slice(0, 300)); }
};
const [machine, u1, u2] = process.argv.slice(2);
const S = [["d3", u1], ["d4", u2]];
for (const [s, u] of S) {
  await show(`${s} store.connect`, () => lfs.store.connect(s, u, { password }));
  await show(`${s} project.init`, () => lfs.project.init(`${s}/p`, empty));
  await show(`${s} branch.fork lane`, () => lfs.branch.fork(`${s}/p`, "lane", "main"));
}
for (const [s] of S) await show(`${s} sandbox.create (sequential)`, () => lfs.sandbox.create(`${s}/p`, s, machine));
for (const [s] of S) {
  const m = await show(`${s} mount lane (sequential)`, () => lfs.workspace.mount(s, "lane"));
  if (m) await show(`${s} unmount`, () => lfs.workspace.unmount(m.ws, { discard: true }));
}
const both = await Promise.all(S.map(([s]) => show(`${s} mount lane (concurrent)`, () => lfs.workspace.mount(s, "lane"))));
for (const m of both) if (m) await show(`${m.sandbox} unmount`, () => lfs.workspace.unmount(m.ws, { discard: true }));
for (const [s] of S) await show(`${s} sandbox.close`, () => lfs.sandbox.close(s));
for (const [s] of S) await show(`${s} store.disconnect`, () => lfs.store.disconnect(s));

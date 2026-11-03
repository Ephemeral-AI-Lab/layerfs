// Diagnosis for item 3 attempt 2 (lane K2): two PostgreSQL Stores in one
// Node process, each with project `p` and a Branch `lane`; then the Branch is
// read back in this process (no machine, no Sandbox). One attempt per call.
//   node diag-two-pg.mjs <url1> <url2>   (LFS535_PG_PASSWORD = env:NAME)
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const { client } = await import(join(process.env.LAYERFS_K_NODE, "src/index.js"));
const lfs = client({ home: process.env.LAYERFS_HOME });
const empty = mkdtempSync(join(tmpdir(), "lfs535k2-d-"));
const password = process.env.LFS535_PG_PASSWORD;
const show = async (label, call) => {
  try { const v = await call(); console.log(label, "OK", JSON.stringify(v).slice(0, 300)); return v; }
  catch (e) { console.log(label, "REFUSED", e.code, e.message.slice(0, 300)); }
};
const [u1, u2] = process.argv.slice(2);
for (const [s, u] of [["d1", u1], ["d2", u2]]) {
  await show(`${s} store.connect`, () => lfs.store.connect(s, u, { password }));
  await show(`${s} project.init`, () => lfs.project.init(`${s}/p`, empty));
  await show(`${s} branch.fork lane`, () => lfs.branch.fork(`${s}/p`, "lane", "main"));
}
for (const s of ["d1", "d2"]) await show(`${s} branch.get lane (sequential)`, () => lfs.branch.get(`${s}/p`, "lane"));
await Promise.all(["d1", "d2"].map((s) => show(`${s} branch.get lane (concurrent)`, () => lfs.branch.get(`${s}/p`, "lane"))));
for (const s of ["d1", "d2"]) await show(`${s} store.disconnect`, () => lfs.store.disconnect(s));

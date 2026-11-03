// "Fixed since fe6f85013b": workspace.mount(sandbox, "main") is refused
// InvalidInput (owner D14: only side Branches are mounted). In the shared
// home (LFS533_HOME) on <machine>: a sqlite Store mm pushed there, project
// mm/p, a Sandbox mm; mount "main" (expected refusal) then mount a side
// Branch "side" (expected to work) and unmount it; all removed afterwards.
// usage: node mountmain.mjs <machine>   (run from the frozen package's folder)
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const pkg = process.env.LFS_PKG;
const { client } = await import(join(pkg, "src/index.js"));
const machine = process.argv[2];
const lfs = client({ home: process.env.LFS533_HOME });
const empty = mkdtempSync(join(tmpdir(), "lfs535l3-mm-"));
const log = (name, value) => console.log(JSON.stringify({ at: new Date().toISOString(), step: name, ...value }));
const step = async (name, call) => {
  try { const v = await call(); log(name, { ok: true, value: v }); return v; }
  catch (e) { log(name, { ok: false, code: e?.code ?? null, message: String(e?.message ?? e) }); return undefined; }
};
await step("store.create", () => lfs.store.create("mm"));
await step("project.init", () => lfs.project.init("mm/p", empty));
await step("branch.fork", () => lfs.branch.fork("mm/p", "side", "main"));
await step("store.push", () => lfs.store.push("mm", machine));
await step("sandbox.create", () => lfs.sandbox.create("mm/p", "mm", machine));
const main = await step("workspace.mount main (expect InvalidInput)", () => lfs.workspace.mount("mm", "main"));
if (main?.ws) await step("workspace.unmount main", () => lfs.workspace.unmount(main.ws, { discard: true }));
const side = await step("workspace.mount side", () => lfs.workspace.mount("mm", "side"));
if (side?.ws) await step("workspace.unmount side", () => lfs.workspace.unmount(side.ws, { discard: true }));
await step("sandbox.close", () => lfs.sandbox.close("mm"));
await step("store.pull", () => lfs.store.pull("mm"));
await step("store.remove", () => lfs.store.remove("mm"));
rmSync(empty, { recursive: true, force: true });

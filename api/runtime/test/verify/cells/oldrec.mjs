// #529 §8 old records, step two: with this change's SDK (LFS527_AGENT),
// the Sandboxes make.mjs made before it (records with no platform) are
// listed by runtime.sandbox.records with platform null, then by
// project.list_sandboxes with the platform their runtime and the home imply
// (LFS529_EXPECT, JSON { sandbox: platform }); a reclaim with no
// LAYERFS_LOCAL_DIR is refused and keeps them; then, with this home's
// LAYERFS_LOCAL_DIR as before #529, each is mounted, used,
// unmounted and closed. Whatever happens, what is still mounted or running
// is released.
//   node oldrec.mjs <project>
// The last stdout line is `LFS529-RESULT <json>`.
import { pathToFileURL } from "node:url";

const [project] = process.argv.slice(2);
const expect = JSON.parse(process.env.LFS529_EXPECT);
const out = { project, ok: false, cause: null, steps: [], checks: {} };
const errorOf = (error) => ({ code: error?.code ?? null, message: String(error?.message ?? error).slice(0, 2000) });
async function step(name, call) {
  try {
    const value = await call();
    out.steps.push({ name, ok: true });
    return value;
  } catch (error) {
    out.steps.push({ name, ok: false, error: errorOf(error) });
    throw Object.assign(new Error(`${name}: ${error?.code ?? ""} ${error?.message ?? error}`.trim()), { step: name });
  }
}
const need = (cond, what) => { if (!cond) throw Object.assign(new Error(what), { step: "check" }); };
const agent = process.env.LFS527_AGENT;
const { LayerFS } = await import(pathToFileURL(agent).href);
const { invokeBackend } = await import(pathToFileURL(agent.replace(/index\.js$/, "internal.js")).href);
const mounted = new Set(), running = new Set(Object.keys(expect));
let layerfs;
try {
  const records = await step("runtime.sandbox.records", () => invokeBackend("runtime.sandbox.records", { project, home: process.env.LAYERFS_HOME }, {}));
  out.checks.records = records.items.map(({ sandbox, runtime, platform, state }) => ({ sandbox, runtime, platform, state }));
  need(out.checks.records.length === Object.keys(expect).length && out.checks.records.every((r) => r.platform === null && r.state === "running"), "the old records are not all running with no platform");
  // a caller with no LAYERFS_LOCAL_DIR (the home's name prefix kept, so only
// the marker rule can refuse): an old local record has no owner
  // marker, so it is refused InvalidInput naming that setting and kept
  // (parent decision, 2026-11-01); the rest below sets it, as an old home did
  const unset = await invokeBackend("runtime.sandbox.reclaim", { project, home: process.env.LAYERFS_HOME, deploy: { sandbox: "docker", ...(process.env.LAYERFS_CONTAINER_PREFIX ? { containerPrefix: process.env.LAYERFS_CONTAINER_PREFIX } : {}) } }, {}).then(
    (value) => ({ value }), (error) => ({ refused: errorOf(error) }));
  const kept = (await invokeBackend("runtime.sandbox.records", { project, home: process.env.LAYERFS_HOME }, {})).items.map(({ sandbox, state }) => ({ sandbox, state }));
  out.checks.noLocalDir = { localDirOfTheRest: process.env.LAYERFS_LOCAL_DIR ?? null, reclaim: unset, kept };
  need(unset.refused?.code === "InvalidInput" && /owner marker/.test(unset.refused.message) && /set LAYERFS_LOCAL_DIR/.test(unset.refused.message) && kept.length === Object.keys(expect).length && kept.every((r) => r.state === "running"),
    `with no LAYERFS_LOCAL_DIR the old local record was not refused and kept: ${JSON.stringify(out.checks.noLocalDir)}`);
  layerfs = await step("open", () => LayerFS.open(project));
  out.checks.listed = (await step("list_sandboxes", () => layerfs.project.list_sandboxes(1000, 0))).items;
  need(JSON.stringify(Object.fromEntries(out.checks.listed.map((s) => [s.sandbox, s.platform]))) === JSON.stringify(expect), `listed ${JSON.stringify(out.checks.listed)}`);
  out.checks.used = {};
  for (const sandbox of Object.keys(expect)) {
    const ws = await step(`mount ${sandbox}`, () => layerfs.workspace.mount(sandbox, "lane"));
    mounted.add(ws);
    need(ws.platform === expect[sandbox], `mount ${sandbox} answered platform ${ws.platform}`);
    const ran = await step(`exec ${sandbox}`, () => layerfs.workspace.exec(ws, "printf ok", "lfs529 old record"));
    out.checks.used[sandbox] = { platform: ws.platform, image: ws.image, exitCode: ran.exitCode, stdout: ran.stdout };
    need(ran.exitCode === 0 && ran.stdout === "ok", `exec in ${sandbox} answered ${ran.exitCode} ${ran.stdout}`);
    await step(`unmount ${sandbox}`, () => layerfs.workspace.unmount(ws, true));
    mounted.delete(ws);
    await step(`close ${sandbox}`, () => layerfs.sandbox.close(sandbox));
    running.delete(sandbox);
  }
  out.checks.after = (await step("list_sandboxes after", () => layerfs.project.list_sandboxes(1000, 0))).items;
  need(out.checks.after.length === 0, "a Sandbox is still listed");
  out.ok = true;
} catch (error) {
  out.cause = error.message;
  out.failedStep = error.step ?? null;
} finally {
  for (const ws of mounted) await layerfs?.workspace.unmount(ws, true, true).catch(() => {});
  for (const sandbox of running) await layerfs?.sandbox.close(sandbox).catch((error) => out.steps.push({ name: `cleanup.close ${sandbox}`, ok: false, error: errorOf(error) }));
  await layerfs?.close().catch(() => {});
}
console.log(`LFS529-RESULT ${JSON.stringify(out)}`);
process.exit(0);

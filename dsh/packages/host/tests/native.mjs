// The native route's Host settings (sandbox: native), without dsh, Docker or a Store:
//   node dsh/packages/host/tests/native.mjs
// Configuration and its defaults, the bash environment an agent gets, Init's
// checks and its Store folder instead of a Docker volume, removal of a
// project's local Sandboxes, and the overview that asks no Docker. The
// Docker route's defaults are checked unchanged beside it.
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, rmSync, statSync, writeFileSync } from "node:fs";
import { register } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";

const STAND_IN = "export function appendPluginRecord(session, type, data) { session.records.push({ type, data }); }";
register(`data:text/javascript,${encodeURIComponent(`
export async function resolve(specifier, context, next) {
  if (specifier === "@deepseek-ai/dsh-session") return { url: ${JSON.stringify(`data:text/javascript,${encodeURIComponent(STAND_IN)}`)}, shortCircuit: true };
  return next(specifier, context);
}`)}`);
const { resolveConfig, missingDeployment, storeDirOf } = await import("../src/config.js");
const { BindingStore } = await import("../src/binding.js");
const { Collector } = await import("../src/collector.js");
const { contributor, projectContext } = await import("../src/env.js");
const { deploymentEnv } = await import("../src/layerfs-client.js");
const { hostAccess, prepareInit } = await import("../src/init.js");
const { removeProject } = await import("../src/remove.js");
const { overview } = await import("../src/runtime.js");
const { deployment } = await import("../../../../sdk/js/src/client.js");

const dir = mkdtempSync(join(tmpdir(), "dsh-layerfs-native-"));
const FOLDER = "/work/website";
const session = (id, cwd) => ({ header: { id, cwd }, records: [] });
const saved = process.env.LAYERFS_SANDBOX;

try {
  // the Docker route is the default, unchanged: image and daemon (and a bundle on macOS) are required
  delete process.env.LAYERFS_SANDBOX;
  const plain = resolveConfig({ home: join(dir, "plain-home"), traceDir: join(dir, "t") }, { dir: join(dir, "p0") });
  assert.equal(plain.native, false);
  assert.deepEqual([plain.localDir, plain.stores], [null, null]);
  assert.deepEqual(missingDeployment(plain), ["image", "daemon", ...(process.platform === "darwin" ? ["bundle"] : [])]);
  // LAYERFS_SANDBOX=local is the SDK's; it does not select anything here
  process.env.LAYERFS_SANDBOX = "local";
  assert.equal(resolveConfig({ home: join(dir, "plain-home"), traceDir: join(dir, "t") }, { dir: join(dir, "p0") }).native, false);
  assert.throws(() => resolveConfig({ sandbox: "vm", home: join(dir, "plain-home") }, { dir: join(dir, "p0") }), /docker or native, not vm/);

  // native: from the config, or the dsh process's LAYERFS_SANDBOX=native; a bundle never ports it
  process.env.LAYERFS_SANDBOX = "native";
  const byEnv = resolveConfig({ home: join(dir, "env-home"), traceDir: join(dir, "t"), bundle: "/b", localDir: join(dir, "env-local"), stores: join(dir, "env-stores") }, { dir: join(dir, "p1") });
  assert.equal(byEnv.native, true);
  assert.equal(byEnv.box, null, "the native route runs no Store container, even with a bundle on macOS");
  assert.match(byEnv.sdk, /\/sdk\/js\/src\/index\.js$/);
  delete process.env.LAYERFS_SANDBOX;

  const spec = resolveConfig({ sandbox: "native", home: join(dir, "home"), traceDir: join(dir, "traces"), localDir: join(dir, "local"), stores: join(dir, "stores") }, { dir: join(dir, "profile") });
  assert.deepEqual(missingDeployment(spec), ["daemon"], "native needs the daemon only: no image, no bundle");
  assert.equal(statSync(spec.localDir).mode & 0o777, 0o755, "a local Sandbox root is 0755");
  assert.ok(existsSync(spec.stores));
  // defaults: everything under the OS temp area, which dsh's workspace-write sandbox may write
  const defaults = resolveConfig({ sandbox: "native", daemon: "/d/layerfs-daemon", stateDir: join(dir, "state-defaults") }, undefined);
  assert.ok(defaults.localDir.endsWith("/dsh-layerfs/local") && defaults.stores.endsWith("/dsh-layerfs/stores") && defaults.home.endsWith("/dsh-layerfs/home"));
  assert.deepEqual(missingDeployment(defaults), []);

  const native = { ...spec, daemon: "/d/layerfs-daemon" };
  const project = { name: "website", source: FOLDER, volume: "layerfs-website-store-abc", created: 1 };
  const env = deploymentEnv(native, project);
  assert.deepEqual(env, {
    HOME: native.home, SANDBOX: "native", DAEMON: "/d/layerfs-daemon", LOCAL_DIR: native.localDir, VOLUME: project.volume,
    STORE_PATH: join(native.stores, project.volume, "global", "store.sqlite"), CONTAINER_PREFIX: "layerfs-",
  });
  assert.equal(storeDirOf(native, project.volume), join(native.stores, project.volume));
  assert.equal(deploymentEnv({ ...native, nfsMount: "/sbin/mount_nfs -o vers=3 127.0.0.1:/ {dir}" }, project).NFS_MOUNT, "/sbin/mount_nfs -o vers=3 127.0.0.1:/ {dir}");

  // the SDK reads that environment as its native deployment, from DSH_LAYERFS_* alone (what a bash call has)
  const keep = Object.fromEntries(Object.keys(process.env).filter((k) => k.startsWith("LAYERFS_") || k.startsWith("DSH_LAYERFS_")).map((k) => [k, process.env[k]]));
  for (const k of Object.keys(keep)) delete process.env[k];
  for (const [k, v] of Object.entries({ ...env, NFS_MOUNT: "/sbin/mount_nfs -o x 127.0.0.1:/ {dir}" })) process.env[`DSH_LAYERFS_${k}`] = v;
  assert.deepEqual(deployment(), { sandbox: "native", daemon: "/d/layerfs-daemon", localDir: native.localDir, containerPrefix: "layerfs-" });
  assert.equal(process.env.LAYERFS_NFS_MOUNT, "/sbin/mount_nfs -o x 127.0.0.1:/ {dir}", "DSH_LAYERFS_NFS_MOUNT stands in for LAYERFS_NFS_MOUNT");
  process.env.LAYERFS_NFS_REFRESH = "0"; process.env.DSH_LAYERFS_NFS_REFRESH = "1";
  deployment();
  assert.equal(process.env.LAYERFS_NFS_REFRESH, "0", "a set LAYERFS_NFS_* wins");
  for (const k of Object.keys(process.env)) if (k.startsWith("LAYERFS_") || k.startsWith("DSH_LAYERFS_")) delete process.env[k];
  Object.assign(process.env, keep);

  // a bash call of a session on a native project: every value declared, none of Docker's
  const live = new Map(), ctx = { sessions: { get: (id) => live.get(id) } };
  const bindings = new BindingStore(native.stateDir), collector = new Collector(ctx, native, bindings);
  const bash = contributor(ctx, native, bindings, collector);
  bindings.addProject(FOLDER, project);
  const root = session("s-root", FOLDER); live.set("s-root", root);
  const vars = bash.resolve({ agent: { session: root }, callId: "c1" });
  for (const key of Object.keys(vars)) assert.ok(key in bash.variables, `${key} is declared`);
  assert.equal(vars.DSH_LAYERFS_SANDBOX, "native");
  assert.equal(vars.DSH_LAYERFS_LOCAL_DIR, native.localDir);
  assert.equal(vars.DSH_LAYERFS_STORE_PATH, env.STORE_PATH);
  assert.ok(!("DSH_LAYERFS_DOCKER" in vars) && !("DSH_LAYERFS_IMAGE" in vars) && !("DSH_LAYERFS_STORE_CONTAINER" in vars));
  assert.match(vars.DSH_LAYERFS_SDK, /\/sdk\/js\/src\/index\.js$/);
  assert.match(projectContext(bindings, collector, root, native), /run natively on this machine/);
  assert.doesNotMatch(projectContext(bindings, collector, root, plain), /natively/);

  // Init: daemon required; the Store folder is made instead of a Docker volume; the home must be this user's
  assert.throws(() => prepareInit(spec, bindings, { folder: dir }), { code: "not-configured", message: /set daemon for/ });
  const prepared = prepareInit(native, bindings, { folder: dir, name: "shop" });
  assert.match(prepared.project.volume, /^layerfs-shop-store-/);
  const uid = process.getuid?.() ?? 1000;
  assert.equal(hostAccess(native, { platform: "darwin", uid }), null);
  assert.match(hostAccess(native, { platform: "darwin", uid: uid + 1 }), /belongs to uid \d+, not this Host's uid/);

  // removal: refused while a daemon of its Sandboxes runs or a Workspace is mounted; else its folders, Store folder and home entry go
  const sb1 = join(native.localDir, "layerfs-website-1"), sb2 = join(native.localDir, "layerfs-website-2"), other = join(native.localDir, "layerfs-website-x-1");
  for (const d of [sb1, sb2, other]) mkdirSync(join(d, "workspaces"), { recursive: true });
  mkdirSync(join(native.stores, project.volume, "global"), { recursive: true });
  writeFileSync(join(native.stores, project.volume, "global", "store.sqlite"), "x");
  mkdirSync(join(native.home, "website"), { recursive: true });
  const fakeTrees = { forget: async () => {} };
  const runWith = ({ pids = "", commands = {}, mounts = "" }) => async (file, args) => {
    if (file === "pgrep") {
      assert.deepEqual(args.slice(0, 1), [process.platform === "darwin" ? "-lf" : "-af"]);
      if (!pids) throw Object.assign(new Error("none"), { code: 1 });
      return { stdout: pids.split("\n").filter(Boolean).map((pid) => `${pid} ${commands[pid] ?? ""}\n`).join("") };
    }
    if (file === "mount") return { stdout: mounts };
    throw new Error(`unexpected ${file}`);
  };
  await assert.rejects(removeProject(native, bindings, fakeTrees, "website", runWith({ pids: "41\n42\n", commands: { 41: `layerfs-daemon --config ${sb1}/config/daemon.setup`, 42: `layerfs-daemon --config ${other}/config/daemon.setup` } })), { code: "busy", message: /1 Sandbox daemon and 0 Workspace mounts/ });
  await assert.rejects(removeProject(native, bindings, fakeTrees, "website", runWith({ mounts: `127.0.0.1:/ on ${sb2}/workspaces/w1 (nfs)\n` })), { code: "busy", message: /1 Workspace mount/ });
  // a check that cannot run refuses and removes nothing: pgrep failing otherwise than "no match", or no mount table
  const failing = (fail) => async (file, args) => {
    if (file === fail) throw Object.assign(new Error(`${file} refused`), { code: fail === "pgrep" ? 3 : "EPERM" });
    return runWith({})(file, args);
  };
  await assert.rejects(removeProject(native, bindings, fakeTrees, "website", failing("pgrep")), { code: "busy", message: /cannot tell whether a Sandbox daemon still runs/ });
  await assert.rejects(removeProject(native, bindings, fakeTrees, "website", failing("mount")), { code: "busy", message: /cannot read the mount table/ });
  assert.ok(existsSync(sb1) && existsSync(sb2) && existsSync(join(native.stores, project.volume)), "nothing was removed");
  // another project's daemon and mount (website-x) do not hold website
  assert.deepEqual(await removeProject(native, bindings, fakeTrees, "website", runWith({ pids: "42\n", commands: { 42: `layerfs-daemon --config ${other}/config/daemon.setup` }, mounts: `127.0.0.1:/ on ${other}/workspaces/w1 (nfs)\n` })), { removed: "website", volume: project.volume });
  assert.deepEqual([existsSync(sb1), existsSync(sb2), existsSync(other), existsSync(join(native.stores, project.volume)), existsSync(join(native.home, "website")), bindings.byName("website")], [false, false, true, false, false, null]);

  // the overview asks no Docker: the engine is native, no containers, no Store container
  const view = await overview(native, bindings, { spec: native }, { runningCalls: () => 0 });
  assert.deepEqual([view.engine.native, view.store, view.leftovers, view.level], [true, null, [], "ok"]);

  console.log("native: ok");
} finally {
  if (saved === undefined) delete process.env.LAYERFS_SANDBOX; else process.env.LAYERFS_SANDBOX = saved;
  rmSync(dir, { recursive: true, force: true });
}

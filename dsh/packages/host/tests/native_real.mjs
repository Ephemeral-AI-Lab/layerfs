// The native route end to end, with the real SDK and a native LayerFS build, without dsh:
//   node dsh/packages/host/tests/native_real.mjs <layerfs-daemon>
//   env: LAYERFS_NATIVE (the addon, else sdk/js/native/layerfs.node)
//        NATIVE_WORKSPACE=0 skips the Workspace part (a machine whose user cannot NFS-mount, as a Linux CI box)
//        NATIVE_NFS_MOUNT: the layerfs-host nfsMount setting (reaches agents as DSH_LAYERFS_NFS_MOUNT)
//        NATIVE_KEEP=1 keeps the test folder; NATIVE_IDLE_WAIT=<s> waits that long for idle Sandboxes before removal (default 150)
// The Host side as dsh web runs it: settings (sandbox native), Init (which
// imports a folder into <stores>/<volume>, starting no Sandbox), the bash
// environment of a session on the project, then agent bash calls as
// separate processes with that environment alone: the lfs CLI (status,
// fork, write, run in a Workspace), a script that mounts a Workspace, writes
// into its mount path from a shell whose working directory it is, commits
// and unmounts, and a merge and publish. The Host then reads History (its
// shared client) and main's files, the trace lines are counted, and the
// project is removed once its Sandbox closed. No Docker is asked anywhere.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { register } from "node:module";
import { tmpdir } from "node:os";
import { join } from "node:path";

const STAND_IN = "export function appendPluginRecord(session, type, data) { session.records.push({ type, data }); }";
register(`data:text/javascript,${encodeURIComponent(`
export async function resolve(specifier, context, next) {
  if (specifier === "@deepseek-ai/dsh-session") return { url: ${JSON.stringify(`data:text/javascript,${encodeURIComponent(STAND_IN)}`)}, shortCircuit: true };
  return next(specifier, context);
}`)}`);
const { resolveConfig, missingDeployment } = await import("../src/config.js");
const { BindingStore } = await import("../src/binding.js");
const { Collector } = await import("../src/collector.js");
const { contributor } = await import("../src/env.js");
const { initProject } = await import("../src/init.js");
const { TreeReader } = await import("../src/trees.js");
const { removeProject } = await import("../src/remove.js");

const daemon = process.argv[2];
if (!daemon || !existsSync(daemon)) { console.log("usage: node native_real.mjs <layerfs-daemon>"); process.exit(2); }
const WORKSPACE = process.env.NATIVE_WORKSPACE !== "0";
const t0 = Date.now(), at = () => `${((Date.now() - t0) / 1000).toFixed(1)}s`;
const say = (text) => console.log(`[${at()}] ${text}`);
const work = mkdtempSync(join(realpathSync(tmpdir()), "dsh-native-real-"));
const folder = join(work, "shop");
mkdirSync(join(folder, "src"), { recursive: true });
writeFileSync(join(folder, "README.md"), "# Shop\n");
writeFileSync(join(folder, "src", "app.js"), "console.log('shop');\n");
writeFileSync(join(folder, ".lww"), "%include .gitignore\n");
writeFileSync(join(folder, ".gitignore"), "node_modules/\n");

const spec = resolveConfig({ sandbox: "native", daemon, home: join(work, "home"), traceDir: join(work, "traces"), localDir: join(work, "local"), stores: join(work, "stores"), stateDir: join(work, "state"), ...(process.env.NATIVE_NFS_MOUNT ? { nfsMount: process.env.NATIVE_NFS_MOUNT } : {}) }, undefined);
const mounts = () => (spawnSync("mount", [], { encoding: "utf8" }).stdout || "").split("\n").filter((l) => l.includes(` on ${spec.localDir}/`));
const daemons = () => (spawnSync("pgrep", ["-f", `layerfs-daemon --config ${spec.localDir}/`], { encoding: "utf8" }).stdout || "").split("\n").filter(Boolean);
let ok = false;
try {
  say(`work=${work} platform=${process.platform} node=${process.version} workspace=${WORKSPACE}`);
  assert.deepEqual(missingDeployment(spec), [], "status: configured");

  // Init: the Store folder, the import (counts read while it runs), the folder's default project
  const steps = [];
  const bindings = new BindingStore(spec.stateDir);
  const project = await initProject(spec, bindings, { folder, name: "shop" }, (r) => steps.push(r.step ?? (r.imported ? "imported" : r.scanned ? "scanned" : "?")));
  say(`init: ${project.name} volume=${project.volume} steps=${[...new Set(steps)].join(",")}`);
  assert.ok(existsSync(join(spec.stores, project.volume, "global", "store.sqlite")), "the Store is installed in <stores>/<volume>/global");
  assert.equal(bindings.workspace(folder).projects.length, 1);

  // the bash environment of a session in that folder: what dsh gives an agent's bash call
  const live = new Map(), ctx = { sessions: { get: (id) => live.get(id) } };
  const collector = new Collector(ctx, spec, bindings);
  const root = { header: { id: "session-root", cwd: folder }, records: [] };
  live.set(root.header.id, root);
  const vars = contributor(ctx, spec, bindings, collector).resolve({ agent: { session: root }, callId: "call_1" });
  assert.equal(vars.DSH_LAYERFS_PROJECT, "shop");
  const env = { PATH: process.env.PATH, HOME: process.env.HOME, TMPDIR: process.env.TMPDIR ?? "/tmp", ...(process.env.LAYERFS_NATIVE ? { LAYERFS_NATIVE: process.env.LAYERFS_NATIVE } : {}), ...vars };
  const bash = (command, input) => {
    const r = spawnSync("bash", ["-c", command], { cwd: folder, env, input, encoding: "utf8", timeout: 300_000 });
    say(`bash$ ${command.split("\n")[0].slice(0, 90)} -> rc=${r.status}\n${(r.stdout + r.stderr).trim().split("\n").map((l) => "    " + l).join("\n")}`);
    return r;
  };
  const cli = 'node "$DSH_LAYERFS_CLI"';
  assert.equal(bash(`${cli} status`).status, 0);
  assert.equal(bash(`${cli} fork feat`).status, 0);
  assert.equal(bash(`printf 'hello native\\n' | ${cli} write feat /hello.txt "add hello"`).status, 0);
  assert.equal(bash(`${cli} cat feat /hello.txt`).stdout.trim(), "hello native");
  if (WORKSPACE) {
    // one Workspace through the CLI: mount, exec, Commit, unmount
    const run = bash(`${cli} run feat "echo from-workspace > ws.txt && id -u && ls"`);
    assert.equal(run.status, 0);
    assert.match(run.stdout, /ws\.txt/);
    // the mount path as a shell's working directory, as an agent's own bash would use it
    const script = `
      const { LayerFS } = await import(process.env.DSH_LAYERFS_SDK);
      const { spawnSync } = await import("node:child_process");
      const lf = new LayerFS(process.env.DSH_LAYERFS_PROJECT);
      const ws = await lf.workspace.mount("feat");
      console.log("MOUNT", ws.sandbox, ws.path);
      const sh = spawnSync("bash", ["-c", "echo direct > direct.txt && cat ws.txt && pwd -P"], { cwd: ws.path, encoding: "utf8" });
      console.log("SHELL", sh.status, sh.stdout.trim().replace(/\\n/g, " | "), sh.stderr.trim());
      const c = await lf.workspace.commit(ws, "direct write in the mount path");
      console.log("COMMIT", c.kind);
      await lf.workspace.unmount(ws);
      console.log("UNMOUNTED");
      await lf.close?.();`;
    const direct = bash(`node --input-type=module -e '${script.replace(/'/g, "'\\''")}'`);
    assert.equal(direct.status, 0);
    assert.match(direct.stdout, /SHELL 0 from-workspace/);
    assert.match(direct.stdout, /COMMIT Committed/);
    assert.equal(bash(`${cli} cat feat /direct.txt`).stdout.trim(), "direct");
  }
  // a second Branch, then merge both into a new one and publish it
  assert.equal(bash(`${cli} fork side && printf 'side\\n' | ${cli} write side /side.txt "add side"`).status, 0);
  const merge = bash(`node --input-type=module -e '
    const { LayerFS } = await import(process.env.DSH_LAYERFS_SDK);
    const lf = new LayerFS(process.env.DSH_LAYERFS_PROJECT);
    const m = await lf.branch.merge("feat", "side", "joined", "join feat and side");
    console.log("MERGE", m.kind);
    const p = await lf.branch.publish("joined");
    console.log("PUBLISH", p.kind);
    await lf.close?.();'`);
  assert.equal(merge.status, 0);
  assert.match(merge.stdout, /MERGE (Merged|Clean|Created|Forked|\w+)/);
  assert.match(merge.stdout, /PUBLISH \w+/);

  // the Host's reads: History and main's files, through its shared client (as the History tab)
  await new Promise((r) => setTimeout(r, spec.pollMs * 3));
  const trees = new TreeReader(spec);
  const state = await trees.state(project);
  const s = state.s;
  const branchNames = Object.keys(s.branches ?? {});
  const commits = Object.keys(s.commits ?? {}).length;
  say(`host state: branches=${JSON.stringify(Object.values(state.labels ?? {}).length)} keys=${Object.keys(s).join(",")} commits=${commits} branchKeys=${branchNames.join(",")}`);
  const mainFiles = await trees.use(project, ({ layerfs }) => layerfs.fs.list("main", "/"));
  const names = mainFiles.items.map((e) => e.name);
  say(`main after publish: ${names.join(" ")}`);
  for (const name of ["hello.txt", "side.txt", ...(WORKSPACE ? ["ws.txt", "direct.txt"] : [])]) assert.ok(names.includes(name), `main has ${name}`);
  trees.dispose();

  // trace lines of the agent's calls (the Host turns them into session records)
  const traceFiles = readdirSync(spec.traceDir, { recursive: true }).filter((f) => String(f).endsWith(".jsonl") || String(f).includes("trace"));
  const lines = traceFiles.flatMap((f) => { try { return readFileSync(join(spec.traceDir, String(f)), "utf8").split("\n").filter(Boolean); } catch { return []; } });
  say(`trace: ${traceFiles.length} file(s), ${lines.length} line(s), methods=${[...new Set(lines.map((l) => { try { return JSON.parse(l).method; } catch { return "?"; } }))].join(",")}`);
  assert.ok(lines.length > 0, "the agent's SDK calls were traced");

  // removal: refused while its Sandbox runs, then the Sandbox closes when idle (2 min) and it goes
  if (WORKSPACE && daemons().length) {
    await assert.rejects(removeProject(spec, bindings, { forget: async () => {} }, "shop"), { code: "busy" });
    say(`remove refused while the Sandbox runs (daemons=${daemons().length}, mounts=${mounts().length}); waiting for it to close itself`);
    const until = Date.now() + Number(process.env.NATIVE_IDLE_WAIT ?? 150) * 1000;
    while (daemons().length && Date.now() < until) await new Promise((r) => setTimeout(r, 2000));
  }
  if (!daemons().length && !mounts().length) {
    assert.deepEqual(await removeProject(spec, bindings, { forget: async () => {} }, "shop"), { removed: "shop", volume: project.volume });
    assert.ok(!existsSync(join(spec.stores, project.volume)) && !existsSync(join(spec.home, "shop")));
    say("removed: Store folder, home entry, Sandbox folders");
  } else say(`NOT removed: daemons=${daemons().length} mounts=${mounts().length} (still live)`);
  ok = true;
} finally {
  say(`leftovers: mounts=${mounts().length} daemons=${daemons().length}`);
  for (const pid of daemons()) try { process.kill(Number(pid), "SIGTERM"); } catch {}
  for (const line of mounts()) {
    const path = / on (.*) \(/.exec(line)?.[1] ?? line.split(" on ")[1]?.split(" ")[0];
    if (path) spawnSync(process.platform === "darwin" ? "/sbin/umount" : "umount", ["-f", path]);
  }
  if (process.env.NATIVE_KEEP !== "1" && !mounts().length) rmSync(work, { recursive: true, force: true });
  console.log(ok ? `native_real: ok (${at()})` : `native_real: FAILED (${at()})`);
}

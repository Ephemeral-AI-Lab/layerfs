// #352 F-029: a call that names a mount finds it in this process's mount table
// and asks the daemon nothing about the Sandbox's other mounts.
//
//   node api/agent/test/verify/exec_mount_lookup_real.mjs outer <daemon> <addon> <image>
//
// `outer` (on the Docker host) makes a fresh Store volume and runs `inner` in
// the runtime image. Process A starts Sandbox 1 (sandbox.create) and mounts four
// Workspaces there, then traces one `workspace.exec`: its trace line carries no daemon exchange
// (before F-029 it carried one status exchange per mount). A child process
// sharing LAYERFS_HOME mounts a fifth Workspace and unmounts one of A's. A then
// execs in the fifth (found from the shared records), is refused NotMounted in
// the one the child removed (after one read of the shared records), and still
// execs in the others. The child's remount after its unmount gets a new
// directory: the daemon never reuses one, so a stale path can't name a new
// Workspace. The Sandbox's container is named `<prefix><project>-<n>` and
// carries its labels (#373). One attempt per step; nothing retried.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { get } from "node:http";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { docker, inRuntime, REPO, STORE_PATH } from "./container.mjs";

const MOUNTS = 4;

/** One Engine read over the Docker socket, its JSON body. */
const engine = (path) => new Promise((resolve, reject) => {
  get({ socketPath: process.env.LAYERFS_DOCKER, path }, (response) => {
    const parts = [];
    response.on("data", (part) => parts.push(part));
    response.on("end", () => resolve(JSON.parse(Buffer.concat(parts).toString("utf8"))));
  }).on("error", reject);
});

async function inner([phase]) {
  const { LayerFS } = await import("@layerfs/agent");
  const home = process.env.LAYERFS_HOME;
  const trace = join(home, "exec.trace.jsonl");
  const shared = join(home, "paths.json");
  Object.assign(process.env, { LAYERFS_STORE_PATH: STORE_PATH, LAYERFS_TRACE: trace });
  if (phase === "child") {
    const layerfs = await LayerFS.open("site");
    const paths = JSON.parse(readFileSync(shared, "utf8"));
    const fifth = await layerfs.workspace.mount("1", "work");
    await layerfs.workspace.unmount(paths[1], true);
    // a remount after that unmount: the daemon never reuses a namespace, so
    // never its directory (layerfs-daemon control/serving.rs)
    const sixth = await layerfs.workspace.mount("1", "work");
    writeFileSync(shared, JSON.stringify([...paths, fifth, sixth]));
    console.log("CHILD ok");
    return;
  }
  const source = mkdtempSync(join(tmpdir(), "layerfs-f029-"));
  writeFileSync(join(source, "README.md"), "workspace file\n");
  const layerfs = await LayerFS.init("site", source);
  const { LayerFSBackend } = await import("@layerfs/backend");
  rmSync(source, { recursive: true, force: true });
  await layerfs.branch.fork("work", "main");
  const paths = [];
  assert.equal((await layerfs.sandbox.create()).sandbox, "1");
  for (let i = 0; i < MOUNTS; i++) paths.push(await layerfs.workspace.mount("1", "work"));
  assert.deepEqual(paths.map((ws) => ws.sandbox), Array(MOUNTS).fill("1"), "every Workspace in Sandbox 1");
  const container = readFileSync(join(home, "site", "sandboxes", "1", "container"), "utf8").trim();
  const seen = await engine(`/v1.54/containers/${container}/json`);
  assert.equal(seen.Name, `/${process.env.LAYERFS_CONTAINER_PREFIX}site-1`, "the container's name says whose and which");
  for (const [key, value] of [["layerfs.role", "sandbox"], ["layerfs.project", "site"], ["layerfs.sandbox", "1"]]) {
    assert.equal(seen.Config.Labels[key], value, `label ${key}`);
  }
  rmSync(trace, { force: true });
  assert.equal((await layerfs.workspace.exec(paths[0], "cat README.md")).exitCode, 0);
  await LayerFSBackend.of(layerfs).daemon.workspace.status(paths[0]);
  const lines = readFileSync(trace, "utf8").trim().split("\n").map((l) => JSON.parse(l)).filter((l) => l.phase === "end");
  const exec = lines.find((l) => l.method === "workspace.exec");
  const status = lines.find((l) => l.method === "backend.daemon.workspace.status");
  // the status call shows what one daemon exchange looks like in the trace
  assert.ok((status.daemon ?? []).length >= 1, `status carries its exchange: ${JSON.stringify(status).slice(0, 300)}`);
  assert.equal((exec.daemon ?? []).length, 0, `exec asks the daemon nothing: ${JSON.stringify(exec.daemon)?.slice(0, 300)}`);
  console.log(`EXEC daemon exchanges ${(exec.daemon ?? []).length} with ${MOUNTS} mounts; status ${status.daemon.length}`);

  writeFileSync(shared, JSON.stringify(paths));
  const child = spawnSync(process.execPath, [fileURLToPath(import.meta.url), "inner", "child"], { encoding: "utf8", timeout: 40_000 });
  console.log((child.stdout + child.stderr).trim());
  assert.equal(child.status, 0, "the child process passes");
  const all = JSON.parse(readFileSync(shared, "utf8"));
  const [fifth, sixth] = all.slice(MOUNTS);
  assert.ok(!paths.some((ws) => ws.path === sixth.path) && sixth.path !== fifth.path, `a remount gets a new directory: ${sixth.path}`);
  assert.equal((await layerfs.workspace.exec(fifth, "cat README.md")).exitCode, 0, "a mount another process made");
  let refused = null;
  try { await layerfs.workspace.exec(paths[1], "true"); } catch (e) { refused = e; }
  // the stale entry sends A to the shared records once; nothing is there now
  assert.equal(refused?.code, "NotMounted", `a mount another process removed: ${refused?.message}`);
  assert.equal((await layerfs.workspace.exec(sixth, "cat README.md")).exitCode, 0, "the other process's remount");
  for (const ws of [paths[0], paths[2], paths[3]]) assert.equal((await layerfs.workspace.exec(ws, "true")).exitCode, 0);
  for (const ws of [paths[0], paths[2], paths[3], fifth, sixth]) await layerfs.workspace.unmount(ws, true);
  console.log("MAIN ok");
}

function outer([daemon, addon, image]) {
  assert.ok(daemon && addon && image, "usage: outer <daemon> <addon> <image digest>");
  const stamp = `f029-${Date.now().toString(36)}`;
  const volume = `layerfs-issue352-${stamp}`;
  const home = join(REPO, "target/f009", `${stamp}-home`);
  mkdirSync(home, { recursive: true });
  chmodSync(home, 0o700); // LAYERFS_HOME must be private
  let r;
  try {
    assert.equal(docker(["volume", "create", volume]).status, 0);
    r = inRuntime({ script: fileURLToPath(import.meta.url), args: ["main"], daemon, addon, image, volume, prefix: `${stamp}-`, env: { LAYERFS_HOME: home }, timeout: 90_000 });
  } finally {
    const left = docker(["ps", "-aq", "--filter", `name=^${stamp}-`]).stdout.trim().split("\n").filter(Boolean);
    if (left.length) docker(["rm", "-f", ...left]);
    docker(["volume", "rm", volume]);
    rmSync(home, { recursive: true, force: true });
  }
  console.log(`== exit ${r.status}\n${r.out.split("\n").slice(-30).join("\n")}`);
  assert.equal(r.status, 0, "the run passes");
  assert.match(r.out, /MAIN ok/);
  console.log("exec mount lookup: ok");
}

const [mode, ...rest] = process.argv.slice(2);
if (mode === "inner") await inner(rest);
else outer(rest);

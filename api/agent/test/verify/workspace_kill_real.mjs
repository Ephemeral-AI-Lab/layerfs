// #539 E2E: a workspace-scoped force kill. A client is SIGKILLed while its
// foreground `sleep 30` runs; a second client's unmount is `Busy` with the
// orphan's id, and `workspace.unmount(ws, { kill: "workspace" })` cancels only
// that Workspace's holders, then unmounts it normally. A live command in
// another Workspace, another Workspace's foreground orphan, and the Sandbox
// itself all survive. A holder whose process group reaches outside the
// Workspace is named `scope_conflict` and left alone. Values other than
// "workspace" are refused `InvalidInput`.
// Under the E2E launcher:
//   node api/agent/test/verify/workspace_kill_real.mjs inner
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { request } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const t0 = Date.now();
const MARK = /^proc_[0-9]+_[0-9a-f]{24}$/;
const log = (line) => console.log(`[${((Date.now() - t0) / 1000).toFixed(2)}s] ${line}`);
const since = (start) => `${((Date.now() - start) / 1000).toFixed(2)}s`;
const attempt = async (promise) => {
  try { return { value: await promise }; } catch (error) { return { error }; }
};

/** This run's Sandbox containers, never the run's own client container. */
function containers(all = true) {
  const filters = encodeURIComponent(JSON.stringify({ name: [process.env.LAYERFS_CONTAINER_PREFIX] }));
  return new Promise((resolve, reject) => {
    const call = request({ socketPath: process.env.LAYERFS_DOCKER, path: `/containers/json?all=${all ? 1 : 0}&filters=${filters}`, method: "GET" }, (answer) => {
      let body = "";
      answer.on("data", (chunk) => (body += chunk));
      answer.on("end", () => {
        try {
          resolve(JSON.parse(body)
            .map((item) => item.Names?.[0]?.replace(/^\//, ""))
            .filter((name) => !name.endsWith("-client")));
        } catch (error) { reject(error); }
      });
    });
    call.on("error", reject);
    call.end();
  });
}

async function alive(lf, ws, pid) {
  return (await lf.workspace.exec(ws, `test -e /proc/${pid}/stat && echo ALIVE || echo GONE`)).stdout.trim();
}

/** Client A: mount a Workspace, start a foreground command, die while it runs. */
async function child() {
  const { LayerFS } = await import("@layerfs/agent");
  const handoff = process.env.HANDOFF;
  const lf = await LayerFS.open(process.env.PROJECT);
  const ws = await lf.workspace.mount(process.env.SANDBOX, "lane");
  writeFileSync(join(handoff, "ws.json"), JSON.stringify(ws));
  lf.workspace.exec(ws, process.env.COMMAND).catch(() => {});
  writeFileSync(join(handoff, "exec-started"), "started");
  await sleep(1200);
  process.kill(process.pid, "SIGKILL");
}

/** An orphaned foreground exec of `command`, and the Workspace it holds. */
async function orphan(project, command, sandbox) {
  const handoff = mkdtempSync(join(tmpdir(), "wk-orphan-"));
  const a = spawn(process.execPath, [fileURLToPath(import.meta.url), "child"], {
    env: { ...process.env, PROJECT: project, SANDBOX: sandbox, HANDOFF: handoff, COMMAND: command },
    stdio: "inherit",
  });
  const exit = await new Promise((resolve) => a.on("exit", (status, signal) => resolve({ status, signal })));
  assert.equal(exit.signal, "SIGKILL", "the client was killed while its foreground exec ran");
  return { ws: JSON.parse(readFileSync(join(handoff, "ws.json"), "utf8")), handoff };
}

/** Reads the foreground record of `id` through a surviving client. */
async function record(lf, ws, id) {
  const text = (await lf.workspace.exec(ws, `cat /tmp/layerfs-exec-${id}.record`)).stdout.trim();
  const [rid, path, pid, start] = text.split(" ");
  assert.equal(rid, id);
  return { path, pid, start };
}

async function inner() {
  assert.ok(process.env.LAYERFS_HOME && process.env.LAYERFS_IMAGE, "the E2E launcher supplies the deployment");
  const { LayerFS } = await import("@layerfs/agent");
  const { LayerFSBackend } = await import("@layerfs/backend");
  const project = `wk-${Date.now().toString(36)}`;
  const source = mkdtempSync(join(tmpdir(), "layerfs-wk-"));
  writeFileSync(join(source, "README.md"), "workspace kill\n");
  let owner;
  const handoffs = [];
  const mounted = [];
  const processes = [];
  try {
    owner = await LayerFS.init(project, source);
    rmSync(source, { recursive: true, force: true });
    await owner.branch.fork("lane");
    await owner.sandbox.create();
    const backend = (lf) => LayerFSBackend.of(lf);

    // ---- A: the target's own orphan is killed; nothing else is -------------
    {
      const lf = await LayerFS.open(project);
      const observer = await lf.workspace.mount("1", "lane");
      mounted.push(observer);
      const victims = await orphan(project, "sleep 30", "1");
      const other = await orphan(project, "sleep 30", "1");
      handoffs.push(victims.handoff, other.handoff);

      const busy = await attempt(lf.workspace.unmount(victims.ws));
      assert.equal(busy.error?.code, "Busy", busy.error?.message);
      const [victimId] = busy.error.detail.processes;
      assert.match(victimId, MARK);
      const victim = await record(lf, observer, victimId);
      const otherBusy = await attempt(lf.workspace.unmount(other.ws));
      assert.equal(otherBusy.error?.code, "Busy", otherBusy.error?.message);
      const [otherId] = otherBusy.error.detail.processes;
      const otherRecord = await record(lf, observer, otherId);

      // A live background command in a third Workspace.
      const live = await lf.workspace.mount("1", "lane");
      mounted.push(live);
      const started = await backend(lf).daemon.process.start(live, "sleep 30");
      processes.push(started.process);
      assert.equal(started.kind, "running");
      const before = await containers();
      assert.equal(before.length, 1, `one Sandbox container: ${JSON.stringify(before)}`);

      // The option is a string enum: anything else, or force plus kill, is
      // refused before anything is signalled.
      const wrong = await attempt(lf.workspace.unmount(victims.ws, false, false, "sandbox"));
      assert.equal(wrong.error?.code, "InvalidInput", wrong.error?.message);
      const combined = await attempt(lf.workspace.unmount(victims.ws, false, true, "workspace"));
      assert.equal(combined.error?.code, "InvalidInput", combined.error?.message);
      log(`A validation -> ${wrong.error.code}, ${combined.error.code}`);
      assert.equal(await alive(lf, observer, victim.pid), "ALIVE", "validation killed nothing");

      const startedAt = Date.now();
      const killed = await attempt(lf.workspace.unmount(victims.ws, false, false, "workspace"));
      log(`A kill -> ${JSON.stringify(killed.value ?? killed.error)} in ${since(startedAt)}`);
      assert.deepEqual(killed.value, { kind: "Unmounted" }, killed.error?.message);
      assert.equal(await alive(lf, observer, victim.pid), "GONE", "the target's orphan is gone");
      assert.equal((await lf.workspace.exec(observer, `test -e /tmp/layerfs-exec-${victimId}.record && echo PRESENT || echo GONE`)).stdout.trim(), "GONE", "its record is gone");

      // Another Workspace's foreground orphan and live background command
      // are untouched, and its Workspace still executes commands.
      assert.equal(await alive(lf, observer, otherRecord.pid), "ALIVE", "another Workspace's orphan survives");
      assert.equal((await lf.workspace.exec(observer, `test -e /tmp/layerfs-exec-${otherId}.record && echo PRESENT || echo GONE`)).stdout.trim(), "PRESENT", "its record survives");
      assert.equal((await lf.workspace.exec(live, "echo ok")).stdout.trim(), "ok", "another Workspace still runs commands");
      const listed = await backend(lf).daemon.process.list();
      assert.ok(listed.items.some((item) => item.id === started.process && item.status === "running"), "the live background command survives");
      log("A other Workspace: orphan and live command survive");

      // Never a whole-Sandbox recovery: the container and the Sandbox record
      // are still there, and the daemon still answers for another Workspace.
      assert.equal((await containers()).length, 1, "the Sandbox container still exists");
      const sandboxes = await owner.project.list_sandboxes();
      assert.deepEqual(sandboxes.items.map((item) => item.sandbox), ["1"], "the Sandbox is still listed");
      assert.deepEqual((await backend(lf).daemon.workspace.status(live)).branch, "lane");
      log("A Sandbox alive: PASS");

      await backend(lf).daemon.process.cancel(otherId);
      await lf.workspace.unmount(other.ws, true);
      await backend(lf).daemon.process.cancel(started.process);
      await lf.workspace.unmount(live, true);
      await lf.workspace.unmount(observer, true);
      await lf.close();
    }

    // ---- B: a group reaching outside the Workspace is left alone ------------
    {
      const lf = await LayerFS.open(project);
      const observer = await lf.workspace.mount("1", "lane");
      mounted.push(observer);
      const outside = await orphan(project, "cd /tmp && sleep 30", "1");
      handoffs.push(outside.handoff);
      const busy = await attempt(lf.workspace.unmount(outside.ws));
      assert.equal(busy.error?.code, "Busy", busy.error?.message);
      const [id] = busy.error.detail.processes;
      const holder = await record(lf, observer, id);
      const startedAt = Date.now();
      const refused = await attempt(lf.workspace.unmount(outside.ws, false, false, "workspace"));
      log(`B kill outside -> ${JSON.stringify(refused.error?.detail)} in ${since(startedAt)}`);
      assert.equal(refused.error?.code, "Busy", "the scope conflict keeps it Busy");
      assert.deepEqual(refused.error.detail.processes, [id]);
      assert.ok(refused.error.detail.scope_conflict.length > 0, "the detail names the outside path");
      assert.ok(refused.error.detail.scope_conflict.some((line) => line.includes("/tmp")), JSON.stringify(refused.error.detail));
      assert.equal(await alive(lf, observer, holder.pid), "ALIVE", "nothing was killed");
      assert.equal((await containers()).length, 1, "the Sandbox container still exists");
      log("B scope conflict: PASS");

      // The ordinary cancel still ends it, then a normal unmount succeeds.
      assert.equal((await backend(lf).daemon.process.cancel(id)).status, "stopped");
      assert.deepEqual((await attempt(lf.workspace.unmount(outside.ws))).value, { kind: "Unmounted" });
      await lf.workspace.unmount(observer, true);
      await lf.close();
    }

    // ---- C: a background holder in the target is killed alone --------------
    {
      const lf = await LayerFS.open(project);
      const target = await lf.workspace.mount("1", "lane");
      const survivor = await lf.workspace.mount("1", "lane");
      mounted.push(target, survivor);
      const mine = await backend(lf).daemon.process.start(target, "sleep 30");
      const theirs = await backend(lf).daemon.process.start(survivor, "sleep 30");
      processes.push(mine.process, theirs.process);
      const busy = await attempt(lf.workspace.unmount(target));
      assert.equal(busy.error?.code, "Busy", busy.error?.message);
      assert.ok(busy.error.detail.processes.includes(mine.process));
      const killed = await lf.workspace.unmount(target, false, false, "workspace");
      assert.deepEqual(killed, { kind: "Unmounted" });
      const listed = await backend(lf).daemon.process.list();
      assert.ok(!listed.items.some((item) => item.id === mine.process && item.status === "running"), "the target's background holder is stopped");
      assert.ok(listed.items.some((item) => item.id === theirs.process && item.status === "running"), "the other Workspace's background command survives");
      log("C background holder scope: PASS");
      await backend(lf).daemon.process.cancel(theirs.process);
      await lf.workspace.unmount(survivor, true);
      await lf.close();
    }

    // The kill never closed the Sandbox: it only went idle (its last
    // Workspace was unmounted) and still holds the container until its own
    // close.
    const idle = (await attempt(owner.project.list_sandboxes())).value.items;
    assert.equal(idle.length, 1, "the Sandbox was never stopped or deleted");
    assert.equal(idle[0].workspaces, 0);
    await owner.sandbox.close("1");
    assert.deepEqual((await attempt(owner.project.list_sandboxes())).value.items, [], "the Sandbox closed only after its own close");
    assert.deepEqual(await containers(), [], "no container is left behind");
    log("workspace-scoped kill: PASS");
  } finally {
    for (const handoff of handoffs) rmSync(handoff, { recursive: true, force: true });
    const lf = await LayerFS.open(project).catch(() => null);
    if (lf) {
      for (const id of processes) await (await import("@layerfs/backend")).LayerFSBackend.of(lf).daemon.process.cancel(id).catch(() => {});
      for (const ws of mounted) await lf.workspace.unmount(ws, true).catch(() => {});
      await lf.close().catch(() => {});
    }
    await owner?.close().catch(() => {});
  }
}

if (process.argv[2] === "child") await child();
else if (process.argv[2] === "inner") await inner();
else throw new Error("usage: workspace_kill_real.mjs inner (under tools/worktree.py run)");

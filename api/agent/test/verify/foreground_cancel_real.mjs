// #63 E2E: a foreground exec leaves a durable `/tmp` record, so a second
// client can cancel an orphan whose first client was killed mid-exec; the
// force unmount gate then names the id instead of detaching the Workspace.
// Three scenarios, one Sandbox number each time:
//   A `sleep 30`            cancel stops it, /proc and the record go
//   B `bash -c 'sleep 30 & wait'`  cancel kills the descendants too
//   C `trap "" TERM; sleep 30`     TERM is ignored: cancel says stopping and
//                                  unmount stays Busy; close(force) ends it
// Under the E2E launcher:
//   node api/agent/test/verify/foreground_cancel_real.mjs inner
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
const attempt = async (promise) => {
  try { return { value: await promise }; } catch (error) { return { error }; }
};

function containers(all = true) {
  const filters = encodeURIComponent(JSON.stringify({ name: [process.env.LAYERFS_CONTAINER_PREFIX] }));
  return new Promise((resolve, reject) => {
    const call = request({ socketPath: process.env.LAYERFS_DOCKER, path: `/containers/json?all=${all ? 1 : 0}&filters=${filters}`, method: "GET" }, (answer) => {
      let body = "";
      answer.on("data", (chunk) => (body += chunk));
      // the run's own client container shares the prefix; it is not a Sandbox
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

/** Client A: mount, start the foreground command, die while it runs. */
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

/** Mounts a second Workspace of the same Sandbox, as the surviving client. */
async function watcher(lf) {
  return lf.workspace.mount(process.env.SANDBOX ?? "1", "lane");
}

async function orphan(lf, project, command, handoff) {
  const childEnv = { ...process.env, PROJECT: project, SANDBOX: "1", HANDOFF: handoff, COMMAND: command };
  const a = spawn(process.execPath, [fileURLToPath(import.meta.url), "child"], { env: childEnv, stdio: "inherit" });
  const exit = await new Promise((resolve) => a.on("exit", (status, signal) => resolve({ status, signal })));
  assert.equal(exit.signal, "SIGKILL", "client A was killed while its foreground exec ran");
  return JSON.parse(readFileSync(join(handoff, "ws.json"), "utf8"));
}

async function inner() {
  assert.ok(process.env.LAYERFS_HOME && process.env.LAYERFS_IMAGE, "the E2E launcher supplies the deployment");
  const { LayerFS } = await import("@layerfs/agent");
  const { LayerFSBackend } = await import("@layerfs/backend");
  const project = `fg-${Date.now().toString(36)}`;
  const source = mkdtempSync(join(tmpdir(), "layerfs-fg-"));
  writeFileSync(join(source, "README.md"), "foreground record\n");
  let owner;
  try {
    owner = await LayerFS.init(project, source);
    rmSync(source, { recursive: true, force: true });
    await owner.branch.fork("lane");
    await owner.sandbox.create();

    // ---- A: cancel a plain foreground orphan ---------------------------------
    {
      const handoff = mkdtempSync(join(tmpdir(), "fg-a-"));
      try {
        const lf = await LayerFS.open(project);
        const ws2 = await watcher(lf);
        const ws = await orphan(lf, project, "sleep 30", handoff);
        const busy = await attempt(lf.workspace.unmount(ws));
        assert.ok(busy.error, "the orphan keeps its Workspace Busy");
        assert.equal(busy.error.code, "Busy", busy.error.message);
        const ids = busy.error.detail?.processes;
        log(`A unmount -> Busy ${JSON.stringify(ids)}`);
        assert.equal(ids.length, 1, "exactly one process id");
        assert.match(ids[0], MARK);
        assert.ok(ids[0].startsWith("proc_1_"), ids[0]);

        const forced = await attempt(lf.workspace.unmount(ws, true, true));
        assert.equal(forced.error?.code, "Busy", "force is gated while the foreground exec runs");
        assert.deepEqual(forced.error.detail.processes, ids, "the gate names the same id");
        log(`A force unmount -> Busy (gate) ${JSON.stringify(forced.error.detail.processes)}`);

        const record = (await lf.workspace.exec(ws2, `cat /tmp/layerfs-exec-${ids[0]}.record`)).stdout.trim();
        const [id, path, pid, start] = record.split(" ");
        log(`A record -> ${JSON.stringify({ id, path, pid, start })}`);
        assert.equal(id, ids[0]);
        assert.equal(path, ws.path);
        assert.match(pid, /^[1-9][0-9]*$/);
        assert.match(start, /^[0-9]+$/);
        assert.equal((await lf.workspace.exec(ws2, `test -e /proc/${pid}/stat && echo ALIVE || echo GONE`)).stdout.trim(), "ALIVE");

        const backend = LayerFSBackend.of(lf);
        const ended = await backend.daemon.process.cancel(ids[0]);
        log(`A cancel -> ${JSON.stringify(ended)}`);
        assert.equal(ended.kind, "Stopped");
        assert.equal(ended.status, "stopped");

        assert.equal((await lf.workspace.exec(ws2, `test -e /proc/${pid}/stat && echo ALIVE || echo GONE`)).stdout.trim(), "GONE", "/proc/<pid>/stat is gone");
        // The check is itself a foreground exec, so it leaves its own record:
        // name the orphan's exactly.
        assert.equal((await lf.workspace.exec(ws2, `test -e /tmp/layerfs-exec-${ids[0]}.record && echo PRESENT || echo GONE`)).stdout.trim(), "GONE", "/tmp/layerfs-exec-<id>.record is gone");
        log("A /proc and record gone");

        const unmounted = await attempt(lf.workspace.unmount(ws));
        // an unmount of a clean Workspace answers { kind: "Unmounted" } only
        assert.deepEqual(unmounted.value, { kind: "Unmounted" }, unmounted.error?.message);
        log(`A unmount after cancel -> ${JSON.stringify(unmounted.value)}`);
        await lf.workspace.unmount(ws2);
        const closed = await attempt(lf.sandbox.close("1"));
        assert.deepEqual(closed.value, { kind: "Closed", sandbox: "1" });
        log("A cancel: PASS");
        await lf.close();
      } finally {
        rmSync(handoff, { recursive: true, force: true });
      }
    }

    // ---- B: cancel kills a foreground command's descendants ------------------
    {
      const handoff = mkdtempSync(join(tmpdir(), "fg-b-"));
      try {
        await owner.sandbox.create();
        const lf = await LayerFS.open(project);
        const ws2 = await watcher(lf);
        const ws = await orphan(lf, project, "bash -c 'sleep 30 & wait'", handoff);
        const before = (await lf.workspace.exec(ws2, "ps -eo comm | grep -cx sleep")).stdout.trim();
        assert.equal(before, "1", "the descendant sleep runs");
        const busy = await attempt(lf.workspace.unmount(ws));
        assert.equal(busy.error?.code, "Busy", busy.error?.message);
        const [id] = busy.error.detail.processes;
        log(`B unmount -> Busy ${JSON.stringify(busy.error.detail.processes)}`);

        const ended = await LayerFSBackend.of(lf).daemon.process.cancel(id);
        log(`B cancel -> ${JSON.stringify(ended)}`);
        assert.equal(ended.status, "stopped");
        const after = (await lf.workspace.exec(ws2, "ps -eo comm | grep -cx sleep || true")).stdout.trim();
        log(`B descendants after cancel -> ${after}`);
        assert.equal(after, "0", "the descendant sleep is gone");
        assert.equal((await lf.workspace.exec(ws2, `test -e /tmp/layerfs-exec-${id}.record && echo PRESENT || echo GONE`)).stdout.trim(), "GONE");
        await lf.workspace.unmount(ws);
        await lf.workspace.unmount(ws2);
        assert.deepEqual((await attempt(lf.sandbox.close("1"))).value, { kind: "Closed", sandbox: "1" });
        log("B descendants: PASS");
        await lf.close();
      } finally {
        rmSync(handoff, { recursive: true, force: true });
      }
    }

    // ---- C: TERM ignored, so cancel cannot stop it; close(force) does ---------
    {
      const handoff = mkdtempSync(join(tmpdir(), "fg-c-"));
      try {
        await owner.sandbox.create();
        const lf = await LayerFS.open(project);
        const ws2 = await watcher(lf);
        const ws = await orphan(lf, project, `trap "" TERM; sleep 30`, handoff);
        const busy = await attempt(lf.workspace.unmount(ws));
        assert.equal(busy.error?.code, "Busy", busy.error?.message);
        const [id] = busy.error.detail.processes;
        assert.match(id, MARK);
        const record = (await lf.workspace.exec(ws2, `cat /tmp/layerfs-exec-${id}.record`)).stdout.trim();
        const pid = record.split(" ")[2];
        const ended = await LayerFSBackend.of(lf).daemon.process.cancel(id);
        log(`C cancel -> ${JSON.stringify(ended)}`);
        assert.equal(ended.kind, "Stopped");
        assert.equal(ended.status, "stopping", "TERM is ignored, so the bounded wait expires");
        assert.equal((await lf.workspace.exec(ws2, `test -e /proc/${pid}/stat && echo ALIVE || echo GONE`)).stdout.trim(), "ALIVE", "it still runs");
        const still = await attempt(lf.workspace.unmount(ws));
        assert.equal(still.error?.code, "Busy");
        log(`C unmount -> still Busy ${JSON.stringify(still.error.detail.processes)}`);
        const forced = await attempt(lf.sandbox.close("1", true));
        assert.deepEqual(forced.value, { kind: "Closed", sandbox: "1" }, forced.error?.message);
        log(`C close(force) -> ${JSON.stringify(forced.value)}; containers ${JSON.stringify(await containers())}`);
        assert.deepEqual(await containers(), [], "the Sandbox container is gone");
        log("C TERM trap: PASS (close(force) is the escape)");
        await lf.close();
      } finally {
        rmSync(handoff, { recursive: true, force: true });
      }
    }
    assert.deepEqual((await attempt(owner.project.list_sandboxes())).value.items, []);
    log("foreground record + cancel: PASS");
  } finally {
    await owner?.close().catch(() => {});
  }
}

if (process.argv[2] === "child") await child();
else if (process.argv[2] === "inner") await inner();
else throw new Error("usage: foreground_cancel_real.mjs inner (under tools/worktree.py run)");

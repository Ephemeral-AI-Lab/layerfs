// @layerfs/runtime on a `vm:<provider>` placement, against a real VM (#515 §6,
// V9): the runtime creates the project's VM, copies the sealed bundle in,
// blocks user namespaces, starts the worker as root and writes the endpoint;
// an agent process uses that endpoint for Init, a fork, a write, a Sandbox, an
// exec and a Commit inside the VM, with the Sandbox as Linux namespaces there
// (H36's backend). Everything it made (the VM, its Store disk, the home and the
// source folder) is removed afterwards.
//   node api/runtime/test/verify/vm_real.mjs <sealed Linux bundle> [microsandbox|apple-container]
// Needs that provider's CLI and the bundle built for the host's Linux VM
// (aarch64 on an M-series Mac). Under 2 minutes for a small source folder; the
// container CLI's first run may need to pull its image.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const SELF = fileURLToPath(import.meta.url);
const CLI = { microsandbox: "msb", "apple-container": "container" };

if (process.argv[2] === "agent" && process.env.AGENT_PHASE === "resume") {
  // after a worker restart inside the VM: the committed lane is still there and
  // a new Sandbox mounts it
  const { LayerFS } = await import("@layerfs/agent");
  const shop = await LayerFS.open(process.env.PROJECT);
  try {
    assert.equal((await shop.fs.read("lane", "/fresh.txt")).text, "two\n");
    const { sandbox } = await shop.sandbox.create();
    const ws = await shop.workspace.mount(sandbox, "lane");
    const ran = await shop.workspace.exec(ws, "cat fresh.txt");
    assert.equal(ran.stdout, "two\n", ran.stderr);
    await shop.workspace.unmount(ws);
    await shop.sandbox.close(sandbox);
  } finally {
    await shop.close();
  }
  console.log("agent after the worker restart: ok");
  process.exit(0);
}

if (process.argv[2] === "agent") {
  // the agent's side: every call goes to the worker named by LAYERFS_WORKER_ENDPOINT
  const { LayerFS } = await import("@layerfs/agent");
  const shop = await LayerFS.open(process.env.PROJECT);
  // teardown runs whatever the assertions decided, so a failure leaves no Sandbox behind
  const undone = [];
  try {
    await shop.branch.fork("lane");
    await shop.fs.write("lane", "/note.txt", "one\n", "Add note");
    const { sandbox } = await shop.sandbox.create();
    const ws = await shop.workspace.mount(sandbox, "lane");
    // undone runs in reverse: unmount, then close
    undone.push(() => shop.sandbox.close(sandbox), () => shop.workspace.unmount(ws, true));
    // the command runs in a namespace Sandbox inside the VM: nonroot, no
    // capabilities, and the home and Store of the VM around it
    const ran = await shop.workspace.exec(ws, "id -u; cat README.md note.txt && echo two > fresh.txt");
    assert.equal(ran.exitCode, 0, ran.stderr);
    assert.match(ran.stdout, /^501\n# Shop\none\n$/m);
    // V4: the VM blocks user namespaces, so a command cannot copy the Workspace
    // mount into one of its own; the command's own identity is nonroot
    const blocked = await shop.workspace.exec(ws, "cat /proc/sys/user/max_user_namespaces; unshare -U /bin/true 2>&1; echo rc=$?");
    assert.match(blocked.stdout, /^0\n/m, blocked.stdout);
    assert.match(blocked.stdout, /rc=[1-9]/, blocked.stdout);
    assert.equal((await shop.workspace.commit(ws, "Add fresh")).kind, "Committed");
    // uncommitted, and so only in this Sandbox's own overlay
    await shop.workspace.exec(ws, "echo hidden > secret.txt");
    await shop.workspace.unmount(ws, true);
    await shop.sandbox.close(sandbox);
    undone.length = 0;
    assert.equal((await shop.fs.read("lane", "/fresh.txt")).text, "two\n");
    // a second Sandbox on the same Store has its own mount: the first Sandbox's
    // uncommitted file is not there, and the commit is
    const { sandbox: second } = await shop.sandbox.create();
    const ws2 = await shop.workspace.mount(second, "lane");
    undone.push(() => shop.sandbox.close(second), () => shop.workspace.unmount(ws2, true));
    const isolated = await shop.workspace.exec(ws2, "test -e secret.txt && echo leaked || echo isolated");
    assert.equal(isolated.stdout.trim(), "isolated", isolated.stdout);
    const again = await shop.workspace.exec(ws2, "id -u; cat fresh.txt");
    assert.match(again.stdout, /^501\ntwo\n$/m, again.stdout);
  } finally {
    for (const step of undone.reverse()) await step().catch((error) => console.error(`teardown: ${error.message}`));
    await shop.close();
  }
  console.log("agent over the VM worker: ok");
  process.exit(0);
}

const [bundle, provider = "microsandbox"] = process.argv.slice(2);
if (!CLI[provider] || !bundle || process.platform !== "darwin") {
  console.error("usage: vm_real.mjs <sealed Linux bundle> [microsandbox|apple-container] (macOS host)");
  process.exit(2);
}
if (!existsSync(bundle)) {
  console.error(`no sealed bundle at ${bundle}`);
  process.exit(2);
}
const { LayerFSRuntime } = await import("@layerfs/runtime");
const { deploymentEnv } = await import("../../src/clients.js");
const { endpointFile } = await import("../../src/platform/shared.js");

const shared = join(homedir(), ".cache", "layerfs-test");
mkdirSync(shared, { recursive: true });
const work = mkdtempSync(join(shared, `lfs-${provider}-`));
const home = join(work, "home"), source = join(work, "shop"), id = Date.now().toString(36);
const prefix = `lfs-${provider}-${id}-`;
mkdirSync(source);
writeFileSync(join(source, "README.md"), "# Shop\n");

const started = Date.now();
let runtime;
try {
  runtime = await LayerFSRuntime.open({ home, bundle, placement: `vm:${provider}`, containerPrefix: prefix, sweep: false });
  const status = await runtime.worker.start();
  assert.equal(status.worker.state, "running", JSON.stringify(status));
  assert.equal(status.vm.state, "running");
  const endpoint = JSON.parse(readFileSync(endpointFile(home), "utf8"));
  console.log(`VM ${status.vm.name} running; worker on 127.0.0.1:${endpoint.port}`);

  let job = await runtime.project.start_init("shop", source);
  for (let i = 0; !job.ended && i < 900; i++) {
    await new Promise((resolve) => setTimeout(resolve, 100));
    job = await runtime.project.get_init(job);
  }
  assert.equal(job.step, "done", `Init: ${JSON.stringify(job.error)}`);
  const { volume } = (await runtime.project.list()).find((project) => project.name === "shop");
  console.log(`Init through the VM worker: ok (volume ${volume})`);

  // V8: the Store is on this VM's disk and nowhere else, so a delete is refused
  await assert.rejects(runtime.vm.delete({ confirm: "delete" }), { code: "Unsupported", message: /never destroys a Store that was not pushed/ });
  // step ① again: a stopped VM is started, and its worker comes back
  assert.equal((await runtime.vm.stop()).state, "stopped");
  const again = await runtime.worker.start();
  assert.equal(again.worker.state, "running", JSON.stringify(again));
  console.log("stopped and restarted the VM: ok");

  const env = {
    ...process.env,
    PROJECT: "shop",
    LAYERFS_WORKER_ENDPOINT: endpointFile(home),
    ...Object.fromEntries(Object.entries(deploymentEnv(runtime.settings, runtime.platform, { name: "shop", volume })).map(([name, value]) => [`LAYERFS_${name}`, value])),
  };
  const agent = await new Promise((resolve) => {
    const child = spawn(process.execPath, [SELF, "agent"], { env, stdio: ["ignore", "pipe", "pipe"] });
    const out = [];
    const timer = setTimeout(() => child.kill("SIGKILL"), 100_000);
    child.stdout.on("data", (chunk) => out.push(chunk));
    child.stderr.on("data", (chunk) => out.push(chunk));
    child.on("close", (code) => { clearTimeout(timer); resolve({ code, text: Buffer.concat(out).toString() }); });
  });
  assert.equal(agent.code, 0, agent.text);
  console.log(agent.text.trim());

  // step ⑤-adjacent: a worker restart inside the VM takes a new secret, and the
  // project's first Commit is still readable afterwards
  const restarted = await runtime.worker.restart({ confirm: "restart" });
  assert.equal(restarted.worker.state, "running");
  const resumed = await new Promise((resolve) => {
    const child = spawn(process.execPath, [SELF, "agent"], { env: { ...env, AGENT_PHASE: "resume" }, stdio: ["ignore", "pipe", "pipe"] });
    const out = [];
    const timer = setTimeout(() => child.kill("SIGKILL"), 60_000);
    child.stdout.on("data", (chunk) => out.push(chunk));
    child.stderr.on("data", (chunk) => out.push(chunk));
    child.on("close", (code) => { clearTimeout(timer); resolve({ code, text: Buffer.concat(out).toString() }); });
  });
  assert.equal(resumed.code, 0, resumed.text);
  console.log(resumed.text.trim());
} finally {
  await runtime?.project.remove("shop", { confirm: "shop", stopSandboxes: true }).catch((error) => console.error(`remove: ${error.message}`));
  await runtime?.vm.delete({ confirm: "delete", discardStore: true }).catch((error) => console.error(`vm delete: ${error.message}`));
  await runtime?.close();
  rmSync(work, { recursive: true, force: true });
}
console.log(`vm real: ok in ${((Date.now() - started) / 1000).toFixed(1)} s`);

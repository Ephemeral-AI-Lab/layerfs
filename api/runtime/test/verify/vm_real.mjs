// @layerfs/runtime on a `vm:<provider>` placement, against real VMs (#515 §6,
// V9, V1): **one Linux VM per Store**. The runtime makes each project's VM,
// copies the sealed bundle into it, blocks user namespaces, starts its worker as
// root and writes that project's endpoint; each project's agent process reaches
// only its own VM, where the Sandboxes are Linux namespaces (H36's backend).
// Two projects run side by side: neither sees the other's Store, removing one
// leaves the other alone, and V8 refuses a delete while its Store exists.
// Everything it made (both VMs, their Store disks, the home and the sources) is
// removed afterwards.
//   node api/runtime/test/verify/vm_real.mjs <sealed Linux bundle> [microsandbox|apple-container]
// Needs that provider's CLI and the bundle built for the host's Linux VM
// (aarch64 on an M-series Mac). Under 2 minutes for small source folders; the
// first run pulls the image. LAYERFS_TEST_DIR (default ~/.cache/layerfs-test)
// holds the run's home, sources and traces.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const SELF = fileURLToPath(import.meta.url);
const CLI = { microsandbox: "msb", "apple-container": "container" };

/** An agent process: it names its project, and the endpoint of that project's VM comes from the deployment. */
async function agent(project, deployment, phase = undefined) {
  const env = { ...process.env, ...deployment, PROJECT: project, ...(phase ? { AGENT_PHASE: phase } : {}) };
  return new Promise((resolve) => {
    const child = spawn(process.execPath, [SELF, "agent"], { env, stdio: ["ignore", "pipe", "pipe"] });
    const out = [];
    const timer = setTimeout(() => child.kill("SIGKILL"), 100_000);
    child.stdout.on("data", (chunk) => out.push(chunk));
    child.stderr.on("data", (chunk) => out.push(chunk));
    child.on("close", (code) => { clearTimeout(timer); resolve({ code, text: Buffer.concat(out).toString() }); });
  });
}

if (process.argv[2] === "agent" && process.env.AGENT_PHASE === "resume") {
  // after the VM's worker was restarted: the committed lane is still there
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
  console.log(`${process.env.PROJECT} after the worker restart: ok`);
  process.exit(0);
}

if (process.argv[2] === "agent" && process.env.AGENT_PHASE === "blog") {
  // the other project's agent: its own VM, its own Store
  const { LayerFS } = await import("@layerfs/agent");
  const blog = await LayerFS.open(process.env.PROJECT);
  try {
    assert.equal((await blog.fs.read("main", "/README.md")).text, "# Blog\n");
    // the first project's Branch and files are not in this Store at all
    assert.ok(!(await blog.project.list_branches(1000, 0)).items.some((branch) => branch.name === "lane"), "the other project's Branch is not here");
    await assert.rejects(blog.fs.read("lane", "/note.txt"), { code: "NoSuchBranch" });
    await blog.branch.fork("blog-work");
    const { sandbox } = await blog.sandbox.create();
    const ws = await blog.workspace.mount(sandbox, "blog-work");
    try {
      const ran = await blog.workspace.exec(ws, "id -u; cat README.md");
      assert.match(ran.stdout, /^501\n# Blog\n$/m, ran.stdout);
    } finally {
      await blog.workspace.unmount(ws, true);
      await blog.sandbox.close(sandbox);
    }
  } finally {
    await blog.close();
  }
  console.log("blog over its own VM worker: ok");
  process.exit(0);
}

if (process.argv[2] === "agent") {
  // the first project's agent: only the deployment's endpoint is set, never LAYERFS_WORKER_ENDPOINT by hand
  const { LayerFS } = await import("@layerfs/agent");
  const shop = await LayerFS.open(process.env.PROJECT);
  const undone = [];
  try {
    await shop.branch.fork("lane");
    await shop.fs.write("lane", "/note.txt", "one\n", "Add note");
    const { sandbox } = await shop.sandbox.create();
    const ws = await shop.workspace.mount(sandbox, "lane");
    undone.push(() => shop.sandbox.close(sandbox), () => shop.workspace.unmount(ws, true));
    // the command runs in a namespace Sandbox inside the project's VM: nonroot,
    // no capabilities, and the VM's home and Store around it
    const ran = await shop.workspace.exec(ws, "id -u; cat README.md note.txt && echo two > fresh.txt");
    assert.equal(ran.exitCode, 0, ran.stderr);
    assert.match(ran.stdout, /^501\n# Shop\none\n$/m);
    // V4: the VM blocks user namespaces, so a command cannot copy the Workspace
    // mount into one of its own; the command's own identity is nonroot
    const blocked = await shop.workspace.exec(ws, "cat /proc/sys/user/max_user_namespaces; unshare -U /bin/true 2>&1; echo rc=$?");
    assert.match(blocked.stdout, /^0\n/m, blocked.stdout);
    assert.match(blocked.stdout, /rc=[1-9]/, blocked.stdout);
    assert.equal((await shop.workspace.commit(ws, "Add fresh")).kind, "Committed");
    await shop.workspace.exec(ws, "echo hidden > secret.txt");
    await shop.workspace.unmount(ws, true);
    await shop.sandbox.close(sandbox);
    undone.length = 0;
    assert.equal((await shop.fs.read("lane", "/fresh.txt")).text, "two\n");
    // a second Sandbox on the same Store has its own mount
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
  console.log("shop over its own VM worker: ok");
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

const shared = process.env.LAYERFS_TEST_DIR || join(homedir(), ".cache", "layerfs-test");
mkdirSync(shared, { recursive: true });
const work = mkdtempSync(join(shared, `lfs-${provider}-`));
const home = join(work, "home"), id = Date.now().toString(36), prefix = `lfs-${provider}-${id}-`;
const sources = {};
for (const [name, text] of [["shop", "# Shop\n"], ["blog", "# Blog\n"]]) {
  sources[name] = join(work, name);
  mkdirSync(sources[name]);
  writeFileSync(join(sources[name], "README.md"), text);
}
const projectEnv = (runtime, name, volume) => Object.fromEntries(Object.entries(deploymentEnv(runtime.settings, runtime.platform, { name, volume })).map(([key, value]) => [`LAYERFS_${key}`, value]));
const initProject = async (runtime, name) => {
  let job = await runtime.project.start_init(name, sources[name]);
  for (let i = 0; !job.ended && i < 900; i++) {
    await new Promise((resolve) => setTimeout(resolve, 100));
    job = await runtime.project.get_init(job);
  }
  assert.equal(job.step, "done", `${name}'s Init: ${JSON.stringify(job.error)}`);
  return (await runtime.project.list()).find((project) => project.name === name);
};

const started = Date.now();
let runtime;
try {
  runtime = await LayerFSRuntime.open({ home, bundle, placement: `vm:${provider}`, containerPrefix: prefix, sweep: false });
  // a project's VM is made on demand, for that project
  const first = await runtime.worker.start("shop");
  assert.equal(first.project, "shop");
  assert.equal(first.worker.state, "running", JSON.stringify(first));
  assert.equal(first.vm.state, "running");
  assert.equal(first.vm.name, `${prefix}shop-vm`);
  const shopEndpoint = JSON.parse(readFileSync(endpointFile(home, "shop"), "utf8"));
  console.log(`${first.vm.name} running; its worker on 127.0.0.1:${shopEndpoint.port}`);

  const shop = await initProject(runtime, "shop");
  const blog = await initProject(runtime, "blog");
  const listed = await runtime.vm.status();
  assert.deepEqual(listed.vms.map((vm) => [vm.project, vm.state, vm.worker.state]).sort(), [["blog", "running", "running"], ["shop", "running", "running"]]);
  const blogEndpoint = JSON.parse(readFileSync(endpointFile(home, "blog"), "utf8"));
  assert.notEqual(blogEndpoint.port, shopEndpoint.port, "each project's VM owns its host port");
  assert.notEqual(blogEndpoint.secret, shopEndpoint.secret, "each project's VM has its own secret");
  assert.notEqual(blogEndpoint.vm.name, shopEndpoint.vm.name);
  console.log(`two projects, two VMs: ok (${shop.volume}, ${blog.volume})`);

  // V8: each Store is on its own VM's disk and nowhere else, so a delete is refused
  await assert.rejects(runtime.vm.delete("shop", { confirm: "delete" }), { code: "Unsupported", message: /never destroys a Store that was not pushed/ });

  const shopEnv = projectEnv(runtime, "shop", shop.volume);
  const blogEnv = projectEnv(runtime, "blog", blog.volume);
  assert.equal(shopEnv.LAYERFS_WORKER_ENDPOINT, endpointFile(home, "shop"), "the deployment names this project's worker");
  assert.equal(process.env.LAYERFS_WORKER_ENDPOINT, undefined, "the runtime hands its routes over in-process, not through its own environment");
  const shopRun = await agent("shop", shopEnv);
  assert.equal(shopRun.code, 0, shopRun.text);
  console.log(shopRun.text.trim());
  const blogRun = await agent("blog", blogEnv, "blog");
  assert.equal(blogRun.code, 0, blogRun.text);
  console.log(blogRun.text.trim());

  // a stopped VM is started again, and its worker comes back on the same port
  assert.equal((await runtime.vm.stop("shop")).state, "stopped");
  assert.equal((await runtime.vm.status("shop")).state, "stopped");
  const again = await runtime.worker.start("shop");
  assert.equal(again.worker.state, "running", JSON.stringify(again));
  assert.equal(again.vm.worker.port, shopEndpoint.port, "the same VM keeps its published port");
  console.log("stopped and restarted the project's VM: ok");

  // the restarted worker takes a new secret, and the project's Commit is still readable
  const restarted = await runtime.worker.restart("shop", { confirm: "restart" });
  assert.equal(restarted.worker.state, "running");
  const resumed = await agent("shop", shopEnv, "resume");
  assert.equal(resumed.code, 0, resumed.text);
  console.log(resumed.text.trim());

  // removing one project takes its own VM and Store disk, and leaves the other alone
  const removed = await runtime.project.remove("blog", { confirm: "blog", stopSandboxes: true });
  assert.equal(removed.kind, "Removed");
  assert.equal((await runtime.vm.status("blog")).state, "missing");
  const other = await runtime.vm.status("shop");
  assert.deepEqual([other.project, other.state, other.worker.state], ["shop", "running", "running"], "the other project's VM is untouched");
  const shopAgain = await agent("shop", shopEnv, "resume");
  assert.equal(shopAgain.code, 0, shopAgain.text);
  console.log("removed one project, the other still runs: ok");
} finally {
  // whatever the assertions decided: every project still here keeps its VM and Store
  // disk to itself, so each one's removal takes exactly those
  for (const project of (await runtime?.project.list().catch(() => [])) ?? []) {
    const volume = project.volume ?? `${project.name}-store`;
    await runtime?.project.remove(project.name, { confirm: project.name, volume, stopSandboxes: true }).catch((error) => console.error(`remove ${project.name}: ${error.message}`));
    await runtime?.vm.delete(project.name, { confirm: "delete", discardStore: true }).catch((error) => console.error(`vm delete ${project.name}: ${error.message}`));
  }
  await runtime?.close();
  rmSync(work, { recursive: true, force: true });
}
console.log(`vm real: ok in ${((Date.now() - started) / 1000).toFixed(1)} s`);

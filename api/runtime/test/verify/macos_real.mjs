// @layerfs/runtime on macOS against Docker Desktop or colima (#432): the
// runtime is given the Mac's own Docker socket (as Docker Desktop has it with
// its default socket off) and starts a worker of its own prefix and home; Init
// runs through it, and an agent process whose LAYERFS_DOCKER is that Mac path
// forks, writes, makes a Sandbox, execs and commits in a Workspace, unmounts
// and closes the Sandbox. The worker and Init's container must use the
// engine's socket inside its VM, never the Mac path. Everything it made (the
// worker, Sandboxes, the volume and the home) is removed afterwards.
//   node api/runtime/test/verify/macos_real.mjs <sealed Linux bundle> <image digest> [Mac Docker socket]
// The socket defaults to Docker Desktop's ~/.docker/run/docker.sock. Its folder is made under
// ~/.cache/layerfs-test, which Docker shares with its VM (macOS's temp folder may not be). macOS only;
// under 2 minutes.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const SELF = fileURLToPath(import.meta.url);

if (process.argv[2] === "agent") {
  // the agent's side: every call goes to the worker named by LAYERFS_WORKER_ENDPOINT
  const { LayerFS } = await import("@layerfs/agent");
  const shop = await LayerFS.open(process.env.PROJECT);
  try {
    await shop.branch.fork("lane");
    await shop.fs.write("lane", "/note.txt", "one\n", "Add note");
    const { sandbox } = await shop.sandbox.create();
    const ws = await shop.workspace.mount(sandbox, "lane");
    const ran = await shop.workspace.exec(ws, "cat README.md note.txt && echo two > fresh.txt");
    assert.deepEqual([ran.exitCode, ran.stdout], [0, "# Shop\none\n"]);
    assert.equal((await shop.workspace.commit(ws, "Add fresh")).kind, "Committed");
    await shop.workspace.unmount(ws);
    await shop.sandbox.close(sandbox);
    assert.equal((await shop.fs.read("lane", "/fresh.txt")).text, "two\n");
  } finally {
    await shop.close();
  }
  console.log("agent over the worker: ok");
  process.exit(0);
}

const [bundle, image, mac = join(homedir(), ".docker", "run", "docker.sock")] = process.argv.slice(2);
if (process.platform !== "darwin" || !bundle || !image) {
  console.error("usage: macos_real.mjs <sealed Linux bundle> <image digest> [Mac Docker socket] (macOS only)");
  process.exit(2);
}
const { LayerFSRuntime } = await import("@layerfs/runtime");
const shared = join(homedir(), ".cache", "layerfs-test");
mkdirSync(shared, { recursive: true });
const work = mkdtempSync(join(shared, "lfs-mac-"));
const home = join(work, "home"), source = join(work, "shop"), id = Date.now().toString(36);
const prefix = `lfs-mac-${id}-`;
const docker = (...args) => execFileSync("docker", ["-H", `unix://${mac}`, ...args], { encoding: "utf8", timeout: 30_000 }).trim();
mkdirSync(source);
writeFileSync(join(source, "README.md"), "# Shop\n");
const started = Date.now();
const runtime = await LayerFSRuntime.open({ home, image, bundle, docker: mac, containerPrefix: prefix, platform: "macos", sweep: false });
try {
  assert.equal((await runtime.worker.start()).worker.state, "running");
  const endpoint = JSON.parse(readFileSync(join(home, "worker", "endpoint.json"), "utf8"));
  console.log(`worker ${prefix}worker running; endpoint docker: ${endpoint.docker ?? "(none)"}`);

  let job = await runtime.project.start_init("shop", source);
  for (let i = 0; !job.ended && i < 600; i++) {
    await new Promise((resolve) => setTimeout(resolve, 100));
    job = await runtime.project.get_init(job);
  }
  assert.equal(job.step, "done", `Init: ${JSON.stringify(job.error)}`);
  const { volume } = (await runtime.project.list()).find((p) => p.name === "shop");
  console.log(`Init through the worker: ok (volume ${volume})`);

  const env = {
    ...process.env, PROJECT: "shop",
    LAYERFS_WORKER_ENDPOINT: join(home, "worker", "endpoint.json"), LAYERFS_HOME: home, LAYERFS_DOCKER: mac,
    LAYERFS_IMAGE: image, LAYERFS_DAEMON: join(bundle, "layerfs-daemon"), LAYERFS_VOLUME: volume,
    LAYERFS_STORE_PATH: `/var/lib/docker/volumes/${volume}/_data/global/store.sqlite`, LAYERFS_CONTAINER_PREFIX: prefix,
  };
  const agent = await new Promise((resolve) => {
    const child = spawn(process.execPath, [SELF, "agent"], { env, stdio: ["ignore", "pipe", "pipe"] });
    const out = [];
    const timer = setTimeout(() => child.kill("SIGKILL"), 80_000);
    child.stdout.on("data", (c) => out.push(c));
    child.stderr.on("data", (c) => out.push(c));
    child.on("close", (code) => { clearTimeout(timer); resolve({ code, text: Buffer.concat(out).toString() }); });
  });
  assert.equal(agent.code, 0, agent.text);
  console.log(agent.text.trim());
} finally {
  await runtime.project.remove("shop", { confirm: "shop", stopSandboxes: true }).catch((error) => console.error(`remove: ${error.message}`));
  await runtime.close();
  for (const name of docker("ps", "-a", "--filter", `name=^${prefix}`, "--format", "{{.Names}}").split("\n").filter(Boolean)) docker("rm", "-f", name);
  for (const name of docker("volume", "ls", "--filter", `name=^${prefix}`, "--format", "{{.Name}}").split("\n").filter(Boolean)) docker("volume", "rm", name);
  rmSync(work, { recursive: true, force: true });
}
console.log(`macos real: ok in ${((Date.now() - started) / 1000).toFixed(1)} s`);

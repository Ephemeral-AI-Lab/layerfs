// The #527 platform pass driver: one cell on one platform row, one sample.
//   node run.mjs <row> <cell> --out <dir> [options]
//   node run.mjs refusals <macos-namespaces|vm-docker> --out <dir> [options]
// It makes a fresh project (Init from an empty folder) through the row's
// route, runs cell.mjs as its own agent process with that row's environment,
// writes one receipt JSON (never overwriting one), then deletes the project
// and everything it made, also on failure. Only the timed phase holds the
// host-wide lock (an atomic mkdir under ~/.cache/layerfs-test). The whole
// invocation is bounded at --budget-ms (default 115 s).
//
// Rows: macos-docker, macos-native, linux-docker, linux-native,
// linux-namespaces, vm-apple-container, vm-microsandbox, vm-ssh, refusals.
// #529: each row's cell makes its Sandbox with sandbox.create({ platform })
// (PLATFORM below), and C01 also runs the home's default and refusal checks
// (cell.mjs). Two Linux rows take the cell `check`: linux-mixed (mixed.mjs:
// a docker and a linux Sandbox of one project, --ns-image the namespaces
// image) and linux-oldrecords (make.mjs with the pre-#529 bundle
// --old-bundle, then oldrec.mjs).
// A linux-* row given on macOS re-runs this file as root inside the Lima VM
// (LFS527_LIMA_VM, default lfs527-linux) and relays the lock over stdio.
//
// Options (each also an environment variable):
//   --linux-bundle  LFS527_LINUX_BUNDLE  sealed Linux bundle (macos-docker, linux-*)
//   --macos-bundle  LFS527_MACOS_BUNDLE  sealed macOS bundle (macos-native, refusals)
//   --vm-bundle     LFS527_VM_BUNDLE     Linux bundle copy carrying images/<hex>.tar (vm-*)
//   --image         LFS527_IMAGE         Sandbox image digest (default: the bundle's images.sandbox)
//   --image-archive LFS527_IMAGE_ARCHIVE docker-archive of the image (linux-namespaces, linux-mixed, linux-oldrecords)
//   --ns-image      LFS529_NS_IMAGE      the namespaces image digest (config digest) of linux-mixed and linux-oldrecords
//   --old-bundle    LFS529_OLD_BUNDLE    sealed Linux bundle from before #529 (linux-oldrecords)
//   --provider      LFS527_PROVIDER      refusals vm-docker: the VM provider (default apple-container)
//   --budget-ms     LFS527_BUDGET_MS     the invocation's deadline (default 115000)
//   --lock-relay    ask the parent for the lock over stdio (set by the Lima relay)
// Other settings: LFS527_DOCKER (macOS Docker socket), LFS527_WORK (work root),
// LFS527_LOCK (lock dir), LFS527_LOCK_WAIT_MS (default 45000), LFS527_VM_CPUS /
// LFS527_VM_MEMORY (default 4 / 4G), LFS527_LIMA_VM, LFS527_LIMA_NODE, and for
// vm-ssh the runtime's own LAYERFS_SSH_* settings. vm-firecracker runs in the
// Lima vz host LFS527_FIRECRACKER_LIMA (default lfs527-vzkvm) from
// LFS527_FIRECRACKER_DIR (default /var/lib/lfs527-firecracker), with 2G of
// memory unless LFS527_VM_MEMORY says otherwise (the host has 4 GiB).
import { spawn, execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmodSync, copyFileSync, existsSync, renameSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { homedir, hostname, loadavg, release, cpus, totalmem } from "node:os";
import { dirname, join } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath, pathToFileURL } from "node:url";
import { CF, command, sha256 } from "./workloads.mjs";

const SELF = fileURLToPath(import.meta.url);
const HERE = dirname(SELF);
const CELL = join(HERE, "cell.mjs");
const ROWS = ["macos-docker", "macos-native", "linux-docker", "linux-native", "linux-namespaces", "vm-apple-container", "vm-microsandbox", "vm-lima", "vm-firecracker", "vm-ssh", "refusals", "linux-mixed", "linux-oldrecords"];
/** #529: the rows that take the cell `check` instead of C01..C12. */
const CHECKS = ["linux-mixed", "linux-oldrecords"];
/** #529: each row's platform, the platforms its home allows and its default (spec §4). */
const LINUX = ["docker", "linux"];
const PLATFORM = {
  "macos-docker": ["docker", ["docker"], "docker"], "macos-native": ["mac", ["mac"], "mac"],
  "linux-docker": ["docker", LINUX, "docker"], "linux-native": ["linux", LINUX, "linux"], "linux-namespaces": ["linux", LINUX, "linux"],
  ...Object.fromEntries(["apple-container", "microsandbox", "lima", "firecracker", "ssh"].map((p) => [`vm-${p}`, [p, [p], p]])),
};
const REFUSALS = ["macos-namespaces", "vm-docker"];

// ---- arguments ------------------------------------------------------------
const argv = process.argv.slice(2);
const [row, cell] = argv;
const flag = (name, envName, fallback = undefined) => {
  const at = argv.indexOf(`--${name}`);
  return at >= 0 ? argv[at + 1] : process.env[envName] || fallback;
};
const relay = argv.includes("--lock-relay");
const outDir = flag("out", "LFS527_OUT");
const budgetMs = Number(flag("budget-ms", "LFS527_BUDGET_MS", "115000"));
const deadline = Date.now() + budgetMs;
const opts = {
  linuxBundle: flag("linux-bundle", "LFS527_LINUX_BUNDLE"),
  macosBundle: flag("macos-bundle", "LFS527_MACOS_BUNDLE"),
  vmBundle: flag("vm-bundle", "LFS527_VM_BUNDLE"),
  image: flag("image", "LFS527_IMAGE"),
  imageArchive: flag("image-archive", "LFS527_IMAGE_ARCHIVE"),
  provider: flag("provider", "LFS527_PROVIDER", "apple-container"),
  nsImage: flag("ns-image", "LFS529_NS_IMAGE"),
  oldBundle: flag("old-bundle", "LFS529_OLD_BUNDLE"),
};
if (!ROWS.includes(row) || (row === "refusals" ? !REFUSALS.includes(cell) : CHECKS.includes(row) ? cell !== "check" : !CF[cell]) || (!outDir && !relay)) {
  console.error(`usage: node run.mjs <${ROWS.join("|")}> <C01..C12 | ${REFUSALS.join("|")}> --out <dir> [--linux-bundle P] [--macos-bundle P] [--vm-bundle P] [--image D] [--image-archive P]`);
  process.exit(2);
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const log = (...args) => console.error(`[lfs527 ${row} ${cell}]`, ...args);
const fileSha = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
const errorOf = (error) => ({ code: error?.code ?? null, message: String(error?.message ?? error).slice(0, 2000), detail: error?.detail ?? null });
const id = `${Date.now().toString(36)}${process.pid.toString(36)}`.slice(-10);
const prefix = `lfs527-${id}-`;
const project = (row === "refusals" ? "refusal" : CHECKS.includes(row) ? row.slice("linux-".length) : cell.toLowerCase());

const receipt = {
  receipt: "lfs527-cell/1",
  row, cell, verdict: null, cause: null,
  project, prefix,
  startedAt: new Date().toISOString(),
  host: { hostname: hostname(), platform: process.platform, release: release(), cpus: cpus().length, memBytes: totalmem(), node: process.version, uid: process.getuid?.() },
  runner: Object.fromEntries(["run.mjs", "cell.mjs", "workloads.mjs", "mixed.mjs", "make.mjs", "oldrec.mjs"].map((name) => [name, fileSha(join(HERE, name))])),
  platform: PLATFORM[row]?.[0] ?? null,
  load1: { start: loadavg()[0] },
  constructionWorkers: "1",
  driver: {},
  teardown: [],
};
if (row !== "refusals" && !CHECKS.includes(row)) {
  receipt.body = { sha256: sha256(CF[cell][0]), commandSha256: sha256(command(cell)), setupSha256: CF[cell][1] ? sha256(CF[cell][1]) : null };
}

// ---- bundle identity --------------------------------------------------------
/** The bundle's manifest identity, and every manifest file's sha256 checked once. */
function bundleIdentity(bundle) {
  const manifest = JSON.parse(readFileSync(join(bundle, "manifest.json"), "utf8"));
  const mismatched = Object.entries(manifest.files ?? {}).filter(([path, hash]) => !existsSync(join(bundle, path)) || fileSha(join(bundle, path)) !== hash).map(([path]) => path);
  return {
    path: bundle, id: manifest.build_id, platform: manifest.platform, profile: manifest.profile,
    sourceCommit: manifest.source?.head ?? null, sourceSha256: manifest.source?.sha256 ?? null,
    manifestSha256: fileSha(join(bundle, "manifest.json")), images: manifest.images ?? null,
    filesChecked: Object.keys(manifest.files ?? {}).length, mismatched,
    imageArchives: existsSync(join(bundle, "images")) ? readdirSync(join(bundle, "images")) : [],
  };
}
const apiUrl = (bundle, path) => pathToFileURL(join(bundle, "api", path)).href;
const agentPath = (bundle) => join(bundle, "api", "agent", "src", "index.js");

// ---- the host-wide lock -----------------------------------------------------
const LOCK = flag("lock", "LFS527_LOCK", join(homedir(), ".cache", "layerfs-test", "lfs527-sample.lock"));
const LOCK_WAIT_MS = Number(process.env.LFS527_LOCK_WAIT_MS || 45_000);
let stdinLines = null;
const pendingStdin = [];
function parentLine() {
  if (!stdinLines) {
    stdinLines = createInterface({ input: process.stdin });
    stdinLines.on("line", (line) => pendingStdin.shift()?.(line));
  }
  return new Promise((resolve) => pendingStdin.push(resolve));
}

/** Takes the lock: {waitMs, load1, ...}. In relay mode the parent takes it. */
async function acquireLock() {
  if (relay) {
    console.log("LFS527-LOCK acquire");
    const line = await parentLine();
    if (!line.startsWith("LFS527-LOCK granted")) throw new Error(`the parent did not grant the lock: ${line}`);
    return { ...JSON.parse(line.slice("LFS527-LOCK granted".length).trim()), relayed: true, load1Local: loadavg()[0] };
  }
  mkdirSync(dirname(LOCK), { recursive: true });
  const asked = Date.now(), until = Math.min(asked + LOCK_WAIT_MS, deadline - 30_000);
  let staleRemoved = null;
  for (;;) {
    try {
      mkdirSync(LOCK);
      writeFileSync(join(LOCK, "owner.json"), JSON.stringify({ pid: process.pid, hostname: hostname(), row, cell, at: new Date().toISOString() }));
      return { lock: LOCK, waitMs: Date.now() - asked, load1: loadavg()[0], grantedAt: new Date().toISOString(), staleRemoved };
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
    }
    // an owner of this host whose process is gone left it: removed once, recorded
    try {
      const owner = JSON.parse(readFileSync(join(LOCK, "owner.json"), "utf8"));
      let alive = true;
      try { process.kill(owner.pid, 0); } catch (error) { alive = error.code !== "ESRCH"; }
      if (owner.hostname === hostname() && !alive && !staleRemoved) {
        rmSync(LOCK, { recursive: true, force: true });
        staleRemoved = owner;
        continue;
      }
    } catch {}
    if (Date.now() >= until) throw new Error(`the host lock ${LOCK} was not free within ${Date.now() - asked} ms`);
    await sleep(100);
  }
}
function releaseLock() {
  if (relay) console.log("LFS527-LOCK release");
  else rmSync(LOCK, { recursive: true, force: true });
}

/**
 * Runs a child that speaks the lock protocol on its stdout and stdin and ends
 * with one `<marker> <json>` line. Killed at `ms`; a lock it holds is released.
 */
function runChild(file, args, env, ms, marker) {
  return new Promise((resolve) => {
    const child = spawn(file, args, { env, stdio: ["pipe", "pipe", "pipe"] });
    const err = [], lock = { grants: [] };
    let result = null, holding = false, killed = false;
    const timer = setTimeout(() => { killed = true; child.kill("SIGKILL"); }, ms);
    createInterface({ input: child.stdout }).on("line", async (line) => {
      if (line === "LFS527-LOCK acquire") {
        try {
          const grant = await acquireLock();
          holding = true;
          lock.grants.push(grant);
          child.stdin.write(`LFS527-LOCK granted ${JSON.stringify(grant)}\n`);
        } catch (error) {
          lock.error = error.message;
          child.stdin.write(`LFS527-LOCK refused ${JSON.stringify(error.message)}\n`);
        }
      } else if (line === "LFS527-LOCK release") {
        if (holding) releaseLock();
        holding = false;
      } else if (line.startsWith(`${marker} `)) {
        result = JSON.parse(line.slice(marker.length + 1));
      } else {
        err.push(`stdout: ${line}\n`);
      }
    });
    child.stderr.on("data", (chunk) => err.push(chunk.toString()));
    child.on("close", (code, signal) => {
      clearTimeout(timer);
      if (holding) releaseLock();
      resolve({ code, signal, killed, result, lock, stderr: err.join("").slice(-4000) });
    });
  });
}

// ---- helpers shared by rows ------------------------------------------------
const workRoot = () => process.env.LFS527_WORK || (process.platform === "linux" ? "/var/lib/lfs527" : join(homedir(), ".cache", "layerfs-test"));
/** A fresh work folder: home (0700), an empty Init source and the row's own folders. */
function makeWork() {
  mkdirSync(workRoot(), { recursive: true, mode: 0o755 });
  const work = mkdtempSync(join(workRoot(), "lfs527-"));
  chmodSync(work, 0o755); // a native Linux Sandbox's commands (501:20) traverse it
  receipt.driver.work = work; // named at once, so a setup cut at the deadline still names it
  const home = join(work, "home"), source = join(work, "src");
  mkdirSync(home, { mode: 0o700 });
  mkdirSync(source);
  return { work, home, source };
}
/** The child's environment: this process's without any LAYERFS_ setting, then the row's. */
function agentEnv(rowEnv, bundle) {
  const base = Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith("LAYERFS_")));
  return { ...base, ...rowEnv, LAYERFS_CONSTRUCTION_WORKERS: "1", LFS527_AGENT: agentPath(bundle) };
}
const prefixed = (env) => Object.fromEntries(Object.entries(env).filter(([, value]) => value !== undefined && value !== "").map(([key, value]) => [`LAYERFS_${key}`, value]));
const teardownStep = async (name, call) => {
  try {
    const value = await call();
    receipt.teardown.push({ name, ok: true, ...(value === undefined ? {} : { value }) });
  } catch (error) {
    receipt.teardown.push({ name, ok: false, error: errorOf(error) });
  }
};
/** Init through @layerfs/runtime: one job, read until it ends or the deadline. */
async function initThrough(runtime, name, source) {
  let job = await runtime.project.start_init(name, source);
  while (!job.ended && Date.now() < deadline - 30_000) {
    await sleep(100);
    job = await runtime.project.get_init(job);
  }
  if (job.step !== "done") throw Object.assign(new Error(`Init ${job.step}: ${JSON.stringify(job.error)}`), { code: job.error?.code });
  return (await runtime.project.list()).find((entry) => entry.name === name);
}
const docker = (socket, ...args) => execFileSync("docker", [...(socket ? ["-H", `unix://${socket}`] : []), ...args], { encoding: "utf8", timeout: 30_000 }).trim();
/** Removes the containers and volumes this run's prefix names, and nothing else. */
function removeDockerLeftovers(socket) {
  const removed = [];
  for (const name of docker(socket, "ps", "-a", "--filter", `name=^${prefix}`, "--format", "{{.Names}}").split("\n").filter(Boolean)) { docker(socket, "rm", "-f", name); removed.push(name); }
  for (const name of docker(socket, "volume", "ls", "--filter", `name=^${prefix}`, "--format", "{{.Name}}").split("\n").filter(Boolean)) { docker(socket, "volume", "rm", name); removed.push(name); }
  return removed;
}

/** A row on @layerfs/runtime: open it, make the project, and its teardown. */
async function runtimeRow({ bundle, settings, extraEnv = () => ({}), socket, after = async () => {} }) {
  const { LayerFSRuntime } = await import(apiUrl(bundle, "runtime/src/index.js"));
  const { deploymentEnv } = await import(apiUrl(bundle, "runtime/src/clients.js"));
  const { work, home, source } = makeWork();
  receipt.driver.work = work;
  let runtime = null;
  const teardown = async () => {
    if (runtime) {
      await teardownStep("project.delete", () => runtime.project.delete(project, { confirm: project, stopSandboxes: true }));
      await after(runtime);
      await teardownStep("runtime.close", () => runtime.close());
    }
    if (socket !== undefined) await teardownStep("docker leftovers", () => removeDockerLeftovers(socket));
    await teardownStep("work", () => rmSync(work, { recursive: true, force: true }));
  };
  try {
    runtime = await LayerFSRuntime.open({ home, bundle, containerPrefix: prefix, sweep: false, ...settings });
    const at = Date.now();
    const made = await initThrough(runtime, project, source);
    receipt.driver.initMs = Date.now() - at;
    receipt.driver.volume = made?.volume ?? null;
    const env = { ...prefixed(deploymentEnv(runtime.settings, runtime.platform, made)), ...extraEnv(home) };
    return { env, teardown, runtime, made };
  } catch (error) {
    error.teardown = teardown;
    throw error;
  }
}

/** A row on the SDK alone (native, namespaces): LayerFS.init in this process. */
async function sdkRow({ bundle, rowEnv }) {
  const { work, home, source } = makeWork();
  receipt.driver.work = work;
  const local = join(work, "local"), store = join(work, "store");
  mkdirSync(local, { mode: 0o755 });
  chmodSync(local, 0o755);
  mkdirSync(store, { mode: 0o700 });
  const env = {
    LAYERFS_SANDBOX: "native", LAYERFS_NATIVE: join(bundle, "layerfs.node"), LAYERFS_DAEMON: join(bundle, "layerfs-daemon"),
    LAYERFS_LOCAL_DIR: local, LAYERFS_STORE_PATH: join(store, "store.sqlite"), LAYERFS_HOME: home, LAYERFS_CONTAINER_PREFIX: prefix,
    ...rowEnv,
  };
  for (const name of Object.keys(process.env)) if (name.startsWith("LAYERFS_")) delete process.env[name];
  Object.assign(process.env, env, { LAYERFS_CONSTRUCTION_WORKERS: "1" });
  const { LayerFS } = await import(pathToFileURL(agentPath(bundle)).href);
  const teardown = async () => {
    // close every Sandbox of the project, force-unmounting what is still mounted
    await teardownStep("sandboxes", async () => {
      const client = await LayerFS.open(project);
      const closed = [];
      try {
        for (const { sandbox } of (await client.project.list_sandboxes(1000, 0)).items) {
          for (const ws of (await client.sandbox.list_workspaces(sandbox, 1000, 0)).items ?? []) await client.workspace.unmount({ sandbox: ws.sandbox, path: ws.path }, true, true);
          await client.sandbox.close(sandbox);
          closed.push(sandbox);
        }
      } finally {
        await client.close();
      }
      return closed;
    });
    await teardownStep("leftover mounts and processes", () => {
      const mounts = execFileSync(process.platform === "linux" ? "cat" : "mount", process.platform === "linux" ? ["/proc/mounts"] : [], { encoding: "utf8" }).split("\n").filter((line) => line.includes(work));
      let pids = "";
      try { pids = execFileSync("pgrep", ["-f", work], { encoding: "utf8" }).trim(); } catch {}
      return { mounts, pids: pids.split("\n").filter(Boolean) };
    });
    await teardownStep("work", () => rmSync(work, { recursive: true, force: true }));
  };
  try {
    if (process.platform === "linux") {
      receipt.driver.fs = execFileSync("findmnt", ["-n", "-o", "FSTYPE", "-T", work], { encoding: "utf8" }).trim();
      receipt.driver.maxUserNamespaces = readFileSync("/proc/sys/user/max_user_namespaces", "utf8").trim();
    }
    const at = Date.now();
    const created = await LayerFS.init(project, source);
    receipt.driver.initMs = Date.now() - at;
    receipt.driver.init = created.created ?? null;
    await created.close();
    return { env, teardown, LayerFS };
  } catch (error) {
    error.teardown = teardown;
    throw error;
  }
}

// ---- rows -------------------------------------------------------------------
const need = (value, name) => {
  if (!value) throw Object.assign(new Error(`${name} is required for ${row}`), { code: "InvalidInput" });
  return value;
};
const macDocker = () => process.env.LFS527_DOCKER || join(homedir(), ".docker", "run", "docker.sock");
const vmSize = (provider) => ({ cpus: Number(process.env.LFS527_VM_CPUS || 4), memory: process.env.LFS527_VM_MEMORY || (provider === "firecracker" ? "2G" : "4G") });

const SETUP = {
  async "macos-docker"() {
    const bundle = need(opts.linuxBundle, "--linux-bundle");
    const image = opts.image || bundleIdentity(bundle).images?.sandbox;
    const socket = macDocker();
    receipt.imageDigest = image;
    const made = await runtimeRow({
      bundle, socket,
      settings: { image, docker: socket, platform: "macos" },
      extraEnv: (home) => ({ LAYERFS_WORKER_ENDPOINT: join(home, "worker", "endpoint.json") }),
    });
    return { ...made, bundle };
  },
  async "macos-native"() {
    const bundle = need(opts.macosBundle, "--macos-bundle");
    receipt.imageDigest = null; // a native Sandbox runs on the Mac itself
    return { ...(await sdkRow({ bundle, rowEnv: {} })), bundle };
  },
  async "linux-docker"() {
    const bundle = need(opts.linuxBundle, "--linux-bundle");
    const image = opts.image || bundleIdentity(bundle).images?.sandbox;
    receipt.imageDigest = image;
    // on Linux the runtime runs Init in this process, with the bundle's addon
    process.env.LAYERFS_NATIVE = join(bundle, "layerfs.node");
    const made = await runtimeRow({
      bundle, socket: null,
      settings: { image, platform: "linux" },
      extraEnv: () => ({ LAYERFS_NATIVE: join(bundle, "layerfs.node") }),
    });
    return { ...made, bundle };
  },
  async "linux-native"() {
    const bundle = need(opts.linuxBundle, "--linux-bundle");
    receipt.imageDigest = null; // the VM's own userland
    return { ...(await sdkRow({ bundle, rowEnv: {} })), bundle };
  },
  async "linux-namespaces"() {
    const bundle = need(opts.linuxBundle, "--linux-bundle");
    const image = opts.image || bundleIdentity(bundle).images?.sandbox;
    receipt.imageDigest = image;
    receipt.imageArchive = need(opts.imageArchive, "--image-archive");
    return { ...(await sdkRow({ bundle, rowEnv: { LAYERFS_SANDBOX: "namespaces", LAYERFS_IMAGE: image, LAYERFS_IMAGE_ARCHIVE: opts.imageArchive } })), bundle };
  },
  "vm-apple-container": () => vmRow("apple-container"),
  "vm-microsandbox": () => vmRow("microsandbox"),
  "vm-lima": () => vmRow("lima"),
  "vm-firecracker": () => vmRow("firecracker"),
  "vm-ssh": () => vmRow("ssh"),
};

async function vmRow(provider) {
  const bundle = need(opts.vmBundle, "--vm-bundle");
  if (provider === "ssh" && !process.env.LAYERFS_SSH_STATE_DIR) {
    // the SSH provider's session records go in this run's own folder (removed with it)
    process.env.LAYERFS_SSH_STATE_DIR = join(workRoot(), `lfs527-ssh-state-${id}`);
    receipt.driver.sshStateDir = process.env.LAYERFS_SSH_STATE_DIR;
  }
  if (provider === "firecracker") {
    // the provider's settings, read by the runtime in this process; its
    // microVM records go in this run's own folder (removed with it)
    process.env.LAYERFS_FIRECRACKER_LIMA ||= process.env.LFS527_FIRECRACKER_LIMA || "lfs527-vzkvm";
    process.env.LAYERFS_FIRECRACKER_DIR ||= process.env.LFS527_FIRECRACKER_DIR || "/var/lib/lfs527-firecracker";
    process.env.LAYERFS_FIRECRACKER_STATE_DIR ||= join(workRoot(), `lfs527-firecracker-state-${id}`);
    receipt.driver.firecracker = { lima: process.env.LAYERFS_FIRECRACKER_LIMA, dir: process.env.LAYERFS_FIRECRACKER_DIR, stateDir: process.env.LAYERFS_FIRECRACKER_STATE_DIR };
  }
  const made = await runtimeRow({
    bundle,
    settings: { placement: `vm:${provider}`, ...(provider === "ssh" ? {} : { vm: vmSize(provider) }) },
    after: async (runtime) => {
      if (provider === "ssh") {
        // vm:ssh never touches the user's disk on delete (ssh.js removeStore is
        // empty); this run's Store, Sandbox folders, home entry and import copy on
        // the pass's own SSH host are removed here, with one ssh command
        await teardownStep("ssh host files", () => {
          const ssh = runtime.settings.vm.ssh, store = ssh.storePath;
          const args = ["-p", String(ssh.port), "-o", "BatchMode=yes", ...(ssh.identityFile ? ["-i", ssh.identityFile] : []),
            ...(ssh.knownHostsFile ? ["-o", `UserKnownHostsFile=${ssh.knownHostsFile}`] : []),
            "-o", `StrictHostKeyChecking=${ssh.strictHostKeyChecking === true ? "yes" : ssh.strictHostKeyChecking || "yes"}`,
            `${ssh.user}@${ssh.host}`, "--", `rm -rf ${store}/global ${store}/local /layerfs/store/global /layerfs/store/local /layerfs/home/${project} /layerfs/import`];
          execFileSync("ssh", args, { encoding: "utf8", timeout: 30_000 });
          return store;
        });
      }
      await teardownStep("vm.delete", () => runtime.vm.delete(project, { confirm: "delete", discardStore: true }));
      if (provider === "ssh") await teardownStep("ssh state", () => rmSync(process.env.LAYERFS_SSH_STATE_DIR, { recursive: true, force: true }));
      if (provider === "firecracker") await teardownStep("firecracker state", () => rmSync(process.env.LAYERFS_FIRECRACKER_STATE_DIR, { recursive: true, force: true }));
    },
  });
  receipt.imageDigest = made.env.LAYERFS_IMAGE ?? null;
  receipt.driver.vm = await made.runtime.vm.status(project).catch((error) => errorOf(error));
  return { ...made, bundle };
}

// ---- refusals -----------------------------------------------------------------
/** One call whose refusal is the observation: { ok, code, message }. */
async function observe(call) {
  try {
    const value = await call();
    return { refused: false, value: JSON.parse(JSON.stringify(value ?? null)) };
  } catch (error) {
    return { refused: true, ...errorOf(error) };
  }
}

async function refusals() {
  if (cell === "macos-namespaces") {
    const bundle = need(opts.macosBundle, "--macos-bundle");
    receipt.bundle = bundleIdentity(bundle);
    const made = await sdkRow({ bundle, rowEnv: {} });
    teardowns.push(made.teardown);
    const image = opts.image || receipt.bundle.images?.sandbox;
    // step 11: LAYERFS_SANDBOX=namespaces; the former pair (native +
    // LAYERFS_SANDBOX_BACKEND=namespaces, kept one release) is observed too
    const attempt = async (extra, named = { LAYERFS_SANDBOX: "namespaces" }) => {
      Object.assign(process.env, named, extra);
      let client = null;
      const opened = await observe(async () => { client = await made.LayerFS.open(project); return "opened"; });
      const created = client ? await observe(() => client.sandbox.create()) : null;
      if (created && !created.refused) await client.sandbox.close(created.value.sandbox).catch(() => {});
      await client?.close();
      delete process.env.LAYERFS_SANDBOX_BACKEND;
      process.env.LAYERFS_SANDBOX = made.env.LAYERFS_SANDBOX;
      delete process.env.LAYERFS_IMAGE;
      delete process.env.LAYERFS_IMAGE_ARCHIVE;
      return { opened, created };
    };
    const without = await attempt({});
    const withImage = await attempt({ LAYERFS_IMAGE: image, LAYERFS_IMAGE_ARCHIVE: join(made.env.LAYERFS_LOCAL_DIR, "none.tar") });
    const former = await attempt({ LAYERFS_IMAGE: image, LAYERFS_IMAGE_ARCHIVE: join(made.env.LAYERFS_LOCAL_DIR, "none.tar") }, { LAYERFS_SANDBOX: "native", LAYERFS_SANDBOX_BACKEND: "namespaces" });
    receipt.observed = {
      "LAYERFS_SANDBOX=namespaces, no image settings": without,
      "LAYERFS_SANDBOX=namespaces, with LAYERFS_IMAGE and LAYERFS_IMAGE_ARCHIVE": withImage,
      "former pair native + LAYERFS_SANDBOX_BACKEND=namespaces, with image settings": former,
    };
    const codes = [without, withImage, former].map((seen) => seen.created?.code ?? seen.opened?.code);
    receipt.verdict = codes.every((code) => code === "Unsupported") ? "PASS" : "FAIL";
    receipt.cause = `expected Unsupported three times; answered ${codes.map((code) => code ?? "no refusal").join(", ")}`;
    return;
  }
  // vm-docker: what a caller who asks for Docker Sandboxes on a vm: home gets
  const bundle = need(opts.vmBundle, "--vm-bundle");
  receipt.bundle = bundleIdentity(bundle);
  const { LayerFSRuntime } = await import(apiUrl(bundle, "runtime/src/index.js"));
  receipt.observed = {};
  receipt.observed["placement vm:docker"] = await observe(() => LayerFSRuntime.open({ home: join(workRoot(), `lfs527-refusal-${id}`), placement: "vm:docker" }));
  rmSync(join(workRoot(), `lfs527-refusal-${id}`), { recursive: true, force: true });
  const made = await vmRow(opts.provider);
  teardowns.push(made.teardown);
  const { LayerFS } = await import(pathToFileURL(agentPath(bundle)).href);
  const attempt = async (extra) => {
    const saved = { ...process.env };
    Object.assign(process.env, made.env, extra);
    let client = null;
    const opened = await observe(async () => { client = await LayerFS.open(project); return "opened"; });
    for (const name of Object.keys(process.env)) if (!(name in saved)) delete process.env[name];
    Object.assign(process.env, saved);
    const created = client ? await observe(() => client.sandbox.create()) : null;
    if (created && !created.refused) await client.sandbox.close(created.value.sandbox).catch(() => {});
    await client?.close();
    return { opened, created };
  };
  receipt.observed[`LAYERFS_SANDBOX=docker on vm:${opts.provider}`] = await attempt({ LAYERFS_SANDBOX: "docker", LAYERFS_VOLUME: `${prefix}none`, LAYERFS_DOCKER: "/var/run/docker.sock", LAYERFS_IMAGE: receipt.bundle.images?.sandbox });
  receipt.observed[`LAYERFS_SANDBOX_BACKEND=docker on vm:${opts.provider}`] = await attempt({ LAYERFS_SANDBOX_BACKEND: "docker" });
  // documented answers (e9b9646148, step 11): vm:docker is no provider, Docker
  // on a vm: home is Unsupported before it is sent, and the former
  // LAYERFS_SANDBOX_BACKEND never names docker
  const codeOf = (seen) => seen?.created?.code ?? seen?.opened?.code ?? seen?.code ?? null;
  const expected = { "placement vm:docker": "Unsupported", [`LAYERFS_SANDBOX=docker on vm:${opts.provider}`]: "Unsupported", [`LAYERFS_SANDBOX_BACKEND=docker on vm:${opts.provider}`]: "InvalidInput" };
  const answered = Object.fromEntries(Object.keys(expected).map((name) => [name, codeOf(receipt.observed[name])]));
  receipt.verdict = Object.entries(expected).every(([name, code]) => answered[name] === code) ? "PASS" : "FAIL";
  receipt.cause = `expected ${JSON.stringify(expected)}; answered ${JSON.stringify(answered)}`;
}

// ---- #529 Linux checks --------------------------------------------------------
/** A Linux home on @layerfs/runtime that also allows `linux` (localDir, imageArchive), with `bundle`'s SDK. */
async function linuxHome(bundle) {
  const image = opts.image || bundleIdentity(bundle).images?.sandbox;
  receipt.imageDigest = image;
  receipt.nsImage = need(opts.nsImage, "--ns-image");
  receipt.imageArchive = need(opts.imageArchive, "--image-archive");
  process.env.LAYERFS_NATIVE = join(bundle, "layerfs.node");
  const localDir = mkdtempSync(join(workRoot(), "lfs527-local-"));
  chmodSync(localDir, 0o755);
  teardowns.push(async () => teardownStep("local dir", () => rmSync(localDir, { recursive: true, force: true })));
  const made = await runtimeRow({
    bundle, socket: null,
    settings: { image, platform: "linux", localDir, imageArchive: opts.imageArchive },
    extraEnv: () => ({ LAYERFS_NATIVE: join(bundle, "layerfs.node") }),
  });
  teardowns.push(made.teardown);
  receipt.bundle = bundleIdentity(bundle);
  receipt.sourceCommit = receipt.bundle.sourceCommit;
  receipt.env = made.env;
  receipt.platforms = (await made.runtime.status()).platforms;
  return made;
}

/** One agent script of this folder as its own process: its `LFS529-RESULT`, bounded by the deadline. */
async function agentRun(script, env, bundle) {
  const ran = await runChild(process.execPath, [join(HERE, script), project], agentEnv(env, bundle), deadline - Date.now() - 20_000, "LFS529-RESULT");
  return { result: ran.result, exit: { code: ran.code, signal: ran.signal, killed: ran.killed, stderr: ran.stderr.slice(-2000) } };
}

async function linuxCheck() {
  const bundle = need(opts.linuxBundle, "--linux-bundle");
  receipt.driver.fs = execFileSync("findmnt", ["-n", "-o", "FSTYPE", "-T", workRoot()], { encoding: "utf8" }).trim();
  const made = await linuxHome(bundle);
  if (row === "linux-mixed") {
    const ran = await agentRun("mixed.mjs", { ...made.env, LFS529_NS_IMAGE: opts.nsImage }, bundle);
    receipt.cellResult = ran.result;
    receipt.cellExit = ran.exit;
    receipt.verdict = ran.result?.ok ? "PASS" : "FAIL";
    receipt.cause = ran.result ? ran.result.cause : `mixed.mjs ended without a result: ${ran.exit.stderr.slice(-500)}`;
    return;
  }
  // old records: the pre-#529 SDK and daemon make a docker and a namespaces Sandbox
  const old = need(opts.oldBundle, "--old-bundle");
  receipt.oldBundle = bundleIdentity(old);
  const oldEnv = { ...made.env, LAYERFS_NATIVE: join(old, "layerfs.node"), LAYERFS_DAEMON: join(old, "layerfs-daemon") };
  const docker = await agentRun("make.mjs", oldEnv, old);
  const namespaces = await agentRun("make.mjs", { ...oldEnv, LAYERFS_SANDBOX: "namespaces", LAYERFS_IMAGE: opts.nsImage }, old);
  receipt.made = { docker, namespaces };
  if (!docker.result?.ok || !namespaces.result?.ok) {
    receipt.verdict = "FAIL";
    receipt.cause = `the old bundle did not make both Sandboxes: ${docker.result?.cause ?? docker.exit.stderr.slice(-300)} / ${namespaces.result?.cause ?? namespaces.exit.stderr.slice(-300)}`;
    return;
  }
  const expect = { [docker.result.created.sandbox]: "docker", [namespaces.result.created.sandbox]: "linux" };
  const ran = await agentRun("oldrec.mjs", { ...made.env, LFS529_EXPECT: JSON.stringify(expect) }, bundle);
  receipt.cellResult = ran.result;
  receipt.cellExit = ran.exit;
  receipt.verdict = ran.result?.ok ? "PASS" : "FAIL";
  receipt.cause = ran.result ? ran.result.cause : `oldrec.mjs ended without a result: ${ran.exit.stderr.slice(-500)}`;
}

// ---- the run ------------------------------------------------------------------
const teardowns = [];
function writeReceipt() {
  receipt.endedAt = new Date().toISOString();
  receipt.load1.end = loadavg()[0];
  const text = JSON.stringify(receipt, null, 2);
  if (relay) console.log(`LFS527-RECEIPT ${JSON.stringify(receipt)}`);
  if (outDir) {
    mkdirSync(outDir, { recursive: true });
    const file = join(outDir, `${row}-${cell}-${receipt.startedAt.replace(/[:.]/g, "")}-${id}.json`);
    writeFileSync(file, `${text}\n`, { flag: "wx" });
    log(`receipt ${file}: ${receipt.verdict}${receipt.cause ? ` (${receipt.cause})` : ""}`);
  }
}

/** A linux-* row asked for on macOS: this file again, as root in the Lima VM, with the lock relayed. */
async function viaLima() {
  const vm = process.env.LFS527_LIMA_VM || "lfs527-linux";
  // The VM reads the Mac's home over a 9p mount that can serve a stale copy of
  // an edited file, so the runner runs from a content-named copy under
  // ~/.cache/layerfs-test (a new folder per runner version), and the inner
  // run's runner hashes must equal this one's.
  const version = sha256(Object.values(receipt.runner).join("")).slice(0, 16);
  const runner = join(homedir(), ".cache", "layerfs-test", `lfs527-runner-${version}`);
  if (!existsSync(join(runner, "run.mjs"))) {
    mkdirSync(`${runner}.tmp-${id}`, { recursive: true });
    for (const name of Object.keys(receipt.runner)) copyFileSync(join(HERE, name), join(`${runner}.tmp-${id}`, name));
    renameSync(`${runner}.tmp-${id}`, runner);
  }
  const pass = Object.entries(process.env).filter(([name]) => name.startsWith("LFS527_") && name !== "LFS527_OUT").map(([name, value]) => `${name}=${value}`);
  const args = ["shell", "--workdir", "/", vm, "sudo", "env", ...pass,
    ...(opts.linuxBundle ? [`LFS527_LINUX_BUNDLE=${opts.linuxBundle}`] : []), ...(opts.image ? [`LFS527_IMAGE=${opts.image}`] : []),
    ...(opts.imageArchive ? [`LFS527_IMAGE_ARCHIVE=${opts.imageArchive}`] : []),
    ...(opts.nsImage ? [`LFS529_NS_IMAGE=${opts.nsImage}`] : []), ...(opts.oldBundle ? [`LFS529_OLD_BUNDLE=${opts.oldBundle}`] : []),
    process.env.LFS527_LIMA_NODE || "node", join(runner, "run.mjs"), row, cell, "--lock-relay", "--budget-ms", String(deadline - Date.now() - 5_000)];
  const ran = await runChild("limactl", args, process.env, deadline - Date.now(), "LFS527-RECEIPT");
  if (ran.result) {
    const inner = ran.result.runner;
    Object.assign(receipt, ran.result, { via: { lima: vm, lock: ran.lock, exit: ran.code, stderr: ran.stderr.slice(-1500) }, startedAt: receipt.startedAt, runner: receipt.runner, host: receipt.host, load1Outer: receipt.load1, runnerCopy: runner });
    if (JSON.stringify(inner) !== JSON.stringify(receipt.runner)) {
      receipt.verdict = "FAIL";
      receipt.cause = `the VM ran another runner: ${JSON.stringify(inner)}`;
    }
  } else {
    receipt.verdict = "FAIL";
    receipt.cause = `the Lima run ended without a receipt (exit ${ran.code}${ran.killed ? ", killed at the deadline" : ""})`;
    receipt.via = { lima: vm, lock: ran.lock, exit: ran.code, signal: ran.signal, stderr: ran.stderr };
  }
}

try {
  if (row.startsWith("linux-") && process.platform === "darwin") {
    await viaLima();
  } else if (row === "refusals") {
    await refusals();
  } else if (CHECKS.includes(row)) {
    await linuxCheck();
  } else {
    const at = Date.now();
    // Setup is bounded by the invocation's deadline too: a setup still running
    // 15 s before it is recorded as a failure, so the receipt is written before
    // the outer bound kills this process (C01/C02 of the 50e2c2bc87 pass had none).
    // What it made then is named by this run's prefix for the sweep to remove.
    let late;
    const made = await Promise.race([
      SETUP[row]().catch((error) => { throw Object.assign(error, { phase: "setup" }); }),
      new Promise((_, reject) => { late = setTimeout(() => reject(Object.assign(new Error(`setup did not finish within ${Math.round((deadline - 15_000 - at) / 1000)} s; left for cleanup: ${receipt.driver.work ?? "no work folder yet"} and anything named ${prefix}*`), { phase: "setup" })), Math.max(0, deadline - 15_000 - Date.now())); }),
    ]).finally(() => clearTimeout(late));
    teardowns.push(made.teardown);
    receipt.driver.setupMs = Date.now() - at;
    receipt.bundle = bundleIdentity(made.bundle);
    receipt.sourceCommit = receipt.bundle.sourceCommit;
    receipt.env = Object.fromEntries(Object.entries(made.env).filter(([name]) => name !== "LAYERFS_SSH_IDENTITY"));
    const childMs = deadline - Date.now() - (row.startsWith("vm-") ? 30_000 : 20_000);
    if (childMs < 15_000) throw Object.assign(new Error(`only ${childMs} ms left for the cell after setup`), { phase: "setup" });
    receipt.driver.cellBudgetMs = childMs;
    const [platform, allowed, fallback] = PLATFORM[row];
    const cellEnv = { ...agentEnv(made.env, made.bundle), LFS529_PLATFORM: platform, LFS529_ALLOWED: allowed.join(","), LFS529_DEFAULT: fallback, ...(cell === "C01" ? { LFS529_EXTRAS: "1" } : {}) };
    const ran = await runChild(process.execPath, [CELL, cell, project], cellEnv, childMs, "LFS527-RESULT");
    receipt.lock = ran.lock.grants[0] ?? (ran.lock.error ? { error: ran.lock.error } : null);
    receipt.cellResult = ran.result;
    receipt.cellExit = { code: ran.code, signal: ran.signal, killed: ran.killed, stderr: ran.stderr };
    const timed = ran.result?.phases?.timed;
    receipt.timings = timed ? {
      mountMs: timed.mountMs, execWallMs: timed.exec.wallMs, commandMs: timed.exec.ms, commitMs: timed.commitMs, commitKind: timed.commitKind, unmountMs: timed.unmountMs,
    } : null;
    receipt.where = ran.result?.where ?? null;
    receipt.imageDigest = timed?.image && timed.image !== "local" ? timed.image : receipt.imageDigest;
    if (ran.result?.extras) {
      receipt.extras = ran.result.extras;
      receipt.extrasVerdict = ran.result.extras.ok ? "PASS" : "FAIL";
    }
    receipt.verdict = ran.result?.ok ? "PASS" : "FAIL";
    receipt.cause = ran.result ? ran.result.cause : `the cell ended without a result (exit ${ran.code}${ran.killed ? ", killed at its deadline" : ""}): ${ran.stderr.slice(-500)}`;
  }
} catch (error) {
  if (error.teardown && !teardowns.includes(error.teardown)) teardowns.push(error.teardown);
  receipt.verdict = "FAIL";
  receipt.cause = `${error.phase ?? "run"}: ${error.code ?? ""} ${error.message}`.trim();
  receipt.error = errorOf(error);
} finally {
  const at = Date.now();
  for (const teardown of teardowns.reverse()) await teardown().catch((error) => receipt.teardown.push({ name: "teardown", ok: false, error: errorOf(error) }));
  if (!receipt.via) receipt.driver.teardownMs = Date.now() - at; // a relayed run keeps the inner driver's
  receipt.teardownOk = receipt.teardown.every((entry) => entry.ok) && !receipt.teardown.some((entry) => entry.value?.mounts?.length || entry.value?.pids?.length);
  writeReceipt();
}
process.exit(receipt.verdict === "PASS" || receipt.verdict === "OBSERVED" ? 0 : 1);

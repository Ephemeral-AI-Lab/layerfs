// `vm:ssh` placement (#515 §6, V3): any Linux host reachable over SSH — an EC2
// instance or a VM the user made, instead of one this runtime creates. The host
// and its Store disk are the user's: this adapter never makes a disk, it only
// proves that the path given is on ext4, on that host's own kernel.
//
// The five adapter steps (§6) on this provider:
//   ① the "VM" is the host; the Store disk is the path `storePath` (a block
//      device mounted at /layerfs/store, or a directory bind-mounted there);
//   ② the bundle is copied in over SSH (one `tar` stream per bundle hash);
//   ③ the worker runs as root with its secret, exactly as on a microVM;
//   ④ the private stream is `ssh -L 127.0.0.1:<host>:127.0.0.1:7070`;
//   ⑤ stop ends the forward and the worker; delete never touches the disk.
//
// Facts this file depends on, and why they are checked rather than assumed:
//   - the host must answer `uname -s` with Linux: a macOS/BSD host has no
//     namespaces and no /proc/sys/user;
//   - the Store path must be an ext4 filesystem on that host: LayerFS's Store
//     names its files and relies on that filesystem's ordering;
//   - key login only, from a file; a password never reaches argv or a log;
//   - host key checking stays on (`StrictHostKeyChecking=yes` unless the
//     setting says otherwise, `accept-new` at the loosest);
//   - `-o ExitOnForwardFailure=yes` so a busy local port fails at once, before
//     anything is uploaded or started, instead of leaving a half-made VM.
//
// The forward runs with its stderr in a log file, not a pipe: a host process
// (the CLI, a test) must be able to exit while the user's VM keeps running, and
// a referenced stdio pipe would hold its event loop open. `unref` on the child
// alone does not release the pipes, so they are files.
import { spawn } from "node:child_process";
import { closeSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { LayerFSError } from "@layerfs/agent";
import { readFile, writeFile } from "./cli.js";

const EXEC_MS = 60_000, COPY_MS = 120_000, FORWARD_READY_MS = 600, END_MS = 3_000;
const WORKER_PORT_DEFAULT = 7070;
const STORE_PATH_DEFAULT = "/layerfs/store";

const SESSIONS = new Map();

const refuse = (code, message) => new LayerFSError(code, message, null);
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** Where this provider keeps its session records: one JSON file per VM name. */
export const stateDir = () => process.env.LAYERFS_SSH_STATE_DIR || join(homedir(), ".layerfs", "ssh");

function sessionFile(name) {
  const dir = stateDir();
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  return join(dir, `${name}.json`);
}

function forwardLog(name) {
  const dir = stateDir();
  mkdirSync(dir, { recursive: true, mode: 0o700 });
  return join(dir, `${name}-forward.log`);
}

function loadSession(name) {
  if (SESSIONS.has(name)) return SESSIONS.get(name);
  const file = sessionFile(name);
  if (!existsSync(file)) return null;
  try {
    const session = JSON.parse(readFileSync(file, "utf8"));
    SESSIONS.set(name, session);
    return session;
  } catch {
    return null;
  }
}

function saveSession(session) {
  SESSIONS.set(session.name, session);
  writeFileSync(sessionFile(session.name), JSON.stringify(session, null, 2), { mode: 0o600 });
}

function deleteSession(name) {
  SESSIONS.delete(name);
  rmSync(sessionFile(name), { force: true });
  rmSync(forwardLog(name), { force: true });
}

/** Whether a pid still runs. Only a plain liveness probe; it proves nothing else. */
export function isAlive(pid) {
  if (!pid || typeof pid !== "number") return false;
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code === "EPERM";
  }
}

/** One argv token for the remote POSIX shell: never left to the shell's word splitting. */
export function shellEscape(arg) {
  const text = String(arg);
  if (/^[A-Za-z0-9_\-./=:@%+,]+$/.test(text)) return text;
  return `'${text.replace(/'/g, "'\\''")}'`;
}

/**
 * The ssh arguments both `exec` and the forward share. `BatchMode=yes` keeps a
 * password prompt from ever being tried (this provider does not support one);
 * the identity file is a path here, its content stays in the file.
 */
function sshArgs(session) {
  const args = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=15"];
  if (session.port && session.port !== 22) args.push("-p", String(session.port));
  if (session.identityFile) args.push("-i", session.identityFile);
  const strict = session.strictHostKeyChecking;
  args.push("-o", `StrictHostKeyChecking=${strict === false ? "accept-new" : typeof strict === "string" ? strict : "yes"}`);
  if (session.knownHostsFile) args.push("-o", `UserKnownHostsFile=${session.knownHostsFile}`);
  if (session.controlMaster && session.controlPath) {
    args.push("-o", "ControlMaster=auto", "-o", `ControlPath=${session.controlPath}`, "-o", "ControlPersist=60s");
  }
  return args;
}

const destinationOf = (session) => (session.user ? `${session.user}@${session.host}` : session.host);

/** One command on the host, its streams kept apart; a call past `timeout` is killed and reported. */
function remote(session, argv, { input = undefined, timeout = EXEC_MS } = {}) {
  const args = [...sshArgs(session), destinationOf(session), "--", ...argv.map(shellEscape)];
  return new Promise((resolve, reject) => {
    const child = spawn("ssh", args, { stdio: ["pipe", "pipe", "pipe"] });
    const out = [], err = [];
    const timer = setTimeout(() => {
      child.kill("SIGKILL");
      reject(new Error(`ssh ${argv.join(" ")} did not end within ${timeout / 1000} s`));
    }, timeout);
    child.stdout.on("data", (chunk) => out.push(chunk));
    child.stderr.on("data", (chunk) => err.push(chunk));
    child.on("error", (error) => { clearTimeout(timer); reject(error); });
    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ code, stdout: Buffer.concat(out).toString(), stderr: Buffer.concat(err).toString() });
    });
    if (input === undefined) child.stdin.end(); else child.stdin.end(input);
  });
}

/** The last `lines` lines of the forward's log, for an error message. */
function logTail(name, lines = 3) {
  try {
    return readFileSync(forwardLog(name), "utf8").trim().split("\n").slice(-lines).join("; ");
  } catch {
    return "";
  }
}

/**
 * Step ④: the private stream. `-o ExitOnForwardFailure=yes` makes a busy local
 * port an immediate failure, so nothing is uploaded or started on a VM that
 * cannot be reached. A forward that exits within the readiness window is
 * reported with its own reason; one that answers will be ended by `endForward`.
 */
async function startForward(session, hostPort, workerPort) {
  const log = forwardLog(session.name);
  const fd = openSync(log, "w", 0o600);
  const child = spawn("ssh", [
    "-N",
    "-o", "ExitOnForwardFailure=yes",
    ...sshArgs(session),
    "-L", `127.0.0.1:${hostPort}:127.0.0.1:${workerPort}`,
    destinationOf(session),
  ], { stdio: ["ignore", "ignore", fd] });
  closeSync(fd);

  const ended = new Promise((resolve) => child.once("exit", (code, signal) => resolve({ code, signal })));
  const first = await Promise.race([ended.then((value) => ({ ended: true, ...value })), delay(FORWARD_READY_MS).then(() => ({ ended: false }))]);
  child.removeAllListeners("exit");
  if (first.ended) {
    child.unref();
    throw new Error(`ssh -L 127.0.0.1:${hostPort} exited (${first.code ?? first.signal}): ${logTail(session.name) || "no reason given"}`);
  }
  child.on("exit", () => { SESSIONS.delete(session.name); });
  child.unref();
  return child;
}

/** Ends a forward, bounded: SIGTERM, then SIGKILL; never waits on it indefinitely. */
async function endForward(session) {
  const pid = session.forwardPid;
  if (!isAlive(pid)) { session.forwardPid = null; return; }
  try { process.kill(pid, "SIGTERM"); } catch { /* already gone */ }
  for (let waited = 0; waited < END_MS && isAlive(pid); waited += 50) await delay(50);
  if (isAlive(pid)) {
    try { process.kill(pid, "SIGKILL"); } catch { /* already gone */ }
    for (let waited = 0; waited < 1_000 && isAlive(pid); waited += 50) await delay(50);
  }
  session.forwardPid = null;
}

/** The record as the runtime reads it: a forward that no longer runs is a stopped VM. */
export async function inspect(name) {
  const session = loadSession(name);
  if (!session) return null;
  const running = isAlive(session.forwardPid);
  return {
    state: running ? "running" : "stopped",
    image: null,
    digest: null,
    labels: session.labels ?? {},
    ports: [{ guest: session.workerPort, host: session.hostPort, bind: "127.0.0.1" }],
    home: session.home ?? null,
    store: session.store ?? null,
    created: session.created ?? null,
  };
}

/**
 * One host serves one Store: the Store path is a single directory on a disk the
 * user owns, and two projects naming it would write one Store file between
 * them. This is the provider's own rule, checked here because the runtime's
 * per-project VM names would otherwise hide the collision.
 */
function sharedWith(name, host, storePath) {
  for (const file of existsSync(stateDir()) ? readdirSync(stateDir()).filter((entry) => entry.endsWith(".json")) : []) {
    if (file === `${name}.json`) continue;
    try {
      const other = JSON.parse(readFileSync(join(stateDir(), file), "utf8"));
      if (other.host === host && other.storePath === storePath) return other.name;
    } catch { /* a record that cannot be read is not one of ours to judge */ }
  }
  return null;
}

/**
 * Step ①–④: the host, its Store path, and the private stream. Everything that
 * can refuse is checked before a session is written, so a create that fails
 * leaves no session, no forward and nothing uploaded.
 */
export async function create(config) {
  const given = config.ssh ?? {};
  const host = String(config.host ?? given.host ?? "").trim();
  if (!host) throw refuse("InvalidInput", "vm:ssh needs the host to reach: settings.vm.host, or LAYERFS_SSH_HOST");
  if (config.password || given.password) throw refuse("InvalidInput", "vm:ssh does not take a password; give a key file as identityFile");

  const session = {
    name: config.name,
    host,
    user: String(config.user ?? given.user ?? "root").trim(),
    port: Number(config.port ?? given.port ?? 22),
    identityFile: config.identityFile ?? given.identityFile ?? null,
    strictHostKeyChecking: config.strictHostKeyChecking ?? given.strictHostKeyChecking ?? "yes",
    knownHostsFile: config.knownHostsFile ?? given.knownHostsFile ?? null,
    controlMaster: config.controlMaster ?? given.controlMaster ?? false,
    storePath: String(config.storePath ?? given.storePath ?? STORE_PATH_DEFAULT).trim(),
    hostPort: config.hostPort,
    workerPort: config.workerPort ?? WORKER_PORT_DEFAULT,
    home: config.home,
    store: config.storeName,
    labels: { "layerfs.home": config.home, "layerfs.role": "vm" },
    created: new Date().toISOString(),
  };
  if (session.identityFile && !existsSync(session.identityFile)) {
    throw refuse("InvalidInput", `the SSH identity file does not exist: ${session.identityFile}`);
  }
  const shared = sharedWith(session.name, session.host, session.storePath);
  if (shared) {
    throw refuse("Busy", `${session.host}:${session.storePath} already holds the Store of ${shared}; one host serves one Store, so give this project its own host or disk`, null);
  }

  // the host, by fact: a Linux kernel, and the Store path on ext4
  const kernel = await remote(session, ["uname", "-s"]).catch((error) => { throw refuse("Failed", `could not reach ${session.host} over SSH: ${error.message}`); });
  if (kernel.code !== 0) throw refuse("Failed", `SSH to ${session.host} failed (exit ${kernel.code}): ${kernel.stderr.trim()}`);
  if (kernel.stdout.trim() !== "Linux") throw refuse("Unsupported", `${session.host} answers ${kernel.stdout.trim() || "nothing"}, not Linux; vm:ssh needs a Linux host`);

  const exists = await remote(session, ["sh", "-c", "test -e \"$1\"", "sh", session.storePath]);
  if (exists.code !== 0) throw refuse("InvalidInput", `the Store path ${session.storePath} does not exist on ${session.host}; vm:ssh uses a disk the user provides`);
  const block = (await remote(session, ["sh", "-c", "test -b \"$1\"", "sh", session.storePath])).code === 0;
  const type = block
    ? (await remote(session, ["sh", "-c", "blkid -s TYPE -o value \"$1\" 2>/dev/null || lsblk -no FSTYPE \"$1\"", "sh", session.storePath])).stdout.trim()
    : (await remote(session, ["findmnt", "-T", session.storePath, "-o", "FSTYPE", "-n"])).stdout.trim();
  if (type !== "ext4") {
    throw refuse("Unsupported", `the Store path ${session.storePath} on ${session.host} is ${type || "an unreadable filesystem"}, not ext4; LayerFS requires an ext4 Store disk`);
  }

  // the guest's own view: /layerfs/store is that path (mounted, or bind-mounted)
  const attached = await remote(session, ["sh", "-c",
    "if [ \"$1\" = /layerfs/store ]; then exit 0; fi; mkdir -p /layerfs/store && mountpoint -q /layerfs/store || { if [ -b \"$1\" ]; then mount -t ext4 \"$1\" /layerfs/store; else mount --bind \"$1\" /layerfs/store; fi; }",
    "sh", session.storePath]);
  if (attached.code !== 0) throw refuse("Failed", `could not attach ${session.storePath} at /layerfs/store on ${session.host}: ${attached.stderr.trim()}`);

  try {
    const forward = await startForward(session, session.hostPort, session.workerPort);
    session.forwardPid = forward.pid;
  } catch (error) {
    throw refuse("Failed", `could not open the private stream to ${session.host}: ${error.message}`);
  }
  saveSession(session);
  return inspect(session.name);
}

/** A stopped VM is started again by re-opening its stream on the port it kept. */
export async function start(name) {
  const session = loadSession(name);
  if (!session) throw refuse("Failed", `there is no vm:ssh session called ${name}`);
  if (isAlive(session.forwardPid)) return inspect(name);
  const forward = await startForward(session, session.hostPort, session.workerPort);
  session.forwardPid = forward.pid;
  saveSession(session);
  return inspect(name);
}

/** Step ⑤, stop: the stream and the worker end; the Store disk and the session record stay. */
export async function stop(name) {
  const session = loadSession(name);
  if (!session) return;
  await endForward(session);
  await remote(session, ["sh", "-c", "if [ -f /layerfs/state/worker.pid ]; then kill \"$(cat /layerfs/state/worker.pid)\" 2>/dev/null; fi"]).catch(() => {});
  saveSession(session);
}

export const restart = async (name) => {
  await stop(name);
  return start(name);
};

/**
 * Step ⑤, delete: the record, the forward and what this adapter put on the host.
 * The Store disk is the user's and is never unmounted, formatted or removed; a
 * delete while the project still holds its Store is refused by the runtime.
 */
export async function remove(name) {
  const session = loadSession(name);
  if (!session) return;
  await endForward(session);
  await remote(session, ["sh", "-c",
    "if [ -f /layerfs/state/worker.pid ]; then kill \"$(cat /layerfs/state/worker.pid)\" 2>/dev/null; fi; rm -rf /layerfs/state /layerfs/bundle"]).catch(() => {});
  deleteSession(name);
}

/** The Store disk is the user's: this adapter has nothing of its own to delete. */
export const removeStore = async () => {};

/** V4: the host-wide user-namespace block; the caller reads it back. */
export const blockUserNamespaces = (name) =>
  exec(name, ["sh", "-c", "echo 0 > /proc/sys/user/max_user_namespaces"]);

/** Free bytes of the Store disk, as the host sees them. */
export async function storeFree(name, guestStorePath = STORE_PATH_DEFAULT) {
  const { code, stdout } = await exec(name, ["df", "-kP", guestStorePath]);
  if (code !== 0) return null;
  const kb = Number(stdout.trim().split("\n").pop().trim().split(/\s+/)[3]);
  return Number.isFinite(kb) ? kb * 1024 : null;
}

export function exec(name, argv, options = {}) {
  const session = loadSession(name);
  if (!session) throw refuse("Failed", `there is no vm:ssh session called ${name}`);
  return remote(session, argv, options);
}

/**
 * Step ②: the bundle (or any folder) as one `tar` stream, unpacked on the host.
 * Both ends are killed if the transfer passes its budget, so no copy outlives
 * its call.
 */
export function copy(name, source, destination) {
  const session = loadSession(name);
  if (!session) throw refuse("Failed", `there is no vm:ssh session called ${name}`);
  const script = `mkdir -p ${shellEscape(destination)} && tar -C ${shellEscape(destination)} -xf -`;
  return new Promise((resolve, reject) => {
    const pack = spawn("tar", ["-C", source, "-cf", "-", "."], { stdio: ["ignore", "pipe", "pipe"] });
    const unpack = spawn("ssh", [...sshArgs(session), destinationOf(session), "--", script], { stdio: ["pipe", "pipe", "pipe"] });
    let err = "", tarErr = "";
    const timer = setTimeout(() => {
      pack.kill("SIGKILL");
      unpack.kill("SIGKILL");
      reject(new Error(`copy into ${name}:${destination} did not end within ${COPY_MS / 1000} s`));
    }, COPY_MS);
    unpack.stderr.on("data", (chunk) => { err += chunk.toString(); });
    pack.stderr.on("data", (chunk) => { tarErr += chunk.toString(); });
    pack.on("error", (error) => { clearTimeout(timer); reject(error); });
    unpack.on("error", (error) => { clearTimeout(timer); reject(error); });
    unpack.on("close", (code) => {
      clearTimeout(timer);
      if (code === 0) resolve();
      else reject(new Error(`copy into ${name}:${destination} failed (exit ${code}): ${(err || tarErr).trim()}`));
    });
    pack.stdout.pipe(unpack.stdin);
  });
}

export const read = (name, path) => readFile("ssh", name, path, exec);
export const write = (name, path, text) => writeFile("ssh", name, path, text, exec);

export default {
  name: "ssh",
  defaultImage: null,
  inspect, create, start, stop, restart, remove, removeStore,
  blockUserNamespaces, storeFree, read, write, copy, exec, isAlive,
};

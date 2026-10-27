// Windows: the worker is one Node process per LayerFS home in a selected WSL2
// distribution, running worker/server.js of a sealed Linux bundle with
// `--stdin` (AGENTS.md: open to any integration, 2026-10-24). Linux owns native
// handles, Store I/O and that distribution's Docker Engine; Windows owns the
// caller. Copied from the dsh Host's windows/host.js (#428). This process
// owns the worker through its stdin: closing the runtime ends stdin, and the
// worker drains its admitted calls and ends. Nothing is killed.
import { spawn } from "node:child_process";
import { createHash, randomBytes } from "node:crypto";
import { createWriteStream, mkdirSync, readFileSync, rmSync } from "node:fs";
import { join, posix } from "node:path";
import { fileURLToPath } from "node:url";
import { connection, frame, readFrames } from "@layerfs/agent/internal";
import { endpointFile, ping, ready, refuse, run, writeEndpoint } from "../shared.js";
import { absoluteLinux, projectHome, wsl, wslArgs } from "./paths.js";

const SERVER = "api/runtime/src/worker/server.js";
const workers = new Map();

const docker = (settings, args, options = {}) => wsl(settings, ["docker", "-H", `unix://${settings.docker}`, ...args], { timeout: 5000, ...options }).then((r) => r.stdout.trim());

/** Only this Windows user and SYSTEM may read the endpoint's folder. */
async function protect(directory) {
  const { stdout } = await run("whoami.exe", ["/user", "/fo", "csv", "/nh"], { windowsHide: true });
  const sid = /S-1-[0-9-]+/.exec(stdout)?.[0];
  if (!sid) throw refuse("Failed", "could not determine the Windows user's SID");
  await run("icacls.exe", [directory, "/inheritance:r", "/grant:r", `*${sid}:(OI)(CI)F`, "*S-1-5-18:(OI)(CI)F"], { windowsHide: true });
}

/** One start per home in this process. A failed start stays failed until worker.start. */
function start(settings) {
  let held = workers.get(settings.home);
  if (held) return held.ready.then(() => status(settings));
  held = { child: null, ready: null };
  workers.set(settings.home, held);
  held.ready = launch(settings, held);
  return held.ready.then(() => status(settings));
}

async function launch(settings, held) {
  if (!settings.bundle || !settings.image) throw refuse("InvalidInput", "the Windows worker needs image and a sealed Linux bundle");
  const directory = join(settings.home, "worker"), file = endpointFile(settings.home);
  mkdirSync(directory, { recursive: true });
  await protect(directory);
  // never replace a worker another live process owns, even a silent one
  let old;
  try { old = JSON.parse(readFileSync(file, "utf8")); } catch (error) { if (error.code !== "ENOENT") throw error; }
  if (old?.pid) {
    let alive = false;
    try { process.kill(old.pid, 0); alive = true; } catch (error) { if (error.code !== "ESRCH") throw error; }
    if (alive) throw refuse("Busy", "this LayerFS home already has a WSL worker owner; close it or select another home");
  }
  const bundle = absoluteLinux(settings.bundle, "bundle");
  const child = spawn("wsl.exe", wslArgs(settings.wsl, [settings.wsl.node, posix.join(bundle, SERVER), "--stdin"]), { windowsHide: true, stdio: ["pipe", "pipe", "pipe"] });
  held.child = child;
  child.stderr.pipe(createWriteStream(join(directory, "worker.log"), { flags: "a" }));
  child.on("error", () => {});
  child.stdin.on("error", () => {});
  const secret = randomBytes(32).toString("hex");
  const answer = await new Promise((resolve, reject) => {
    const fail = (error) => { clearTimeout(timer); reject(error); };
    const timer = setTimeout(() => { child.stdin.end(); fail(refuse("Failed", "the WSL worker did not become ready within 60 s; see worker/worker.log")); }, 60_000);
    child.once("error", fail);
    child.once("exit", (code, signal) => fail(refuse("Failed", `the WSL worker ended before readiness (${signal || code}); see worker/worker.log`)));
    readFrames(child.stdout, (value) => {
      if (!Number.isInteger(value.port) || value.port < 1) return fail(refuse("Failed", "invalid WSL worker readiness reply"));
      clearTimeout(timer);
      resolve(value);
    }, () => fail(refuse("Failed", "invalid WSL worker readiness frame")));
    child.stdin.write(frame({ secret, home: settings.wsl.home, bundle }));
  });
  // this process's @layerfs/agent must be the sealed build's (Git line endings normalized)
  const agent = fileURLToPath(new URL("../", import.meta.resolve("@layerfs/agent")));
  try {
    if (!answer.sdk || !Object.keys(answer.sdk).length) throw refuse("Failed", "the WSL worker supplied no SDK identity");
    for (const [path, expected] of Object.entries(answer.sdk)) {
      if (!path.startsWith("api/agent/") || path.includes("..")) throw refuse("Failed", "invalid SDK identity path");
      const source = readFileSync(join(agent, path.slice("api/agent/".length)), "utf8").replaceAll("\r\n", "\n");
      if (createHash("sha256").update(source).digest("hex") !== expected) throw refuse("Failed", `this @layerfs/agent differs from the sealed Linux build: ${path}`);
    }
  } catch (error) { child.stdin.end(); throw error; }
  const info = { home: settings.wsl.home, port: answer.port, build: answer.build, secret, pid: child.pid, startedAt: new Date().toISOString() };
  writeEndpoint(file, info);
  // loopback forwarding may become ready after WSL's listener itself
  await ready(info, 10_000, "the WSL worker's loopback listener");
  return info;
}

async function status(settings) {
  let info;
  try { info = JSON.parse(readFileSync(endpointFile(settings.home), "utf8")); } catch { return { worker: { name: "wsl-worker", state: "missing" } }; }
  const reply = await ping(info);
  return { worker: { name: "wsl-worker", state: reply?.ready ? "running" : "unknown", answering: !!reply?.ready, active: reply?.active, stores: reply?.stores, build: reply?.build, startedAt: info.startedAt } };
}

/** Graceful only: stdin ends, admitted calls drain. Nothing is killed. */
async function close(settings) {
  const held = workers.get(settings.home);
  if (!held?.child) return;
  const child = held.child;
  child.stdin.end();
  await new Promise((resolve, reject) => {
    if (child.exitCode !== null || child.signalCode !== null) return resolve();
    const timer = setTimeout(() => reject(refuse("Busy", "the WSL worker is still draining; it was left running")), 10_000);
    child.once("exit", () => { clearTimeout(timer); resolve(); });
  });
  workers.delete(settings.home);
  rmSync(endpointFile(settings.home), { force: true });
}

async function restart(settings) {
  const { worker } = await status(settings);
  if (!worker.answering || worker.active) throw refuse("Busy", "the WSL worker is not answering or has active calls; it was not stopped");
  await close(settings);
  return start(settings);
}

export default {
  name: "windows",
  endpoint: (settings) => endpointFile(settings.home),
  status,
  async start(settings) {
    const held = workers.get(settings.home);
    if (held?.child && held.child.exitCode === null && held.child.signalCode === null) throw refuse("Busy", "the WSL worker already runs; restart it instead");
    workers.delete(settings.home);
    return start(settings);
  },
  restart,
  close,
  /** Init runs in the worker, which reads the folder through WSL. */
  async init(settings) { await start(settings); },
  path: (settings, hostPath) => hostPath,
  docker,
  storePath: (settings, volume) => `/var/lib/docker/volumes/${volume}/_data/global/store.sqlite`,
  async homeNames(settings) {
    const { stdout } = await wsl(settings, ["find", settings.wsl.home, "-mindepth", "2", "-maxdepth", "2", "-name", "project.record"], { timeout: 5000 }).catch(() => ({ stdout: "" }));
    return stdout.split("\n").filter(Boolean).map((line) => posix.basename(posix.dirname(line))).sort();
  },
  // rm gets one checked child directory as an argv item, never shell text
  async removeHome(settings, name) { await wsl(settings, ["rm", "-rf", "--", projectHome(settings.wsl, name)], { timeout: 10_000 }); },
  /** Before a volume is deleted, the worker closes its cached Store of it. */
  async releaseStore(settings, volume) {
    if (!/^[a-zA-Z0-9][a-zA-Z0-9_.-]*$/.test(volume)) throw refuse("InvalidInput", "invalid Store volume name");
    const info = JSON.parse(readFileSync(endpointFile(settings.home), "utf8"));
    const answer = await connection(info).call({ fn: "releaseStore", args: { home: settings.wsl.home, storePath: `/var/lib/docker/volumes/${volume}/_data/global/store.sqlite` } });
    if (answer.refused) throw refuse(answer.refused.code, answer.refused.message, answer.refused.detail);
  },
  async free(settings) {
    try {
      const { stdout } = await wsl(settings, ["df", "-kP", settings.wsl.home], { timeout: 5000 });
      const kb = Number(stdout.trim().split("\n").pop().trim().split(/\s+/)[3]);
      return Number.isFinite(kb) ? kb * 1024 : null;
    } catch { return null; }
  },
};

// The native binding of a ported Live project (macOS only; the owner's
// exception of 2026-10-20 to the Direct Store rule, AGENTS.md). The real SDK
// runs in this process; each binding call goes to the Store container's worker
// (box.js, worker.mjs), which makes it with the real Linux binding and keeps
// the project's Store and daemon connections open while they are in use. This
// process holds one connection to the worker, idle connections do not keep it
// alive, and the worker's port and secret come from the LayerFS home's Store container
// file. Init alone runs in its own container (run.mjs), which mounts its
// source folder.
import { execFile, spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { connect } from "node:net";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { callMounts, DAEMON_HOST, storeFile } from "../box.js";
import { layout, containerized, homeInContainer } from "../container.js";
import { frame, parseValue, readFrames, stringifyValue } from "./frames.mjs";

const RUN = fileURLToPath(new URL("./run.mjs", import.meta.url));
/** Stores this process has opened, by path, with their clients. */
const opened = new Map();
const setting = (name) => process.env[`LAYERFS_${name}`] || process.env[`DSH_LAYERFS_${name}`];
const refused = (code, message) => ({ refused: { code, message, detail: null } });

/** Whose calls found the Store container down or unreachable: the Host, which starts it again. */
const hooks = new Set();
/** @param {() => void} hook @returns {() => void} removes it */
export const onUnreachable = (hook) => (hooks.add(hook), () => hooks.delete(hook));
/** A call's own answer when the Store container is not there or does not answer; the hooks are told, and the call is never sent again. */
const down = (code, message) => {
  for (const hook of hooks) hook();
  return refused(code, message);
};
/** The messages of those answers, as an agent's trace line carries them to the Host. */
export const UNREACHABLE = /the Store container [^\s']+(?:'s worker did not answer| has not started| is not there| did not run project\.init)|the connection to the Store container [^\s']+ was lost/;

/**
 * One binding call in Linux: its answer, `{ value }` or `{ refused }`. A call
 * to a named Sandbox (the operator's) carries no deployment, and the Linux
 * binding takes the daemon's host only from one: every call gets the Store container's.
 */
function inBox(fn, op, args) {
  const box = setting("STORE_CONTAINER"), bundle = setting("BUNDLE");
  if (!box || !bundle) return Promise.resolve(refused("Unsupported", "no Store container is set (DSH_LAYERFS_STORE_CONTAINER, DSH_LAYERFS_BUNDLE): Store calls run only in Linux"));
  const sent = { ...args, deploy: { ...args.deploy, daemonHost: args.deploy?.daemonHost ?? DAEMON_HOST } };
  // a Windows host's paths mean nothing in the container: a call names the home
  // as it is mounted there, while this process keeps its own path to find the Store file
  if (containerized) sent.home = homeInContainer(sent.home ?? setting("HOME"));
  const request = { fn, op, args: sent, open: opened.has(args.storePath) };
  // this process's own home: the mounts and the Store file are the Host's, while
  // `sent` already carries the path the container knows (line above)
  const home = args.home ?? setting("HOME");
  return op === "project.init" ? initCall(box, bundle, request, home) : worker(box, home).call(request);
}

export const call = (op, args) => inBox("call", op, args);
export const operatorCall = (op, args) => inBox("operatorCall", op, args);

/** The Store opens in the worker; here only its clients are counted. */
export async function storeOpen({ storePath }) {
  opened.set(storePath, (opened.get(storePath) ?? 0) + 1);
  return { value: null };
}
export async function storeClose({ storePath }) {
  const left = (opened.get(storePath) ?? 1) - 1;
  if (left > 0) opened.set(storePath, left); else opened.delete(storePath);
  return { value: null };
}

/** The worker returns a spooled exec's output whole, so no spool is left to read here. */
export async function execOutput() {
  return refused("Failed", "a ported exec's output arrives whole; there is no spool to read");
}

let link = null;
/**
 * This process's connection to the Store container's worker, opened on first use. Calls
 * wait for the worker's answer to the secret: a connection lost before it
 * carried none of them (Failed), one lost after may have run them (Unknown).
 */
function worker(box, home) {
  if (link && !link.closed) return link;
  let info;
  try { info = JSON.parse(readFileSync(storeFile(home), "utf8")); } catch { info = null; }
  if (!info || info.box !== box) return { call: async () => down("Failed", `the Store container ${box} has not started for ${home}: dsh starts it; run the call again once it is up`) };
  const pending = new Map(), waiting = [];
  let next = 1, greeted = false;
  const socket = connect({ host: "127.0.0.1", port: info.port });
  socket.setNoDelay(true);
  socket.unref();
  socket.write(frame({ secret: info.secret }));
  const send = (id, request) => socket.write(frame({ id, ...request }));
  readFrames(socket, ({ id, answer }) => {
    if (id === 0) {
      greeted = true;
      for (const [waitingId, request] of waiting.splice(0)) send(waitingId, request);
      return;
    }
    const p = pending.get(id);
    if (!p) return;
    pending.delete(id);
    if (!pending.size) socket.unref();
    if (answer?.thrown) p.reject(new Error(answer.thrown)); else p.resolve(answer);
  }, () => socket.destroy());
  const self = { closed: false };
  const lost = (error) => {
    self.closed = true;
    const why = error ? ` (${error.message})` : "";
    const answer = greeted
      ? down("Unknown", `the connection to the Store container ${box} was lost${why}: check the project before going on`)
      : down("Failed", `the Store container ${box}'s worker did not answer${why}: dsh starts it again; run the call again once it is up`);
    for (const p of pending.values()) p.resolve(answer);
    pending.clear();
    waiting.length = 0;
  };
  socket.on("error", lost);
  socket.on("close", () => lost(null));
  self.call = (request) => {
    if (self.closed) return worker(box, home).call(request);
    return new Promise((resolve, reject) => {
      const id = next++;
      pending.set(id, { resolve, reject });
      socket.ref();
      if (greeted) send(id, request); else waiting.push([id, request]);
    });
  };
  return (link = self);
}

/**
 * Init in its own container on the Store container's image, which the build was made for
 * (the project's Sandboxes may run another), with the source folder mounted
 * read-only, so the Store container shares no host folder beyond the ones every call uses.
 */
async function initCall(box, bundle, request, home) {
  const { args } = request;
  let image;
  try { image = await boxImage(box); } catch { return down("Failed", `the Store container ${box} is not there to run project.init`); }
  const env = Object.keys(process.env).filter((k) => /^(DSH_)?LAYERFS_/.test(k) && !k.endsWith("_NATIVE")).flatMap((k) => ["-e", k]);
  // the mounts name the HOST's folders: `args.home` is the container's path, which a
  // container cannot mount. The source folder is mounted at its own path on a POSIX
  // host, under the container root on Windows.
  const path = layout({ home, bundle, port: dirname(RUN), source: args.path });
  const where = ["run", "--rm", "-i", "--init", "--pull", "never", ...callMounts({ home, bundle, source: args.path }), ...env, "--entrypoint", "node", image, `${path.port}/run.mjs`];
  return new Promise((resolve, reject) => {
    const child = spawn("docker", where, { stdio: ["pipe", "pipe", "inherit"] });
    const out = [];
    child.stdout.on("data", (chunk) => out.push(chunk));
    child.on("error", reject);
    child.on("close", (code) => {
      const bytes = Buffer.concat(out);
      if (code === 0 && bytes.length) {
        const answer = parseValue(bytes.toString("utf8"));
        return answer.thrown ? reject(new Error(answer.thrown)) : resolve(answer);
      }
      // 125 and up: docker could not start the process, so the call did not run
      resolve(code >= 125
        ? refused("Failed", `the Store container ${box} did not run project.init (docker run exit ${code})`)
        : refused("Unknown", `project.init ended without an answer (exit ${code}): check the project before going on`));
    });
    // the container opens the source folder at its container path
    child.stdin.end(stringifyValue({ ...request, args: { ...args, path: path.source } }));
  });
}

/** The image the Store container runs. */
function boxImage(box) {
  return new Promise((resolve, reject) => execFile("docker", ["container", "inspect", "-f", "{{.Image}}", box], (error, out) => (error ? reject(error) : resolve(out.trim()))));
}

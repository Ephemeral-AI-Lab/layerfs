// The worker client (#428): the binding of a process whose Store calls run
// in its LayerFS home's worker (`@layerfs/runtime`'s worker/server.js), a
// Linux container on macOS or a Node process in a WSL2 distribution on
// Windows. The real SDK runs in this process; each binding call is one frame
// to the worker, which makes it with the real Linux addon and keeps each
// project's Store and daemon connections open while they are in use.
// Merged from the dsh Host's port/binding.js (macOS) and windows/binding.js.
//
// The endpoint file (LAYERFS_WORKER_ENDPOINT), written by runtime's platform
// launcher, is JSON: { home, port, secret, build, daemonHost?, docker?, init? }.
// `daemonHost` is where the worker reaches a daemon's loopback-published
// port (macOS: the Docker host); `docker` (macOS) is the Docker socket the
// worker and Init's container reach, inside the engine's VM, and replaces
// every call's deploy.docker, a socket of this machine they cannot see;
// `init` (macOS) runs Init in a container of
// its own with the source folder mounted read-only: { argv, image, script }.
// Elsewhere Init is one more call to the worker, which maps the folder.
//
// One connection per endpoint, opened on first use and kept while calls are
// pending; idle, it does not keep the process alive. A call is sent once: a
// connection lost before it was sent answers Failed, one lost after Unknown,
// and nothing is sent again.
import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { connect } from "node:net";
import { decode, encode, frame, readFrames } from "./frames.js";

const refused = (code, message, detail = null) => ({ refused: { code, message, detail } });
const links = new Map();

/** The endpoint a launcher wrote, checked. */
export function endpointOf(file) {
  const info = JSON.parse(readFileSync(file, "utf8"));
  if (!Number.isInteger(info.port) || info.port < 1 || !/^[0-9a-f]{64}$/.test(info.secret ?? "") || typeof info.home !== "string") {
    throw new Error(`invalid LayerFS worker endpoint ${file}`);
  }
  return info;
}

/**
 * The connection to the worker at `info` (one per port and secret): its
 * `call(request)` answers `{ value }` or `{ refused }`. Calls wait for the
 * worker's answer to the secret before they are sent.
 */
export function connection(info) {
  const key = `${info.port}:${info.secret}`;
  if (links.has(key)) return links.get(key);
  const socket = connect({ host: "127.0.0.1", port: info.port });
  const pending = new Map();
  let next = 1, greeted = false, closed = false;
  socket.setNoDelay(true);
  socket.unref();
  socket.write(frame({ secret: info.secret }));
  const timer = setTimeout(() => lost("the worker did not answer its secret within 5 s"), 5000);
  timer.unref();
  const lost = (why) => {
    if (closed) return;
    closed = true;
    clearTimeout(timer);
    links.delete(key);
    for (const p of pending.values()) {
      p.resolve(p.sent
        ? refused("Unknown", `the connection to the LayerFS worker was lost (${why}) after the call was sent; it may have completed: check the project before going on`)
        : refused("Failed", `the LayerFS worker did not answer (${why}); the call was not sent`));
    }
    pending.clear();
    socket.destroy();
  };
  const send = (id, p) => { p.sent = true; socket.write(frame({ id, ...p.request })); };
  readFrames(socket, ({ id, answer }) => {
    if (id === 0) {
      if (greeted || answer?.value !== "hello") return lost("invalid greeting");
      greeted = true;
      clearTimeout(timer);
      for (const [n, p] of pending) send(n, p);
      return;
    }
    const p = pending.get(id);
    if (!p) return;
    pending.delete(id);
    if (!pending.size) socket.unref();
    p.resolve(answer?.thrown ? refused("Unknown", `the LayerFS worker failed while making the call: ${answer.thrown}; check the project before going on`) : answer);
  }, (error) => lost(error.message));
  socket.on("error", (error) => lost(error.message));
  socket.on("close", () => lost("socket closed"));
  const link = {
    call(request) {
      if (closed) return Promise.resolve(refused("Failed", "the LayerFS worker connection is closed; the call was not sent"));
      return new Promise((resolve) => {
        const id = next++, p = { resolve, request, sent: false };
        pending.set(id, p);
        socket.ref();
        if (greeted) send(id, p);
      });
    },
  };
  links.set(key, link);
  return link;
}

/**
 * Init in its own container (macOS), with the source folder mounted
 * read-only, made once: its answer on stdout, or Failed when docker could
 * not start it, or Unknown when it ended without an answer.
 */
function initRun(init, request) {
  const path = request.args.path;
  const env = Object.keys(process.env).filter((k) => /^LAYERFS_/.test(k) && !/_(NATIVE|WORKER_ENDPOINT)$/.test(k)).flatMap((k) => ["-e", k]);
  const argv = [...init.argv, "-v", `${path}:${path}:ro`, ...env, init.image, init.script];
  return new Promise((resolve) => {
    const child = spawn("docker", argv, { stdio: ["pipe", "pipe", "inherit"] });
    const out = [];
    child.stdout.on("data", (chunk) => out.push(chunk));
    child.on("error", (error) => resolve(refused("Failed", `docker could not run project.init: ${error.message}`)));
    child.on("close", (code) => {
      const bytes = Buffer.concat(out);
      if (code === 0 && bytes.length) {
        try {
          const answer = decode(bytes);
          return resolve(answer.thrown ? refused("Unknown", `project.init failed in its container: ${answer.thrown}; check the project before going on`) : answer);
        } catch (error) {
          return resolve(refused("Unknown", `project.init reply could not be decoded: ${error.message}; check the project before going on`));
        }
      }
      // 125 and up: docker could not start the process, so the call did not run
      resolve(code >= 125
        ? refused("Failed", `docker did not run project.init (exit ${code})`)
        : refused("Unknown", `project.init ended without an answer (exit ${code}): check the project before going on`));
    });
    child.stdin.end(encode(request));
  });
}

/** The binding that sends every call to the worker named by the endpoint file `file`. */
export function workerClient(file) {
  /** Stores this process has opened, by path, with their clients. */
  const opened = new Map();
  async function send(fn, op, args) {
    let info;
    try { info = endpointOf(file); } catch (error) { return refused("Failed", `the LayerFS worker is not running for this home: ${error.message}`); }
    const deploy = {
      ...args.deploy,
      ...(info.daemonHost ? { daemonHost: args.deploy?.daemonHost ?? info.daemonHost } : {}),
      ...(info.docker ? { docker: info.docker } : {}),
    };
    const sent = info.daemonHost || info.docker ? { ...args, deploy } : args;
    const request = { fn, op, args: sent, open: opened.has(args.storePath) };
    if (fn === "call" && op === "project.init" && info.init) return initRun(info.init, request);
    return connection(info).call(request);
  }
  return {
    call: (op, args) => send("call", op, args),
    backendCall: (op, args) => send("backendCall", op, args),
    /** The Store opens in the worker; here only its clients are counted. */
    async storeOpen({ storePath }) {
      opened.set(storePath, (opened.get(storePath) ?? 0) + 1);
      return { value: null };
    },
    async storeClose({ storePath }) {
      const left = (opened.get(storePath) ?? 1) - 1;
      if (left > 0) opened.set(storePath, left); else opened.delete(storePath);
      return { value: null };
    },
    /** The worker returns a spooled exec's output whole, so there is no spool to read here. */
    async execOutput() {
      return refused("Failed", "a worker exec's output arrives whole; there is no spool to read");
    },
  };
}

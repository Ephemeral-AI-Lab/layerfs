// The worker (#428): one program for both platforms whose Store calls cannot
// run in the caller's process (macOS and Windows; AGENTS.md, open to any
// integration since 2026-10-24). Merged from the dsh Host's
// port/worker.mjs and windows/worker.js. It makes every call with the real
// Linux addon of a sealed bundle, whose manifest.json hashes it checks
// first, and keeps each project's Store open while calls use it, closing it
// after a quiet minute.
//
//   node server.js           macOS: the main process of the `<prefix>worker`
//                            container. LAYERFS_WORKER_SECRET (64 hex),
//                            LAYERFS_HOME, LAYERFS_BUNDLE; listens on
//                            LAYERFS_WORKER_LISTEN (default 0.0.0.0:7070,
//                            published to the Mac's loopback only).
//   node server.js --stdin   Windows: a Node process in a WSL2 distribution.
//                            The parent sends one startup frame on stdin,
//                            { secret, home, bundle } (never a secret in
//                            argv); the worker listens on 127.0.0.1, answers
//                            { port, build, sdk } on stdout, and drains and
//                            ends when stdin ends.
//
// A connection's first frame must carry the secret (answered as id 0), in at
// most FIRST_LIMIT bytes and within SECRET_MS, or the socket is destroyed; then
// each frame { id, fn, op, args, open } is answered by { id, answer }. `ping`
// answers { ready, active, stores, build } at once. `fn` is call,
// backendCall or releaseStore, and `args.home` must be the worker's home.
// Each call is made once; one that throws answers { thrown }.
import { execFile } from "node:child_process";
import { createHash, randomBytes, timingSafeEqual } from "node:crypto";
import { createReadStream, mkdirSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { createRequire } from "node:module";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, posix, resolve, sep } from "node:path";
import { promisify } from "node:util";
import { decode, frame, readFrames } from "@layerfs/agent/internal";
import { made, storeOf } from "./call.js";

const IDLE_MS = 60_000;
/** The unauthenticated secret frame: its largest length, and how long a connection may wait to send it. */
const FIRST_LIMIT = 4096, SECRET_MS = 5000;
const FUNCTIONS = new Set(["call", "backendCall", "releaseStore"]);
process.env.UV_THREADPOOL_SIZE ??= "64";
process.env.LAYERFS_CONSTRUCTION_WORKERS ??= "1";

/** The bundle's manifest, after every file it lists hashed as recorded. */
async function sealed(bundle) {
  const manifest = JSON.parse(readFileSync(join(bundle, "manifest.json"), "utf8"));
  if (manifest.status !== "sealed" || manifest.platform !== "linux") throw new Error("the worker needs a sealed Linux bundle");
  for (const [name, expected] of Object.entries(manifest.files)) {
    const path = resolve(bundle, name);
    if (!path.startsWith(resolve(bundle) + sep)) throw new Error(`bundle file escapes its root: ${name}`);
    const hash = createHash("sha256");
    for await (const chunk of createReadStream(path)) hash.update(chunk);
    if (hash.digest("hex") !== expected) throw new Error(`sealed bundle file changed: ${name}`);
  }
  return manifest;
}

/** Startup settings: from the environment, or the one frame on stdin. */
async function settings(stdin) {
  if (!stdin) {
    const [host, port] = (process.env.LAYERFS_WORKER_LISTEN || "0.0.0.0:7070").split(/:(?=\d+$)/);
    return { secret: process.env.LAYERFS_WORKER_SECRET, home: process.env.LAYERFS_HOME, bundle: process.env.LAYERFS_BUNDLE, host, port: Number(port) };
  }
  // read with a listener, not an iterator: stdin stays open, and its end later drains the worker
  return new Promise((resolve, reject) => {
    let bytes = Buffer.alloc(0);
    const data = (chunk) => {
      bytes = Buffer.concat([bytes, chunk]);
      if (bytes.length < 4 || bytes.length < 4 + bytes.readUInt32BE(0)) return;
      process.stdin.off("data", data).off("end", early).pause();
      try { resolve({ ...decode(bytes.subarray(4, 4 + bytes.readUInt32BE(0))), host: "127.0.0.1", port: 0 }); } catch (error) { reject(error); }
    };
    const early = () => reject(new Error("stdin ended before the startup frame"));
    process.stdin.on("data", data).on("end", early);
  });
}

/** A Windows path of Init's source folder as this distribution sees it. */
async function linuxPath(path) {
  if (path.startsWith("/")) return posix.normalize(path);
  const unc = /^\\\\(?:wsl\.localhost|wsl\$)\\([^\\]+)(\\.*)?$/i.exec(path);
  if (unc) {
    if (unc[1].toLowerCase() !== (process.env.WSL_DISTRO_NAME ?? "").toLowerCase()) throw new Error("the import path names another WSL distribution");
    return posix.normalize((unc[2] || "\\").replaceAll("\\", "/"));
  }
  if (!/^[a-z]:[\\/]/i.test(path)) throw new Error("an import path must be absolute");
  const { stdout } = await promisify(execFile)("wslpath", ["-a", "-u", path], { timeout: 5000 });
  return posix.normalize(stdout.trim());
}

const stdin = process.argv.includes("--stdin");
const config = await settings(stdin);
const secret = Buffer.from(String(config.secret ?? ""), "hex");
if (secret.length !== 32 || !config.home?.startsWith("/") || !config.bundle?.startsWith("/")) {
  console.error("layerfs worker: needs a 64-hex secret, an absolute home and an absolute bundle");
  process.exit(2);
}
const manifest = await sealed(config.bundle);
mkdirSync(config.home, { recursive: true, mode: 0o700 });
// The addon opens each Store with a folder under TMPDIR named by pid. A
// restarted container keeps its /tmp and may reuse the pid, so a private
// folder per start keeps an earlier run's folders from refusing the open.
for (const name of readdirSync(tmpdir())) if (name.startsWith("layerfs-worker-")) rmSync(join(tmpdir(), name), { recursive: true, force: true });
const temp = join(tmpdir(), `layerfs-worker-${randomBytes(12).toString("hex")}`);
mkdirSync(temp, { mode: 0o700 });
process.env.TMPDIR = temp;
const binding = createRequire(import.meta.url)(process.env.LAYERFS_NATIVE || join(config.bundle, "layerfs.node"));

/** Open Stores by path: { opening, users, timer, args, closing }. */
const stores = new Map(), sockets = new Set();
let active = 0, closing = false, finishing = false;

async function closeStore(path) {
  const held = stores.get(path);
  if (!held) return { value: null };
  if (held.users) return { refused: { code: "Busy", message: "the project's Store has active calls", detail: null } };
  clearTimeout(held.timer);
  held.closing ??= held.opening.then(() => binding.storeClose(storeOf(held.args))).then((answer) => {
    if (answer?.refused) throw new Error(JSON.stringify(answer.refused));
  }).finally(() => { if (stores.get(path) === held) stores.delete(path); });
  await held.closing;
  return { value: null };
}

/** Runs `work` with the call's Store open: a Store that does not open answers its refusal. */
async function withStore(args, work) {
  let held = stores.get(args.storePath);
  while (held?.closing) { await held.closing.catch(() => {}); held = stores.get(args.storePath); }
  if (!held) stores.set(args.storePath, (held = { opening: binding.storeOpen(storeOf(args)), users: 0, timer: null, args }));
  held.users++;
  clearTimeout(held.timer);
  try {
    const opened = await held.opening.catch((error) => ({ thrown: error }));
    if (opened?.refused || opened?.thrown) {
      if (stores.get(args.storePath) === held) stores.delete(args.storePath);
      if (opened.thrown) throw opened.thrown;
      return opened;
    }
    return await work();
  } finally {
    if (--held.users === 0 && stores.get(args.storePath) === held) {
      held.timer = setTimeout(() => closeStore(args.storePath).catch((error) => console.error(`layerfs worker: closing ${args.storePath}: ${error}`)), IDLE_MS);
    }
  }
}

async function answer({ fn, op, args, open }) {
  if (fn === "releaseStore") return closeStore(args.storePath);
  const sent = stdin && fn === "call" && op === "project.init" && typeof args.path === "string" ? { ...args, path: await linuxPath(args.path) } : args;
  const call = () => made(binding, fn, op, sent);
  return open ? withStore(sent, call) : call();
}

async function shutdown() {
  if (!closing) { closing = true; server.close(); }
  if (active || finishing) return;
  finishing = true;
  for (const path of [...stores.keys()]) await closeStore(path).catch((error) => console.error(error));
  for (const socket of sockets) socket.end();
  rmSync(temp, { recursive: true, force: true });
  process.stdin.destroy();
}

const server = createServer((socket) => {
  sockets.add(socket);
  socket.setNoDelay(true);
  socket.on("error", () => {}); // a caller that went away; its calls still finish here
  socket.on("close", () => sockets.delete(socket));
  let trusted = false;
  const silent = setTimeout(() => socket.destroy(), SECRET_MS);
  socket.on("close", () => clearTimeout(silent));
  readFrames(socket, async (request) => {
    if (!trusted) {
      const given = Buffer.from(String(request?.secret ?? ""), "hex");
      if (given.length !== secret.length || !timingSafeEqual(given, secret)) return socket.destroy();
      trusted = true;
      clearTimeout(silent);
      return socket.write(frame({ id: 0, answer: { value: "hello" } }));
    }
    const { id, fn, args } = request;
    if (fn === "ping") return socket.write(frame({ id, answer: { value: { ready: true, active, stores: stores.size, build: manifest.build_id } } }));
    if (closing) return socket.write(frame({ id, answer: { refused: { code: "Failed", message: "the LayerFS worker is closing; the call was not attempted", detail: null } } }));
    if (!FUNCTIONS.has(fn) || args?.home !== config.home) return socket.destroy();
    active++;
    let result;
    try { result = await answer(request); } catch (error) { result = { thrown: String(error?.stack ?? error) }; } finally { active--; }
    if (!socket.destroyed) socket.write(frame({ id, answer: result }));
    if (closing && active === 0) shutdown().catch((error) => console.error(error));
  }, (error) => { console.error(error.message); socket.destroy(); }, { firstLimit: FIRST_LIMIT });
});

server.listen(config.port, config.host, () => {
  const port = server.address().port;
  console.error(`layerfs worker: listening on ${config.host}:${port}, build ${manifest.build_id}`);
  if (!stdin) return;
  // the caller checks its own @layerfs/agent against the sealed one
  const sdk = Object.fromEntries(Object.entries(manifest.files).filter(([path]) => path.startsWith("api/agent/src/")));
  process.stdout.write(frame({ port, build: manifest.build_id, sdk }));
  process.stdin.on("end", () => shutdown().catch((error) => console.error(error)));
  process.stdin.resume();
});

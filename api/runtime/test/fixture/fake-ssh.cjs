// The `ssh` stand-in for @layerfs/runtime's vm:ssh tests (#515 §6): no host.
// It answers exactly the calls `platform/vm/ssh.js` makes, keeps one guest root
// under FAKE_SSH_STATE/guest, and serves the private stream itself:
//
//   ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:H:127.0.0.1:G …   the forward
//   ssh … -- uname -s | findmnt | blkid | df -kP | sh -c SCRIPT sh ARGS…
//   tar -C <source> -cf - . | ssh … -- mkdir -p DEST && tar -C DEST -xf -
//
// Facts worth knowing before changing it:
//   - the forward is lazy: the worker's port is read from state/worker.port when
//     a client connects, so the forward may start before the worker does, and
//     two test runs never fight over one fixed port;
//   - it exits 255 with `Address already in use` when its local port is taken,
//     which is what `-o ExitOnForwardFailure=yes` reports;
//   - `FAKE_SSH_UNREACHABLE=1` makes every call fail like a dead network,
//     `FAKE_SSH_STORE_MISSING=/path` makes `test -e` fail for that path, and
//     `FAKE_SSH_STORE_NON_EXT4=xfs` answers that filesystem instead of ext4;
//   - every call is appended to FAKE_SSH_STATE/calls.jsonl as { at, argv }.
const { appendFileSync, existsSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } = require("node:fs");
const { spawn } = require("node:child_process");
const { createServer, connect } = require("node:net");
const { dirname, join } = require("node:path");

const state = process.env.FAKE_SSH_STATE;
if (!state) {
  console.error("fake ssh: FAKE_SSH_STATE is not set");
  process.exit(2);
}

const argv = process.argv.slice(2);
appendFileSync(join(state, "calls.jsonl"), `${JSON.stringify({ at: Date.now(), argv })}\n`);

const guestIn = (path) => join(state, "guest", path.replace(/^\//, ""));
const workerPortFile = join(state, "worker.port");
const workerPidFile = () => guestIn("/layerfs/state/worker.pid");
const log = (...parts) => process.stderr.write(`${parts.join(" ")}\n`);

/** A port nothing listens on now: the stand-in worker's own, chosen per launch. */
function freePort() {
  return new Promise((resolve) => {
    const server = createServer();
    server.listen(0, "127.0.0.1", () => {
      const { port } = server.address();
      server.close(() => resolve(port));
    });
  });
}

function endWorker() {
  const file = workerPidFile();
  if (!existsSync(file)) return;
  try { process.kill(Number(readFileSync(file, "utf8").trim())); } catch { /* already gone */ }
}

/** The bundle's own worker, on a free port this stand-in records. */
async function launchWorker() {
  endWorker();
  const envFile = guestIn("/layerfs/state/worker.env");
  const env = Object.fromEntries(readFileSync(envFile, "utf8").split("\n").filter(Boolean).map((line) => [line.slice(0, line.indexOf("=")), line.slice(line.indexOf("=") + 1)]));
  const script = guestIn("/layerfs/bundle/api/runtime/src/worker/server.js");
  if (!existsSync(script)) { log("the bundle holds no worker/server.js"); process.exit(1); }
  const port = await freePort();
  writeFileSync(workerPortFile, `${port}\n`);
  const logFile = guestIn("/layerfs/state/worker.log");
  mkdirSync(dirname(logFile), { recursive: true });
  const child = spawn(process.execPath, [script], {
    env: { ...process.env, ...env, LAYERFS_HOME: guestIn(env.LAYERFS_HOME), LAYERFS_BUNDLE: guestIn(env.LAYERFS_BUNDLE), LAYERFS_WORKER_LISTEN: `127.0.0.1:${port}`, LAYERFS_NATIVE: process.env.FAKE_SSH_NATIVE },
    detached: true,
    stdio: ["ignore", "ignore", openSync(logFile, "a")],
  });
  child.unref();
  mkdirSync(dirname(workerPidFile()), { recursive: true });
  writeFileSync(workerPidFile(), `${child.pid}\n`);
}

/**
 * The private stream. Lazy on purpose: a client that arrives before the worker
 * exists waits up to a second for state/worker.port rather than failing.
 */
function forward(hostPort) {
  const server = createServer((client) => {
    client.on("error", () => {});
    let tries = 0;
    const dial = () => {
      const port = existsSync(workerPortFile) ? Number(readFileSync(workerPortFile, "utf8").trim()) : NaN;
      if (!Number.isFinite(port)) {
        if (tries++ < 20) return setTimeout(dial, 50);
        return client.destroy();
      }
      const upstream = connect(port, "127.0.0.1");
      upstream.on("error", () => client.destroy());
      client.pipe(upstream);
      upstream.pipe(client);
    };
    dial();
  });
  server.on("error", (error) => {
    if (error.code === "EADDRINUSE") log(`bind [127.0.0.1]:${hostPort}: Address already in use`);
    else log(`forward error: ${error.message}`);
    process.exit(255);
  });
  server.listen(hostPort, "127.0.0.1");
}

/** `sh -c SCRIPT sh ARG…`: the handful of scripts the adapter sends. `argv[0]` is
 * the `sh` that names the script's `$0`, so the script's `$1` is `args[1]`. */
function shellScript(script, args) {
  const path = args[1];
  if (script.includes("max_user_namespaces")) {
    const file = guestIn("/proc/sys/user/max_user_namespaces");
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, "0\n");
    return 0;
  }
  if (script.includes("test -f \"$1\" && cat \"$1\"")) {
    const file = guestIn(path);
    if (!existsSync(file)) return 1;
    process.stdout.write(readFileSync(file));
    return 0;
  }
  if (script.includes("cat > \"$1\"")) {
    const file = guestIn(path);
    mkdirSync(dirname(file), { recursive: true, mode: 0o700 });
    writeFileSync(file, readFileSync(0), { mode: 0o600 });
    return 0;
  }
  if (script.includes("test -e \"$1\"")) return process.env.FAKE_SSH_STORE_MISSING === path ? 1 : 0;
  if (script.includes("blkid")) { process.stdout.write(`${process.env.FAKE_SSH_STORE_NON_EXT4 || "ext4"}\n`); return 0; }
  if (script.includes("test -b \"$1\"")) return 1;
  if (script.includes("mount") && script.includes("/layerfs/store")) return 0;
  if (script.includes("nohup node")) return launchWorker().then(() => 0);
  if (script.includes("worker.pid") && script.includes("rm -rf")) {
    endWorker();
    rmSync(guestIn("/layerfs/state"), { recursive: true, force: true });
    rmSync(guestIn("/layerfs/bundle"), { recursive: true, force: true });
    return 0;
  }
  if (script.includes("worker.pid")) { endWorker(); return 0; }
  return 0;
}

/** The unpack end of a bundle copy: `mkdir -p DEST && tar -C DEST -xf -`. */
function unpack(script) {
  const match = /tar -C ('([^']+)'|\S+) -xf -/.exec(script);
  const destination = guestIn(match ? match[2] ?? match[1] : "/layerfs/bundle");
  mkdirSync(destination, { recursive: true });
  const tar = spawn("tar", ["-C", destination, "-xf", "-"], { stdio: ["pipe", "ignore", "inherit"] });
  process.stdin.pipe(tar.stdin);
  tar.on("close", (code) => process.exit(code ?? 0));
}

async function main() {
  if (process.env.FAKE_SSH_UNREACHABLE) {
    log("ssh: connect to host: Network is unreachable");
    process.exit(255);
  }
  const dash = argv.indexOf("--");
  const remoteArgv = dash === -1 ? [] : argv.slice(dash + 1);
  const joined = remoteArgv.join(" ");

  if (argv.includes("-N")) {
    const index = argv.indexOf("-L");
    const port = index === -1 ? NaN : Number(/^127\.0\.0\.1:(\d+):/.exec(argv[index + 1])?.[1]);
    if (!Number.isFinite(port)) { log("ssh: no local port to forward"); process.exit(255); }
    return forward(port);
  }
  if (!remoteArgv.length) { log("ssh: no command"); process.exit(255); }
  if (remoteArgv[0] === "uname") { process.stdout.write(`${process.env.FAKE_SSH_OS || "Linux"}\n`); return; }
  if (remoteArgv[0] === "findmnt") { process.stdout.write(`${process.env.FAKE_SSH_STORE_NON_EXT4 || "ext4"}\n`); return; }
  if (remoteArgv[0] === "df") {
    process.stdout.write("Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/sda1 100000000 5000000 95000000 5% /layerfs/store\n");
    return;
  }
  if (remoteArgv[0] === "tar" || /tar -C/.test(joined)) return unpack(joined);
  if (remoteArgv[0] === "rm") { rmSync(guestIn(remoteArgv.at(-1)), { recursive: true, force: true }); return; }
  if (remoteArgv[0] === "sh") {
    const code = await shellScript(remoteArgv[2] ?? "", remoteArgv.slice(3));
    process.exit(code);
  }
  log(`fake ssh does not know ${joined}`);
  process.exit(1);
}

main();

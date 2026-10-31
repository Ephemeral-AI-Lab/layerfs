// The `limactl` and `ssh` stand-in for @layerfs/runtime's vm:lima and
// vm:firecracker tests (#527 platform pass): no VM. Called as
// `fake-lima.cjs limactl …` or `fake-lima.cjs ssh …` (the test puts one shim of
// each name on PATH). It answers exactly the calls `platform/vm/lima.js` and
// `platform/vm/firecracker.js` make, and maps each guest's absolute paths
// under FAKE_LIMA_STATE/guest/<vm>, so guest reads and writes are real files:
//
//   limactl list --json NAME | create --name N --tty=false - | start --tty=false N
//   limactl stop N | delete --force N | disk list --json | disk create D … | disk delete D
//   ssh -F <dir>/ssh.config lima-N -- sudo -n -- ARGV…      a root command in instance N
//       (a Lima VM's /layerfs/home is the host folder its template mounts there)
//   ssh -F … -N -o ExitOnForwardFailure=yes -L 127.0.0.1:H:IP:7070 lima-N   a forward
//
// ARGV arrives shell-quoted, as the remote shell would see it, and is unquoted
// here. In a Lima VM it is the adapter's guest command; in the Firecracker host
// it is one of firecracker.js's marked scripts (`# layerfs-firecracker:<op>`),
// and the `guest` script's own arguments are unquoted once more for the
// microVM. A started worker is the bundle's own worker/server.js with the
// stand-in binding: a Lima VM's listens on its forwarded host port, a microVM's
// on a free port behind the stand-in forward. `FAKE_FC_NO_KVM=1` makes the
// Firecracker host check fail. Every call is appended to calls.jsonl as
// { at, tool, argv } with ARGV unquoted.
const { appendFileSync, cpSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, rmSync, writeFileSync } = require("node:fs");
const { spawn } = require("node:child_process");
const { createServer, connect } = require("node:net");
const { dirname, join } = require("node:path");

const state = process.env.FAKE_LIMA_STATE;
if (!state) { console.error("fake lima: FAKE_LIMA_STATE is not set"); process.exit(2); }
const [tool, ...argv] = process.argv.slice(2);

/** One POSIX-quoted word, unquoted: bare text, '…' runs and backslash escapes. */
function unquote(word) {
  let out = "", quoted = false;
  for (let i = 0; i < word.length; i++) {
    const c = word[i];
    if (quoted) { if (c === "'") quoted = false; else out += c; }
    else if (c === "'") quoted = true;
    else if (c === "\\") out += word[++i] ?? "";
    else out += c;
  }
  return out;
}

const fail = (message, code = 1) => { console.error(message); process.exit(code); };
const file = (...parts) => join(state, ...parts);
const readJson = (path) => (existsSync(path) ? JSON.parse(readFileSync(path, "utf8")) : null);
const saveJson = (path, value) => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, JSON.stringify(value, null, 1)); };
const guestIn = (vm) => (path) => join(state, "guest", vm, path.replace(/^\//, ""));
const stdin = () => readFileSync(0);
const log = (args) => appendFileSync(file("calls.jsonl"), `${JSON.stringify({ at: Date.now(), tool, argv: args })}\n`);

function endWorker(vm) {
  const pid = guestIn(vm)("/layerfs/state/worker.pid");
  if (!existsSync(pid)) return;
  try { process.kill(Number(readFileSync(pid, "utf8").trim())); } catch { /* already gone */ }
  rmSync(pid, { force: true });
}

/** A port nothing listens on now. */
const freePort = () => new Promise((resolve) => {
  const server = createServer();
  server.listen(0, "127.0.0.1", () => { const { port } = server.address(); server.close(() => resolve(port)); });
});

/** The bundle's worker in guest `vm`, listening on `port`. */
function launchWorker(vm, port) {
  endWorker(vm);
  const guest = guestIn(vm);
  const env = Object.fromEntries(readFileSync(guest("/layerfs/state/worker.env"), "utf8").split("\n").filter(Boolean).map((line) => [line.slice(0, line.indexOf("=")), line.slice(line.indexOf("=") + 1)]));
  const script = guest("/layerfs/bundle/api/runtime/src/worker/server.js");
  if (!existsSync(script)) fail("the bundle holds no worker/server.js");
  mkdirSync(guest("/layerfs/state"), { recursive: true });
  const child = spawn(process.execPath, [script], {
    env: { ...process.env, ...env, LAYERFS_HOME: guest(env.LAYERFS_HOME), LAYERFS_BUNDLE: guest(env.LAYERFS_BUNDLE), LAYERFS_WORKER_LISTEN: `127.0.0.1:${port}`, LAYERFS_NATIVE: process.env.FAKE_LIMA_NATIVE },
    detached: true, stdio: ["ignore", "ignore", openSync(guest("/layerfs/state/worker.log"), "a")],
  });
  child.unref();
  writeFileSync(guest("/layerfs/state/worker.pid"), `${child.pid}\n`);
}

/** A guest command (Lima VM or microVM) as the adapters send it; answers its exit code. */
async function guestCommand(vm, args, workerPort) {
  const guest = guestIn(vm);
  const [cmd, flag, script, , ...rest] = args;
  if (cmd === "rm") { rmSync(guest(args.at(-1)), { recursive: true, force: true }); return 0; }
  if (cmd === "df") {
    process.stdout.write("Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/vdb 20971520 1048576 15728640 6% /layerfs/store\n");
    return 0;
  }
  if (cmd !== "sh" || flag !== "-c") { console.error(`fake guest does not know ${args.join(" ")}`); return 1; }
  const path = rest[0];
  const put = (target, bytes) => { mkdirSync(dirname(guest(target)), { recursive: true }); writeFileSync(guest(target), bytes); };
  if (/test -f "\$1" && cat "\$1"/.test(script)) {
    if (!existsSync(guest(path))) return 1;
    process.stdout.write(readFileSync(guest(path)));
  } else if (/cat > "\$1"/.test(script)) put(path, stdin());
  else if (/umask 022/.test(script)) mkdirSync(guest(path), { recursive: true, mode: 0o755 });
  else if (/max_user_namespaces/.test(script)) put("/proc/sys/user/max_user_namespaces", "0\n");
  else if (/mount --bind/.test(script)) mkdirSync(guest("/layerfs/store"), { recursive: true });
  else if (/cp -Rp "\$1\/\." "\$2"/.test(script)) {
    // a Lima VM's /layerfs/home is the host home it mounts (virtiofs)
    const home = readJson(vmFile(vm))?.config.mounts?.find((mount) => mount.mountPoint === "/layerfs/home")?.location;
    const from = home && path.startsWith("/layerfs/home/") ? join(home, path.slice("/layerfs/home/".length)) : guest(path);
    if (!existsSync(from)) { console.error(`cp: cannot stat '${path}/.': No such file or directory`); return 1; }
    cpSync(from, guest(rest[1]), { recursive: true });
  }
  else if (/tar -C "\$1" -xf -/.test(script)) {
    mkdirSync(guest(path), { recursive: true });
    return new Promise((resolve) => {
      const tar = spawn("tar", ["-C", guest(path), "-xf", "-"], { stdio: ["pipe", "ignore", "inherit"] });
      process.stdin.pipe(tar.stdin);
      tar.on("close", (code) => resolve(code ?? 1));
    });
  } else if (/nohup node/.test(script)) launchWorker(vm, await workerPort());
  return 0;
}

// --- limactl ---

const vmFile = (name) => file("vms", `${name}.json`);
function limactl() {
  log(argv);
  const [command, ...args] = argv;
  if (command === "list") {
    const name = args.at(-1), vm = readJson(vmFile(name));
    if (!vm) fail(`level=warning msg="No instance matching ${name} found."\nlevel=fatal msg="unmatched instances"`);
    process.stdout.write(`${JSON.stringify({ name, status: vm.status, dir: join(process.env.LIMA_HOME, name), vmType: vm.config.vmType, config: vm.config })}\n`);
  } else if (command === "create") {
    const name = args[args.indexOf("--name") + 1];
    if (readJson(vmFile(name))) fail(`level=fatal msg="instance \\"${name}\\" already exists"`);
    const config = JSON.parse(stdin().toString());
    saveJson(vmFile(name), { status: "Stopped", config, template: config });
  } else if (command === "start" || command === "stop") {
    const name = args.at(-1), vm = readJson(vmFile(name)) ?? fail(`instance ${name} does not exist`);
    if (command === "stop") endWorker(name);
    vm.status = command === "start" ? "Running" : "Stopped";
    saveJson(vmFile(name), vm);
  } else if (command === "delete") {
    const name = args.at(-1);
    endWorker(name);
    rmSync(vmFile(name), { force: true });
    rmSync(join(state, "guest", name), { recursive: true, force: true });
  } else if (command === "disk") {
    const [sub, name] = args;
    if (sub === "list") {
      for (const entry of existsSync(file("disks")) ? readdirSync(file("disks")) : []) process.stdout.write(`${JSON.stringify(readJson(file("disks", entry)))}\n`);
    } else if (sub === "create") {
      if (existsSync(file("disks", `${name}.json`))) fail(`disk ${name} already exists`);
      saveJson(file("disks", `${name}.json`), { name, size: args[args.indexOf("--size") + 1], format: args[args.indexOf("--format") + 1] });
    } else if (sub === "delete") rmSync(file("disks", `${name}.json`), { force: true });
  } else fail(`fake limactl does not know ${command}`);
}

// --- the Firecracker host's marked scripts ---

const fcDir = (name) => file("fc", "vms", name);
const fcIp = (name) => `172.30.${readJson(join(fcDir(name), "vm.json")).index}.2`;

async function firecrackerScript(op, args) {
  const [, name] = args;
  if (op === "check") {
    if (process.env.FAKE_FC_NO_KVM) { console.error("there is no /dev/kvm (the Lima instance needs nestedVirtualization: true, an M3 or newer and macOS 15+)"); return 1; }
    return 0;
  }
  if (op === "disk") {
    const disk = file("fc", "disks", `${args[1]}.json`);
    if (existsSync(disk)) return 0;
    saveJson(disk, { name: args[1], size: args[2] });
    process.stdout.write("made\n");
    return 0;
  }
  if (op === "create") {
    if (existsSync(fcDir(name))) { console.error(`${fcDir(name)} already exists`); return 17; }
    const used = existsSync(file("fc", "vms")) ? readdirSync(file("fc", "vms")).map((vm) => readJson(join(fcDir(vm), "vm.json")).index) : [];
    let index = 1;
    while (used.includes(index)) index++;
    saveJson(join(fcDir(name), "vm.json"), { index, store: args[2], cpus: Number(args[3]), mem: Number(args[4]), vmm: "stopped" });
    writeFileSync(join(fcDir(name), "meta.json"), stdin());
    return 0;
  }
  if (op === "boot" || op === "stop") {
    const vm = readJson(join(fcDir(name), "vm.json"));
    if (!vm) return 0;
    if (op === "stop") endWorker(name);
    vm.vmm = op === "boot" ? "running" : "stopped";
    saveJson(join(fcDir(name), "vm.json"), vm);
    if (op === "stop" && args[2] === "delete") rmSync(fcDir(name), { recursive: true, force: true });
    return 0;
  }
  if (op === "inspect") {
    const vm = readJson(join(fcDir(name), "vm.json"));
    if (!vm) return 3;
    process.stdout.write(`${vm.vmm}\n${fcIp(name)}\n${readFileSync(join(fcDir(name), "meta.json"), "utf8")}`);
    return 0;
  }
  if (op === "guest") {
    const guestArgv = args.slice(2).map(unquote);
    const workerPort = async () => {
      const port = await freePort();
      writeFileSync(join(fcDir(name), "worker.port"), `${port}\n`);
      return port;
    };
    return guestCommand(name, guestArgv, workerPort);
  }
  console.error(`fake firecracker host does not know ${op}`);
  return 1;
}

/** The forward to a microVM: lazy, it reads the worker's port when a client connects. */
function forward(hostPort, ip) {
  const server = createServer((client) => {
    client.on("error", () => {});
    const vm = (existsSync(file("fc", "vms")) ? readdirSync(file("fc", "vms")) : []).find((name) => fcIp(name) === ip);
    const portFile = vm ? join(fcDir(vm), "worker.port") : null;
    let tries = 0;
    const dial = () => {
      const port = portFile && existsSync(portFile) ? Number(readFileSync(portFile, "utf8").trim()) : NaN;
      if (!Number.isFinite(port)) return tries++ < 20 ? setTimeout(dial, 50) : client.destroy();
      const upstream = connect(port, "127.0.0.1");
      upstream.on("error", () => client.destroy());
      client.pipe(upstream);
      upstream.pipe(client);
    };
    dial();
  });
  server.on("error", (error) => { console.error(`bind [127.0.0.1]:${hostPort}: ${error.code === "EADDRINUSE" ? "Address already in use" : error.message}`); process.exit(255); });
  server.listen(hostPort, "127.0.0.1");
}

async function ssh() {
  if (argv.includes("-N")) {
    log(argv);
    const spec = /^127\.0\.0\.1:(\d+):([\d.]+):(\d+)$/.exec(argv[argv.indexOf("-L") + 1] ?? "");
    if (!spec) fail("ssh: no forward to open", 255);
    return forward(Number(spec[1]), spec[2]);
  }
  const dash = argv.indexOf("--");
  const host = argv[dash - 1].replace(/^lima-/, "");
  const remote = argv.slice(dash + 1).map(unquote);
  log([...argv.slice(0, dash + 1), ...remote]);
  if (remote[0] !== "sudo" || remote[1] !== "-n" || remote[2] !== "--") fail(`fake ssh: not a root command: ${remote.join(" ")}`, 255);
  const args = remote.slice(3);
  const vm = readJson(vmFile(host));
  if (!vm || vm.status !== "Running") fail(`ssh: connect to lima-${host}: Connection refused`, 255);
  const marker = args[0] === "sh" && /^# layerfs-firecracker:(\w+)/.exec(args[2] ?? "")?.[1];
  if (marker) process.exit(await firecrackerScript(marker, args.slice(4)));
  if (args[0] === "sh" && args[2] === "rm -f \"$1\"") { rmSync(file("fc", "disks", `${args[4].split("/").pop().replace(/\.ext4$/, "")}.json`), { force: true }); return; }
  const port = vm.config.portForwards?.find((rule) => rule.guestPort)?.hostPort;
  process.exit(await guestCommand(host, args, async () => port));
}

if (tool === "limactl") limactl();
else if (tool === "ssh") ssh();
else fail(`fake lima: call me as limactl or ssh, not ${tool}`);

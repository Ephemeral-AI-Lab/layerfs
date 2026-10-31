// The `limactl` and `ssh` stand-in for @layerfs/runtime's vm:lima tests
// (#527 platform pass): no VM. Called as
// `fake-lima.cjs limactl …` or `fake-lima.cjs ssh …` (the test puts one shim of
// each name on PATH). It answers exactly the calls `platform/vm/lima.js` makes,
// and maps each guest's absolute paths
// under FAKE_LIMA_STATE/guest/<vm>, so guest reads and writes are real files:
//
//   limactl list --json NAME | create --name N --tty=false - | start --tty=false N
//   limactl stop N | delete --force N | disk list --json | disk create D … | disk delete D
//   ssh -F <dir>/ssh.config lima-N -- sudo -n -- ARGV…      a root command in instance N
//       (a Lima VM's /layerfs/home is the host folder its template mounts there)
//
// ARGV arrives shell-quoted, as the remote shell would see it, and is unquoted
// here. A started worker is the bundle's own worker/server.js with the stand-in
// binding, listening on the VM's forwarded host port. Every call is appended to
// calls.jsonl as { at, tool, argv } with ARGV unquoted.
const { appendFileSync, cpSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, rmSync, writeFileSync } = require("node:fs");
const { spawn } = require("node:child_process");
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

/** A guest command as the adapter sends it; answers its exit code. */
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

async function ssh() {
  const dash = argv.indexOf("--");
  const host = argv[dash - 1].replace(/^lima-/, "");
  const remote = argv.slice(dash + 1).map(unquote);
  log([...argv.slice(0, dash + 1), ...remote]);
  if (remote[0] !== "sudo" || remote[1] !== "-n" || remote[2] !== "--") fail(`fake ssh: not a root command: ${remote.join(" ")}`, 255);
  const args = remote.slice(3);
  const vm = readJson(vmFile(host));
  if (!vm || vm.status !== "Running") fail(`ssh: connect to lima-${host}: Connection refused`, 255);
  const port = vm.config.portForwards?.find((rule) => rule.guestPort)?.hostPort;
  process.exit(await guestCommand(host, args, async () => port));
}

if (tool === "limactl") limactl();
else if (tool === "ssh") ssh();
else fail(`fake lima: call me as limactl or ssh, not ${tool}`);

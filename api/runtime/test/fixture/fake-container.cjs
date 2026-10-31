// The `container` stand-in for @layerfs/runtime's Apple Container test: no VM.
// It answers the calls `platform/vm/apple-container.js` makes **in the shape the
// real CLI prints**, copied from `container` 1.6.1 (build aa8d76b) on macOS 27.2,
// 2026-10-31: `inspect --format json` is an array of
// `{ id, status: { state, startedDate, networks }, configuration: { image,
// labels, mounts, publishedPorts, creationDate } }`, mounts carry
// `type.volume.name`, and it needs `-i` on `exec` for a caller's stdin. What the
// stand-in cannot prove is the real CLI's behaviour under load; the real run is
// `api/runtime/test/verify/vm_real.mjs <bundle> apple-container`.
//
//   container inspect --format json NAME
//   container run --detach --name N … IMAGE sleep infinity
//   container start|stop NAME, container delete --force NAME
//   container volume inspect NAME | create -s SIZE NAME | delete NAME
//   container copy HOST NAME:DEST
//   container exec [-i] NAME sh -c SCRIPT sh ARGS…
// Records live in FAKE_CONTAINER_STATE/vms/<name>.json, volumes in
// FAKE_CONTAINER_STATE/volumes/<name>.json, guest paths under
// FAKE_CONTAINER_STATE/guest/<name>.
const { appendFileSync, cpSync, existsSync, mkdirSync, openSync, readFileSync, readdirSync, rmSync, writeFileSync } = require("node:fs");
const { spawn } = require("node:child_process");
const { dirname, join } = require("node:path");

const state = process.env.FAKE_CONTAINER_STATE;
if (!state) {
  console.error("fake container: FAKE_CONTAINER_STATE is not set");
  process.exit(2);
}
const recordFile = (name) => join(state, "vms", `${name}.json`);
const volumeFile = (name) => join(state, "volumes", `${name}.json`);
const guestIn = (name) => (path) => join(state, "guest", name, path.replace(/^\//, ""));
const argv = process.argv.slice(2);
appendFileSync(join(state, "calls.jsonl"), `${JSON.stringify({ at: Date.now(), argv })}\n`);

const fail = (message) => { console.error(`error: ${message}`); process.exit(1); };
const at = (flag) => (argv.includes(flag) ? argv[argv.indexOf(flag) + 1] : undefined);
const all = (flag) => argv.flatMap((value, index) => (value === flag ? [argv[index + 1]] : []));
const read = (name) => (existsSync(recordFile(name)) ? JSON.parse(readFileSync(recordFile(name), "utf8")) : null);
const save = (value) => { mkdirSync(dirname(recordFile(value.name)), { recursive: true }); writeFileSync(recordFile(value.name), JSON.stringify(value, null, 1)); };

function endWorker(vm) {
  if (!vm?.pid) return;
  try { process.kill(vm.pid); } catch { /* already gone */ }
  vm.pid = null;
  save(vm);
}

function launchWorker(vm) {
  endWorker(vm); // a start ends the worker an earlier start left, as the launch script does
  const guest = guestIn(vm.name);
  const env = Object.fromEntries(readFileSync(guest("/layerfs/state/worker.env"), "utf8").split("\n").filter(Boolean).map((line) => [line.slice(0, line.indexOf("=")), line.slice(line.indexOf("=") + 1)]));
  const child = spawn(process.execPath, [guest("/layerfs/bundle/api/runtime/src/worker/server.js")], {
    env: { ...process.env, ...env, LAYERFS_HOME: guest(env.LAYERFS_HOME), LAYERFS_BUNDLE: guest(env.LAYERFS_BUNDLE), LAYERFS_WORKER_LISTEN: `127.0.0.1:${vm.port}`, LAYERFS_NATIVE: process.env.FAKE_CONTAINER_NATIVE },
    detached: true, stdio: ["ignore", "ignore", openSync(guest("/layerfs/state/worker.log"), "a")],
  });
  child.unref();
  vm.pid = child.pid;
  save(vm);
  const pid = guest("/layerfs/state/worker.pid");
  mkdirSync(dirname(pid), { recursive: true });
  writeFileSync(pid, `${child.pid}\n`);
}

const command = argv[0];
if (command === "inspect") {
  const name = argv.at(-1);
  const vm = read(name) ?? fail(`container not found: ${name}`);
  process.stdout.write(JSON.stringify([{
    id: vm.name,
    status: {
      state: vm.state,
      startedDate: vm.started ?? vm.created,
      networks: [{ network: "default", ipv4Address: "192.168.64.2/24", ipv4Gateway: "192.168.64.1", variant: "reserved" }],
    },
    configuration: {
      creationDate: vm.created,
      image: { reference: vm.image, descriptor: { digest: `sha256:${"1".repeat(64)}` } },
      labels: vm.labels,
      mounts: [
        { source: vm.home, destination: "/layerfs/home", type: { virtiofs: {} } },
        { source: join(state, "volumes", `${vm.store}.img`), destination: "/layerfs/store", type: { volume: { name: vm.store, format: "ext4" } } },
      ],
      publishedPorts: [{ hostPort: vm.port, containerPort: 7070, hostAddress: "127.0.0.1", count: 1, proto: "tcp" }],
    },
  }]));
} else if (command === "run") {
  const name = at("--name");
  if (read(name)) fail(`container ${name} already exists`);
  const mounts = all("--mount").map((value) => Object.fromEntries(value.split(",").map((pair) => pair.split("="))));
  const store = mounts.find((entry) => entry.target === "/layerfs/store")?.source ?? "";
  const home = mounts.find((entry) => entry.target === "/layerfs/home")?.source ?? "";
  // H39v's finding: `container run` makes a named volume it was given before it
  // fails on a busy port, and the container record never appears
  if (process.env.FAKE_CONTAINER_FAIL_RUN === "port") {
    if (!existsSync(volumeFile(store))) {
      mkdirSync(join(state, "volumes"), { recursive: true });
      writeFileSync(volumeFile(store), JSON.stringify({ name: store, size: "20G", format: "ext4", creationDate: new Date().toISOString(), labels: {} }));
    }
    fail(`failed to publish 127.0.0.1:${String(at("--publish") ?? "").split(":")[1]}:7070: port is already allocated`);
  }
  if (!existsSync(volumeFile(store))) fail(`volume not found: ${store}`);
  const labels = Object.fromEntries(all("--label").map((pair) => pair.split("=")));
  save({
    name, state: "running", created: new Date().toISOString(), started: new Date().toISOString(), pid: null,
    labels, home, store, port: Number(String(at("--publish") ?? "").split(":")[1]),
    image: argv[argv.length - 3],
  });
} else if (command === "start") {
  const vm = read(argv[1]) ?? fail(`container not found: ${argv[1]}`);
  vm.state = "running";
  vm.started = new Date().toISOString();
  save(vm);
} else if (command === "stop") {
  const vm = read(argv[1]) ?? fail(`container not found: ${argv[1]}`);
  endWorker(vm);
  vm.state = "stopped";
  save(vm);
} else if (command === "delete") {
  const vm = read(argv.at(-1)) ?? fail(`container not found: ${argv.at(-1)}`);
  endWorker(vm);
  rmSync(recordFile(vm.name), { force: true });
  rmSync(join(state, "guest", vm.name), { recursive: true, force: true });
} else if (command === "volume") {
  const [action, ...rest] = argv.slice(1);
  const name = action === "create" ? rest.at(-1) : rest.at(-1);
  if (action === "inspect") {
    if (!existsSync(volumeFile(name))) fail(`volume not found: ${name}`);
    process.stdout.write(JSON.stringify([{ id: name, configuration: JSON.parse(readFileSync(volumeFile(name), "utf8")) }]));
  } else if (action === "create") {
    if (existsSync(volumeFile(name))) fail(`volume ${name} already exists`);
    mkdirSync(join(state, "volumes"), { recursive: true });
    const size = at("-s") ?? "549755813888";
    writeFileSync(volumeFile(name), JSON.stringify({ name, sizeInBytes: Number(String(size).replace(/[KMGTP]$/, "")), size, format: "ext4", creationDate: new Date().toISOString(), labels: {} }));
  } else if (action === "delete") {
    rmSync(volumeFile(name), { force: true });
  }
} else if (command === "copy") {
  const target = argv[2];
  const guest = guestIn(target.slice(0, target.indexOf(":")));
  const destination = guest(target.slice(target.indexOf(":") + 1));
  mkdirSync(destination, { recursive: true });
  for (const entry of readdirSync(argv[1], { withFileTypes: true })) cpSync(join(argv[1], entry.name), join(destination, entry.name), { recursive: entry.isDirectory() });
} else if (command === "exec") {
  // `exec [-i] NAME argv…`
  const interactive = argv.includes("-i");
  const rest = argv.slice(1).filter((value) => value !== "-i");
  const [name, ...command_] = rest;
  const vm = read(name) ?? fail(`container not found: ${name}`);
  const guest = guestIn(vm.name);
  const put = (path, text) => { mkdirSync(dirname(guest(path)), { recursive: true }); writeFileSync(guest(path), text); };
  const [tool, flag, script, ...args] = command_;
  if (tool === "rm") {
    rmSync(guest(script), { recursive: true, force: true });
  } else if (tool === "df") {
    const volume = JSON.parse(readFileSync(volumeFile(vm.store), "utf8"));
    const blocks = Math.floor(volume.sizeInBytes / 1024);
    process.stdout.write(`Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/vdc ${blocks} 1048576 ${blocks - 1048576} 1% /layerfs/store\n`);
  } else if (tool === "sh" && flag === "-c") {
    if (/test -f "\$1" && cat "\$1"/.test(script)) {
      if (!existsSync(guest(args[1]))) process.exit(1);
      process.stdout.write(readFileSync(guest(args[1])));
    } else if (/cat > "\$1"/.test(script)) {
      if (!interactive) fail("stdin was not kept open: the adapter must pass -i");
      put(args[1], readFileSync(0));
    } else if (/umask 022/.test(script)) {
      mkdirSync(guest(args[1]), { recursive: true, mode: 0o755 });
    } else if (/max_user_namespaces/.test(script)) {
      if (!/remount,rw \/proc\/sys/.test(script)) fail("the guest mounts /proc/sys read-only: remount it first");
      put("/proc/sys/user/max_user_namespaces", "0\n");
    } else if (/nohup node/.test(script)) {
      launchWorker(vm);
    }
  }
} else {
  fail(`fake container does not know ${command}`);
}

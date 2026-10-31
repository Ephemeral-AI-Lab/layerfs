// The `container` stand-in for @layerfs/runtime's Apple Container test: no VM.
// It answers the calls `platform/vm/apple-container.js` makes, in the shape
// that file documents as UNVERIFIED (no `container` CLI on the machine H37 was
// written on), so the test proves the LayerFS-side flow only — never that the
// real CLI's output matches. Guest absolute paths map under FAKE_CONTAINER_STATE/guest.
//
//   container inspect --format json NAME   [{ status, configuration: { mounts, publishedPorts, labels, image } }]
//   container run --detach --name N … IMAGE sleep infinity
//   container start|stop NAME, container delete --force NAME, container volume delete NAME
//   container copy HOST NAME:DEST
//   container exec NAME sh -c SCRIPT sh ARGS…
const { appendFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } = require("node:fs");
const { spawn } = require("node:child_process");
const { dirname, join } = require("node:path");

const state = process.env.FAKE_CONTAINER_STATE;
if (!state) {
  console.error("fake container: FAKE_CONTAINER_STATE is not set");
  process.exit(2);
}
const guest = (path) => join(state, "guest", path.replace(/^\//, ""));
const recordFile = join(state, "vm.json");
const record = () => (existsSync(recordFile) ? JSON.parse(readFileSync(recordFile, "utf8")) : null);
const save = (value) => writeFileSync(recordFile, JSON.stringify(value, null, 1));
const argv = process.argv.slice(2);
appendFileSync(join(state, "calls.jsonl"), `${JSON.stringify({ at: Date.now(), argv })}\n`);

const fail = (message) => { console.error(`error: ${message}`); process.exit(1); };
const at = (flag) => (argv.includes(flag) ? argv[argv.indexOf(flag) + 1] : undefined);
const all = (flag) => argv.flatMap((value, index) => (value === flag ? [argv[index + 1]] : []));
const put = (path, text) => { mkdirSync(dirname(guest(path)), { recursive: true }); writeFileSync(guest(path), text); };
const append = (path) => { mkdirSync(dirname(guest(path)), { recursive: true }); return require("node:fs").openSync(guest(path), "a"); };

function endWorker(vm) {
  if (!vm?.pid) return;
  try { process.kill(vm.pid); } catch { /* already gone */ }
  vm.pid = null;
  save(vm);
}

function launchWorker(vm) {
  endWorker(vm); // a start ends the worker an earlier start left, as the launch script does
  const env = Object.fromEntries(readFileSync(guest("/layerfs/state/worker.env"), "utf8").split("\n").filter(Boolean).map((line) => [line.slice(0, line.indexOf("=")), line.slice(line.indexOf("=") + 1)]));
  const child = spawn(process.execPath, [guest("/layerfs/bundle/api/runtime/src/worker/server.js")], {
    env: { ...process.env, ...env, LAYERFS_HOME: guest(env.LAYERFS_HOME), LAYERFS_BUNDLE: guest(env.LAYERFS_BUNDLE), LAYERFS_WORKER_LISTEN: `127.0.0.1:${vm.port}`, LAYERFS_NATIVE: process.env.FAKE_CONTAINER_NATIVE },
    detached: true, stdio: ["ignore", "ignore", append("/layerfs/state/worker.log")],
  });
  child.unref();
  vm.pid = child.pid;
  save(vm);
  put("/layerfs/state/worker.pid", `${child.pid}\n`);
}

const command = argv[0];
if (command === "inspect") {
  const vm = record() ?? fail(`container not found: ${argv.at(-1)}`);
  process.stdout.write(JSON.stringify([{
    id: vm.id, status: vm.state === "running" ? "running" : "stopped", createdAt: vm.created,
    configuration: {
      labels: vm.labels,
      image: { reference: vm.image, descriptor: { digest: `sha256:${"1".repeat(64)}` } },
      mounts: [
        { source: vm.home, destination: "/layerfs/home" },
        { source: vm.store, destination: "/layerfs/store" },
      ],
      publishedPorts: [{ hostPort: vm.port, containerPort: 7070, hostAddress: "127.0.0.1" }],
    },
  }]));
} else if (command === "run") {
  if (record()) fail(`container ${argv.at(-1)} already exists`);
  const mounts = all("--mount").map((value) => Object.fromEntries(value.split(",").map((pair) => pair.split("="))));
  const store = mounts.find((entry) => entry.target === "/layerfs/store")?.source ?? "";
  const home = mounts.find((entry) => entry.target === "/layerfs/home")?.source ?? "";
  const labels = Object.fromEntries(all("--label").map((pair) => pair.split("=")));
  save({
    id: `c-${Date.now()}`, name: at("--name"), state: "running", created: new Date().toISOString(), pid: null,
    labels, home, store, port: Number(String(at("--publish") ?? "").split(":")[1]),
    image: argv[argv.length - 3],
  });
} else if (command === "start") {
  const vm = record() ?? fail(`container not found: ${argv[1]}`);
  vm.state = "running";
  save(vm);
} else if (command === "stop") {
  const vm = record() ?? fail(`container not found: ${argv[1]}`);
  endWorker(vm);
  vm.state = "stopped";
  save(vm);
} else if (command === "delete") {
  endWorker(record() ?? fail(`container not found: ${argv.at(-1)}`));
  rmSync(recordFile, { force: true });
} else if (command === "volume") {
  // `volume delete NAME`: nothing is stored for volumes here
} else if (command === "copy") {
  const target = argv[2];
  const destination = guest(target.slice(target.indexOf(":") + 1));
  mkdirSync(destination, { recursive: true });
  for (const entry of readdirSync(argv[1], { withFileTypes: true })) cpSync(join(argv[1], entry.name), join(destination, entry.name), { recursive: entry.isDirectory() });
} else if (command === "exec") {
  const vm = record() ?? fail(`container not found: ${argv[1]}`);
  const [tool, flag, script, ...args] = argv.slice(2);
  if (tool === "rm") {
    rmSync(guest(argv[3]), { recursive: true, force: true });
  } else if (tool === "df") {
    process.stdout.write("Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/vdb 20971520 1048576 15728640 6% /layerfs/store\n");
  } else if (tool === "sh" && flag === "-c") {
    if (/test -f "\$1" && cat "\$1"/.test(script)) {
      if (!existsSync(guest(args[1]))) process.exit(1);
      process.stdout.write(readFileSync(guest(args[1])));
    } else if (/cat > "\$1"/.test(script)) {
      put(args[1], readFileSync(0));
    } else if (/umask 077/.test(script)) {
      mkdirSync(guest(args[1]), { recursive: true, mode: 0o700 });
    } else if (/max_user_namespaces/.test(script)) {
      put("/proc/sys/user/max_user_namespaces", "0\n");
    } else if (/nohup node/.test(script)) {
      launchWorker(vm);
    }
  }
} else {
  fail(`fake container does not know ${command}`);
}

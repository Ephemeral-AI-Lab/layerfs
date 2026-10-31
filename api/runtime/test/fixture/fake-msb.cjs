// The `msb` stand-in for @layerfs/runtime's VM tests (#515 §6): no microVM.
// It answers the calls `platform/vm/microsandbox.js` makes, keeps one VM
// record, and maps the guest's absolute paths under FAKE_MSB_STATE/guest, so
// the adapter's guest file reads and writes are real file operations.
//
//   msb inspect --format json NAME   the record, else "sandbox not found"
//   msb create --name N … IMAGE      record it as Running with the -p host port
//   msb start|stop|restart NAME      flip the record's state
//   msb remove --force NAME          forget it (and end its worker)
//   msb volume remove NAME           log it; nothing else happens
//   msb copy HOST NAME:DEST          copy HOST's contents into DEST
//   msb exec NAME --no-tty -- sh -c SCRIPT sh ARGS…
//       the scripts the adapter sends: a guest read, a guest write from stdin,
//       the user-namespace block, `rm -rf PATH`, and the worker's launch (which
//       starts the real worker/server.js of the bundle, with the stand-in
//       binding, listening on the VM's published port).
// Every call is appended to FAKE_MSB_STATE/calls.jsonl as { at, argv }.
const { appendFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } = require("node:fs");
const { spawn } = require("node:child_process");
const { dirname, join } = require("node:path");

const state = process.env.FAKE_MSB_STATE;
if (!state) {
  console.error("fake msb: FAKE_MSB_STATE is not set");
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
const put = (path, text) => { mkdirSync(dirname(guest(path)), { recursive: true }); writeFileSync(guest(path), text); };
const append = (path) => { mkdirSync(dirname(guest(path)), { recursive: true }); return require("node:fs").openSync(guest(path), "a"); };

function endWorker(vm) {
  if (!vm?.pid) return;
  try { process.kill(vm.pid); } catch { /* already gone */ }
  vm.pid = null;
  save(vm);
}

/** The worker the adapter launched: the bundle's own server.js, on the VM's published port. */
function launchWorker(vm) {
  endWorker(vm); // a start ends the worker an earlier start left, as the launch script does
  const env = Object.fromEntries(readFileSync(guest("/layerfs/state/worker.env"), "utf8").split("\n").filter(Boolean).map((line) => [line.slice(0, line.indexOf("=")), line.slice(line.indexOf("=") + 1)]));
  const script = guest("/layerfs/bundle/api/runtime/src/worker/server.js");
  if (!existsSync(script)) fail("the bundle holds no worker/server.js");
  const child = spawn(process.execPath, [script], {
    env: { ...process.env, ...env, LAYERFS_HOME: guest(env.LAYERFS_HOME), LAYERFS_BUNDLE: guest(env.LAYERFS_BUNDLE), LAYERFS_WORKER_LISTEN: `127.0.0.1:${vm.port}`, LAYERFS_NATIVE: process.env.FAKE_MSB_NATIVE },
    detached: true, stdio: ["ignore", "ignore", append("/layerfs/state/worker.log")],
  });
  child.unref();
  vm.pid = child.pid;
  save(vm);
  put("/layerfs/state/worker.pid", `${child.pid}\n`);
}

const command = argv[0];
if (command === "inspect") {
  const vm = record() ?? fail(`sandbox not found: ${argv.at(-1)}`);
  process.stdout.write(JSON.stringify({
    name: vm.name,
    status: vm.state === "running" ? "Running" : "Stopped",
    created_at: vm.created,
    active_config: {
      image: { Oci: { reference: vm.image, manifest_digest: `sha256:${"0".repeat(64)}` } },
      labels: vm.labels,
      mounts: [
        { guest: "/layerfs/home", host: vm.home, type: "Bind", options: { readonly: false } },
        { guest: "/layerfs/store", name: vm.store, type: "Named", options: { readonly: false } },
      ],
      network: { ports: [{ guest_port: 7070, host_port: vm.port, host_bind: "127.0.0.1", protocol: "tcp" }] },
    },
  }));
} else if (command === "create") {
  if (record()) fail(`sandbox already exists: sandbox '${argv.at(-1)}' already exists; remove it, start the stopped sandbox, or recreate with .replace()`);
  const labels = Object.fromEntries(argv.flatMap((value, index) => (value === "--label" ? [argv[index + 1].split("=")] : [])));
  const published = String(at("-p") ?? "").split(":");
  save({
    name: at("--name"), state: "running", created: new Date().toISOString(), image: argv.at(-1), pid: null,
    labels, home: String(at("-v") ?? "").split(":")[0], store: String(at("--mount-named") ?? "").split(":")[0],
    port: Number(published[1]),
  });
} else if (command === "start" || command === "restart") {
  const vm = record() ?? fail(`sandbox not found: ${argv[1]}`);
  vm.state = "running";
  save(vm);
} else if (command === "stop") {
  const vm = record() ?? fail(`sandbox not found: ${argv[1]}`);
  endWorker(vm);
  vm.state = "stopped";
  save(vm);
} else if (command === "remove") {
  endWorker(record() ?? fail(`sandbox not found: ${argv.at(-1)}`));
  rmSync(recordFile, { force: true });
} else if (command === "volume") {
  // `volume remove NAME`: nothing is stored for volumes here
} else if (command === "copy") {
  const target = argv[2];
  const destination = guest(target.slice(target.indexOf(":") + 1));
  mkdirSync(destination, { recursive: true });
  for (const entry of readdirSync(argv[1], { withFileTypes: true })) {
    const from = join(argv[1], entry.name), to = join(destination, entry.name);
    if (entry.isDirectory()) cpSync(from, to, { recursive: true }); else cpSync(from, to);
  }
} else if (command === "exec") {
  const vm = record() ?? fail(`sandbox not found: ${argv[1]}`);
  const rest = argv.slice(argv.indexOf("--") + 1);
  const [tool, flag, script, ...args] = rest;
  if (tool === "rm") {
    rmSync(guest(rest[2]), { recursive: true, force: true });
  } else if (tool === "df") {
    process.stdout.write("Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/vdc 20971520 1048576 15728640 6% /layerfs/store\n");
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
  fail(`fake msb does not know ${command}`);
}

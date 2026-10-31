// Firecracker (#527 platform pass, the fifth VM of #516): one Firecracker
// microVM per project, run inside a long-lived Lima `vz` instance with nested
// virtualization (the "Firecracker host": macOS has no KVM, so Firecracker
// cannot run on the Mac itself; nested KVM needs an M3 or newer and macOS 15+).
// Per #516 this placement is pass/fail only: its timings describe the nesting.
//
// The host is the user's, like an SSH host: it is made once with Lima and named
// by `LAYERFS_FIRECRACKER_LIMA` (default `layerfs-firecracker`). This adapter
// makes no host; it proves what it needs by fact before the first VM — the
// host runs, `/dev/kvm` exists in it, and its folder `LAYERFS_FIRECRACKER_DIR`
// (default `/var/lib/layerfs-firecracker`) holds `bin/firecracker`, the guest
// kernel `vmlinux`, the base root filesystem `rootfs.ext4` (an ext4 image with
// node 22 on root's PATH and an sshd that accepts the key) and that key,
// `id_ed25519`. The five adapter steps (§6):
//   ① the VM is `<dir>/vms/<name>`: its own copy of the root filesystem, a
//      Firecracker config file (guest memory on 2 MiB huge pages when the host
//      has enough free: under nesting each first touch of a 4 KiB page is a
//      fault through both hypervisors, #516), a tap device `lfsfc<n>` with the /30 network
//      `172.30.<n>.0` (host .1, guest .2, set by the kernel's `ip=` argument),
//      and the Store disk `<dir>/disks/<store>.ext4`, a sparse file made ext4
//      by `mkfs.ext4`, attached as the second drive and mounted at the Store
//      directory after boot; `firecracker --config-file` runs detached;
//   ② the bundle is copied in as one `tar` stream: Lima's SSH into the host,
//      then the host's SSH into the microVM, unpacked as root;
//   ③ the worker runs as root with its secret in a 0600 file written from
//      stdin, as on every provider;
//   ④ the private stream is `ssh -L 127.0.0.1:<host>:172.30.<n>.2:7070` to
//      the Lima host, from this machine, with `ExitOnForwardFailure`;
//   ⑤ stop ends the forward, syncs and resets the guest (Firecracker exits on
//      a guest reset) and removes its tap; delete removes the VM's folder and
//      keeps the Store disk unless discarded.
// The microVM's SSH is a point-to-point tap inside the host, its address is
// reused by the next VM of that number, and each VM's root filesystem is a copy
// of one image: its host key is not checked (`UserKnownHostsFile=/dev/null`).
// A VM is "running" when both its Firecracker process and this machine's
// forward run; a VM whose forward ended reads as stopped, and `start` opens the
// forward again without rebooting it. One call per step, one attempt, no retry;
// the boot's wait for the guest's sshd is a readiness wait.
import { spawn } from "node:child_process";
import { closeSync, existsSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { readFile, writeFile } from "./cli.js";
import lima, { instanceDir, sshTo, stream, UNPACK } from "./lima.js";
import { isAlive, shellEscape } from "./ssh.js";

const BOOT_MS = 120_000, CALL_MS = 60_000, FORWARD_READY_MS = 600, END_MS = 3_000;
const WORKER_PORT = 7070;

/** The Lima instance that runs Firecracker, and its folder of binaries, images and VMs. */
export const host = () => process.env.LAYERFS_FIRECRACKER_LIMA || "layerfs-firecracker";
export const hostDir = () => process.env.LAYERFS_FIRECRACKER_DIR || "/var/lib/layerfs-firecracker";
const vmDir = (name) => `${hostDir()}/vms/${name}`;
const diskFile = (store) => `${hostDir()}/disks/${store}.ext4`;
/** This machine's forward records, one per VM. */
export const stateDir = () => process.env.LAYERFS_FIRECRACKER_STATE_DIR || join(homedir(), ".layerfs", "firecracker");

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const mib = (size) => Number(size.slice(0, -1)) * (size.endsWith("G") ? 1024 : 1);

/** One root command in the Firecracker host. */
const onHost = (argv, options = {}) => lima.exec(host(), argv, { timeout: CALL_MS, ...options });

/** One script in the host; a failure is an error naming the step. */
async function script(what, body, args, options = {}) {
  const result = await onHost(["sh", "-c", body, "sh", ...args.map(String)], options);
  if (result.code !== 0) throw new Error(`${what} failed in the Firecracker host ${host()} (exit ${result.code}): ${(result.stderr || result.stdout).trim()}`);
  return result.stdout;
}

const SSH_GUEST = "-o BatchMode=yes -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=5";

const CHECK = `# layerfs-firecracker:check
test -c /dev/kvm || { echo "there is no /dev/kvm (the Lima instance needs nestedVirtualization: true, an M3 or newer and macOS 15+)" >&2; exit 1; }
for f in bin/firecracker vmlinux rootfs.ext4 id_ed25519; do test -f "$1/$f" || { echo "$1/$f does not exist" >&2; exit 1; }; done
command -v mkfs.ext4 >/dev/null && command -v ip >/dev/null && command -v flock >/dev/null || { echo "the host needs mkfs.ext4, ip and flock" >&2; exit 1; }`;

const DISK = `# layerfs-firecracker:disk
f="$1/disks/$2.ext4"
test -f "$f" && exit 0
mkdir -p "$1/disks" && truncate -s "$3" "$f.part" && mkfs.ext4 -q -F "$f.part" && mv "$f.part" "$f" && echo made`;

const CREATE = `# layerfs-firecracker:create
set -eu
root=$1 name=$2 store=$3 cpus=$4 mem=$5
d="$root/vms/$name"
mkdir -p "$root/vms"
exec 9>"$root/vms/.lock"
flock 9
test ! -e "$d" || { echo "$d already exists" >&2; exit 17; }
test -f "$store" || { echo "the Store disk $store does not exist" >&2; exit 1; }
n=1
while grep -qx "INDEX=$n" "$root"/vms/*/vm.env 2>/dev/null; do n=$((n+1)); done
[ "$n" -le 250 ] || { echo "every network number is taken" >&2; exit 1; }
mkdir "$d"
cat > "$d/meta.json"
printf 'INDEX=%s\\nTAP=lfsfc%s\\nHOST_IP=172.30.%s.1\\nGUEST_IP=172.30.%s.2\\n' "$n" "$n" "$n" "$n" > "$d/vm.env"
cp --sparse=always "$root/rootfs.ext4" "$d/rootfs.ext4"
mac=$(printf '06:00:AC:1E:%02X:02' "$n")
# 2 MiB huge pages when the host has them free for the whole guest: nested first-touch faults are slow
huge='"None"'
[ "$(( $(awk '/^HugePages_Free/ {print $2}' /proc/meminfo) * 2 ))" -ge "$mem" ] && huge='"2M"'
cat > "$d/config.json" <<EOF
{"boot-source":{"kernel_image_path":"$root/vmlinux","boot_args":"console=ttyS0 reboot=k panic=1 pci=off ip=172.30.$n.2::172.30.$n.1:255.255.255.252::eth0:off"},
 "drives":[{"drive_id":"rootfs","path_on_host":"$d/rootfs.ext4","is_root_device":true,"is_read_only":false},
  {"drive_id":"store","path_on_host":"$store","is_root_device":false,"is_read_only":false}],
 "machine-config":{"vcpu_count":$cpus,"mem_size_mib":$mem,"huge_pages":$huge},
 "network-interfaces":[{"iface_id":"eth0","guest_mac":"$mac","host_dev_name":"lfsfc$n"}]}
EOF`;

const BOOT = `# layerfs-firecracker:boot
set -eu
root=$1 d="$1/vms/$2"
. "$d/vm.env"
if [ -f "$d/firecracker.pid" ] && kill -0 "$(cat "$d/firecracker.pid")" 2>/dev/null; then exit 0; fi
ip link show "$TAP" >/dev/null 2>&1 || ip tuntap add dev "$TAP" mode tap
ip addr replace "$HOST_IP/30" dev "$TAP"
ip link set "$TAP" up
setsid nohup "$root/bin/firecracker" --no-api --config-file "$d/config.json" > "$d/firecracker.log" 2>&1 < /dev/null &
echo $! > "$d/firecracker.pid"
i=0
until ssh -i "$root/id_ed25519" ${SSH_GUEST} "root@$GUEST_IP" true 2>/dev/null; do
  kill -0 "$(cat "$d/firecracker.pid")" 2>/dev/null || { echo "firecracker ended: $(tail -n 5 "$d/firecracker.log")" >&2; exit 1; }
  i=$((i+1)); [ "$i" -lt 120 ] || { echo "the microVM did not answer ssh within 60 s" >&2; exit 1; }
  sleep 0.5
done
ssh -i "$root/id_ed25519" ${SSH_GUEST} "root@$GUEST_IP" 'mkdir -p /layerfs/store && { mountpoint -q /layerfs/store || mount -t ext4 /dev/vdb /layerfs/store; } && test "$(findmnt -n -o FSTYPE /layerfs/store)" = ext4'`;

const INSPECT = `# layerfs-firecracker:inspect
d="$1/vms/$2"
test -f "$d/meta.json" || exit 3
. "$d/vm.env"
pid=$(cat "$d/firecracker.pid" 2>/dev/null || true)
if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then echo running; else echo stopped; fi
echo "$GUEST_IP"
cat "$d/meta.json"`;

const STOP = `# layerfs-firecracker:stop
root=$1 d="$1/vms/$2"
test -f "$d/vm.env" || exit 0
. "$d/vm.env"
pid=$(cat "$d/firecracker.pid" 2>/dev/null || true)
if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
  timeout 10 ssh -i "$root/id_ed25519" ${SSH_GUEST} "root@$GUEST_IP" 'sync; reboot -f' >/dev/null 2>&1 || true
  i=0; while kill -0 "$pid" 2>/dev/null && [ "$i" -lt 50 ]; do sleep 0.1; i=$((i+1)); done
  kill -0 "$pid" 2>/dev/null && kill -9 "$pid"
fi
rm -f "$d/firecracker.pid"
ip link del "$TAP" 2>/dev/null || true
if [ "\${3:-}" = delete ]; then rm -rf "$d"; fi`;

/** One command in the microVM, as root, through the host: `$@` stays quoted for the guest's shell. */
const GUEST = `# layerfs-firecracker:guest
d="$1/vms/$2"; key="$1/id_ed25519"; shift 2
. "$d/vm.env"
exec ssh -i "$key" ${SSH_GUEST} "root@$GUEST_IP" -- "$@"`;

const guestArgv = (name, argv) => ["sh", "-c", GUEST, "sh", hostDir(), name, ...argv.map(shellEscape)];

export const exec = (name, argv, { input = undefined, timeout = CALL_MS } = {}) => onHost(guestArgv(name, argv), { input, timeout });

// --- this machine's forward (step ④), its record and its log ---

function recordFile(name) {
  mkdirSync(stateDir(), { recursive: true, mode: 0o700 });
  return join(stateDir(), `${name}.json`);
}
const forwardLog = (name) => join(stateDir(), `${name}-forward.log`);
function forwardOf(name) {
  try { return JSON.parse(readFileSync(recordFile(name), "utf8")); } catch { return null; }
}

/**
 * `ssh -N -L 127.0.0.1:<port>:<guest>:7070` to the host, its stderr in a log
 * file so this process can exit while the VM runs. Its own connection, not
 * Lima's shared one: Lima's ssh.config keeps a persistent control master, to
 * which a multiplexed `-N -L` hands the forward and exits at once, leaving a
 * forward this adapter can neither see nor end. A forward that ends within the
 * readiness window is reported with its own reason.
 */
async function openForward(name, hostPort, guestIp) {
  const log = forwardLog(name);
  mkdirSync(stateDir(), { recursive: true, mode: 0o700 });
  const fd = openSync(log, "w", 0o600);
  const child = spawn("ssh", [
    "-F", join(instanceDir(host()), "ssh.config"), "-N", "-o", "ExitOnForwardFailure=yes", "-o", "ControlMaster=no", "-o", "ControlPath=none",
    "-L", `127.0.0.1:${hostPort}:${guestIp}:${WORKER_PORT}`, `lima-${host()}`,
  ], { stdio: ["ignore", "ignore", fd], detached: true });
  closeSync(fd);
  const ended = new Promise((resolve) => child.once("exit", (code, signal) => resolve({ code, signal })));
  const first = await Promise.race([ended.then((value) => ({ ended: true, ...value })), delay(FORWARD_READY_MS).then(() => ({ ended: false }))]);
  child.removeAllListeners("exit");
  child.unref();
  if (first.ended) {
    const said = existsSync(log) ? readFileSync(log, "utf8").trim().split("\n").slice(-3).join("; ") : "";
    throw new Error(`ssh -L 127.0.0.1:${hostPort} to ${host()} exited (${first.code ?? first.signal}): ${said || "no reason given"}`);
  }
  writeFileSync(recordFile(name), JSON.stringify({ name, pid: child.pid, hostPort, guestIp }), { mode: 0o600 });
}

/** Ends the forward, bounded: SIGTERM, then SIGKILL. */
async function closeForward(name) {
  const pid = forwardOf(name)?.pid;
  if (isAlive(pid)) {
    try { process.kill(pid, "SIGTERM"); } catch { /* already gone */ }
    for (let waited = 0; waited < END_MS && isAlive(pid); waited += 50) await delay(50);
    if (isAlive(pid)) try { process.kill(pid, "SIGKILL"); } catch { /* already gone */ }
  }
  rmSync(recordFile(name), { force: true });
}

// --- the provider ---

/** The VM's record, or null when the host holds no VM of that name. */
export async function inspect(name) {
  const result = await onHost(["sh", "-c", INSPECT, "sh", hostDir(), name]);
  if (result.code === 3) return null;
  if (result.code !== 0) await hostRuns();
  if (result.code !== 0) throw new Error(`could not read the VM ${name} in the Firecracker host ${host()}: ${result.stderr.trim()}`);
  const [vmm, ip, ...rest] = result.stdout.split("\n");
  const meta = JSON.parse(rest.join("\n"));
  const running = vmm === "running" && isAlive(forwardOf(name)?.pid);
  return {
    state: running ? "running" : "stopped",
    vmm,
    ip,
    image: `${hostDir()}/rootfs.ext4`,
    digest: null,
    labels: meta.labels ?? {},
    ports: [{ guest: WORKER_PORT, host: meta.hostPort, bind: "127.0.0.1" }],
    home: meta.home ?? null,
    store: meta.store ?? null,
    created: meta.created ?? null,
  };
}

/** The host exists and runs; otherwise the error says which and how. */
async function hostRuns() {
  const found = await lima.inspect(host());
  if (!found) throw new Error(`there is no Lima instance ${host()} to run Firecracker in; make one with vmType: vz and nestedVirtualization: true, or name yours in LAYERFS_FIRECRACKER_LIMA`);
  if (found.state !== "running") throw new Error(`the Firecracker host ${host()} is ${found.state}; start it with limactl start ${host()}`);
}

/** The host runs and holds what a VM needs; each missing thing is named. */
async function checkHost() {
  await hostRuns();
  await script("the host check", CHECK, [hostDir()]);
}

/** Store disks this process made: a create that fails removes the one it made. */
const newlyCreatedStores = new Set();

export async function ensureStore(name, size) {
  await checkHost();
  if ((await script("making the Store disk", DISK, [hostDir(), name, size], { timeout: BOOT_MS })).trim() === "made") newlyCreatedStores.add(name);
}

export async function create(config) {
  const made = newlyCreatedStores.delete(config.storeName);
  const meta = { name: config.name, home: config.home, store: config.storeName, hostPort: config.hostPort, created: new Date().toISOString(), labels: { "layerfs.role": "vm", "layerfs.home": config.home } };
  try {
    await script("creating the VM", CREATE, [hostDir(), config.name, diskFile(config.storeName), config.cpus, mib(config.memory)], { input: `${JSON.stringify(meta)}\n` });
  } catch (error) {
    if (made) await removeStore(config.storeName).catch(() => {});
    throw error;
  }
  // a boot that fails keeps the VM's folder for inspection; the caller reports it
  await script("booting the VM", BOOT, [hostDir(), config.name], { timeout: BOOT_MS });
  const booted = await inspect(config.name);
  await openForward(config.name, config.hostPort, booted.ip);
  return inspect(config.name);
}

/** Boots a stopped VM, or only re-opens the forward of one that still runs. */
export async function start(name) {
  const found = await inspect(name);
  if (!found) throw new Error(`the Firecracker host holds no VM ${name}`);
  if (found.state === "running") return found;
  if (found.vmm !== "running") await script("booting the VM", BOOT, [hostDir(), name], { timeout: BOOT_MS });
  await closeForward(name);
  await openForward(name, found.ports[0].host, found.ip);
  return inspect(name);
}

export async function stop(name) {
  await closeForward(name);
  await script("stopping the VM", STOP, [hostDir(), name]);
}

export const restart = async (name) => { await stop(name); return start(name); };

/** The VM's folder and its forward; the Store disk is a separate file and stays. */
export async function remove(name) {
  await closeForward(name);
  rmSync(forwardLog(name), { force: true });
  await script("deleting the VM", STOP, [hostDir(), name, "delete"]);
}

export const removeStore = (storeName) => script("deleting the Store disk", "rm -f \"$1\"", [diskFile(storeName)]);

/** V4: the microVM-wide user-namespace block; the caller reads it back. */
export const blockUserNamespaces = (name) => exec(name, ["sh", "-c", "echo 0 > /proc/sys/user/max_user_namespaces"]);

export async function storeFree(name) {
  const { code, stdout } = await exec(name, ["df", "-kP", "/layerfs/store"]);
  if (code !== 0) return null;
  const kb = Number(stdout.trim().split("\n").pop().trim().split(/\s+/)[3]);
  return Number.isFinite(kb) ? kb * 1024 : null;
}

/** Step ②: one tar stream through both SSH hops, unpacked as root in the microVM. */
export const copy = (name, source, destination) =>
  stream("ssh", [...sshTo(host()), "--", ...["sudo", "-n", "--", ...guestArgv(name, [...UNPACK, destination])].map(shellEscape)], source, `${name}:${destination}`);

export const read = (name, path) => readFile("ssh", name, path, exec);
export const write = (name, path, text) => writeFile("ssh", name, path, text, exec);

export default {
  name: "firecracker",
  defaultImage: null,
  inspect, ensureStore, create, start, stop, restart, remove, removeStore, blockUserNamespaces, storeFree, read, write, copy, exec,
};

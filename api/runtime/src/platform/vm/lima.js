// Lima (`limactl`, `vmType: vz`, #527 platform pass, the fourth VM of #516):
// one Lima instance per project, an Ubuntu cloud image booted by Apple's
// Virtualization.framework. The five adapter steps (§6) on this provider:
//   ① the VM is one instance made from a template this file writes (stdin to
//      `limactl create`), and the Store disk is a Lima disk (`limactl disk
//      create --format raw`, the format vz needs) that Lima formats ext4 on
//      first attach and mounts at `/mnt/lima-<disk>`; this adapter bind-mounts
//      it at the Store directory after every boot and checks it is ext4.
//      Lima finds an already formatted disk by its ext4 label `lima-<disk>`,
//      and an ext4 label holds 16 bytes: a longer disk name is cut, never
//      found, and **formatted again at every boot** (found on Lima 2.2.1,
//      #527). So the Lima disk is named `lfs` + 8 hex of the Store volume's
//      name (16 bytes with `lima-`), and the Store volume's own name is kept
//      in the instance as its `LAYERFS_STORE` param;
//   ② the bundle is copied onto the VM's own disk through the home bind mount
//      (virtiofs): cloned into a private `<home>/.vm-copy-*` staging folder,
//      copied by the guest with `cp -Rp`, the staging folder removed whatever
//      the outcome (the route Apple Container takes, #527: a 2.7 GB bundle in
//      seconds rather than a stream through SSH);
//   ③ the worker runs as root (`sudo -n`, the Lima user's passwordless sudo)
//      with its secret in a 0600 file written from stdin, never in argv;
//   ④ the private stream is the instance's `portForwards` rule
//      `127.0.0.1:<host> -> guest 7070`; every other guest port is ignored, so
//      Lima forwards nothing else;
//   ⑤ `limactl stop` keeps the instance and its disk; `limactl delete --force`
//      removes the instance and leaves the disk, which only `limactl disk
//      delete` (the runtime's `discardStore`) removes.
// The guest has no Node.js of its own: a system provision script installs the
// pinned Node.js 22 tarball from nodejs.org, checked against that release's
// SHASUMS256.txt, once, on first boot. `settings.vm.image` names the Lima
// image when it is an https URL or an absolute file path; the shared default
// (an OCI reference for the microVM providers) does not describe a Lima disk,
// so the Ubuntu 24.04 cloud image is used instead.
// Commands go through ssh rather than `limactl shell` so the argv quoting is
// this file's (ssh.js's `shellEscape`), not the CLI's. One call per step, one
// attempt, no retry.
import { createHash } from "node:crypto";
import { constants, cpSync, mkdtempSync, rmSync } from "node:fs";
import { basename, isAbsolute, relative, resolve, sep } from "node:path";
import { homedir } from "node:os";
import { join } from "node:path";
import { answer, capture, readFile, writeFile } from "./cli.js";
import { shellEscape } from "./ssh.js";

const CLI = "limactl";
/** First boot converts the image and installs Node.js: Lima's own wait is longer than a call's. */
const BOOT_MS = 300_000, CALL_MS = 30_000, EXEC_MS = 60_000, COPY_MS = 120_000;
const NODE = "v22.23.4";
const UBUNTU = "https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg";

const call = (args, options = {}) => answer(CLI, args, { timeout: CALL_MS, ...options });

/** The Lima disk of a Store volume: a name whose `lima-` ext4 label fits 16 bytes (see the header). */
export const diskName = (storeName) => `lfs${createHash("sha256").update(storeName).digest("hex").slice(0, 8)}`;

/** The instance's folder: Lima keeps it, and its ssh.config, under LIMA_HOME (default ~/.lima). */
export const instanceDir = (name) => join(process.env.LIMA_HOME || join(homedir(), ".lima"), name);

/** The ssh arguments that reach an instance as its Lima user, from Lima's own ssh.config. */
export const sshTo = (name) => ["-F", join(instanceDir(name), "ssh.config"), `lima-${name}`];

/** One command in the instance as root; stdin is passed through when given. */
export const exec = (name, argv, { input = undefined, timeout = EXEC_MS } = {}) =>
  capture("ssh", [...sshTo(name), "--", ...["sudo", "-n", "--", ...argv].map(shellEscape)], { input, timeout });

/** Lima's sizes are binary units: `2G` is `2GiB`. */
const limaSize = (size) => size.replace(/([MG])$/, "$1iB");

/** The instance as `limactl list --json` gives it, or null when Lima has none of that name. */
export async function inspect(name) {
  let text;
  try {
    text = await call(["list", "--json", name]);
  } catch (error) {
    if (/No instance matching/i.test(`${error.stderr ?? ""}${error.message ?? ""}`)) return null;
    throw error;
  }
  if (!text) return null;
  const record = JSON.parse(text.split("\n")[0]);
  if (record.name !== name || typeof record.status !== "string") throw new Error(`limactl list answered a record this adapter does not know for ${name}`);
  const config = record.config ?? {};
  const forwards = (config.portForwards ?? []).filter((rule) => !rule.ignore && rule.guestPort);
  return {
    state: record.status.toLowerCase(),
    vmType: record.vmType ?? config.vmType ?? null,
    image: config.images?.[0]?.location ?? null,
    digest: null,
    labels: config.param?.LAYERFS_HOME ? { "layerfs.home": config.param.LAYERFS_HOME } : {},
    ports: forwards.map((rule) => ({ guest: Number(rule.guestPort), host: Number(rule.hostPort), bind: rule.hostIP ?? null })),
    home: (config.mounts ?? []).find((mount) => mount.mountPoint === "/layerfs/home")?.location ?? null,
    store: config.param?.LAYERFS_STORE ?? null,
    disk: config.additionalDisks?.[0]?.name ?? null,
    created: null,
  };
}

/** The Lima disks by name, from `limactl disk list --json` (one object per line). */
async function disks() {
  const text = await call(["disk", "list", "--json"]);
  return text.split("\n").filter(Boolean).map((line) => JSON.parse(line));
}

/** Store disks this process made: a create that fails removes the one it made, and only that one. */
const newlyCreatedStores = new Set();

/** Step ①'s Store disk: a raw Lima disk of `size`, made once; an existing one keeps its size. */
export async function ensureStore(name, size) {
  if ((await disks()).some((disk) => disk.name === diskName(name))) return;
  await call(["disk", "create", diskName(name), "--size", limaSize(size), "--format", "raw"], { timeout: CALL_MS });
  newlyCreatedStores.add(name);
}

/** Installs the pinned Node.js once; the worker needs it on root's PATH. */
const NODE_SCRIPT = `#!/bin/sh
set -eu
command -v node >/dev/null 2>&1 && exit 0
case "$(uname -m)" in aarch64) a=arm64 ;; x86_64) a=x64 ;; *) echo "no Node.js build for $(uname -m)" >&2; exit 1 ;; esac
f=node-${NODE}-linux-$a.tar.xz
t=$(mktemp -d)
curl -fsSL -o "$t/$f" https://nodejs.org/dist/${NODE}/$f
curl -fsSL -o "$t/SHASUMS256.txt" https://nodejs.org/dist/${NODE}/SHASUMS256.txt
(cd "$t" && grep " $f\\$" SHASUMS256.txt | sha256sum -c -)
tar -xJf "$t/$f" -C /usr/local --strip-components=1
rm -rf "$t"
`;

/** The instance's template: vz, the home mount, the Store disk, one forwarded port. */
export function template(config) {
  const image = /^(https:\/\/|\/)/.test(config.image ?? "") ? [{ location: config.image }] : [
    { location: `${UBUNTU}-arm64.img`, arch: "aarch64" },
    { location: `${UBUNTU}-amd64.img`, arch: "x86_64" },
  ];
  // JSON is YAML: every value is quoted exactly, and Lima reads it as its template
  return `${JSON.stringify({
    vmType: "vz",
    cpus: config.cpus,
    memory: limaSize(config.memory),
    images: image,
    containerd: { system: false, user: false },
    mountType: "virtiofs",
    mounts: [{ location: config.home, mountPoint: "/layerfs/home", writable: true }],
    additionalDisks: [{ name: diskName(config.storeName), format: true, fsType: "ext4" }],
    // what the runtime checks a found instance against, kept in the instance itself
    param: { LAYERFS_STORE: config.storeName, LAYERFS_HOME: config.home },
    portForwards: [
      { guestPort: config.workerPort, hostIP: "127.0.0.1", hostPort: config.hostPort },
      { guestIP: "0.0.0.0", guestPortRange: [1, 65535], ignore: true },
    ],
    provision: [
      { mode: "system", script: NODE_SCRIPT },
      { mode: "system", script: "#!/bin/sh\nprintf 'store=%s\\nhome=%s\\n' \"$PARAM_LAYERFS_STORE\" \"$PARAM_LAYERFS_HOME\" > /etc/layerfs-vm\n" },
    ],
  }, null, 2)}\n`;
}

/**
 * The Store disk at the Store directory, by fact: Lima mounts it at
 * `/mnt/lima-<disk>`; it must be ext4 there, and it is bind-mounted at
 * `/layerfs/store` unless it already is (a reboot drops the bind mount).
 */
async function attach(name, store) {
  const { code, stderr } = await exec(name, ["sh", "-c",
    "test \"$(findmnt -n -o FSTYPE \"$1\")\" = ext4 || { echo \"$1 is not an ext4 mount\" >&2; exit 1; }; mkdir -p /layerfs/store && { mountpoint -q /layerfs/store || mount --bind \"$1\" /layerfs/store; }",
    "sh", `/mnt/lima-${store}`]);
  if (code !== 0) throw new Error(`could not attach the Store disk ${store} at /layerfs/store in ${name}: ${stderr.trim()}`);
}

/** Step ①: `limactl create` from the template on stdin, then the first boot. */
export async function create(config) {
  const made = newlyCreatedStores.has(config.storeName);
  newlyCreatedStores.delete(config.storeName);
  const created = await capture(CLI, ["create", "--name", config.name, "--tty=false", "-"], { input: template(config), timeout: CALL_MS });
  if (created.code !== 0) {
    // nothing was made under the name: the disk this create made is removed
    if (made) await removeStore(config.storeName).catch(() => {});
    throw new Error(`limactl create ${config.name} failed (exit ${created.code}): ${created.stderr.trim().split("\n").slice(-3).join("; ")}`);
  }
  // a boot that fails keeps the instance for inspection; the caller reports its state
  await call(["start", "--tty=false", config.name], { timeout: BOOT_MS });
  await attach(config.name, diskName(config.storeName));
  return inspect(config.name);
}

export async function start(name) {
  await call(["start", "--tty=false", name], { timeout: BOOT_MS });
  const found = await inspect(name);
  if (found?.state === "running" && found.disk) await attach(name, found.disk);
  return found;
}

export const stop = (name) => call(["stop", name], { timeout: CALL_MS * 2 });
export const restart = async (name) => { await stop(name); return start(name); };
/** The instance only: Lima unlocks its disk and keeps it. */
export const remove = (name) => call(["delete", "--force", name], { timeout: CALL_MS * 2 });
export const removeStore = (storeName) => call(["disk", "delete", diskName(storeName)]);

/** V4: the VM-wide user-namespace block; the caller reads it back. */
export const blockUserNamespaces = (name) => exec(name, ["sh", "-c", "echo 0 > /proc/sys/user/max_user_namespaces"]);

/** Free bytes of the Store disk, or null when `df` cannot answer. */
export async function storeFree(name) {
  const { code, stdout } = await exec(name, ["df", "-kP", "/layerfs/store"]);
  if (code !== 0) return null;
  const kb = Number(stdout.trim().split("\n").pop().trim().split(/\s+/)[3]);
  return Number.isFinite(kb) ? kb * 1024 : null;
}

/**
 * Step ②, and an Init source: a host folder into a guest folder, as a copy the
 * VM owns, read through the home the instance mounts (see the header). A folder
 * already inside the home is read in place. One attempt; nothing is retried.
 */
export async function copy(name, source, destination) {
  const found = await inspect(name);
  if (!found?.home) throw new Error(`the VM ${name} mounts no LayerFS home at /layerfs/home, so a folder cannot be staged for it`);
  const guestCopy = async (from) => {
    const done = await exec(name, ["sh", "-c", "mkdir -p \"$2\" && cp -Rp \"$1/.\" \"$2\"", "sh", from, destination], { timeout: COPY_MS });
    if (done.code !== 0) throw new Error(`could not copy ${source} into ${name}:${destination}: ${(done.stderr || done.stdout).trim()}`);
  };
  const inside = relative(found.home, resolve(source));
  if (inside && !inside.startsWith("..") && !isAbsolute(inside)) return guestCopy(`/layerfs/home/${inside.split(sep).join("/")}`);
  const stage = mkdtempSync(join(found.home, ".vm-copy-"));
  try {
    cpSync(source, join(stage, "c"), { recursive: true, mode: constants.COPYFILE_FICLONE, verbatimSymlinks: true, preserveTimestamps: true });
    await guestCopy(`/layerfs/home/${basename(stage)}/c`);
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}

export const read = (name, path) => readFile(CLI, name, path, exec);
export const write = (name, path, text) => writeFile(CLI, name, path, text, exec);

export default {
  name: "lima",
  defaultImage: null,
  inspect, ensureStore, create, start, stop, restart, remove, removeStore, blockUserNamespaces, storeFree, read, write, copy, exec,
};

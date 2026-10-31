// LayerFSRuntime.open's settings, resolved once into absolute values. Copied
// from the dsh Host's config.js (#428). A missing required setting does not
// refuse open: status() lists it, so a runtime page can show what to fix.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { isAbsolute, join, posix } from "node:path";
import { LayerFSError } from "@layerfs/agent";

const PLATFORMS = { linux: "linux", darwin: "macos", win32: "windows" };
const DEFAULT_DOCKER = "/var/run/docker.sock";
/** Where a home's Store and Sandboxes live: this machine (`local`, today) or one Linux VM per home (V9). */
const PROVIDERS = ["microsandbox", "apple-container"];
/** The VM's own settings: the image that holds node and the guest base, and its size. */
const VM = { image: "node:22-trixie-slim", cpus: 2, memory: "2G", storeSize: "20G" };
/** Where the VM sees the home, the Store disk, the bundle and its working state (§5, §6). */
export const GUEST = Object.freeze({
  home: "/layerfs/home",
  store: "/layerfs/store",
  bundle: "/layerfs/bundle",
  import: "/layerfs/import",
  state: "/layerfs/state",
  local: "/layerfs/local",
  daemon: "/layerfs/bundle/layerfs-daemon",
});

/**
 * `placement`: absent or `local` (this machine's Docker, macOS worker container
 * or WSL2 worker; behavior unchanged), or `vm:<provider>` (one Linux VM per
 * home, §6). A VM provider name that is not implemented is refused here, not
 * later, so a typo cannot look like `local`.
 */
function placement(given) {
  const text = given ?? process.env.LAYERFS_PLACEMENT ?? "local";
  if (typeof text !== "string" || !text) throw new LayerFSError("InvalidInput", "placement must be `local` or `vm:<provider>`", null);
  if (text === "local") return { kind: "local", name: "local" };
  const provider = /^vm:(.+)$/.exec(text)?.[1];
  if (!provider) throw new LayerFSError("InvalidInput", `placement is \`local\` or \`vm:<provider>\`, not ${text}`, null);
  if (!PROVIDERS.includes(provider)) throw new LayerFSError("Unsupported", `no VM provider is implemented for ${text}; it is vm:${PROVIDERS.join(" or vm:")}`, null);
  return { kind: "vm", name: text, provider };
}

/** The VM's size and image, from `settings.vm` else `LAYERFS_VM_*` else the defaults. */
function vmSettings(given = {}) {
  const NAMES = { image: "LAYERFS_VM_IMAGE", cpus: "LAYERFS_VM_CPUS", memory: "LAYERFS_VM_MEMORY", storeSize: "LAYERFS_VM_STORE_SIZE" };
  const read = (name) => given[name] ?? (process.env[NAMES[name]] || undefined);
  const image = read("image") ?? VM.image;
  if (typeof image !== "string" || !/^[^\s]+$/.test(image)) throw new LayerFSError("InvalidInput", "the VM image is an image reference, such as node:22-trixie-slim", null);
  const cpus = Number(read("cpus") ?? VM.cpus);
  if (!Number.isSafeInteger(cpus) || cpus < 1) throw new LayerFSError("InvalidInput", "the VM's cpus is a whole number of at least 1", null);
  const size = (name, fallback) => {
    const value = read(name) ?? fallback;
    if (typeof value !== "string" || !/^[1-9][0-9]{0,5}[MG]$/.test(value)) throw new LayerFSError("InvalidInput", `the VM's ${name} is a size such as 2G or 512M`, null);
    return value;
  };
  return { image, cpus, memory: size("memory", VM.memory), storeSize: size("storeSize", VM.storeSize) };
}

/**
 * The Docker socket when none is given: the engine's usual path, or on macOS
 * Docker Desktop's own `~/.docker/run/docker.sock` when only that one exists
 * (its "Allow the default Docker socket" setting off). Checked once, when the
 * settings resolve.
 */
function defaultDocker(platform) {
  if (platform !== "macos" || existsSync(DEFAULT_DOCKER)) return DEFAULT_DOCKER;
  const desktop = join(homedir(), ".docker", "run", "docker.sock");
  return existsSync(desktop) ? desktop : DEFAULT_DOCKER;
}

/**
 * @param {object} given
 * @param {string} given.home the LayerFS home: project entries and records (required)
 * @param {string} [given.image] runtime image digest Sandboxes run
 * @param {string} [given.daemon] Linux layerfs-daemon (default: the bundle's)
 * @param {string} [given.bundle] sealed Linux build folder; macOS and Windows need it
 * @param {string} [given.docker] Docker Engine Unix socket (default /var/run/docker.sock; on
 *   macOS Docker Desktop's ~/.docker/run/docker.sock when only that one exists)
 * @param {string} [given.volumes] Docker's volumes directory (Linux; default /var/lib/docker/volumes)
 * @param {string} [given.traceDir] per-agent trace files events.subscribe reads (default <home>/traces)
 * @param {string} [given.containerPrefix] this home's container names (default `layerfs-`)
 * @param {string} [given.stateDir] the runtime's project records (default <home>/.runtime)
 * @param {object} [given.wsl] Windows: { distribution, user, node, home }
 * @param {string} [given.placement] `local` (default) or `vm:<provider>`; also read from
 *   LAYERFS_PLACEMENT when not given (V9). A VM holds the home's Stores and Sandboxes.
 * @param {object} [given.vm] VM placement: { image, cpus, memory, storeSize } (also
 *   LAYERFS_VM_IMAGE, LAYERFS_VM_CPUS, LAYERFS_VM_MEMORY, LAYERFS_VM_STORE_SIZE)
 * @param {string} [given.platform] linux | macos | windows (default this process's)
 * @param {boolean} [given.sweep] sweep this home's stopped Sandboxes once at open (default true)
 */
export function resolveSettings(given = {}) {
  if (given === null || typeof given !== "object") throw new LayerFSError("InvalidInput", "settings must be an object", null);
  if (typeof given.home !== "string" || !isAbsolute(given.home)) throw new LayerFSError("InvalidInput", "settings.home must be an absolute LayerFS home folder", null);
  const platform = given.platform ?? PLATFORMS[process.platform];
  if (!Object.values(PLATFORMS).includes(platform)) throw new LayerFSError("Unsupported", `LayerFS runs on Linux, macOS and Windows, not ${process.platform}`, null);
  const bundle = given.bundle || null;
  const where = placement(given.placement);
  const settings = {
    platform,
    placement: where,
    home: given.home,
    image: given.image || null,
    bundle,
    // a VM runs the bundle's daemon beside its helper, at the guest's path
    daemon: where.kind === "vm" ? GUEST.daemon : given.daemon || (bundle ? (platform === "windows" ? posix.join(bundle, "layerfs-daemon") : join(bundle, "layerfs-daemon")) : null),
    docker: given.docker || defaultDocker(platform),
    volumes: given.volumes || "/var/lib/docker/volumes",
    traceDir: given.traceDir || join(given.home, "traces"),
    containerPrefix: given.containerPrefix || "layerfs-",
    stateDir: given.stateDir || join(given.home, ".runtime"),
    pollMs: given.pollMs ?? 400,
    sweep: given.sweep !== false,
  };
  if (where.kind === "vm") settings.vm = { ...vmSettings(given.vm), ...where, home: GUEST.home, store: GUEST.store };
  if (platform === "windows") {
    const hash = createHash("sha256").update(given.home).digest("hex").slice(0, 16);
    settings.wsl = {
      distribution: given.wsl?.distribution || "Ubuntu",
      user: given.wsl?.user || "root",
      node: given.wsl?.node || "/usr/local/bin/node",
      home: given.wsl?.home || `/var/lib/layerfs-windows/${hash}`,
    };
  }
  // A new home is made private first, as Init requires; an existing one keeps
  // its mode, and Init refuses it if it is not private.
  mkdirSync(settings.home, { recursive: true, mode: 0o700 });
  for (const dir of [settings.stateDir, settings.traceDir]) mkdirSync(dir, { recursive: true });
  return settings;
}

/** Settings Init and Workspaces need that are not given. macOS, Windows and a VM need a bundle too. */
export const missing = (settings) =>
  [...(settings.placement.kind === "vm" ? [] : ["image"]), "daemon", ...(settings.platform === "linux" && settings.placement.kind !== "vm" ? [] : ["bundle"])].filter((name) => !settings[name]);

/** The LayerFS home as LayerFS calls see it (Windows: the WSL one; a VM: the guest's). */
export const callHome = (settings) => settings.vm?.home ?? settings.wsl?.home ?? settings.home;

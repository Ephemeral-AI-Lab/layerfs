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
 * @param {string} [given.platform] linux | macos | windows (default this process's)
 * @param {boolean} [given.sweep] sweep this home's stopped Sandboxes once at open (default true)
 */
export function resolveSettings(given = {}) {
  if (given === null || typeof given !== "object") throw new LayerFSError("InvalidInput", "settings must be an object", null);
  if (typeof given.home !== "string" || !isAbsolute(given.home)) throw new LayerFSError("InvalidInput", "settings.home must be an absolute LayerFS home folder", null);
  const platform = given.platform ?? PLATFORMS[process.platform];
  if (!Object.values(PLATFORMS).includes(platform)) throw new LayerFSError("Unsupported", `LayerFS runs on Linux, macOS and Windows, not ${process.platform}`, null);
  const bundle = given.bundle || null;
  const settings = {
    platform,
    home: given.home,
    image: given.image || null,
    bundle,
    daemon: given.daemon || (bundle ? (platform === "windows" ? posix.join(bundle, "layerfs-daemon") : join(bundle, "layerfs-daemon")) : null),
    docker: given.docker || defaultDocker(platform),
    volumes: given.volumes || "/var/lib/docker/volumes",
    traceDir: given.traceDir || join(given.home, "traces"),
    containerPrefix: given.containerPrefix || "layerfs-",
    stateDir: given.stateDir || join(given.home, ".runtime"),
    pollMs: given.pollMs ?? 400,
    sweep: given.sweep !== false,
  };
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

/** Settings Init and Workspaces need that are not given. macOS and Windows need a bundle too. */
export const missing = (settings) => [
  ...["image", "daemon", ...(settings.platform === "linux" ? [] : ["bundle"])].filter((name) => !settings[name]),
];

/** The LayerFS home as LayerFS calls see it (Windows: the WSL one). */
export const callHome = (settings) => settings.wsl?.home ?? settings.home;

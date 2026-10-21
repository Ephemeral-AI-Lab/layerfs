// Host plugin configuration and its resolution into absolute settings.
import { mkdirSync, realpathSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { storeName, VOLUMES } from "@layerfs/port";

/**
 * @typedef {object} Config
 * @property {string} [image] LayerFS runtime image digest (`sha256:…`) Sandboxes run; Init refuses without it.
 * @property {string} [daemon] Host path of the built Linux layerfs-daemon; Init refuses without it.
 * @property {string} [bundle] Folder of a sealed Linux LayerFS build (layerfs-daemon, layerfs.node,
 *   sdk/js). On macOS, Live projects need it: their SDK calls run in the Store container (@layerfs/port).
 * @property {string} [containerPrefix] Prefix of this home's container names (default `layerfs-`):
 *   Sandboxes `<prefix><project>-<n>`, the Store container `<prefix>store`. A second home on
 *   the same Docker engine (a test, another checkout) sets its own.
 * @property {string} [docker] Local Docker Engine Unix socket.
 * @property {string} [volumes] Docker's volumes directory (default `/var/lib/docker/volumes`), for an
 *   engine whose data-root is elsewhere. On Linux, Init installs the Store into a volume's directory
 *   there, so the Host's user must be able to write it.
 * @property {string} [home] LayerFS home for discovery records, shared by the Host and agents' bash calls.
 * @property {string} [stateDir] Directory for project bindings and agent names.
 * @property {string} [traceDir] Directory for per-agent SDK trace files; bash calls must be able to write it.
 * @property {number} [pollMs] Interval at which trace files are read for new lines.
 */

/**
 * @typedef {object} Spec
 * @property {string|null} image
 * @property {string|null} daemon
 * @property {string|null} bundle
 * @property {string} prefix this home's container name prefix
 * @property {string|null} box the Store container's name, on macOS with a bundle (else null)
 * @property {string} docker
 * @property {string} volumes
 * @property {string} home
 * @property {string} stateDir
 * @property {string} traceDir
 * @property {number} pollMs
 * @property {string} sdk file URL of the SDK entry Live projects use (on macOS, @layerfs/port/sdk)
 * @property {string} operator file URL of its operator entry
 * @property {string} simSdk file URL of the SDK on the simulator (sim/sdk.js), for simulated projects
 * @property {string} simOperator file URL of its operator entry (sim/operator.js)
 */

/**
 * Resolves configured values and defaults into one absolute specification.
 * The LayerFS home and trace files default to the host temp area, which the
 * bash sandbox's workspace-write mode may write; bindings default to the
 * profile directory.
 * @param {Config} config
 * @param {{ dir: string } | undefined} profile the launcher's profile, when present
 * @returns {Spec}
 */
export function resolveConfig(config, profile) {
  const temp = join(realpathSync(tmpdir()), "dsh-layerfs");
  const bundle = config.bundle || null, home = config.home || join(temp, "home");
  // macOS runs no Store call itself: a bundle's SDK calls run in the Store container (@layerfs/port)
  const ported = process.platform === "darwin" && bundle !== null;
  const prefix = config.containerPrefix || "layerfs-";
  const spec = {
    image: config.image || null,
    daemon: config.daemon || (bundle && join(bundle, "layerfs-daemon")),
    bundle,
    prefix,
    box: ported ? storeName(prefix) : null,
    docker: config.docker || "/var/run/docker.sock",
    volumes: config.volumes || VOLUMES,
    home,
    stateDir: config.stateDir || join(profile ? profile.dir : join(homedir(), ".dsh"), "dsh-layerfs"),
    traceDir: config.traceDir || join(temp, "traces"),
    pollMs: config.pollMs ?? 400,
    sdk: ported ? import.meta.resolve("@layerfs/port/sdk") : import.meta.resolve("@layerfs/sdk"),
    operator: ported ? import.meta.resolve("@layerfs/port/operator") : import.meta.resolve("@layerfs/sdk/operator"),
    simSdk: new URL("./sim/sdk.js", import.meta.url).href,
    simOperator: new URL("./sim/operator.js", import.meta.url).href,
  };
  for (const dir of [spec.stateDir, spec.traceDir]) mkdirSync(dir, { recursive: true });
  mkdirSync(spec.home, { recursive: true, mode: 0o700 });
  return spec;
}

/** Deployment settings Init and bash calls need that are not configured (macOS: a bundle too). */
export const missingDeployment = (spec) => ["image", "daemon", ...(process.platform === "darwin" ? ["bundle"] : [])].filter((name) => !spec[name]);

/** The SDK entry as a path, for Node imports in the Host. */
export const sdkPath = (spec) => fileURLToPath(spec.sdk);

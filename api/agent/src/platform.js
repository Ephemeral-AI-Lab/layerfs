// A Sandbox's platform (#529): what a caller chooses at sandbox.create and
// reads back. The home decides which platforms it allows and which one a
// call that names none gets (§4); LayerFS.init and where a home keeps its
// Stores do not change. Each platform runs a Sandbox runtime (the record's
// `runtime`): docker a Docker container; mac a native daemon on this Mac;
// linux a namespaces daemon on a Linux host (native when the home's
// LAYERFS_SANDBOX is native); a VM provider namespaces inside the project's
// VM. Every later call on a Sandbox follows its record, not these settings.
import { deployment, transportOf } from "./client.js";
import { LayerFSError } from "./errors.js";

/** The eight platform names (#529 P4). There is no `windows`: a Windows Sandbox is `docker` through the WSL2 worker (P5). */
export const PLATFORMS = Object.freeze(["docker", "mac", "linux", "apple-container", "microsandbox", "lima", "firecracker", "ssh"]);

/** The runtime LAYERFS_SANDBOX (and the former LAYERFS_SANDBOX_BACKEND, kept one release) names: docker, native or namespaces. */
function homeRuntime(deploy) {
  if (deploy.sandboxBackend === "namespaces" || deploy.sandbox === "namespaces") return "namespaces";
  return deploy.sandbox === "native" ? "native" : "docker";
}

/**
 * The home a client's calls reach: `{ name, allowed, default }`. A worker whose
 * endpoint names a VM is a `vm:<provider>` home, which allows that provider; any
 * other worker (macOS, Windows) runs Docker. In this process, Linux allows
 * docker and linux, macOS mac when LAYERFS_SANDBOX is native and docker
 * otherwise, and Windows docker. The default is the platform LAYERFS_SANDBOX
 * names, which a refusal reports when the home does not allow it. `os` is
 * this process's platform (a parameter for tests).
 */
export function homeOf(client, os = process.platform) {
  const transport = transportOf(client);
  const runtime = homeRuntime(client.options.deploy);
  if (transport.worker) {
    const vm = transport.place?.();
    if (vm === undefined) throw new LayerFSError("Failed", "the LayerFS worker is not running for this home, so its platforms are unknown; nothing was made", null);
    if (vm) return { name: `vm:${vm.provider}`, allowed: [vm.provider], default: vm.provider };
    return { name: `${os === "win32" ? "Windows" : "macOS"} local (worker)`, allowed: ["docker"], default: "docker" };
  }
  const named = runtime === "docker" ? "docker" : os === "darwin" && runtime === "native" ? "mac" : "linux";
  if (os === "linux") return { name: "Linux local", allowed: ["docker", "linux"], default: named };
  if (os === "darwin") return runtime === "native" ? { name: "macOS local (native)", allowed: ["mac"], default: "mac" } : { name: "macOS local", allowed: ["docker"], default: named };
  return { name: "Windows local", allowed: ["docker"], default: named };
}

/** `platform` checked against the eight names and the home: refused before any effect. */
export function checkedPlatform(client, platform, os = process.platform) {
  const home = homeOf(client, os);
  const chosen = platform === undefined ? home.default : platform;
  if (typeof chosen !== "string" || !PLATFORMS.includes(chosen)) {
    throw new LayerFSError("InvalidInput", `platform is one of ${PLATFORMS.join(", ")}, not ${typeof chosen === "string" ? chosen : typeof chosen}`, null);
  }
  if (!home.allowed.includes(chosen)) {
    throw new LayerFSError("Unsupported", `platform ${chosen} is not available on this ${home.name} home, which allows ${home.allowed.join(" and ")}${platform === undefined ? " (it is the default LAYERFS_SANDBOX names)" : ""}`, null);
  }
  return { home, platform: chosen };
}

/**
 * The deployment a new Sandbox of `platform` starts with. The home's default
 * keeps the home's own settings as they are (the former pair included, which
 * the binding maps or refuses); another platform names its runtime instead.
 * The image is the caller's `image`, else the platform's own default: `docker`
 * LAYERFS_IMAGE; `linux` and the VM platforms the one image their
 * LAYERFS_IMAGE_ARCHIVE holds (its config digest, which the binding reads),
 * since Docker's image store may name the same image by another digest.
 */
export function platformDeployment(client, home, platform, image, os = process.platform) {
  const given = { ...client.options.deploy, ...(image ? { image } : {}) };
  let named = given;
  if (platform !== home.default) {
    const runtime = platform === "docker" ? "docker" : platform === "mac" ? "native"
      : platform === "linux" && homeRuntime(given) === "native" ? "native" : "namespaces";
    named = { ...given, sandbox: runtime, sandboxBackend: undefined };
  }
  if (!image && platform !== "docker" && platform !== "mac") named = { ...named, image: undefined };
  // a worker's Sandboxes run on its Linux host, the addon's on this one
  return deployment(named, Boolean(client.options.postgresql), transportOf(client).worker ? "linux" : os);
}

/**
 * The platform of a record from before #529, which names none: the one its
 * `runtime` and the home imply (a read-time default; the record is not
 * rewritten). Docker is docker; native is mac on a macOS home and linux
 * elsewhere; namespaces is the provider on a vm: home and linux elsewhere.
 */
export function platformOf(item, home, os = process.platform) {
  if (item.platform) return item.platform;
  if (!item.runtime || item.runtime === "docker") return "docker";
  if (home.name.startsWith("vm:")) return home.default;
  return item.runtime === "native" && os === "darwin" && !home.name.includes("worker") ? "mac" : "linux";
}

/** A Sandbox's image as callers read it: null for a native one (the binding says `local`). */
export const imageOf = (image) => (image && image !== "local" ? image : null);

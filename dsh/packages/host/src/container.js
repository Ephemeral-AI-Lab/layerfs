// How a host folder appears inside a ported project's containers (box.js,
// port/binding.js). A POSIX host mounts every folder at the path it already
// has, so one path means the same inside and outside — the LayerFS home, the
// build (and its layerfs.node), the port's own code, and Init's source folder.
// Windows cannot: a drive-letter path is not a Linux path, and `docker -v`
// reads its colon as the source/target separator. There each mounted folder
// gets a fixed name under one container root, and every path a container is
// given is that container path. Container paths are POSIX text in both cases:
// never `node:path` output, which uses the host's separator.
/** The root a Windows host's folders are mounted under inside a container. */
export const ROOT = "/dsh-layerfs";

/** A drive-letter host path cannot be a container path. */
export const containerized = process.platform === "win32";

/** Hosts where the real SDK runs in the Store container rather than in this process. */
export const portedHost = () => process.platform === "darwin" || process.platform === "win32";

/** The path `host` has inside a container, mounted under `name`. */
const at = (host, name) => (containerized ? `${ROOT}/${name}` : host);

/** The LayerFS home as a container sees it; the Host still uses its own path to find the Store file. */
export const homeInContainer = (home) => at(home, "home");

/**
 * The folders a ported container mounts and the path each has inside it: the
 * home (writable), the build and the port's code (read-only), and Init's
 * source folder when the call has one.
 */
export function layout({ home, bundle, port, source }) {
  return {
    home: at(home, "home"),
    bundle: at(bundle, "bundle"),
    port: at(port, "port"),
    source: source == null ? null : at(source, "source"),
  };
}

/**
 * The Docker CLI's endpoint: a configured socket path (`/var/run/docker.sock`
 * becomes `unix:///var/run/docker.sock`), an endpoint already written as a URL
 * or a Windows named pipe (`npipe:////./pipe/dockerDesktopLinuxEngine`), or
 * none — the engine the CLI is pointed at, which is how Docker Desktop is
 * reached on Windows.
 */
export const dockerHost = (docker) => docker == null ? [] : ["-H", /^[a-z]+:\/\//i.test(docker) ? docker : docker.startsWith("/") ? `unix://${docker}` : docker];

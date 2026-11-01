// runtime.status().platforms (#529 §5): every Sandbox platform, with whether
// this home allows it (§4), whether its settings and tools are present
// (`ready`) and what is missing. `missing` uses status.missing's setting
// names, `tool:<cli>` for a command not on PATH and `host:<fact>` for what
// this host lacks; a platform the home does not allow is not checked and
// misses nothing. It starts nothing. Connect builds its New Sandbox menu from it.
import { existsSync } from "node:fs";
import { delimiter, join } from "node:path";
import { PLATFORMS } from "@layerfs/agent/internal";

/** The command each VM provider's adapter runs (platform/vm/). */
const TOOLS = { "apple-container": "container", microsandbox: "msb", lima: "limactl", firecracker: "limactl", ssh: "ssh" };

const onPath = (tool) => (process.env.PATH ?? "").split(delimiter).some((dir) => dir && existsSync(join(dir, tool)));

/** The platforms a home allows: its VM provider on a `vm:` home, docker and linux on Linux, docker elsewhere. */
export const allowedOf = (settings) =>
  settings.placement.kind === "vm" ? [settings.placement.provider] : settings.platform === "linux" ? ["docker", "linux"] : ["docker"];

/** A linux Sandbox (namespaces on this host) needs its settings, root, FUSE and cgroup v2; its image is the one its archive holds. */
const linuxMissing = (settings) => [
  ...["daemon", "localDir", "imageArchive"].filter((name) => !settings[name]),
  ...(process.getuid?.() === 0 ? [] : ["host:root"]),
  ...(existsSync("/dev/fuse") ? [] : ["host:/dev/fuse"]),
  ...(existsSync("/sys/fs/cgroup/cgroup.controllers") ? [] : ["host:cgroup2"]),
];

/** `[{ id, allowed, ready, missing }]` for all eight platforms; `missing` is status.missing, `docker` status.docker. */
export function platforms(settings, missing, docker) {
  const allowed = allowedOf(settings);
  return PLATFORMS.map((id) => {
    if (!allowed.includes(id)) return { id, allowed: false, ready: false, missing: [] };
    const lacks = id === "docker" ? [...missing, ...(docker.level === "ok" ? [] : ["tool:docker"])]
      : id === "linux" ? linuxMissing(settings)
      : [...missing, ...(onPath(TOOLS[id]) ? [] : [`tool:${TOOLS[id]}`])];
    return { id, allowed: true, ready: lacks.length === 0, missing: lacks };
  });
}

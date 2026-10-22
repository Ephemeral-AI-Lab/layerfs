// Removes a LayerFS project: what Init made, in reverse. One attempt, no
// retry. It refuses while a running container (a Sandbox) still uses the
// project's volume, unless the caller asks to stop them (a demo's Replay:
// a stopped run leaves Workspaces mounted, which keep their Sandboxes from
// closing when idle). Stopped containers on the volume, left by failed
// starts and idle Sandboxes, are removed with it. The project's Sandbox
// records are in its home entry (<project>/sandboxes/, #373), removed with
// it; records of the older layout (_sandboxes/) are removed too: its service
// Sandbox's, and each that names its volume (unique to one Init). The Host's
// own cached client is released first.
// The workspace folder, session transcripts and line journals are untouched;
// sessions that used the project have no LayerFS project afterwards. The home
// entry is the LayerFS project, so deleting it frees the name for Init at once.
// A simulated project has no container or volume: its background processes
// are stopped and its home entry deleted.
import { execFile } from "node:child_process";
import { existsSync, readFileSync, readdirSync, rmSync } from "node:fs";
import { join, relative } from "node:path";
import { promisify } from "node:util";
import { isSim } from "./layerfs-client.js";
import { stopProcesses } from "./sim/binding.js";
import { dockerHost } from "./container.js";

const execute = promisify(execFile);
/** Container states that hold the volume but run nothing: what a failed start leaves. */
const STOPPED = new Set(["created", "exited", "dead"]);
const refuse = (code, message, status = 409) => Object.assign(new Error(message), { code, status });

/** The project's Sandbox records of the older layout: its service Sandbox's, and each whose record names its volume. */
function forgetSandboxes(spec, name, volume) {
  const root = join(spec.home, "_sandboxes");
  if (!existsSync(root)) return;
  for (const entry of readdirSync(root)) {
    let record = Buffer.alloc(0);
    try { record = readFileSync(join(root, entry, "sandbox.record")); } catch { /* claimed, not yet recorded */ }
    if (entry === `_service.${name}` || record.includes(volume)) rmSync(join(root, entry), { recursive: true, force: true });
  }
}

/**
 * @param {object} spec resolved Host settings
 * @param {import("./binding.js").BindingStore} bindings
 * @param {import("./trees.js").TreeReader} trees
 * @param {string} name the project
 * @param {(file: string, args: string[]) => Promise<{ stdout: string }>} [run] runs docker
 * @param {{ stopSandboxes?: boolean }} [options] stopSandboxes: stop the running containers on the volume instead of refusing
 * @returns {Promise<{ removed: string, volume: string }>}
 */
export async function removeProject(spec, bindings, trees, name, run = execute, { stopSandboxes = false } = {}) {
  const project = bindings.byName(name);
  if (!project) throw refuse("not-found", `no LayerFS project named ${name}`, 404);
  const home = join(spec.home, project.name), inside = relative(spec.home, home);
  if (!inside || inside.startsWith("..") || inside.includes("/")) throw refuse("invalid-name", `${project.name} is not a project name`);
  await trees.forget(project.name);
  if (isSim(project)) {
    stopProcesses(spec.home, project.name);
    if (existsSync(home)) rmSync(home, { recursive: true, force: true });
    bindings.remove(project.name);
    return { removed: project.name, volume: project.volume };
  }
  const docker = dockerHost(spec.docker);
  const { stdout } = await run("docker", [...docker, "ps", "-a", "--filter", `volume=${project.volume}`, "--format", "{{.ID}} {{.State}}"]);
  const containers = stdout.split("\n").filter(Boolean).map((line) => line.split(" "));
  const users = containers.filter(([, state]) => !STOPPED.has(state)).length;
  if (users && !stopSandboxes) throw refuse("busy", `${users} container${users === 1 ? "" : "s"} still use${users === 1 ? "s" : ""} ${project.volume}; a Sandbox closes 2 minutes after its last Workspace is unmounted, or stop them`);
  // stopped containers on the volume are left by failed starts; the name was confirmed, so they go with it
  const stopped = containers.map(([id]) => id);
  if (stopped.length) await run("docker", [...docker, "rm", ...(users ? ["-f"] : []), ...stopped]);
  await run("docker", [...docker, "volume", "rm", project.volume]);
  forgetSandboxes(spec, project.name, project.volume);
  if (existsSync(home)) rmSync(home, { recursive: true, force: true });
  bindings.remove(project.name);
  return { removed: project.name, volume: project.volume };
}

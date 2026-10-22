// The move to #373's layout, when the Host starts. Sandboxes of the older
// kinds (each project's service Sandbox and named ones, records under
// <home>/_sandboxes/) are removed only when they have nothing to lose: a
// container that no longer runs, or a running one with no mounted Workspace
// (a background process needs one). A running one with a mounted Workspace
// may hold uncommitted changes or a running process, and this Host cannot
// ask an older daemon which: it is kept, with its records, and reported for
// the user to decide. Records go only once every older container is gone:
// then <home>/_sandboxes/ and each project's service.record are deleted.
// Stores are untouched. One attempt each, no retry; the next start looks again.
import { execFile } from "node:child_process";
import { existsSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";
import { promisify } from "node:util";
import { dockerHost } from "./container.js";

const execute = promisify(execFile);
const gone = (error) => /No such (container|object)/i.test(String(error?.stderr ?? error?.message));

/**
 * @param {object} spec resolved Host settings
 * @param {(file: string, args: string[]) => Promise<{ stdout: string }>} [run] runs docker
 * @returns {Promise<{ kept: { sandbox: string, container: string, workspaces: number, reason: string }[] }>}
 *   the older Sandboxes left running, each with why
 */
export async function migrate(spec, run = execute, warn = console.warn) {
  const root = join(spec.home, "_sandboxes"), kept = [];
  const docker = (...args) => run("docker", [...dockerHost(spec.docker), ...args]);
  if (existsSync(root)) {
    for (const sandbox of readdirSync(root)) {
      const dir = join(root, sandbox);
      let container = null;
      try { container = readFileSync(join(dir, "container"), "utf8").trim(); } catch { /* claimed, never started */ }
      if (!container) continue;
      let running;
      try {
        running = (await docker("inspect", "-f", "{{.State.Running}}", container)).stdout.trim() === "true";
      } catch (error) {
        if (gone(error)) continue;
        kept.push({ sandbox, container, workspaces: null, reason: `Docker could not inspect it: ${error.message}` });
        continue;
      }
      const workspaces = readdirSync(dir).filter((f) => f.startsWith("mount-") && f.endsWith(".record")).length;
      if (running && workspaces) {
        kept.push({ sandbox, container, workspaces, reason: `${workspaces} mounted Workspace${workspaces === 1 ? "" : "s"}, which may hold uncommitted changes or running processes` });
        continue;
      }
      try { await docker("rm", "-f", container); } catch (error) {
        if (!gone(error)) kept.push({ sandbox, container, workspaces, reason: `Docker could not remove it: ${error.message}` });
      }
    }
    for (const k of kept) {
      warn(`layerfs: the older Sandbox ${k.sandbox} (container ${k.container.slice(0, 12)}) was left running: ${k.reason}. Commit or discard its Workspaces with the older build, or remove it yourself (docker rm -f ${k.container.slice(0, 12)}); the Host looks again at its next start`);
    }
    if (kept.length) return { kept };
    rmSync(root, { recursive: true, force: true });
  }
  for (const entry of readdirSync(spec.home)) {
    const dir = join(spec.home, entry);
    if (statSync(dir).isDirectory()) rmSync(join(dir, "service.record"), { force: true });
  }
  return { kept };
}

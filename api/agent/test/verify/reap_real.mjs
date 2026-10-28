// A process a command leaves behind is reaped when it exits: the Sandbox's
// PID 1 is an init that waits on orphans, not the daemon, so no zombie
// accumulates (#347 BUGS row 30: 290 Chromium zombies after 16 browser
// proofs; #352 F-054: dsh run.spec.ts timed out on a zombie grandchild). The
// daemon still serves the Workspace, and the command's own exit status is
// still its own.
//
//   node api/agent/test/verify/reap_real.mjs outer <daemon> <addon> <image>
//
// `outer` (on the Docker host) makes a fresh Store volume and runs `inner` in
// the runtime image, as the direct route needs (F-020). One attempt per step;
// nothing retried.
import assert from "node:assert/strict";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { docker, inRuntime, REPO, STORE_PATH } from "./container.mjs";

// processes in state Z (zombie), from /proc, as "/proc/<pid>/status <name>" lines
const ZOMBIES = `for f in /proc/[0-9]*/status; do awk '/^Name:/{n=$2} /^State:/{s=$2} END{if(s=="Z")print FILENAME" "n}' "$f" 2>/dev/null; done`;

async function inner() {
  const { LayerFS } = await import("@layerfs/agent");
  let layerfs;
  const source = mkdtempSync(join(tmpdir(), "layerfs-reap-"));
  writeFileSync(join(source, "README.md"), "workspace file\n");
  process.env.LAYERFS_STORE_PATH = STORE_PATH;
  layerfs = await LayerFS.init("reap", source);
  rmSync(source, { recursive: true, force: true });
  await layerfs.branch.fork("feature", "main");
  const ws = await layerfs.workspace.mount((await layerfs.sandbox.create()).sandbox, "feature");
  const sh = (command) => layerfs.workspace.exec(ws, command);
  // three orphans: each outlives the command that started it, then exits
  const left = await sh("for i in 1 2 3; do (sleep 1 &) ; done; exit 0");
  assert.equal(left.exitCode, 0);
  const after = await sh(`sleep 3; ${ZOMBIES}`);
  assert.equal(after.exitCode, 0);
  assert.equal(after.stdout.trim(), "", `orphans that exited are reaped, but zombies remain:\n${after.stdout}`);
  // the daemon is not PID 1, and a command's exit status is still its own
  const init = await sh("cat /proc/1/comm");
  console.log(`PID1 ${init.stdout.trim()}`);
  assert.notEqual(init.stdout.trim(), "layerfs-daemon", "an init, not the daemon, is PID 1");
  assert.equal((await sh("exit 7")).exitCode, 7);
  await layerfs.workspace.unmount(ws, true);
  console.log("MAIN ok");
}

function outer([daemon, addon, image]) {
  assert.ok(daemon && addon && image, "usage: outer <daemon> <addon> <image digest>");
  const stamp = `reap-${Date.now().toString(36)}`;
  const volume = `layerfs-issue352-${stamp}`;
  const home = join(REPO, "target/f054", `${stamp}-home`);
  mkdirSync(home, { recursive: true });
  chmodSync(home, 0o700); // LAYERFS_HOME must be private
  let r;
  try {
    assert.equal(docker(["volume", "create", volume]).status, 0);
    r = inRuntime({ script: fileURLToPath(import.meta.url), daemon, addon, image, volume, prefix: `${stamp}-`, env: { LAYERFS_HOME: home }, timeout: 90_000 });
  } finally {
    const left = docker(["ps", "-aq", "--filter", `name=^${stamp}-`]).stdout.trim().split("\n").filter(Boolean);
    if (left.length) docker(["rm", "-f", ...left]);
    docker(["volume", "rm", volume]);
    rmSync(home, { recursive: true, force: true });
  }
  console.log(`== exit ${r.status}\n${r.out.split("\n").slice(-30).join("\n")}`);
  assert.equal(r.status, 0, "the run passes");
  assert.match(r.out, /MAIN ok/);
  console.log("reap: ok");
}

const [mode, ...rest] = process.argv.slice(2);
if (mode === "inner") await inner(rest);
else outer(rest);

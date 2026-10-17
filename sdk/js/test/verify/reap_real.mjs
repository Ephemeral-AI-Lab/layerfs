// A process a command leaves behind is reaped when it exits: the Sandbox's
// PID 1 is an init that waits on orphans, not the daemon, so no zombie
// accumulates (#347 BUGS row 30: 290 Chromium zombies after 16 browser
// proofs). The daemon still serves the Workspace, and the command's own exit
// status is still its own.
//   LAYERFS_DAEMON=<linux daemon> node sdk/js/test/verify/reap_real.mjs
import assert from "node:assert/strict";
import { mount, setup, sh, teardown } from "./lib.mjs";

const ctx = await setup("reap", { project: `reap-${Date.now().toString(36)}` });
const l = ctx.layerfs;
// processes in state Z (zombie), from /proc, as "/proc/<pid>/status <name>" lines
const ZOMBIES = `for f in /proc/[0-9]*/status; do awk '/^Name:/{n=$2} /^State:/{s=$2} END{if(s=="Z")print FILENAME" "n}' "$f" 2>/dev/null; done`;
try {
  await l.project.init(ctx.source);
  await l.branch.fork("feature");
  const sandbox = await l.sandbox.create("reap-box", l.projectName, ctx.deploy.image);
  const ws = await mount(ctx, sandbox, "feature");
  // three orphans: each outlives the command that started it, then exits
  const left = await sh(ctx, ws, "for i in 1 2 3; do (sleep 1 &) ; done; exit 0");
  assert.equal(left.exitCode, 0);
  const after = await sh(ctx, ws, `sleep 3; ${ZOMBIES}`);
  assert.equal(after.exitCode, 0);
  assert.equal(after.stdout.trim(), "", `orphans that exited are reaped, but zombies remain:\n${after.stdout}`);
  // the daemon is not PID 1, and a command's exit status is still its own
  const init = await sh(ctx, ws, "cat /proc/1/comm");
  assert.notEqual(init.stdout.trim(), "layerfs-daemon", "an init, not the daemon, is PID 1");
  assert.equal((await sh(ctx, ws, "exit 7")).exitCode, 7);
  console.log("reap: ok");
} finally {
  const problems = await teardown(ctx);
  if (problems.length) console.error("teardown:", problems);
}

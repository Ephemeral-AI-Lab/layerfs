// @layerfs/runtime on Linux against a real Store and Docker (#428): status,
// the refused worker, one Init job into a project volume (no timeout: no
// limit), one whose 1 ms timeout fails Deadline and leaves no project
// (#431), the project list, get_state, live events from
// the trace and remove. The
// launcher mounts one Store volume, so a "volume" here is a folder under
// <Store volume>/rt-volumes, and its image has no docker CLI (the runtime
// calls the CLI, as the dsh Host did): `volume create/rm` make and remove
// that folder, `version` and `ps` answer an engine with no containers.
// Store, Init and reads are real.
// Under the E2E launcher (its Store volume, home and deployment):
//   node api/runtime/test/verify/runtime_real.mjs inner
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

if (process.argv[2] !== "inner") throw new Error("usage: runtime_real.mjs inner (under tools/worktree.py run)");
const { LayerFSRuntime } = await import("@layerfs/runtime");
const volumes = join(dirname(dirname(process.env.LAYERFS_STORE_PATH)), "rt-volumes");
mkdirSync(volumes, { recursive: true });
const runtime = await LayerFSRuntime.open({
  home: process.env.LAYERFS_HOME, image: process.env.LAYERFS_IMAGE, daemon: process.env.LAYERFS_DAEMON, docker: process.env.LAYERFS_DOCKER,
  volumes, containerPrefix: process.env.LAYERFS_CONTAINER_PREFIX, traceDir: dirname(process.env.LAYERFS_TRACE), pollMs: 50,
});
const docker = [];
runtime.platform = {
  ...runtime.platform,
  async docker(settings, args) {
    docker.push(args.slice(0, 2).join(" "));
    // Init labels the volume (--label ...), so its name is the last argument
    if (args[0] === "volume" && args[1] === "create") { mkdirSync(join(volumes, args.at(-1), "_data"), { recursive: true }); return args.at(-1); }
    if (args[0] === "volume" && args[1] === "rm") { rmSync(join(volumes, args[2]), { recursive: true }); return args[2]; }
    if (args[0] === "version") return "stub";
    if (args[0] === "ps") return "";
    throw new Error(`unexpected docker ${args.join(" ")}`);
  },
};
const name = `rt-${Date.now().toString(36)}`;
const source = mkdtempSync(join(tmpdir(), "layerfs-rt-"));
writeFileSync(join(source, "README.md"), "runtime\n");
mkdirSync(join(source, "src"));
writeFileSync(join(source, "src", "index.js"), "export {};\n");
try {
  const state = await runtime.status();
  assert.deepEqual([state.platform, state.missing, state.docker.level, state.worker], ["linux", [], "ok", null]);
  assert.ok(state.disk.home > 0);
  await assert.rejects(runtime.worker.start(), { code: "InvalidInput" });

  // one Init job, read until it ends
  const ended = async (job) => {
    const started = Date.now();
    while (!job.ended && Date.now() - started < 60_000) {
      await new Promise((resolve) => setTimeout(resolve, 50));
      job = await runtime.project.get_init(job.id);
    }
    return job;
  };
  const job = await ended(await runtime.project.start_init(name, source));
  assert.equal(job.step, "done", JSON.stringify(job.error));
  assert.deepEqual(job.totals, { entries: 4, files: 2, bytes: 19 });
  const listed = (await runtime.project.list()).find((p) => p.name === name);
  assert.deepEqual([listed.initialized, existsSync(join(volumes, listed.volume, "_data/global/store.sqlite"))], [true, true]);

  // live events: the runtime's own traced calls appear with their project
  const events = runtime.events.subscribe({ project: name });
  const view = await runtime.project.get_state(name);
  assert.deepEqual([view.kind, view.sandboxes, view.gone], ["ok", [], []]);
  assert.ok(view.layers.length >= 1);
  assert.deepEqual(await runtime.project.get_state(name, view.version), { kind: "unchanged", version: view.version });
  const seen = (await events.next()).value;
  assert.equal(seen.project, name);
  events.close();

  // #431: a 1 ms timeout on a 2,000-file source fails Deadline, once, and
  // the runtime removes what the Init made: no record, home entry or volume
  const late = `${name}-late`, big = mkdtempSync(join(tmpdir(), "layerfs-rt-big-"));
  for (let i = 0; i < 2000; i++) writeFileSync(join(big, `f${i}.txt`), `file ${i}\n`.repeat(20));
  const before = docker.length;
  const timed = await ended(await runtime.project.start_init(late, big, { timeout: 1 }));
  assert.deepEqual([timed.step, timed.error?.code], ["failed", "Failed"], JSON.stringify(timed.error));
  assert.match(timed.error.message, /Deadline/);
  const made = docker.slice(before);
  assert.deepEqual(made, ["volume create", "volume rm"], "one Init, then its volume removed");
  assert.equal((await runtime.project.list()).some((p) => p.name === late), false, "no project is left");
  assert.equal(existsSync(join(process.env.LAYERFS_HOME, late)), false, "no home entry is left");
  console.log(`deadline: ${timed.error.message.slice(0, 160)}`);
  rmSync(big, { recursive: true });

  // remove names what it destroys
  await assert.rejects(runtime.project.remove(name), { code: "InvalidInput" });
  assert.deepEqual(await runtime.project.remove(name, { confirm: name }), { kind: "Removed", project: name, volume: listed.volume });
  assert.equal((await runtime.project.list()).some((p) => p.name === name), false);
  assert.equal(existsSync(join(volumes, listed.volume)), false);
  console.log(`docker CLI calls (stubbed): ${docker.join(", ")}`);
  console.log("runtime real: ok");
} finally {
  await runtime.close();
  rmSync(source, { recursive: true, force: true });
}

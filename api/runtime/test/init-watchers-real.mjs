// Real Linux Disposable Init consumed through subscribe_init, without a host
// get_init loop. Only Docker's volume CLI is stubbed; Store and Init are real.
// Build engine/crates/layerfs-node/layerfs.node first, then:
//   /usr/bin/node api/runtime/test/init-watchers-real.mjs
import assert from "node:assert/strict";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { setTimeout as pause } from "node:timers/promises";

const { LayerFSRuntime } = await import("@layerfs/runtime");
const work = mkdtempSync(join(process.cwd(), ".tmp-init-watchers-real-"));
const home = join(work, "home");
const volumes = join(work, "volumes");
const source = join(work, "source");
mkdirSync(source, { recursive: true });
for (let i = 0; i < 1200; i++) writeFileSync(join(source, `file-${String(i).padStart(4, "0")}.txt`), `Init watcher payload ${i}\n`.repeat(16));
const runtime = await LayerFSRuntime.open({
  home, image: `sha256:${"a".repeat(64)}`, daemon: join(work, "unused-daemon"), volumes,
  docker: join(work, "unused-docker.sock"), platform: "linux", sweep: false,
});
runtime.platform = {
  ...runtime.platform,
  async docker(_settings, args) {
    if (args[0] === "volume" && args[1] === "create") {
      mkdirSync(join(volumes, args.at(-1), "_data"), { recursive: true });
      return args.at(-1);
    }
    if (args[0] === "volume" && args[1] === "rm") {
      rmSync(join(volumes, args[2]), { recursive: true, force: true });
      return args[2];
    }
    throw new Error(`unexpected docker ${args.join(" ")}`);
  },
};
let hostReads = 0;
runtime.project.get_init = async () => { hostReads++; throw new Error("the host get_init loop is forbidden in this test"); };
let events;
try {
  const job = await runtime.project.start_init("shop", source);
  events = await runtime.project.subscribe_init(job);
  assert.equal(events.snapshot.step, "scan");
  const iterator = events[Symbol.asyncIterator]();
  const seen = [];
  const deadline = Date.now() + 60_000;
  let terminal;
  while (!terminal) {
    const remaining = deadline - Date.now();
    assert.ok(remaining > 0, "the Init watcher did not reach a terminal event within 60 seconds");
    const next = await Promise.race([
      iterator.next(),
      pause(remaining, { timeout: true }).then(() => { throw new Error("Init watcher wait exceeded its fixed deadline"); }),
    ]);
    assert.equal(next.done, false, "the watcher closed before terminal Init progress");
    const event = next.value;
    assert.equal(event.type, "init.progress");
    assert.equal(event.source, "runtime");
    assert.equal(event.seq, seen.length + 1);
    seen.push(event);
    if (["done", "failed"].includes(event.payload.job.step)) terminal = event.payload.job;
  }
  assert.equal(terminal.step, "done", JSON.stringify(terminal.error));
  assert.deepEqual(terminal.totals, { entries: 1201, files: 1200, bytes: Array.from({ length: 1200 }, (_, i) => `Init watcher payload ${i}\n`.repeat(16).length).reduce((a, b) => a + b, 0) });
  const outer = new Set(seen.map((event) => event.payload.job.step));
  for (const step of ["scan", "volume", "import", "done"]) assert.ok(outer.has(step), `missing outer ${step} report`);
  const imported = seen.map((event) => event.payload.job.imported?.step).filter(Boolean);
  assert.ok(imported.some((step) => ["scan", "files", "tree", "publish", "seal"].includes(step)), "no native imported phase was observed");
  assert.equal(terminal.project.name, "shop");
  assert.ok(terminal.project.volume);
  assert.ok(existsSync(join(volumes, terminal.project.volume, "_data/global/store.sqlite")));
  assert.equal(hostReads, 0);
  console.log(`PASS init-watchers-real (${seen.length} events; imported phases: ${[...new Set(imported)].join(",")})`);
} finally {
  await events?.close();
  await runtime.close();
  rmSync(work, { recursive: true, force: true });
}

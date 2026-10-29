// Focused Init-job watcher coverage with controlled jobs and the runtime owner.
//   /usr/bin/node api/runtime/test/init-watchers.mjs
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const FAKE = fileURLToPath(new URL("../../agent/test/fixture/fake-native.cjs", import.meta.url));
process.env.LAYERFS_NATIVE = FAKE;
const { LayerFSError, LayerFSRuntime } = await import("@layerfs/runtime").then(async (runtime) => ({ ...runtime, ...(await import("@layerfs/agent")) }));
const { InitJobs } = await import("../src/project/jobs.js");
const work = mkdtempSync(join(tmpdir(), "layerfs-init-watchers-"));
const checks = [];
const check = async (name, run) => {
  await run();
  checks.push(name);
};
const controlled = () => {
  const starts = [];
  const jobs = new InitJobs((prepared, report) => new Promise((resolve, reject) => starts.push({ prepared, report, resolve, reject })));
  return { jobs, starts };
};
const settle = async () => { await Promise.resolve(); await Promise.resolve(); };
const publicSurface = (events) => {
  assert.deepEqual(Object.keys(events).sort(), ["close", "cursor", "poll", "snapshot"]);
  assert.deepEqual(Object.getOwnPropertySymbols(events), [Symbol.asyncIterator]);
};

try {
  await check("current snapshot, immutable reports, terminal done/failed, no synthetic history", async () => {
    const { jobs, starts } = controlled();
    const job = jobs.begin({ name: "shop", source: "/src/shop" });
    const events = jobs.subscribe(job.id);
    publicSurface(events);
    assert.equal(events.cursor, 0);
    assert.equal(events.snapshot.step, "scan");
    assert.equal(events.snapshot.scanned, null);
    assert.deepEqual(await events.poll(), { events: [], cursor: 0, hasMore: false, stale: false, closed: false });

    starts[0].report({ scanned: { entries: 2, files: 1, bytes: 5 } });
    const first = await events.poll();
    assert.equal(first.events.length, 1);
    assert.deepEqual(first.events[0], {
      seq: 1, type: "init.progress", source: "runtime",
      payload: { job: { ...events.snapshot, scanned: { entries: 2, files: 1, bytes: 5 } } },
    });
    const queued = first.events[0].payload.job;
    starts[0].report({ step: "volume", scanned: { entries: 4, files: 2, bytes: 9 }, totals: { entries: 4, files: 2, bytes: 9 } });
    assert.deepEqual(queued.scanned, { entries: 2, files: 1, bytes: 5 }, "later Object.assign reports do not mutate queued values");
    const second = await events.poll();
    assert.equal(second.events[0].seq, 2);
    assert.deepEqual(second.events[0].payload.job.totals, { entries: 4, files: 2, bytes: 9 });
    assert.equal(second.hasMore, false);

    const project = { name: "shop", volume: "vol-shop", initialized: true };
    starts[0].resolve(project);
    await settle();
    const done = await events.poll();
    assert.equal(done.events.length, 1);
    assert.equal(done.events[0].seq, 3);
    assert.equal(done.events[0].payload.job.step, "done");
    assert.deepEqual(done.events[0].payload.job.project, project);
    assert.equal(typeof done.events[0].payload.job.ended, "number");
    assert.deepEqual(await events.poll(), { events: [], cursor: 3, hasMore: false, stale: false, closed: false });
    const terminal = jobs.subscribe(job.id);
    assert.equal(terminal.snapshot.step, "done");
    assert.deepEqual(await terminal.poll(), { events: [], cursor: 0, hasMore: false, stale: false, closed: false });

    const failedJob = jobs.begin({ name: "bad", source: "/src/bad" });
    const failed = jobs.subscribe(failedJob.id);
    starts[1].reject(Object.assign(new Error("import refused"), { code: "InvalidInput" }));
    await settle();
    const failure = await failed.poll();
    assert.equal(failure.events.length, 1);
    assert.equal(failure.events[0].type, "init.progress");
    assert.equal(failure.events[0].payload.job.step, "failed");
    assert.deepEqual(failure.events[0].payload.job.error, { code: "InvalidInput", message: "import refused" });
    assert.equal(failure.events[0].payload.job.ended !== null, true);
    assert.throws(() => jobs.subscribe("missing"), { code: "InvalidInput", message: /unknown Init job$/ });
    jobs.close();
  });

  await check("registration/report order, one waiter, close wakeup and retention", async () => {
    const { jobs, starts } = controlled();
    const job = jobs.begin({ name: "shop", source: "/src" });
    const events = jobs.subscribe(job.id);
    assert.equal(events.snapshot.step, "scan");
    starts[0].report({ step: "volume", totals: { entries: 1, files: 0, bytes: 0 } });
    const page = await events.poll();
    assert.equal(page.events[0].seq, 1);
    assert.equal(page.events[0].payload.job.step, "volume");

    const waiting = events.poll(5000);
    await assert.rejects(events.poll(), { code: "InvalidInput", message: /a poll is already outstanding$/ });
    starts[0].report({ step: "import" });
    assert.equal((await waiting).events[0].payload.job.step, "import");

    const closed = events.poll(5000);
    await events.close();
    await events.close();
    assert.equal((await closed).closed, true);
    assert.equal((await events.poll()).closed, true);

    const retained = jobs.subscribe(job.id);
    const retainedBefore = retained.snapshot;
    jobs.jobs.delete(job.id);
    assert.throws(() => jobs.subscribe(job.id), { code: "InvalidInput" });
    starts[0].report({ imported: { step: "files", entries: 3, files: 2, bytes: 8 } });
    const afterExpiry = await retained.poll();
    assert.equal(afterExpiry.events[0].payload.job.imported.step, "files");
    assert.deepEqual(retainedBefore.imported, null);
    jobs.close();
  });

  await check("small-cap overflow, stale resync and old-cursor resync", async () => {
    const { jobs, starts } = controlled();
    const job = jobs.begin({ name: "shop", source: "/src" });
    const events = jobs.subscribe(job.id);
    const owned = [...jobs.watchers.get(job.id)][0];
    owned.capacity = 4;
    for (let i = 1; i <= 5; i++) starts[0].report({ scanned: { entries: i, files: 0, bytes: 0 } });
    const stale = await events.poll();
    assert.equal(stale.stale, true);
    assert.equal(stale.cursor, 5);
    assert.deepEqual(stale.events, []);
    assert.deepEqual(stale.snapshot.scanned, { entries: 5, files: 0, bytes: 0 });
    assert.deepEqual(events.snapshot, stale.snapshot);
    assert.deepEqual(await events.poll(), { events: [], cursor: 5, hasMore: false, stale: false, closed: false });
    events.cursor = 0;
    assert.equal((await events.poll()).stale, true);
    jobs.close();
  });

  await check("iterator order, close, stale and no refusal retry", async () => {
    const { jobs, starts } = controlled();
    const job = jobs.begin({ name: "shop", source: "/src" });
    let events = jobs.subscribe(job.id);
    starts[0].report({ step: "volume" });
    starts[0].report({ step: "import" });
    const iterator = events[Symbol.asyncIterator]();
    assert.equal((await iterator.next()).value.payload.job.step, "volume");
    assert.equal((await iterator.next()).value.payload.job.step, "import");
    const waiting = iterator.next();
    await events.close();
    assert.deepEqual(await waiting, { done: true, value: undefined });

    events = jobs.subscribe(job.id);
    const owned = [...jobs.watchers.get(job.id)][0];
    owned.capacity = 4;
    for (let i = 0; i < 5; i++) starts[0].report({ scanned: { entries: i, files: 0, bytes: 0 } });
    await assert.rejects(events[Symbol.asyncIterator]().next(), { code: "InvalidInput", message: /stream went stale; resubscribe$/ });

    events = jobs.subscribe(job.id);
    let attempts = 0;
    events.poll = async () => { attempts++; throw new LayerFSError("Busy", "no wait", null); };
    await assert.rejects(events[Symbol.asyncIterator]().next(), { code: "Busy" });
    assert.equal(attempts, 1);
    jobs.close();
  });

  await check("runtime subscribe_init and runtime.close ownership", async () => {
    const home = join(work, "home");
    const source = join(work, "source");
    const volumes = join(work, "volumes");
    mkdirSync(source, { recursive: true });
    writeFileSync(join(source, "README.md"), "hello\n");
    const runtime = await LayerFSRuntime.open({ home, image: `sha256:${"a".repeat(64)}`, daemon: "/bin/daemon", volumes, platform: "linux", sweep: false });
    runtime.platform = {
      ...runtime.platform,
      async docker(_settings, args) {
        if (args[0] === "volume" && args[1] === "create") mkdirSync(join(volumes, args.at(-1), "_data"), { recursive: true });
        return "";
      },
    };
    await assert.rejects(runtime.project.subscribe_init("missing"), { code: "InvalidInput", message: /unknown Init job$/ });
    const job = await runtime.project.start_init("shop", source);
    const events = await runtime.project.subscribe_init(job);
    publicSurface(events);
    const waiting = events.poll(5000);
    await runtime.close();
    assert.equal((await waiting).closed, true);
    await assert.rejects(runtime.project.subscribe_init(job), { code: "InvalidInput", message: /closed/ });
  });
} finally {
  rmSync(work, { recursive: true, force: true });
}
console.log(`PASS init-watchers (${checks.length} groups)`);

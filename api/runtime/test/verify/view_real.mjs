// A live view of a Store file in MinIO (#510 §7.3), with the real binding:
// a Store with two projects (Branches, Commits, nested folders, 2,000 small
// files, a chunked and a large file) is published to MinIO; a view of the
// file reads its tree, sizes and history, never file contents, and must
// answer each of those reads exactly as the same Store opened locally
// (projects, Branch heads, Commit lists, folder listings, stats, each
// file's revisions and storage figures). It refuses content reads
// Unsupported, writes, Workspaces and Sandboxes ReadOnly, and, once a push
// lands over the file, fails a read it has not cached with Changed. Each
// step prints the view's counters (requests, bytes, cache hits and misses).
//
//   node api/runtime/test/verify/view_real.mjs linux <bundle> <image digest>
//       on a Mac or Linux Docker host: runs `inner` in the runtime image,
//       where the runtime makes Store calls in its own process
//
// It starts its own MinIO (removed after) and a private home with its own
// container prefix. One attempt per step; nothing is retried.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const docker = (args) => spawnSync("docker", args, { encoding: "utf8" });
const must = (args) => {
  const r = docker(args);
  assert.equal(r.status, 0, `docker ${args.join(" ")}: ${r.stderr}`);
  return r.stdout.trim();
};
const MINIO = "cgr.dev/chainguard/minio@sha256:4692462f35d97d7e82c30371d82f057703c5d9489bcae726010594c812f2d285";
const HOST = "host.docker.internal";

function minio(prefix) {
  const access = `lfs${Date.now().toString(36)}`, secret = `s${Math.random().toString(36).slice(2)}${Date.now()}`;
  const name = `${prefix}minio`;
  must(["run", "-d", "--pull", "never", "--name", name, "-p", "127.0.0.1::9000", "-e", `MINIO_ROOT_USER=${access}`, "-e", `MINIO_ROOT_PASSWORD=${secret}`, MINIO, "server", "/data"]);
  // a bounded readiness wait, not a retry of a product request
  for (let i = 0; docker(["exec", name, "mc", "alias", "set", "local", "http://127.0.0.1:9000", access, secret]).status !== 0; i++) {
    assert.ok(i < 60, "MinIO not ready in 30 s");
    spawnSync("sleep", ["0.5"]);
  }
  must(["exec", name, "mc", "mb", "local/layerfs"]);
  const port = Number(/:(\d+)$/m.exec(must(["port", name, "9000/tcp"]))[1]);
  return { endpoint: `http://${HOST}:${port}`, accessKey: access, secretKey: secret };
}

/** `lines` numbered lines of text: deterministic, and different per seed. */
const text = (lines, seed) => Array.from({ length: lines }, (_, i) => `${seed}:${i}:${((i * 2654435761 + seed) >>> 0).toString(36).repeat(3)}\n`).join("");

function folder(root, name, files) {
  const dir = join(root, `src-${name}`);
  for (const [path, body] of Object.entries(files)) {
    mkdirSync(join(dir, path, ".."), { recursive: true });
    writeFileSync(join(dir, path), body);
  }
  return dir;
}

async function run({ bundle, image, home, prefix, work, s3 }) {
  const { LayerFSRuntime } = await import("@layerfs/runtime");
  const runtime = await LayerFSRuntime.open({ home, image, bundle, platform: "linux", containerPrefix: prefix, sweep: false, daemon: join(bundle, "layerfs-daemon") });
  const ended = async (job) => {
    for (let i = 0; i < 1200; i++) {
      const now = (await runtime.sync.get(job.id)) ?? (await runtime.project.get_init(job.id));
      if (now?.ended) {
        assert.equal(now.step, "done", `${job.kind ?? "init"}: ${JSON.stringify(now.error)}`);
        return now;
      }
      await new Promise((next) => setTimeout(next, 100));
    }
    throw new Error(`job ${job.id} did not end in 120 s`);
  };
  const step = (line) => console.log(`STEP ${line}`);
  const local = async (name) => (await runtime.project.list()).find((p) => p.name === name && p.store === "team");
  const viewed = (view, name) => runtime.views.project(view.view, name);
  const use = (record, work) => runtime.clients.use(record, work);
  /** Every read the comparison makes of one project (tree, sizes, history), as plain values. */
  const reads = (record, files, dirs) => use(record, async (c) => {
    const { LayerFSBackend } = await import("@layerfs/backend");
    const backend = LayerFSBackend.of(c);
    const branches = (await c.project.list_branches()).items.map(({ name, base, head }) => ({ name, base, head }));
    const commits = {}, listings = {}, stats = {}, revisions = {}, storage = {};
    for (const b of branches) {
      if (b.name !== "main") commits[b.name] = (await c.branch.list_commits(b.name)).items;
      for (const dir of dirs) listings[`${b.name}:${dir}`] = (await c.fs.list(b.name, dir, 1000).catch((e) => ({ refused: e.code }))).items ?? "absent";
    }
    for (const [branch, path] of files) {
      stats[`${branch}:${path}`] = await c.fs.stat(branch, path);
      revisions[`${branch}:${path}`] = (await backend.store.fs.list_revisions(branch, path)).items;
      storage[`${branch}:${path}`] = await backend.store.fs.get_storage(branch, path);
    }
    const globbed = (await c.fs.glob("main", "**/*.txt", "/", 1000)).items;
    return { branches, commits, listings, stats, revisions, storage, globbed, totals: await backend.store.project.get_storage() };
  });
  const shown = async (view, what) => {
    const s = await runtime.remote.view_stats(view.view);
    console.log(`  ${what}: ${s.requests} requests, ${s.bytes} bytes of ${s.fileBytes} (${(100 * s.bytes / s.fileBytes).toFixed(1)}%), cache ${s.hits} hits / ${s.misses} misses, ${s.resident} resident of ${s.capacity}`);
    return s;
  };
  try {
    step("a Store with two projects, Branches, Commits and files of three sizes");
    await runtime.store.create("team");
    const shop = {
      "README.md": "shop\n",
      "src/cart.js": "export const cart = [];\n",
      "src/deep/chunked.txt": text(60_000, 1), // a few MB: several chunks
      "src/deep/large.txt": text(300_000, 2), // about 9 MB
    };
    // a wide tree: its metadata reaches past what the open reads
    for (let d = 0; d < 40; d++) for (let f = 0; f < 50; f++) shop[`wide/d${d}/f${f}.txt`] = `${d}/${f}\n`;
    await ended(await runtime.project.start_init("shop", folder(work, "shop", shop), { store: "team" }));
    await ended(await runtime.project.start_init("docs", folder(work, "docs", { "index.md": "# docs\n", "src/a.md": "a\n" }), { store: "team" }));
    await use(await local("shop"), async (c) => {
      await c.branch.fork("feature", "main");
      await c.fs.write("feature", "/src/cart.js", "export const cart = [1];\n", "one");
      await c.fs.write("feature", "/src/deep/new.txt", text(500, 3), "two");
      await c.branch.fork("other", "feature");
      await c.fs.write("other", "/README.md", "other\n", "three");
    });
    await use(await local("docs"), (c) => c.branch.fork("draft", "main").then(() => c.fs.write("draft", "/index.md", "# draft\n", "draft")));
    const files = {
      shop: [["main", "/README.md"], ["feature", "/src/cart.js"], ["main", "/src/deep/chunked.txt"], ["feature", "/src/deep/new.txt"], ["other", "/README.md"], ["main", "/src/deep/large.txt"]],
      docs: [["main", "/index.md"], ["draft", "/index.md"], ["main", "/src/a.md"]],
    };
    const dirs = { shop: ["/", "/src", "/src/deep", "/wide", "/wide/d0", "/wide/d39"], docs: ["/", "/src"] };

    step("publish the Store to MinIO (VACUUM INTO snapshot)");
    await runtime.remote.add({ name: "minio", provider: "minio", endpoint: s3.endpoint, bucket: "layerfs", prefix: "lfc-test-view" });
    const credentials = { accessKey: s3.accessKey, secretKey: s3.secretKey };
    const published = await ended(await runtime.store.start_publish("team", { remote: "minio", credentials }));
    console.log(`  pushed ${published.result.transfer.bytes} bytes`);

    step("open a view: one HEAD, then the reads that open the Store");
    await assert.rejects(runtime.remote.open_view("minio", { file: "absent", credentials }), { code: "NoSuchFile" });
    await assert.rejects(runtime.remote.open_view("minio", { file: "team", credentials: { ...credentials, secretKey: "wrong" } }), { code: "Denied" });
    const view = await runtime.remote.open_view("minio", { file: "team", credentials });
    assert.match(view.storePath, /^remote:minio\/team#[0-9a-f]{24}$/);
    assert.equal(view.bytes, published.result.transfer.bytes);
    assert.equal(view.sha256, published.result.transfer.sha256);
    assert.equal(view.heads, published.result.transfer.heads);
    assert.deepEqual([...view.projects].sort(), ["docs", "shop"]);
    assert.deepEqual((await runtime.project.list({ view: view.view })).map((p) => p.name).sort(), ["docs", "shop"]);
    await shown(view, "opened");

    step("every tree, size and history read through the view equals the local Store's");
    for (const name of ["shop", "docs"]) {
      const there = await reads(viewed(view, name), files[name], dirs[name]);
      await shown(view, `${name} read`);
      const here = await reads(await local(name), files[name], dirs[name]);
      assert.deepEqual(there, here, `${name}: the view answers as the local Store`);
      console.log(`  ${name}: ${there.branches.length} Branches, ${Object.values(there.commits).flat().length} Commits, ${Object.values(there.listings).flat().length} listed entries, ${Object.keys(there.stats).length} stats and revision lists, ${there.globbed.length} globbed: identical`);
    }
    const state = await runtime.project.get_state("shop", undefined, { view: view.view });
    const localState = await runtime.project.get_state("shop", undefined, { store: "team" });
    assert.deepEqual([state.branches, state.commits, state.layers], [localState.branches, localState.commits, localState.layers]);
    assert.deepEqual(state.sandboxes, []);

    step("the view reads no file contents: text, grep, blame and diffs are refused Unsupported");
    await use(viewed(view, "shop"), async (c) => {
      const before = await runtime.remote.view_stats(view.view);
      await assert.rejects(c.fs.read("main", "/README.md"), { code: "Unsupported" });
      await assert.rejects(c.fs.grep("main", "cart"), { code: "Unsupported" });
      await assert.rejects(c.fs.blame("feature", "/src/cart.js"), { code: "Unsupported" });
      const commits = (await c.branch.list_commits("feature")).items;
      await assert.rejects(c.branch.diff_commits(commits.at(-1).id, commits[0].id), { code: "Unsupported" });
      await assert.rejects(c.branch.diff_branches("main", "feature"), { code: "Unsupported" });
      assert.equal((await runtime.remote.view_stats(view.view)).requests, before.requests, "a refused read sends nothing");
    });

    step("the view is read-only: writes, Workspaces and Sandboxes are refused");
    await use(viewed(view, "shop"), async (c) => {
      await assert.rejects(c.fs.write("main", "/x.txt", "x\n"), { code: "ReadOnly" });
      await assert.rejects(c.branch.fork("nope", "main"), { code: "ReadOnly" });
      await assert.rejects(c.sandbox.create(), { code: "ReadOnly" });
      await assert.rejects(c.workspace.mount("1", "main"), { code: "ReadOnly" });
    });
    assert.deepEqual((await runtime.store.list()).map((s) => s.name), ["team"], "a view is no Store of the home");

    step("a push over the file mid-view: an uncached read fails Changed");
    const second = await runtime.remote.open_view("minio", { file: "team", credentials });
    await use(await local("shop"), (c) => c.fs.write("feature", "/late.txt", "after the view opened\n", "late"));
    await ended(await runtime.store.start_push("team", { credentials }));
    await use(viewed(second, "shop"), async (c) => {
      // this view has read only what its open needed: tree reads soon reach an uncached block
      let failed = null, served = 0;
      for (let d = 0; d < 40 && !failed; d++) {
        try {
          await c.fs.list("main", `/wide/d${d}`);
          await c.fs.stat("main", `/wide/d${d}/f49.txt`);
          served++;
        } catch (error) {
          failed = error;
        }
      }
      assert.ok(failed, `every tree read was cached (${served} folders served)`);
      assert.equal(failed.code, "Changed", failed.message);
      console.log(`  ${served} folders read from cache, then Changed`);
      // the failed read quarantined the view's session: later reads answer the same kind
      await assert.rejects(c.fs.list("main", "/wide/d39"), { code: "Changed" });
      await assert.rejects(c.branch.list_commits("feature"), { code: "Changed" });
    });
    const after = await shown(second, "after Changed");
    const again = await runtime.remote.view_stats(second.view);
    assert.equal(again.requests, after.requests, "no request after Changed");
    const fresh = await runtime.remote.open_view("minio", { file: "team", credentials });
    await use(viewed(fresh, "shop"), async (c) => assert.equal((await c.fs.stat("feature", "/late.txt")).size, "after the view opened\n".length));

    step("close: the view and its clients go");
    for (const v of [view, second, fresh]) assert.deepEqual(await runtime.remote.close_view(v.view), { closed: true });
    await assert.rejects(runtime.remote.view_stats(view.view), { code: "NoSuchView" });
    await runtime.store.remove("team", { confirm: "team" });
    console.log("MAIN ok (linux)");
  } finally {
    await runtime.close();
  }
}

const [mode, bundle, image] = process.argv.slice(2);
if (mode === "inner") {
  const s3 = JSON.parse(process.env.LAYERFS_TEST_S3);
  const root = mkdtempSync(join(tmpdir(), "layerfs-view-inner-"));
  const home = join(root, "home"), work = join(root, "work");
  mkdirSync(home, { mode: 0o700 });
  mkdirSync(work);
  try {
    await run({ bundle, image, home, prefix: process.env.LAYERFS_TEST_PREFIX, work, s3 });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
} else if (mode === "linux") {
  assert.ok(bundle && /^sha256:[0-9a-f]{64}$/.test(image ?? ""), "usage: linux <bundle> <image digest>");
  const prefix = `lfc-test-view${Date.now().toString(36)}-`;
  const root = mkdtempSync(join(tmpdir(), "layerfs-view-real-"));
  chmodSync(root, 0o755);
  const s3 = minio(prefix);
  let status = 1;
  try {
    const script = fileURLToPath(import.meta.url);
    const repo = join(script, "../../../../..");
    const r = spawnSync("docker", ["run", "--rm", "--name", `${prefix}client`, "-v", "/var/run/docker.sock:/var/run/docker.sock",
      "-v", "/var/lib/docker/volumes:/var/lib/docker/volumes", "-v", `${root}:${root}`, "-v", `${bundle}:${bundle}:ro`, "-v", `${repo}:${repo}:ro`,
      "-v", `${join(script, "../docker-shim")}:/opt/docker-shim:ro`, "-e", "PATH=/opt/docker-shim:/usr/local/bin:/usr/bin:/bin",
      "-e", `LAYERFS_TEST_S3=${JSON.stringify(s3)}`, "-e", `LAYERFS_TEST_PREFIX=${prefix}`, "-e", `LAYERFS_NATIVE=${join(bundle, "layerfs.node")}`, "-e", `LAYERFS_DAEMON_HOST=${HOST}`, "-e", "LAYERFS_CONSTRUCTION_WORKERS=1",
      "-w", join(repo, "api"), "--entrypoint", "node", image, script, "inner", bundle, image], { encoding: "utf8", stdio: "inherit", timeout: 110_000 });
    status = r.status ?? 1;
  } finally {
    const left = must(["ps", "-aq", "--filter", `name=^${prefix}`]).split("\n").filter(Boolean);
    if (left.length) docker(["rm", "-f", "-v", ...left]);
    const volumes = must(["volume", "ls", "-q", "--filter", "name=^" + prefix]).split("\n").filter(Boolean);
    if (volumes.length) docker(["volume", "rm", ...volumes]);
    spawnSync("rm", ["-rf", root]);
  }
  process.exit(status);
} else {
  throw new Error("select linux or inner");
}

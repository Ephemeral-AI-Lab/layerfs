// The Store-first calls end to end (#506 c), with the real binding: one
// Store holds two projects beside a second Store and a project's own Store;
// a Sandbox mounts a project of the shared Store; the shared Store publishes
// to MinIO (#509: linked to one remote file), sends a copy to a second file
// that loads into a fresh Store with identical history, pushes through its
// link after the send (the wrong-conflict case of links.md section 2), and
// pulls back over a later Commit; unlink, publish elsewhere and remote
// remove InUse; it imports into PostgreSQL with the same Commit ids;
// a Sandbox on the PostgreSQL Store mounts, runs commands and Commits into
// the schema (#506 d), and the PostgreSQL Store attaches again, also by its
// schema alone. One
// project of a shared Store, and the project a Store was made by, are
// removed while the Store and its other projects stay. The Connect Storage
// asks (#506 E1-E9): Store sizes and heads in store.list, heads that move
// with a Commit and equal the pushed file's, store.check against MinIO,
// remote.list_stores, remote.providers, push progress phases sampled while
// a larger Store goes up, and postgresql.status.
//
//   node api/runtime/test/verify/stores_real.mjs macos <bundle> <image digest>
//       on a Mac: the runtime's worker container makes every Store call
//   node api/runtime/test/verify/stores_real.mjs linux <bundle> <image digest>
//       on a Mac or Linux Docker host: runs `inner` in the runtime image,
//       where the runtime makes Store calls in its own process
//
// It starts its own MinIO and PostgreSQL (removed after), and a private home
// with its own container prefix, so no other home's worker or Store is
// touched. One attempt per step; nothing is retried.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
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
const POSTGRES = "postgres:17-bookworm";
/** How containers reach a port the Docker host published on its loopback. */
const HOST = "host.docker.internal";

/** MinIO and PostgreSQL for this run: `{ minio: { endpoint, accessKey, secretKey }, url, names }`. */
function services(prefix) {
  const access = `lfs${Date.now().toString(36)}`, secret = `s${Math.random().toString(36).slice(2)}${Date.now()}`;
  const minio = `${prefix}minio`, pg = `${prefix}pg`;
  must(["run", "-d", "--pull", "never", "--name", minio, "-p", "127.0.0.1::9000", "-e", `MINIO_ROOT_USER=${access}`, "-e", `MINIO_ROOT_PASSWORD=${secret}`, MINIO, "server", "/data"]);
  must(["run", "-d", "--pull", "never", "--name", pg, "-p", "127.0.0.1::5432", "-e", "POSTGRES_PASSWORD=lfspg", "-e", "POSTGRES_USER=lfs", "-e", "POSTGRES_DB=layerfs", POSTGRES]);
  const port = (name, inner) => Number(/:(\d+)$/m.exec(must(["port", name, inner]))[1]);
  // bounded readiness waits, not retries of a product request
  for (let i = 0; docker(["exec", minio, "mc", "alias", "set", "local", "http://127.0.0.1:9000", access, secret]).status !== 0; i++) {
    assert.ok(i < 60, "MinIO not ready in 30 s");
    spawnSync("sleep", ["0.5"]);
  }
  must(["exec", minio, "mc", "mb", "local/layerfs"]);
  for (let i = 0; docker(["exec", pg, "pg_isready", "-U", "lfs", "-d", "layerfs"]).status !== 0; i++) {
    assert.ok(i < 60, "PostgreSQL not ready in 30 s");
    spawnSync("sleep", ["0.5"]);
  }
  spawnSync("sleep", ["1"]);
  return {
    minio: { endpoint: `http://${HOST}:${port(minio, "9000/tcp")}`, accessKey: access, secretKey: secret },
    url: `postgresql://lfs:lfspg@${HOST}:${port(pg, "5432/tcp")}/layerfs?sslmode=disable`,
    names: [minio, pg],
  };
}

/** A source folder for one project. */
function folder(root, name, files) {
  const dir = join(root, `src-${name}`);
  for (const [path, body] of Object.entries(files)) {
    mkdirSync(join(dir, path, ".."), { recursive: true });
    writeFileSync(join(dir, path), body);
  }
  return dir;
}

async function run({ platform, bundle, image, home, prefix, work, svc }) {
  const { LayerFSRuntime } = await import("@layerfs/runtime");
  const runtime = await LayerFSRuntime.open({ home, image, bundle, platform, containerPrefix: prefix, sweep: false, ...(platform === "linux" ? { daemon: join(bundle, "layerfs-daemon") } : {}) });
  const ended = async (job) => {
    for (let i = 0; i < 1200; i++) {
      const now = (await runtime.sync.get(job.id)) ?? (await runtime.project.get_init(job.id));
      if (now?.ended) {
        assert.equal(now.step, "done", `${job.kind ?? "init"} ${job.store ?? job.name}: ${JSON.stringify(now.error)}`);
        return now;
      }
      await new Promise((next) => setTimeout(next, 100));
    }
    throw new Error(`job ${job.id} did not end in 120 s`);
  };
  const project = async (name, store) => (await runtime.project.list()).find((p) => p.name === name && p.store === store);
  const use = async (name, store, work) => runtime.clients.use(await project(name, store), work);
  /** A project's Branches and each Branch's Commit ids: its history. */
  const history = (name, store) => use(name, store, async (client) => {
    const branches = (await client.project.list_branches()).items;
    const commits = {};
    // main is the line of Layers, with no Commits of its own
    for (const b of branches.filter((b) => b.name !== "main")) commits[b.name] = (await client.branch.list_commits(b.name)).items.map((c) => c.id ?? c.commit ?? c);
    return { branches: branches.map(({ name: n, base, head }) => ({ name: n, base, head })), commits };
  });
  const step = (text) => console.log(`STEP ${text}`);
  try {
    step("a Store holds two projects, beside a second Store and a project's own Store");
    assert.equal((await runtime.store.create("team")).name, "team");
    await ended(await runtime.project.start_init("shop", folder(work, "shop", { "src/cart.js": "export const cart = [];\n", "README.md": "shop\n" }), { store: "team" }));
    await ended(await runtime.project.start_init("docs", folder(work, "docs", { "index.md": "# docs\n" }), { store: "team" }));
    await runtime.store.create("scratch");
    await ended(await runtime.project.start_init("solo", folder(work, "solo", { "a.txt": "solo\n" })));
    await assert.rejects(runtime.project.start_init("shop", folder(work, "again", { "x": "x" }), { store: "team" }), { code: "Exists" });
    const listed = Object.fromEntries((await runtime.store.list()).map((s) => [s.name, s.projects]));
    assert.deepEqual(listed, { scratch: [], solo: ["solo"], team: ["shop", "docs"] });
    await use("shop", "team", async (c) => assert.equal((await c.fs.read("main", "/src/cart.js")).text, "export const cart = [];\n"));
    await use("docs", "team", async (c) => assert.equal((await c.fs.read("main", "/index.md")).text, "# docs\n"));

    step("a Sandbox mounts a project of the shared Store and commits");
    await use("shop", "team", async (c) => {
      await c.branch.fork("feature", "main");
      console.log("  forked feature");
      const box = await c.sandbox.create();
      console.log(`  Sandbox ${box.sandbox}`);
      const named = must(["ps", "--filter", "label=layerfs.store=team", "--filter", "label=layerfs.project=shop", "--format", "{{.Names}}"]);
      assert.match(named, new RegExp(`^${prefix}team\\.shop-${box.sandbox}$`));
      const ws = await c.workspace.mount(box.sandbox, "feature");
      console.log(`  mounted ${ws.path}`);
      assert.equal((await c.workspace.exec(ws, "cat src/cart.js")).stdout, "export const cart = [];\n");
      assert.equal((await c.workspace.exec(ws, "echo '// paid' >> src/cart.js")).exitCode, 0);
      assert.equal((await c.workspace.commit(ws, "pay")).kind, "Committed");
      await c.workspace.unmount(ws);
      await c.sandbox.close(box.sandbox);
    });
    step("store.list: sizes, open state and heads (#506 E1, E2)");
    const teamNow = async () => (await runtime.store.list()).find((s) => s.name === "team");
    const before = await teamNow();
    assert.ok(before.bytes.file > 0 && Number.isInteger(before.bytes.wal), JSON.stringify(before.bytes));
    assert.equal(typeof before.open, "boolean");
    assert.match(before.heads, /^[0-9a-f]{64}$/);
    assert.equal((await teamNow()).heads, before.heads, "unchanged history, unchanged heads");
    await use("docs", "team", async (c) => { await c.branch.fork("draft", "main"); await c.fs.write("draft", "/new.md", "new\n"); });
    const afterCommit = await teamNow();
    assert.notEqual(afterCommit.heads, before.heads, "a fork and a Commit change the heads");
    console.log(`  team ${afterCommit.bytes.file} + ${afterCommit.bytes.wal} WAL bytes, open ${afterCommit.open}`);
    const team = { shop: await history("shop", "team"), docs: await history("docs", "team") };
    assert.ok(team.shop.commits.feature.length >= 1);

    step("publish the whole Store to MinIO: a new file, linked (#509)");
    const remote = await runtime.remote.add({ name: "minio-a", provider: "minio", endpoint: svc.minio.endpoint, bucket: "layerfs", prefix: "lfc" });
    assert.equal(remote.credentials, "env:LAYERFS_REMOTE");
    await runtime.remote.add({ name: "minio-b", provider: "minio", endpoint: svc.minio.endpoint, bucket: "layerfs", prefix: "lfc-b" });
    const credentials = { accessKey: svc.minio.accessKey, secretKey: svc.minio.secretKey };
    await assert.rejects(runtime.store.start_push("team", { credentials }), { code: "NotLinked" });
    const pushed = await ended(await runtime.store.start_publish("team", { remote: "minio-a", credentials }));
    assert.equal(pushed.result.object, "team/store.sqlite");
    assert.deepEqual([pushed.result.transfer.remote, pushed.result.transfer.file], ["minio-a", "team"]);

    step("after the publish: the link's heads, store.check and list_stores against MinIO (#506 E2, E3, E9)");
    const recorded = (await teamNow()).link.last;
    assert.deepEqual([(await teamNow()).link.remote, (await teamNow()).link.file, recorded.direction], ["minio-a", "team", "push"]);
    assert.equal(recorded.heads, pushed.result.transfer.heads);
    assert.ok(recorded.at > 1e12, "times in ms");
    assert.equal((await teamNow()).heads, recorded.heads, "the pushed snapshot has the Store's heads");
    const checked = await runtime.store.check("team", { credentials });
    assert.deepEqual([checked.file, checked.present, checked.bytes, checked.sha256, checked.heads], ["team", true, pushed.result.transfer.bytes, pushed.result.transfer.sha256, recorded.heads]);
    assert.ok(checked.modified > 0 && checked.checkedAt >= checked.modified - 120_000, JSON.stringify(checked));
    await assert.rejects(runtime.store.check("team", { credentials: { accessKey: credentials.accessKey, secretKey: "wrong" } }), { code: "Denied" });
    const providers = await runtime.remote.providers();
    assert.deepEqual(providers.find((p) => p.id === "minio").available, true);
    console.log(`  providers ${providers.map((p) => `${p.id}:${p.available}`).join(" ")}`);

    step("send a copy to a new file, then push through the link: no wrong conflict (links.md section 2)");
    await assert.rejects(runtime.store.start_send("team", { remote: "minio-a", file: "team", credentials }), { code: "Exists" });
    const sent = await ended(await runtime.store.start_send("team", { remote: "minio-a", file: "team-snap", credentials }));
    assert.equal(sent.step, "done", JSON.stringify(sent.error));
    assert.deepEqual([sent.result.remote, sent.result.file, sent.result.sha256], ["minio-a", "team-snap", pushed.result.transfer.sha256]);
    assert.deepEqual((await teamNow()).link.last, recorded, "a send changes no record of the Store");
    const relinked = await ended(await runtime.store.start_push("team", { credentials }));
    assert.equal(relinked.step, "done", JSON.stringify(relinked.error));
    const onRemote = await runtime.remote.list_stores("minio-a", { credentials });
    assert.deepEqual(onRemote.map((c) => [c.file, c.linkedBy]), [["team", "team"], ["team-snap", null]]);

    step("load the sent file into a fresh Store, linked to it");
    await assert.rejects(runtime.store.start_load("minio-a", { file: "team", store: "other", credentials }), { code: "Linked", message: /team/ });
    const cloned = await ended(await runtime.store.start_load("minio-a", { file: "team-snap", store: "copy", credentials }));
    assert.equal(cloned.step, "done", JSON.stringify(cloned.error));
    assert.deepEqual(cloned.result.projects.sort(), ["docs", "shop"]);
    assert.equal(cloned.result.transfer.sha256, pushed.result.transfer.sha256);
    assert.deepEqual([(await runtime.store.list()).find((s) => s.name === "copy").link.file], ["team-snap"]);
    for (const name of ["shop", "docs"]) assert.deepEqual(await history(name, "copy"), team[name], `${name}: the loaded Store's history is the pushed Store's`);
    await use("shop", "copy", async (c) => assert.equal((await c.fs.read("feature", "/src/cart.js")).text, "export const cart = [];\n// paid\n"));
    const linkedCopy = await runtime.store.check("copy", { credentials });
    assert.deepEqual([linkedCopy.file, linkedCopy.present], ["team-snap", true], "a loaded Store checks the file it came from");

    step("a Commit after the push is replaced by a pull through the link");
    await use("shop", "team", (c) => c.fs.write("feature", "/late.txt", "after the push\n"));
    assert.notDeepEqual(await history("shop", "team"), team.shop);
    await assert.rejects(runtime.store.start_pull("team", { credentials }), { code: "InvalidInput" });
    await ended(await runtime.store.start_pull("team", { credentials, confirm: "team" }));
    assert.deepEqual(await history("shop", "team"), team.shop, "the pull put back the pushed history");
    const link = (await runtime.store.list()).find((s) => s.name === "team").link;
    assert.deepEqual([link.remote, link.file, link.last.direction, link.last.sha256], ["minio-a", "team", "pull", relinked.result.transfer.sha256]);
    // the pulled file has the pushed history: its heads are the push's, and the Store's now
    assert.equal(link.last.heads, recorded.heads);
    assert.equal((await teamNow()).heads, recorded.heads);

    step("unlink, publish elsewhere; a connection a Store links to is not removed");
    await assert.rejects(runtime.store.unlink("copy", {}), { code: "InvalidInput" });
    assert.equal((await runtime.store.unlink("copy", { confirm: "copy" })).file, "team-snap");
    await assert.rejects(runtime.store.check("copy", { credentials }), { code: "NotLinked" });
    // the unlinked file is still there: a publish onto it is refused by its HEAD, nothing overwritten
    const onto = await ended(await runtime.store.start_publish("copy", { remote: "minio-a", file: "team-snap", credentials }));
    assert.deepEqual([onto.step, onto.error?.code], ["failed", "Exists"], JSON.stringify(onto.error));
    const elsewhere = await ended(await runtime.store.start_publish("copy", { remote: "minio-b", file: "copy", credentials }));
    assert.equal(elsewhere.step, "done", JSON.stringify(elsewhere.error));
    await assert.rejects(runtime.remote.remove("minio-b", { confirm: "minio-b" }), { code: "InUse", message: /copy/ });
    assert.deepEqual((await runtime.remote.list_stores("minio-a", { credentials })).map((c) => [c.file, c.linkedBy]), [["team", "team"], ["team-snap", null]], "the unlinked file stays, loadable");

    step("a larger Store's push reports its phases while it runs (#506 E4)");
    await runtime.store.create("big");
    const bulk = join(work, "src-big");
    mkdirSync(bulk, { recursive: true });
    // incompressible bytes, so the Store holds them whole
    const { randomBytes } = await import("node:crypto");
    for (let i = 0; i < 6; i++) writeFileSync(join(bulk, `blob-${i}.bin`), randomBytes(16 * 1024 * 1024));
    await ended(await runtime.project.start_init("bulk", bulk, { store: "big" }));
    const big = await runtime.store.start_publish("big", { remote: "minio-a", credentials });
    const samples = [];
    for (let i = 0; i < 6000; i++) {
      const now = await runtime.sync.get(big.id);
      if (now.ended) { assert.equal(now.step, "done", JSON.stringify(now.error)); break; }
      samples.push([now.phase, now.bytes, now.total]);
      await new Promise((next) => setTimeout(next, 20));
    }
    const phases = [...new Set(samples.map(([phase]) => phase))];
    console.log(`  ${samples.length} samples, phases ${phases.join(" ")}`);
    assert.ok(phases.some((p) => ["snapshot", "hash", "upload"].includes(p)), `no byte phase sampled: ${phases}`);
    assert.ok(samples.every(([phase]) => [null, "check", "snapshot", "hash", "upload", "record"].includes(phase)), JSON.stringify(phases));
    assert.ok(samples.some(([phase, bytes, total]) => ["snapshot", "hash", "upload"].includes(phase) && Number.isInteger(bytes) && total >= 96 * 1024 * 1024), "a byte phase with its total");
    await runtime.store.remove("big", { confirm: "big" });

    step("import the shared Store into PostgreSQL: the same Commit ids");
    const imported = await ended(await runtime.store.start_import_postgresql("team", { name: "team-pg", connection: "pg-local", url: svc.url, schema: "lfc_team" }));
    assert.deepEqual(imported.result.created.projects.sort(), ["docs", "shop"]);
    assert.ok(!readFileSync(join(home, "stores.json"), "utf8").includes("lfspg"), "no PostgreSQL password in stores.json");
    assert.equal((await runtime.store.list()).find((s) => s.name === "team-pg").importedFrom.store, "team", "#506 E6");

    step("postgresql.status: version, latency and schema sizes on one connection (#506 E5, E7)");
    const pgStatus = await runtime.postgresql.status({ url: svc.url, schemas: ["lfc_team", "lfc_none"] });
    assert.equal(pgStatus.reachable, true);
    assert.match(pgStatus.serverVersion, /^17\./);
    assert.ok(pgStatus.schemas.lfc_team > 0 && pgStatus.schemas.lfc_none === null, JSON.stringify(pgStatus.schemas));
    console.log(`  PostgreSQL ${pgStatus.serverVersion}, ${pgStatus.latencyUs} us, lfc_team ${pgStatus.schemas.lfc_team} bytes`);
    const refusedPg = await runtime.postgresql.status({ url: svc.url.replace("lfspg", "wrong"), schemas: [] });
    assert.deepEqual([refusedPg.reachable, refusedPg.error.code], [false, "Denied"]);
    const { LayerFS } = await import("@layerfs/agent");
    const env = { LAYERFS_HOME: home, LAYERFS_STORE: "team-pg", LAYERFS_STORE_PATH: "postgresql:team-pg", LAYERFS_STORE_POSTGRESQL: JSON.stringify({ url: svc.url }) };
    const saved = Object.fromEntries(Object.keys(env).map((k) => [k, process.env[k]]));
    Object.assign(process.env, env);
    let pg;
    try { pg = await LayerFS.open("shop"); } finally { for (const [k, v] of Object.entries(saved)) if (v === undefined) delete process.env[k]; else process.env[k] = v; }
    try {
      const branches = (await pg.project.list_branches()).items.map(({ name: n, base, head }) => ({ name: n, base, head }));
      assert.deepEqual(branches, team.shop.branches);
      assert.deepEqual((await pg.branch.list_commits("feature")).items.map((c) => c.id ?? c.commit ?? c), team.shop.commits.feature);
      assert.equal((await pg.fs.read("feature", "/src/cart.js")).text, "export const cart = [];\n// paid\n");
    } finally {
      await pg.close();
    }

    step("a Sandbox on the PostgreSQL Store mounts, runs a command and Commits into the schema");
    const pgShop = await project("shop", "team-pg");
    await assert.rejects(runtime.project.get_state("shop", undefined, { store: "team-pg" }), { code: "InvalidInput" });
    await runtime.clients.use({ ...pgShop, url: svc.url }, async (c) => {
      const box = await c.sandbox.create();
      const named = must(["ps", "--filter", "label=layerfs.store=team-pg", "--filter", "label=layerfs.project=shop", "--format", "{{.Names}}"]);
      assert.match(named, new RegExp(`^${prefix}team-pg\\.shop-${box.sandbox}$`));
      const ws = await c.workspace.mount(box.sandbox, "feature");
      console.log(`  Sandbox ${box.sandbox}, mounted ${ws.path}`);
      assert.equal((await c.workspace.exec(ws, "cat src/cart.js")).stdout, "export const cart = [];\n// paid\n");
      // no Store volume, and the daemon's config (with the URL) is out of a command's reach
      const peek = await c.workspace.exec(ws, "grep -c ' /layerfs-store' /proc/mounts; cat /layerfs-local/config/daemon.setup; grep -l lfspg /proc/*/environ /proc/*/cmdline; echo end");
      assert.match(peek.stdout, /^0\n/);
      assert.ok(!`${peek.stdout}${peek.stderr}`.includes("lfspg"), "a command read the PostgreSQL password");
      assert.equal((await c.workspace.exec(ws, "echo '// shipped' >> src/cart.js")).exitCode, 0);
      const begun = Date.now();
      assert.equal((await c.workspace.commit(ws, "ship")).kind, "Committed");
      console.log(`  Commit ${Date.now() - begun} ms (diagnostic, not a measurement)`);
      await c.workspace.unmount(ws);
      const state = await runtime.project.get_state("shop", undefined, { store: "team-pg", url: svc.url });
      assert.deepEqual(state.sandboxes.map((s) => s.id), [box.sandbox]);
      await c.sandbox.close(box.sandbox);
    });
    await runtime.clients.use({ ...pgShop, url: svc.url }, async (c) => {
      const commits = (await c.branch.list_commits("feature")).items.map((x) => x.id ?? x.commit ?? x);
      assert.equal(commits.length, team.shop.commits.feature.length + 1);
      assert.deepEqual(commits.filter((id) => team.shop.commits.feature.includes(id)), team.shop.commits.feature);
      assert.equal((await c.fs.read("feature", "/src/cart.js")).text, "export const cart = [];\n// paid\n// shipped\n");
    });
    const attached =await runtime.store.attach_postgresql({ name: "team-pg", connection: "pg-local", url: svc.url, schema: "lfc_team", synchronousCommit: "local" });
    assert.deepEqual([attached.kind, attached.synchronousCommit, attached.projects.sort()], ["postgresql", "local", ["docs", "shop"]]);

    step("a schema this home forgot attaches again by the binding it records");
    await runtime.store.remove("team-pg", { confirm: "team-pg" });
    const again = await runtime.store.attach_postgresql({ name: "team-pg2", connection: "pg-local", url: svc.url, schema: "lfc_team" });
    assert.deepEqual(again.projects.sort(), ["docs", "shop"]);
    await runtime.project.delete("docs", { store: "team-pg2", confirm: "docs", url: svc.url });
    assert.deepEqual((await runtime.store.list()).find((s) => s.name === "team-pg2").projects, ["shop"]);

    step("remove one project of a shared Store, and the project a Store was made by");
    const removed = await runtime.project.delete("docs", { store: "team", confirm: "docs" });
    assert.deepEqual([removed.kind, removed.store, removed.packsKept], ["Removed", "team", true]);
    assert.deepEqual((await runtime.store.list()).find((s) => s.name === "team").projects, ["shop"]);
    assert.deepEqual(await history("shop", "team"), team.shop, "the other project stays whole");
    await ended(await runtime.project.start_init("extra", folder(work, "extra", { "e.txt": "e\n" }), { store: "solo" }));
    await runtime.project.delete("solo", { store: "solo", confirm: "solo" });
    const solo = (await runtime.store.list()).find((s) => s.name === "solo");
    assert.deepEqual([solo.legacy, solo.projects], [false, ["extra"]]);
    await use("extra", "solo", async (c) => assert.equal((await c.fs.read("main", "/e.txt")).text, "e\n"));
    for (const name of ["copy", "team", "scratch", "solo", "team-pg2"]) await runtime.store.remove(name, { confirm: name });
    assert.deepEqual((await runtime.store.list()).map((s) => s.name), []);
    console.log(`MAIN ok (${platform})`);
  } finally {
    await runtime.close();
  }
}

const [mode, bundle, image] = process.argv.slice(2);
if (mode === "inner") {
  // in the runtime image: the runtime makes Store calls in this process
  // this process (root here) owns its own home and sources
  const svc = JSON.parse(process.env.LAYERFS_TEST_SERVICES);
  const root = mkdtempSync(join(tmpdir(), "layerfs-stores-inner-"));
  const home = join(root, "home"), work = join(root, "work");
  mkdirSync(home, { mode: 0o700 });
  mkdirSync(work);
  try {
    await run({ platform: "linux", bundle, image, home, prefix: process.env.LAYERFS_TEST_PREFIX, work, svc });
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
} else if (mode === "macos" || mode === "linux") {
  assert.ok(bundle && /^sha256:[0-9a-f]{64}$/.test(image ?? ""), "usage: macos|linux <bundle> <image digest>");
  const stamp = Date.now().toString(36);
  const prefix = `lfs506c${stamp}-`;
  const root = mkdtempSync(join(tmpdir(), "layerfs-stores-real-"));
  const home = join(root, "home"), work = join(root, "work");
  mkdirSync(home, { mode: 0o700 });
  chmodSync(home, 0o700);
  mkdirSync(work);
  const svc = services(prefix);
  let status = 1;
  try {
    if (mode === "macos") {
      await run({ platform: "macos", bundle, image, home, prefix, work, svc });
      status = 0;
    } else {
      const script = fileURLToPath(import.meta.url);
      const repo = join(script, "../../../../..");
      const r = spawnSync("docker", ["run", "--rm", "--name", `${prefix}client`, "-v", "/var/run/docker.sock:/var/run/docker.sock",
        "-v", "/var/lib/docker/volumes:/var/lib/docker/volumes", "-v", `${root}:${root}`, "-v", `${bundle}:${bundle}:ro`, "-v", `${repo}:${repo}:ro`,
        "-v", `${join(script, "../docker-shim")}:/opt/docker-shim:ro`, "-e", "PATH=/opt/docker-shim:/usr/local/bin:/usr/bin:/bin",
        "-e", `LAYERFS_TEST_SERVICES=${JSON.stringify(svc)}`, "-e", `LAYERFS_TEST_PREFIX=${prefix}`,
        "-e", `LAYERFS_NATIVE=${join(bundle, "layerfs.node")}`, "-e", `LAYERFS_DAEMON_HOST=${HOST}`, "-e", "LAYERFS_CONSTRUCTION_WORKERS=1",
        "-w", join(repo, "api"), "--entrypoint", "node", image, script, "inner", bundle, image], { encoding: "utf8", stdio: "inherit", timeout: 110_000 });
      status = r.status ?? 1;
    }
  } finally {
    const left = must(["ps", "-aq", "--filter", `name=^${prefix}`]).split("\n").filter(Boolean);
    if (left.length) docker(["rm", "-f", "-v", ...left]);
    const volumes = must(["volume", "ls", "-q", "--filter", "name=^" + prefix]).split("\n").filter(Boolean);
    if (volumes.length) docker(["volume", "rm", ...volumes]);
    spawnSync("rm", ["-rf", root]);
  }
  process.exit(status);
} else {
  throw new Error("select macos, linux or inner");
}

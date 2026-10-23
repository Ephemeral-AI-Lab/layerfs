// The ported SDK of Live projects on macOS (src/port/, src/box.js), with real
// Docker and a sealed Linux LayerFS build:
//   node packages/host/tests/port_real.mjs <bundle folder> <image digest>
// A plain Mac script uses the ported SDK as an agent would: Init (which
// installs the Store, starting no Sandbox), Store calls (fork, write, read,
// list, publish), Workspace calls (mount, which starts the project's
// Sandbox 1, exec, commit, unmount), a refusal and an exec whose output is
// spooled; then the Store container is stopped, and later removed, and a
// call's own failure starts it again (the Host's self-heal). Each SDK call
// goes to the Store container's long-lived worker,
// which keeps the project's Store open between calls; Init runs in its own
// container. The Store container is `<prefix>store` with this test's own
// prefix, so dsh's `layerfs-store` is never touched; another home with that
// prefix is refused it. It, the volume, the
// containers and the home are removed afterwards.
// Docker is reached at LAYERFS_DOCKER (Docker Desktop's /var/run/docker.sock
// by default; colima's socket, for instance). macOS only; under 2 minutes.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { DAEMON_HOST, ensureBox, storeName, storePathOf } from "../src/box.js";

const [bundle, image] = process.argv.slice(2);
if (process.platform !== "darwin" || !bundle || !image) {
  console.log("usage (macOS): node packages/host/tests/port_real.mjs <bundle folder> <image digest>");
  process.exit(2);
}
const work = mkdtempSync(join(realpathSync(tmpdir()), "lfs-port-"));
const home = join(work, "home"), source = join(work, "shop"), id = Date.now().toString(36);
const volume = `lfs-port-test-${id}`, prefix = `lfs-port-test-${id}-`;
const spec = { docker: process.env.LAYERFS_DOCKER || "/var/run/docker.sock", home, bundle, image, prefix, box: storeName(prefix) };
const docker = (...args) => execFileSync("docker", ["-H", `unix://${spec.docker}`, ...args], { encoding: "utf8" }).trim();
mkdirSync(home, { mode: 0o700 });
mkdirSync(source);
writeFileSync(join(source, "README.md"), "# Shop\n");
Object.assign(process.env, {
  DSH_LAYERFS_STORE_CONTAINER: spec.box, DSH_LAYERFS_BUNDLE: bundle, LAYERFS_HOME: home, LAYERFS_DOCKER: spec.docker,
  LAYERFS_IMAGE: image, LAYERFS_VOLUME: volume, LAYERFS_DAEMON: join(bundle, "layerfs-daemon"),
  LAYERFS_STORE_PATH: storePathOf(volume), LAYERFS_DAEMON_HOST: DAEMON_HOST,
  LAYERFS_CONTAINER_PREFIX: prefix, LAYERFS_CONSTRUCTION_WORKERS: "1",
});

const timed = async (label, work) => {
  const t0 = performance.now();
  const value = await work();
  console.log(`${label.padEnd(26)} ${String(Math.round(performance.now() - t0)).padStart(5)} ms`);
  return value;
};
let layerfs;
try {
  docker("volume", "create", volume);
  await timed("Store container start", () => ensureBox(spec));
  assert.equal(docker("inspect", "-f", '{{index .Config.Labels "layerfs.role"}}', spec.box), "store");
  assert.equal(docker("inspect", "-f", '{{index .Config.Labels "layerfs.home"}}', spec.box), home);
  assert.equal(docker("inspect", "-f", "{{.HostConfig.RestartPolicy.Name}}", spec.box), "unless-stopped");
  // another home with the same prefix is refused; this home's container stays
  const other = await ensureBox({ ...spec, home: join(work, "other") }).catch((e) => e);
  assert.match(String(other?.message), /serves the LayerFS home .*its own containerPrefix/);
  assert.equal(docker("inspect", "-f", "{{.State.Running}}", spec.box), "true");
  const { LayerFS, isRefusal } = await import("../src/port/sdk.js");
  const { LayerFSOperator } = await import("../src/port/operator.js");
  layerfs = new LayerFS("shop");
  const operator = new LayerFSOperator(layerfs);
  // as the Host does: settings exist only while the client is made, so a call
  // must use its client's deployment, never this process's environment
  for (const name of ["IMAGE", "DAEMON_HOST"]) delete process.env[`LAYERFS_${name}`];
  const init = await timed("project.init", () => layerfs.project.init(source));
  assert.equal(init.kind, "Initialized");
  const forked = await timed("branch.fork", () => layerfs.branch.fork("work"));
  assert.equal(forked.branch, "work");
  await timed("fs.write", () => layerfs.fs.write("work", "/hello.js", "console.log('hello, shop');\n", "add hello.js"));
  assert.equal((await timed("fs.read", () => layerfs.fs.read("work", "/hello.js"))).text, "console.log('hello, shop');\n");
  const refused = await timed("fs.read (missing)", () => layerfs.fs.read("work", "/nope.js").catch((e) => e));
  assert.ok(isRefusal(refused, "NoSuchPath"), `a refusal arrives as the SDK's own: ${refused}`);
  assert.equal(docker("ps", "-aq", "--filter", `volume=${volume}`), "", "Init and Store calls start no Sandbox");
  const ws = await timed("workspace.mount", () => layerfs.workspace.mount("work"));
  assert.equal(ws.sandbox, "1", "the project's first Sandbox");
  assert.equal(docker("inspect", "-f", '{{index .Config.Labels "layerfs.sandbox"}}', `${prefix}shop-1`), "1");
  // the operator's Sandbox calls carry no deployment, yet reach the daemon
  const snap = await timed("operator inspect.snapshot", () => operator.inspect.snapshot());
  assert.deepEqual([snap.kind, snap.sandboxes.map((x) => x.name)], ["ok", [`${prefix}shop-1`]]);
  const ran = await timed("workspace.exec", () => layerfs.workspace.exec(ws, "node hello.js | tee hello.txt", "run hello"));
  assert.equal(ran.exitCode, 0);
  assert.equal(ran.stdout, "hello, shop\n");
  const big = await timed("workspace.exec (2 MiB out)", () => layerfs.workspace.exec(ws, "head -c 2097152 /dev/zero | tr '\\0' a"));
  assert.equal(big.stdout.length, 2097152, "a spooled output arrives whole");
  assert.equal((await timed("workspace.commit", () => layerfs.workspace.commit(ws, "step 1: node hello.js (exit 0)"))).kind, "Committed");
  await timed("workspace.unmount", () => layerfs.workspace.unmount(ws));
  assert.equal((await timed("branch.publish", () => layerfs.branch.publish("work"))).kind, "Added");
  const branches = await timed("project.list_branches", () => layerfs.project.list_branches());
  assert.deepEqual(branches.items.map((b) => b.name).sort(), ["main", "work"]);
  // self-heal, as the Host wires it: a call that finds the Store container stopped
  // or missing reports its own error, and the hook starts it again; the next call works
  const { onUnreachable } = await import("../src/port/binding.js");
  let healing = null;
  const unhook = onUnreachable(() => { healing ??= ensureBox(spec); });
  try {
    for (const [what, stop] of [["stopped", () => docker("stop", "-t", "1", spec.box)], ["missing", () => docker("rm", "-f", spec.box)]]) {
      stop();
      healing = null;
      const lost = await layerfs.fs.read("work", "/hello.js").catch((e) => e);
      assert.ok(isRefusal(lost, "Failed") || isRefusal(lost, "Unknown"), `${what}: the call keeps its own answer: ${lost}`);
      assert.ok(healing, `${what}: the call started the heal`);
      await timed(`heal (${what})`, () => healing);
      assert.equal(docker("inspect", "-f", "{{.State.Running}}", spec.box), "true");
      assert.equal((await timed(`fs.read after heal (${what})`, () => layerfs.fs.read("work", "/hello.js"))).text, "console.log('hello, shop');\n");
    }
  } finally {
    unhook();
  }
  console.log("port: ok");
} finally {
  await layerfs?.close().catch(() => {});
  // each step on its own, so one failure leaves the rest done; the box made
  // the home's files, so it removes them
  const step = (label, work) => { try { work(); } catch (error) { console.log(`cleanup ${label}: ${error.message.split("\n")[0]}`); } };
  step("home", () => docker("exec", spec.box, "find", home, "-mindepth", "1", "-delete")); // home is the Store container's mount point
  // the Sandboxes and the Store container (<prefix>store)
  step("containers", () => {
    const left = docker("ps", "-aq", "--filter", `name=^${prefix}`).split("\n").filter(Boolean);
    if (left.length) docker("rm", "-f", ...left);
  });
  step("volume", () => docker("volume", "rm", volume));
  step("work", () => rmSync(work, { recursive: true, force: true }));
}

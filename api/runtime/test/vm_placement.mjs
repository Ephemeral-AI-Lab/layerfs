// @layerfs/runtime's `vm:<provider>` placement (#515 §6, V9) with a stand-in
// provider CLI and the real worker: no microVM, no Store, no daemon. One Linux
// VM per Store (#515 V1): every case names a project, and its VM, Store disk,
// host port, bundle copy and worker secret are that project's alone. The
// fixture keeps one VM record per project, maps each guest's absolute paths
// under a temp folder, and starts the adapter's bundle worker so step ④'s
// readiness is real.
//   node api/runtime/test/vm_placement.mjs settings|flow|two|bundle|delete|hosts|apple-container
import assert from "node:assert/strict";
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const FAKE = fileURLToPath(new URL("../../agent/test/fixture/fake-native.cjs", import.meta.url));
const SOURCE = fileURLToPath(new URL("../src", import.meta.url));
const MSB = fileURLToPath(new URL("fixture/fake-msb.cjs", import.meta.url));
const CONTAINER = fileURLToPath(new URL("fixture/fake-container.cjs", import.meta.url));
const SSH = fileURLToPath(new URL("fixture/fake-ssh.cjs", import.meta.url));
process.env.LAYERFS_NATIVE = FAKE;
const { LayerFSRuntime } = await import("@layerfs/runtime");
const { resolveSettings, missing, callHome, GUEST } = await import("../src/settings.js");
const { deploymentEnv } = await import("../src/clients.js");
const { endpointFile } = await import("../src/platform/shared.js");

const selected = process.argv[2];
const work = mkdtempSync(join(tmpdir(), "layerfs-vm-"));
const home = join(work, "home"), bundle = join(work, "bundle"), prefix = "lfs-vm-test-";
/** A project's VM and Store disk names: one VM per Store. */
const vmName = (project) => `${prefix}${project}-vm`;
const storeName = (project) => `${prefix}${project}-vm-store`;

/** A stand-in provider CLI on PATH: `name` shells out to the fixture with this state folder. */
function standIn(name, fixture, state) {
  const bin = join(work, "bin");
  mkdirSync(bin, { recursive: true });
  const file = join(bin, name);
  writeFileSync(file, `#!/bin/sh\nexec "${process.execPath}" "${fixture}" "$@"\n`);
  chmodSync(file, 0o755);
  const saved = process.env.PATH;
  process.env.PATH = `${bin}:${saved}`;
  const envKey = name === "msb" ? "FAKE_MSB" : name === "container" ? "FAKE_CONTAINER" : "FAKE_SSH";
  process.env[`${envKey}_STATE`] = state;
  process.env[`${envKey}_NATIVE`] = FAKE;
  if (name === "ssh") process.env.LAYERFS_SSH_STATE_DIR = join(state, "sessions");
  return () => {
    process.env.PATH = saved;
    if (name === "ssh") delete process.env.LAYERFS_SSH_STATE_DIR;
  };
}

/** A sealed Linux bundle with the adapter's worker and the real SDK packages it imports.
 * `image` adds a Sandbox image archive at `images/<hex>.tar` (H38's convention, V7). */
function makeBundle(tag = "build-one", image = null) {
  rmSync(bundle, { recursive: true, force: true });
  const api = join(bundle, "api");
  cpSync(join(SOURCE, "worker"), join(api, "runtime/src/worker"), { recursive: true });
  cpSync(join(SOURCE, "../../node_modules/@layerfs"), join(api, "node_modules/@layerfs"), { recursive: true, dereference: true });
  if (image) {
    mkdirSync(join(bundle, "images"), { recursive: true });
    writeFileSync(join(bundle, "images", `${image}.tar`), "an uncompressed docker save archive\n");
  }
  writeFileSync(join(bundle, "manifest.json"), JSON.stringify({ version: 1, status: "sealed", platform: "linux", build_id: tag, files: {} }));
  return bundle;
}

const calls = (state) => (existsSync(join(state, "calls.jsonl")) ? readFileSync(join(state, "calls.jsonl"), "utf8").trim().split("\n").filter(Boolean).map((line) => JSON.parse(line).argv) : []);

/** Every await that could wait on a provider has a ceiling here, so a stuck
 * call fails this case with a name instead of hanging the harness. */
const deadline = (ms, what, promise) =>
  Promise.race([
    promise,
    new Promise((_, reject) => setTimeout(() => reject(new Error(`${what} did not finish within ${ms / 1000} s`)), ms).unref()),
  ]);

// The whole case has a ceiling too: an unref'd watchdog that exits non-zero
// rather than letting CI or a shell wait forever.
setTimeout(() => {
  process.stderr.write(`vm_placement/${selected}: FAIL, the case did not finish in 300 s\n`);
  process.exit(1);
}, 300_000).unref();
/** A guest path inside one project's VM. */
const guestPath = (state, vm, path) => join(state, "guest", vm, path.replace(/^\//, ""));
/** The project is gone from this home: its entry and the runtime's record of it. */
const forget = (name) => {
  rmSync(join(home, name), { recursive: true, force: true });
  rmSync(join(home, ".runtime", "projects", `${name}.json`), { force: true });
};

/** A project of the home: its entry, and the runtime's record of its volume. */
const project = (name) => {
  mkdirSync(join(home, name), { recursive: true });
  writeFileSync(join(home, name, "project.record"), "x");
  mkdirSync(join(home, ".runtime", "projects"), { recursive: true });
  writeFileSync(join(home, ".runtime", "projects", `${name}.json`), JSON.stringify({ name, volume: `lfs-${name}-store-1`, created: 1 }));
};
const endpointAt = (name) => JSON.parse(readFileSync(endpointFile(home, name), "utf8"));

try {
  if (selected === "settings") {
    const local = resolveSettings({ home, platform: "linux", image: `sha256:${"a".repeat(64)}` });
    assert.deepEqual([local.placement.kind, local.placement.name, local.vm ?? null], ["local", "local", null]);
    assert.deepEqual(missing(local), ["daemon"], "a local Linux home needs its daemon only");
    // LAYERFS_PLACEMENT is the V9 name; the settings object wins when both are set
    process.env.LAYERFS_PLACEMENT = "vm:microsandbox";
    const vm = resolveSettings({ home, platform: "linux", bundle: "/b", containerPrefix: prefix });
    assert.deepEqual([vm.placement.kind, vm.placement.provider, vm.vm.provider, vm.vm.image], ["vm", "microsandbox", "microsandbox", "node:22-trixie-slim"]);
    assert.equal(vm.daemon, GUEST.daemon, "the daemon is the guest's, beside its helper");
    assert.deepEqual([vm.vm.cpus, vm.vm.memory, vm.vm.storeSize], [2, "2G", "20G"]);
    assert.equal(callHome(vm), GUEST.home, "LayerFS calls see the VM's home");
    assert.deepEqual(missing(vm), [], "a VM home needs no runtime image and no daemon setting");
    // sizes and cpus come from settings.vm, else LAYERFS_VM_*
    process.env.LAYERFS_VM_CPUS = "3";
    process.env.LAYERFS_VM_STORE_SIZE = "64G";
    const sized = resolveSettings({ home, platform: "linux", bundle: "/b" });
    assert.deepEqual([sized.vm.cpus, sized.vm.storeSize], [3, "64G"]);
    assert.throws(() => resolveSettings({ home, platform: "linux", placement: "vm:nope" }), { code: "Unsupported", message: /vm:microsandbox or vm:apple-container/ });
    assert.throws(() => resolveSettings({ home, platform: "linux", placement: "nope" }), { code: "InvalidInput", message: /local.*vm:<provider>/ });
    assert.throws(() => resolveSettings({ home, platform: "linux", placement: "vm:microsandbox", vm: { memory: "8" } }), { code: "InvalidInput", message: /memory/ });
    // vm:ssh settings
    const sshSettings = resolveSettings({
      home, platform: "linux", bundle: "/b", placement: "vm:ssh",
      vm: { host: "10.0.0.1", port: 2222, identityFile: "/path/to/key.pem", storePath: "/mnt/store" }
    });
    assert.deepEqual([sshSettings.placement.kind, sshSettings.placement.provider, sshSettings.vm.provider], ["vm", "ssh", "ssh"]);
    assert.deepEqual(
      [sshSettings.vm.ssh.host, sshSettings.vm.ssh.port, sshSettings.vm.ssh.identityFile, sshSettings.vm.ssh.storePath, sshSettings.vm.ssh.strictHostKeyChecking],
      ["10.0.0.1", 2222, "/path/to/key.pem", "/mnt/store", true]
    );
    assert.deepEqual(missing(sshSettings), []);
    // missing host in vm:ssh
    const sshNoHost = resolveSettings({ home, platform: "linux", bundle: "/b", placement: "vm:ssh" });
    assert.deepEqual(missing(sshNoHost), ["host"]);
    // password rejected
    assert.throws(
      () => resolveSettings({ home, platform: "linux", bundle: "/b", placement: "vm:ssh", vm: { host: "10.0.0.1", password: "secret" } }),
      { code: "InvalidInput", message: /password/ }
    );
    delete process.env.LAYERFS_PLACEMENT;
    delete process.env.LAYERFS_VM_CPUS;
    delete process.env.LAYERFS_VM_STORE_SIZE;
  } else if (selected === "flow") {
    const image = "a".repeat(64);
    makeBundle("build-one", image);
    const state = join(work, "msb");
    mkdirSync(state);
    const restore = standIn("msb", MSB, state);
    const runtime = await LayerFSRuntime.open({ home, bundle, placement: "vm:microsandbox", containerPrefix: prefix, platform: "linux", sweep: false });
    try {
      assert.equal(existsSync(endpointFile(home, "shop")), false);
      // a project's VM is named after the project: there is no home-wide VM
      await assert.rejects(runtime.worker.start(), { code: "InvalidInput", message: /project name/ });
      project("shop");
      const started = await runtime.worker.start("shop");
      assert.equal(started.project, "shop");
      assert.equal(started.worker.state, "running");
      assert.deepEqual([started.vm.state, started.vm.store], ["running", storeName("shop")]);
      const endpoint = endpointAt("shop");
      assert.equal(endpoint.home, GUEST.home, "the endpoint names the VM's home");
      assert.equal(endpoint.vm.name, vmName("shop"));
      assert.equal(endpoint.vm.project, "shop");
      assert.deepEqual(Object.keys(endpoint.vm).sort(), ["name", "project", "provider"]);
      assert.equal(process.env.LAYERFS_WORKER_ENDPOINT, undefined, "the endpoint is handed over in-process");

      const first = calls(state);
      const create = first.find((argv) => argv[0] === "create");
      assert.ok(create, "step ① created the project's VM");
      assert.ok(create.includes("--name") && create[create.indexOf("--name") + 1] === vmName("shop"));
      assert.ok(create.includes("--mount-named"), "the Store is a named disk volume of its own");
      assert.equal(create[create.indexOf("--mount-named") + 1], `${storeName("shop")}:/layerfs/store:kind=disk,size=20G`);
      assert.equal(create[create.indexOf("-v") + 1], `${home}:/layerfs/home`);
      assert.equal(create[create.indexOf("-p") + 1], `127.0.0.1:${endpoint.port}:7070`);
      assert.equal(create.at(-1), "node:22-trixie-slim");
      assert.ok(first.some((argv) => argv[0] === "copy" && argv[2] === `${vmName("shop")}:/layerfs/bundle`), "step ② copied the bundle into that VM");
      assert.ok(first.some((argv) => argv[0] === "exec" && /max_user_namespaces/.test(argv.join(" "))), "step ③ blocked user namespaces");
      assert.ok(first.some((argv) => argv[0] === "exec" && /nohup node/.test(argv.join(" "))), "step ③ started the worker");
      assert.equal(readFileSync(guestPath(state, vmName("shop"), "/layerfs/state/worker.env"), "utf8").includes(endpoint.secret), true, "the secret reached the VM in a file, not in a command line");
      assert.ok(!JSON.stringify(first).includes(endpoint.secret), "no call carried the secret in its arguments");

      // the Sandbox image the bundle carries (V7, H38's convention): this VM's own copy,
      // named for the worker as a guest path, and the deployment's image digest
      const workerEnv = readFileSync(guestPath(state, vmName("shop"), "/layerfs/state/worker.env"), "utf8");
      assert.match(workerEnv, new RegExp(`^LAYERFS_IMAGE=sha256:${image}$`, "m"));
      assert.match(workerEnv, new RegExp(`^LAYERFS_IMAGE_ARCHIVE=/layerfs/bundle/images/${image}\\.tar$`, "m"));
      assert.equal(existsSync(guestPath(state, vmName("shop"), `/layerfs/bundle/images/${image}.tar`)), true, "the image archive came in with the bundle");

      // the client side: this project's deployment, and this project's endpoint
      const env = deploymentEnv(runtime.settings, runtime.platform, { name: "shop" });
      assert.equal(env.WORKER_ENDPOINT, endpointFile(home, "shop"), "agents an integration launches reach this project's VM");
      assert.equal(env.STORE_PATH, `${GUEST.store}/global/store.sqlite`, "the Store disk is the Store's directory");
      assert.equal(env.SANDBOX_BACKEND, "namespaces");
      assert.equal(env.IMAGE, `sha256:${image}`, "a bundle with an image gives the deployment its digest");
      assert.equal(env.IMAGE_ARCHIVE, `${GUEST.bundle}/images/${image}.tar`, "and the archive in this VM's own copy of the bundle");
      assert.equal(env.LOCAL_DIR, `${GUEST.store}/local`, "Sandbox directories are on the Store disk, never the VM's own root");
      assert.match(readFileSync(join(state, "calls.jsonl"), "utf8"), /\/layerfs\/store\/local/, "the guest's Sandbox directory was made on the Store disk");

      // the same project again: the VM and its bundle are reused, and only the VM is read
      const counted = () => calls(state).filter((argv) => argv[0] === "create" || argv[0] === "copy" || argv[0] === "exec").length;
      const before = counted();
      await runtime.worker.start("shop");
      assert.equal(counted(), before, "a running worker with its bundle is reused");

      // a stopped VM is started, not created
      assert.deepEqual(await runtime.vm.stop("shop"), { project: "shop", name: vmName("shop"), state: "stopped" });
      assert.equal(existsSync(endpointFile(home, "shop")), false, "a stopped VM's endpoint is gone");
      const restarted = await runtime.worker.start("shop");
      assert.equal(restarted.worker.state, "running");
      assert.equal(calls(state).filter((argv) => argv[0] === "create").length, 1, "the VM was started, not created again");

      // status: host Docker is not this home's, and each project's VM is listed
      const status = await runtime.status();
      assert.deepEqual([status.placement, status.docker.level], ["vm:microsandbox", "not-used"]);
      assert.equal(status.worker, null, "no single worker serves the home");
      assert.deepEqual(status.vm.vms.map((vm) => [vm.project, vm.state, vm.worker.state]), [["shop", "running", "running"]]);
      assert.ok(status.disk.volumes > 0, "the project's Store disk has free bytes");
      assert.equal((await runtime.vm.status("shop")).name, vmName("shop"));
      assert.deepEqual((await runtime.vm.status()).vms.map((vm) => vm.project), ["shop"]);
    } finally {
      restore();
      await runtime.close();
      restoreWorker(work);
    }
  } else if (selected === "two") {
    makeBundle();
    const state = join(work, "msb");
    mkdirSync(state);
    const restore = standIn("msb", MSB, state);
    const runtime = await LayerFSRuntime.open({ home, bundle, placement: "vm:microsandbox", containerPrefix: prefix, platform: "linux", sweep: false });
    try {
      project("shop");
      project("blog");
      await runtime.worker.start("shop");
      await runtime.worker.start("blog");
      // one VM, one Store disk and one endpoint per project, and they all differ
      const shop = endpointAt("shop"), blog = endpointAt("blog");
      assert.notEqual(shop.port, blog.port, "each VM owns its host port");
      assert.notEqual(shop.secret, blog.secret, "each VM has its own worker secret");
      assert.deepEqual([shop.vm.name, blog.vm.name], [vmName("shop"), vmName("blog")]);
      const created = calls(state).filter((argv) => argv[0] === "create").map((argv) => argv[argv.indexOf("--mount-named") + 1]);
      assert.deepEqual(created.sort(), [`${storeName("blog")}:/layerfs/store:kind=disk,size=20G`, `${storeName("shop")}:/layerfs/store:kind=disk,size=20G`], "each project's Store is on its own disk");
      assert.equal(calls(state).filter((argv) => argv[0] === "copy" && argv[2] === `${vmName("blog")}:/layerfs/bundle`).length, 1, "the bundle is copied into each VM");
      // neither VM's Store disk holds the other's Store: they are different disks
      const shopStore = guestPath(state, vmName("shop"), "/layerfs/store/global/store.sqlite");
      mkdirSync(join(shopStore, ".."), { recursive: true });
      writeFileSync(shopStore, "shop");
      assert.equal(existsSync(guestPath(state, vmName("blog"), "/layerfs/store/global/store.sqlite")), false, "the other project's VM has its own Store disk");
      // and one VM's secrets, ports and bundle copy are its own
      assert.notDeepEqual(readFileSync(guestPath(state, vmName("shop"), "/layerfs/state/worker.env"), "utf8"), readFileSync(guestPath(state, vmName("blog"), "/layerfs/state/worker.env"), "utf8"));
      const listed = await runtime.vm.status();
      assert.deepEqual(listed.vms.map((vm) => [vm.project, vm.state, vm.worker.port]).sort(), [["blog", "running", blog.port], ["shop", "running", shop.port]]);
      // the hook project.delete uses takes that project's VM and disk, and nothing else
      // (the whole call, a Sandbox closing included, is vm_real.mjs's, with a real VM)
      await runtime.platform.removeStore(runtime, { name: "blog", volume: "lfs-blog-store-1" });
      assert.equal(existsSync(join(state, "vms", `${vmName("blog")}.json`)), false, "the project's VM is gone");
      assert.ok(calls(state).some((argv) => argv[0] === "volume" && argv[1] === "remove" && argv[2] === storeName("blog")));
      assert.equal(existsSync(join(state, "vms", `${vmName("shop")}.json`)), true, "the other project's VM is untouched");
      assert.equal(existsSync(join(state, "vms", `${vmName("shop")}.json`)) && endpointAt("shop").vm.name, vmName("shop"));
      assert.equal((await runtime.worker.start("shop")).worker.state, "running", "and the other project's worker still answers");
      // V8 still holds for the project that is left
      await assert.rejects(runtime.vm.delete("shop", { confirm: "delete" }), { code: "Unsupported", message: /never destroys a Store that was not pushed/ });
      forget("shop");
      assert.deepEqual(await runtime.vm.delete("shop", { confirm: "delete", discardStore: true }), { project: "shop", name: vmName("shop"), state: "deleted", store: "discarded" });
      assert.ok(calls(state).some((argv) => argv[0] === "volume" && argv[1] === "remove" && argv[2] === storeName("shop")));
    } finally {
      restore();
      await runtime.close();
      restoreWorker(work);
    }
  } else if (selected === "bundle") {
    makeBundle("build-one");
    const state = join(work, "msb");
    mkdirSync(state);
    const restore = standIn("msb", MSB, state);
    const runtime = await LayerFSRuntime.open({ home, bundle, placement: "vm:microsandbox", containerPrefix: prefix, platform: "linux", sweep: false });
    try {
      await runtime.worker.start("shop");
      const copies = (vm) => calls(state).filter((argv) => argv[0] === "copy" && argv[2] === `${vm}:/layerfs/bundle`).length;
      assert.equal(copies(vmName("shop")), 1);
      // the same bundle hash: step ② is not repeated
      await runtime.worker.start("shop");
      assert.equal(copies(vmName("shop")), 1);
      // a second project's VM gets its own copy
      await runtime.worker.start("blog");
      assert.equal(copies(vmName("shop")), 1);
      assert.equal(copies(vmName("blog")), 1);
      // a changed bundle is copied again into the VM that restarts, and the worker reports it
      makeBundle("build-two");
      const restarted = await runtime.worker.start("shop");
      assert.equal(copies(vmName("shop")), 2, "a changed bundle hash is copied once more");
      assert.equal(restarted.worker.build, "build-two");
      assert.equal(readFileSync(guestPath(state, vmName("shop"), "/layerfs/bundle/.sealed"), "utf8").trim().length, 64);
      // a bundle without a Sandbox image sets neither: the backend needs none there
      const plain = readFileSync(guestPath(state, vmName("shop"), "/layerfs/state/worker.env"), "utf8");
      assert.ok(!/LAYERFS_IMAGE/.test(plain), "no image is named when the bundle carries none");
      assert.equal(copies(vmName("blog")), 1, "the other project's VM keeps its copy until it restarts");
    } finally {
      restore();
      await runtime.close();
      restoreWorker(work);
    }
  } else if (selected === "delete") {
    makeBundle();
    const state = join(work, "msb");
    mkdirSync(state);
    const restore = standIn("msb", MSB, state);
    const runtime = await LayerFSRuntime.open({ home, bundle, placement: "vm:microsandbox", containerPrefix: prefix, platform: "linux", sweep: false });
    try {
      await runtime.worker.start("shop");
      // V8: a Store that lives only on the project's VM disk is never destroyed by a delete
      project("shop");
      await assert.rejects(runtime.vm.delete("shop"), { code: "InvalidInput", message: /confirm/ });
      await assert.rejects(runtime.vm.delete("shop", { confirm: "delete" }), { code: "Unsupported", message: /never destroys a Store that was not pushed/ });
      assert.equal(existsSync(join(state, "vms", `${vmName("shop")}.json`)), true, "the VM is untouched by the refusal");
      // with no project left, the VM record goes and its Store disk stays unless asked
      forget("shop");
      assert.deepEqual(await runtime.vm.delete("shop", { confirm: "delete" }), { project: "shop", name: vmName("shop"), state: "deleted", store: "kept" });
      assert.equal(existsSync(join(state, "vms", `${vmName("shop")}.json`)), false);
      assert.equal(calls(state).some((argv) => argv[0] === "volume"), false, "the Store disk is kept by default");
      await runtime.vm.delete("shop", { confirm: "delete", discardStore: true });
      // a store whose VM is gone answers "missing" rather than inventing one
      assert.equal((await runtime.vm.delete("shop", { confirm: "delete" })).state, "missing");
    } finally {
      restore();
      await runtime.close();
      restoreWorker(work);
    }
  } else if (selected === "hosts") {
    makeBundle();
    const state = join(work, "msb");
    mkdirSync(state);
    const restore = standIn("msb", MSB, state);
    const runtime = await LayerFSRuntime.open({ home, bundle, placement: "vm:microsandbox", containerPrefix: prefix, platform: "linux", sweep: false });
    try {
      project("shop");
      await runtime.worker.start("shop");
      // host Docker describes another machine: the Docker-shaped helpers refuse by name
      await assert.rejects(runtime.sandbox.list(), { code: "Unsupported", message: /runtime\.sandbox\.list reads host Docker.*project\.list_sandboxes/ });
      await assert.rejects(runtime.sandbox.list_leftovers(), { code: "Unsupported", message: /Linux namespaces inside the VM/ });
      await assert.rejects(runtime.volume.list(), { code: "Unsupported", message: /directories on the VM's disk/ });
      assert.deepEqual(await runtime.sandbox.sweep(), { reclaimed: {}, failed: [], host: false });
      // status lists a project's VM even before it was started
      project("late");
      assert.deepEqual((await runtime.vm.status()).vms.map((vm) => [vm.project, vm.state]), [["late", "missing"], ["shop", "running"]]);
      // a local home has no vm group at all
      const local = await LayerFSRuntime.open({ home: join(work, "local"), image: `sha256:${"a".repeat(64)}`, daemon: "/bin/daemon", platform: "linux", sweep: false });
      assert.equal(local.vm, undefined);
      await local.close();
    } finally {
      restore();
      await runtime.close();
      restoreWorker(work);
    }
  } else if (selected === "apple-container") {
    makeBundle();
    const state = join(work, "container");
    mkdirSync(state);
    const restore = standIn("container", CONTAINER, state);
    const runtime = await LayerFSRuntime.open({ home, bundle, placement: "vm:apple-container", containerPrefix: prefix, platform: "macos", sweep: false });
    try {
      const started = await runtime.worker.start("shop");
      assert.equal(started.vm.state, "running");
      const first = calls(state);
      // the Store volume is made first, at the configured size, and mounted by name
      const volume = first.find((argv) => argv[0] === "volume" && argv[1] === "create");
      assert.ok(volume, "the Store volume is made before the VM");
      assert.equal(volume[volume.indexOf("-s") + 1], "20G");
      assert.equal(volume.at(-1), storeName("shop"));
      const run = first.find((argv) => argv[0] === "run");
      assert.ok(run, "the VM was created with `container run`");
      const capabilities = run.flatMap((value, index) => (value === "--cap-add" ? [run[index + 1]] : []));
      assert.deepEqual(capabilities, ["SYS_ADMIN", "SYS_RESOURCE", "NET_ADMIN"], "#512's capabilities for FUSE, cgroups and loopback");
      assert.ok(run.includes("--publish"));
      assert.equal(run[run.indexOf("--mount") + 1], `type=volume,source=${storeName("shop")},target=/layerfs/store`);
      // the guest's /proc/sys is read-only there: the block remounts it first
      const block = first.find((argv) => argv[0] === "exec" && /max_user_namespaces/.test(argv.join(" ")));
      assert.match(block.join(" "), /remount,rw \/proc\/sys/);
      assert.ok(first.some((argv) => argv[0] === "exec" && /nohup node/.test(argv.join(" "))));
      // the verified 1.6.1 inspect shape: the adapter reads this VM's state, store and home
      const entry = await runtime.vm.status("shop");
      assert.deepEqual([entry.state, entry.store, entry.home, entry.worker.state], ["running", storeName("shop"), home, "running"]);
      // copying the bundle into an Apple Container VM is `container copy HOST NAME:DEST`
      assert.ok(first.some((argv) => argv[0] === "copy" && argv[2] === `${vmName("shop")}:/layerfs/bundle`));
    } finally {
      restore();
      await runtime.close();
      restoreWorker(work);
    }
  } else if (selected === "ssh") {
    makeBundle();
    const state = join(work, "ssh");
    mkdirSync(state);
    const restore = standIn("ssh", SSH, state);
    const runtime = await LayerFSRuntime.open({
      home, bundle, placement: "vm:ssh", containerPrefix: prefix, platform: "linux", sweep: false,
      vm: { host: "127.0.0.1", port: 22, storePath: "/layerfs/store" },
    });
    try {
      const started = await deadline(60_000, "worker.start", runtime.worker.start("shop"));
      assert.equal(started.vm.state, "running");
      assert.equal(started.worker.state, "running");
      const first = calls(state);
      assert.ok(first.some((argv) => argv.includes("uname")), "the host's kernel is read before anything else");
      assert.ok(first.some((argv) => argv.includes("findmnt")), "the Store path's filesystem is checked");
      assert.ok(first.some((argv) => argv.includes("-N") && argv.includes("-L")), "the private stream is an ssh -L forward");
      assert.ok(first.some((argv) => argv.includes("StrictHostKeyChecking=yes")), "host key checking stays on");
      assert.ok(!first.some((argv) => argv.includes("StrictHostKeyChecking=no")), "never turned off behind the user's back");
      assert.ok(first.some((argv) => argv.some((value) => typeof value === "string" && value.includes("tar -C"))), "the bundle arrives as one tar stream");
      const entry = await deadline(30_000, "vm.status", runtime.vm.status("shop"));
      assert.deepEqual([entry.state, entry.worker.state], ["running", "running"]);
      // the endpoint the worker was written at is this host's forwarded port
      const endpoint = endpointAt("shop");
      assert.equal(endpoint.vm.provider, "ssh");
      assert.ok(endpoint.port > 1024);
      // a second start is served by the same forward: no second copy, no second forward
      await deadline(30_000, "worker.start again", runtime.worker.start("shop"));
      assert.equal(calls(state).filter((argv) => argv.includes("-N")).length, 1, "one forward per VM");
      // stop ends the forward; the record stays and start re-opens it
      assert.equal((await deadline(30_000, "vm.stop", runtime.vm.stop("shop"))).state, "stopped");
      assert.equal((await deadline(30_000, "status after stop", runtime.vm.status("shop"))).state, "stopped");
      const again = await deadline(60_000, "worker.start after stop", runtime.worker.start("shop"));
      assert.deepEqual([again.vm.state, again.worker.state], ["running", "running"]);
    } finally {
      await runtime.close();
      restore();
      restoreWorker(work);
    }
  } else if (selected === "cleanup") {
    // H39v's two findings: a provider create that fails must leave nothing
    // behind — no Store volume for Apple Container (the CLI makes one before it
    // fails on a busy port), and no hidden sandbox directory for Microsandbox.
    const containerState = join(work, "container");
    mkdirSync(containerState);
    const restoreContainer = standIn("container", CONTAINER, containerState);
    try {
      const container = (await import("../src/platform/vm/apple-container.js")).default;
      await container.ensureStore("lfs-vm-test-clean-store", "20G");
      process.env.FAKE_CONTAINER_FAIL_RUN = "port";
      await assert.rejects(container.create({
        name: "lfs-vm-test-clean", image: "node:22-trixie-slim", cpus: 2, memory: "2G",
        storeName: "lfs-vm-test-clean-store", storeSize: "20G", home, hostPort: 18080, workerPort: 7070,
      }), /port is already allocated/);
      assert.equal(existsSync(join(containerState, "volumes", "lfs-vm-test-clean-store.json")), false, "the Store volume this create made is gone");
      assert.equal(existsSync(join(containerState, "vms", "lfs-vm-test-clean.json")), false, "and no VM record");
    } finally {
      delete process.env.FAKE_CONTAINER_FAIL_RUN;
      restoreContainer();
    }
    const msbState = join(work, "msb");
    mkdirSync(msbState);
    const restoreMsb = standIn("msb", MSB, msbState);
    const hidden = join(tmpdir(), ".microsandbox-clean-check");
    try {
      const msb = (await import("../src/platform/vm/microsandbox.js")).default;
      const name = `lfs-vm-test-hidden-${process.pid}`;
      const dir = join(process.env.HOME ?? join(tmpdir(), "home"), ".microsandbox", "sandboxes", name);
      process.env.FAKE_MSB_FAIL_CREATE = "mount";
      await assert.rejects(msb.create({
        name, image: "node:22-trixie-slim", cpus: 2, memory: "2G", storeName: "lfs-vm-test-hidden-store",
        storeSize: "20G", home, hostPort: 18081, workerPort: 7070,
      }), /host path for one of the mounts/);
      assert.equal(existsSync(dir), false, `the hidden sandbox directory ${dir} the failed create made is gone`);
    } finally {
      delete process.env.FAKE_MSB_FAIL_CREATE;
      restoreMsb();
      rmSync(hidden, { recursive: true, force: true });
    }
  } else {
    throw new Error("select settings, flow, two, bundle, delete, hosts, apple-container, ssh or cleanup");
  }
} finally {
  rmSync(work, { recursive: true, force: true });
}
console.log(`PASS vm_placement/${selected}`);
// The run is over: exit explicitly so a stand-in child that outlived its case
// cannot hold this process open. `restoreWorker` already ended them.
process.exit(0);

/** The stand-in's worker outlives its CLI: end it from the pid the fixture wrote. */
function restoreWorker(folder) {
  for (const provider of ["msb", "container", "ssh"]) {
    const guests = join(folder, provider, "guest");
    if (existsSync(guests)) {
      for (const vm of readdirSync(guests)) {
        for (const file of [join(guests, vm, "layerfs/state/worker.pid"), join(guests, "layerfs/state/worker.pid")]) {
          if (!existsSync(file)) continue;
          try { process.kill(Number(readFileSync(file, "utf8").trim())); } catch { /* already gone */ }
        }
      }
    }
    const sessions = join(folder, provider, "sessions");
    if (existsSync(sessions)) {
      for (const file of readdirSync(sessions)) {
        try {
          const session = JSON.parse(readFileSync(join(sessions, file), "utf8"));
          if (session.forwardPid) process.kill(session.forwardPid);
        } catch {}
      }
    }
  }
}

// @layerfs/runtime's `vm:<provider>` placement (#515 §6, V9) with a stand-in
// provider CLI and the real worker: no microVM, no Store, no daemon. One Linux
// VM per Store (#515 V1): every case names a project, and its VM, Store disk,
// host port, bundle copy and worker secret are that project's alone. The
// fixture keeps one VM record per project, maps each guest's absolute paths
// under a temp folder, and starts the adapter's bundle worker so step ④'s
// readiness is real.
//   node api/runtime/test/vm_placement.mjs settings|flow|two|bundle|delete|hosts|stores|apple-container|ssh|lima|firecracker|cleanup
import assert from "node:assert/strict";
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const FAKE = fileURLToPath(new URL("../../agent/test/fixture/fake-native.cjs", import.meta.url));
const SOURCE = fileURLToPath(new URL("../src", import.meta.url));
const MSB = fileURLToPath(new URL("fixture/fake-msb.cjs", import.meta.url));
const CONTAINER = fileURLToPath(new URL("fixture/fake-container.cjs", import.meta.url));
const SSH = fileURLToPath(new URL("fixture/fake-ssh.cjs", import.meta.url));
const LIMA = fileURLToPath(new URL("fixture/fake-lima.cjs", import.meta.url));
process.env.LAYERFS_NATIVE = FAKE;
const { LayerFSRuntime } = await import("@layerfs/runtime");
const { resolveSettings, missing, callHome, GUEST } = await import("../src/settings.js");
const { deploymentEnv } = await import("../src/clients.js");
const { endpointFile } = await import("../src/place/shared.js");

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

/** The `limactl` and `ssh` stand-ins of vm:lima and vm:firecracker: one fixture, two names on PATH. */
function limaStandIn(state) {
  const bin = join(work, "lima-bin");
  mkdirSync(bin, { recursive: true });
  for (const tool of ["limactl", "ssh"]) {
    writeFileSync(join(bin, tool), `#!/bin/sh\nexec "${process.execPath}" "${LIMA}" ${tool} "$@"\n`);
    chmodSync(join(bin, tool), 0o755);
  }
  const saved = { PATH: process.env.PATH };
  process.env.PATH = `${bin}:${saved.PATH}`;
  Object.assign(process.env, { FAKE_LIMA_STATE: state, FAKE_LIMA_NATIVE: FAKE, LIMA_HOME: join(state, "lima-home") });
  return () => {
    process.env.PATH = saved.PATH;
    for (const name of ["FAKE_LIMA_STATE", "FAKE_LIMA_NATIVE", "LIMA_HOME", "LAYERFS_FIRECRACKER_LIMA", "LAYERFS_FIRECRACKER_STATE_DIR", "FAKE_FC_NO_KVM"]) delete process.env[name];
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
/** The remote command of a stand-in `ssh` call (unquoted by the fixture), or null. */
const remoteOf = (argv) => (argv.includes("--") ? argv.slice(argv.indexOf("--") + 1) : null);

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
    // #527 step 8: each provider runs on its own host systems, refused when the settings resolve
    assert.throws(() => resolveSettings({ home, platform: "linux", placement: "vm:apple-container" }), { code: "Unsupported", message: /vm:apple-container runs on macOS hosts, not Linux/ });
    assert.throws(() => resolveSettings({ home, platform: "windows", placement: "vm:apple-container" }), { code: "Unsupported", message: /runs on macOS hosts, not Windows/ });
    for (const provider of ["microsandbox", "ssh"]) {
      assert.throws(() => resolveSettings({ home, platform: "windows", placement: `vm:${provider}` }), { code: "Unsupported", message: new RegExp(`vm:${provider} runs on Linux and macOS hosts, not Windows`) });
      for (const host of ["linux", "macos"]) assert.equal(resolveSettings({ home, platform: host, placement: `vm:${provider}` }).placement.provider, provider);
    }
    assert.equal(resolveSettings({ home, platform: "macos", placement: "vm:apple-container" }).placement.provider, "apple-container");
    // Lima makes vz instances (Virtualization.framework), and Firecracker runs nested in one
    for (const provider of ["lima", "firecracker"]) {
      for (const host of ["linux", "windows"]) {
        assert.throws(() => resolveSettings({ home, platform: host, placement: `vm:${provider}` }), { code: "Unsupported", message: new RegExp(`vm:${provider} runs on macOS hosts, not ${host === "linux" ? "Linux" : "Windows"}`) });
      }
      assert.equal(resolveSettings({ home, platform: "macos", placement: `vm:${provider}`, bundle: "/b" }).placement.provider, provider);
    }
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
      // #527 F4: the VM deployment names its backend; a host LAYERFS_SANDBOX or the former
      // LAYERFS_SANDBOX_BACKEND does not replace it (step 11: it is LAYERFS_SANDBOX=namespaces)
      process.env.LAYERFS_SANDBOX = "docker";
      process.env.LAYERFS_SANDBOX_BACKEND = "native";
      const env = deploymentEnv(runtime.settings, runtime.platform, { name: "shop" });
      delete process.env.LAYERFS_SANDBOX;
      delete process.env.LAYERFS_SANDBOX_BACKEND;
      assert.equal(env.WORKER_ENDPOINT, endpointFile(home, "shop"), "agents an integration launches reach this project's VM");
      assert.equal(env.STORE_PATH, `${GUEST.store}/global/store.sqlite`, "the Store disk is the Store's directory");
      assert.deepEqual([env.SANDBOX, env.SANDBOX_BACKEND], ["namespaces", ""]);
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
      await runtime.platform.removeStore(runtime, { name: "blog", volume: "lfs-blog-store-1", legacy: true });
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
  } else if (selected === "stores") {
    // #527 F3: a `vm:` home keeps no Store of its own (one Store per project, in its VM), so
    // every call on the home's Stores, remotes or PostgreSQL refuses Unsupported before any effect
    makeBundle();
    const state = join(work, "msb");
    mkdirSync(state);
    const restore = standIn("msb", MSB, state);
    const runtime = await LayerFSRuntime.open({ home, bundle, placement: "vm:microsandbox", containerPrefix: prefix, platform: "linux", sweep: false });
    try {
      project("shop");
      // a PostgreSQL Store a registry could still hold
      writeFileSync(join(home, "stores.json"), JSON.stringify({ version: 2, stores: [{ name: "pgs", kind: "postgresql", connection: "pgc", schema: "pgs", synchronousCommit: "off", created: 1, projects: [{ name: "pga" }] }] }));
      const source = join(work, "source");
      mkdirSync(source);
      const refused = { code: "Unsupported", message: /vm:microsandbox home: each project's Store is in that project's own VM/ };
      const url = "postgresql://user@db/layerfs";
      await assert.rejects(runtime.store.create("fresh"), refused);
      await assert.rejects(runtime.store.start_load("r2", { file: "fresh" }), refused);
      await assert.rejects(runtime.store.start_publish("shop", { remote: "r2" }), refused);
      await assert.rejects(runtime.store.start_push("shop"), refused);
      await assert.rejects(runtime.store.start_send("shop", { remote: "r2", file: "copy" }), refused);
      await assert.rejects(runtime.store.remove("shop", { confirm: "shop" }), refused);
      await assert.rejects(runtime.store.attach_postgresql({ name: "pg2", connection: "pgc", schema: "pg2", url }), refused);
      await assert.rejects(runtime.store.start_import_postgresql("shop", { connection: "pgc", schema: "pg3", url }), refused);
      await assert.rejects(runtime.remote.list(), refused);
      await assert.rejects(runtime.remote.add({ name: "r2", provider: "minio" }), refused);
      await assert.rejects(runtime.postgresql.status({ url }), refused);
      await assert.rejects(runtime.project.start_init("blog", source, { store: "shop" }), { ...refused, message: /runtime\.project\.start_init into a Store/ });
      // the platform hooks those calls reach refuse too, rather than crash on `project.name` or
      // map every Store of the home to the one guest path each VM holds its own Store at
      await assert.rejects(runtime.platform.init(runtime.settings), refused);
      assert.throws(() => runtime.platform.storePath(runtime.settings, "lfs-shop-store-1"), refused);
      // the Store hooks are keyed by Store, and a VM holds only its project's own (#527 step 10b)
      for (const hook of ["createStore", "storeHolders", "removeStore"]) {
        await assert.rejects(runtime.platform[hook](runtime, { name: "fresh", volume: "lfs-store-fresh" }), refused, hook);
      }
      await assert.rejects(runtime.platform.releaseStore(runtime.settings, { name: "fresh", volume: "lfs-store-fresh" }), refused);
      // a PostgreSQL project is refused before its client is made
      await assert.rejects(runtime.project.get_state("pga", undefined, { store: "pgs", url }), { code: "Unsupported", message: /PostgreSQL Store pgs.*vm:microsandbox/ });
      // nothing reached the provider: no VM, no disk, no copy
      assert.deepEqual(calls(state), [], "every refusal came before any effect");
      assert.equal(existsSync(endpointFile(home, "shop")), false);
      // the home's Stores are still listed, from its records, with no stats asked of any process
      assert.deepEqual((await runtime.store.list()).map((s) => [s.name, s.kind, s.bytes]), [["pgs", "postgresql", null], ["shop", "sqlite", null]]);
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
      // the bundle reaches the VM through a staging folder in the home it already
      // mounts, and the guest copies it onto its own disk: no `container copy`
      // (35 MB/s on 1.6.1, #527), and the staging folder is gone afterwards
      assert.ok(!first.some((argv) => argv[0] === "copy"), "no `container copy`");
      const staged = first.find((argv) => argv[0] === "exec" && /cp -Rp/.test(argv.join(" ")) && argv.at(-1) === "/layerfs/bundle");
      assert.ok(staged, "the guest copied the bundle from the home bind mount");
      assert.match(staged.at(-2), /^\/layerfs\/home\/\.vm-copy-[^/]+\/c$/);
      assert.ok(existsSync(join(state, "guest", vmName("shop"), "layerfs/bundle/manifest.json")), "the VM owns its copy");
      assert.deepEqual(readdirSync(home).filter((name) => name.startsWith(".vm-copy-")), [], "no staging folder is left in the home");
      // the same route serves an Init source; a folder inside the home is read in place, never staged into itself
      const source = join(work, "src");
      mkdirSync(join(source, "d"), { recursive: true });
      writeFileSync(join(source, "d", "a.txt"), "a\n");
      const container = (await import("../src/place/vm/apple-container.js")).default;
      await deadline(30_000, "copy", container.copy(vmName("shop"), source, "/layerfs/import/x"));
      assert.equal(readFileSync(join(state, "guest", vmName("shop"), "layerfs/import/x/d/a.txt"), "utf8"), "a\n");
      assert.deepEqual(readdirSync(home).filter((name) => name.startsWith(".vm-copy-")), []);
      mkdirSync(join(home, "inside"), { recursive: true });
      writeFileSync(join(home, "inside", "b.txt"), "b\n");
      await deadline(30_000, "copy", container.copy(vmName("shop"), join(home, "inside"), "/layerfs/import/z"));
      assert.equal(readFileSync(join(state, "guest", vmName("shop"), "layerfs/import/z/b.txt"), "utf8"), "b\n");
      assert.equal(calls(state).filter((argv) => argv[0] === "exec").at(-1).at(-2), "/layerfs/home/inside", "read in place through the bind mount");
      // a failed guest copy still removes the staging folder, and is reported
      await assert.rejects(container.copy(vmName("shop"), join(work, "no-such-folder"), "/layerfs/import/y"), /ENOENT|no such file/i);
      assert.deepEqual(readdirSync(home).filter((name) => name.startsWith(".vm-copy-")), []);
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
      if (process.platform === "darwin") {
        // a folder whose entries carry macOS metadata (com.apple.provenance,
        // which this process's files get) arrives without the Mac tar's
        // AppleDouble `._` entries (#527 platform pass: an Init source copied
        // this way stored `._.` in the project)
        const source = join(work, "xattr-source");
        mkdirSync(source);
        writeFileSync(join(source, "a.txt"), "a\n");
        for (const path of [source, join(source, "a.txt")]) execFileSync("xattr", ["-w", "com.apple.provenance", "", path]);
        const ssh = (await import("../src/place/vm/ssh.js"));
        await deadline(30_000, "copy", ssh.copy(vmName("shop"), source, "/layerfs/import/xattr"));
        const copied = readFileSync(join(state, "copies.jsonl"), "utf8").trim().split("\n").map((line) => JSON.parse(line)).find((copy) => copy.destination === "/layerfs/import/xattr");
        assert.deepEqual(copied.names.sort(), ["./", "./a.txt"], JSON.stringify(copied.names));
      }
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
  } else if (selected === "lima") {
    makeBundle("build-one", "c".repeat(64));
    const state = join(work, "lima");
    mkdirSync(state);
    const restore = limaStandIn(state);
    const runtime = await LayerFSRuntime.open({ home, bundle, placement: "vm:lima", containerPrefix: prefix, platform: "macos", sweep: false });
    try {
      const started = await deadline(60_000, "worker.start", runtime.worker.start("shop"));
      assert.deepEqual([started.vm.state, started.worker.state, started.vm.store, started.vm.home], ["running", "running", storeName("shop"), home]);
      const endpoint = endpointAt("shop");
      assert.equal(endpoint.vm.provider, "lima");
      // Lima finds a formatted disk by its ext4 label `lima-<disk>` (16 bytes at most): a longer
      // name would be formatted again at every boot, so the disk has a short name of its own
      const { diskName, template: limaTemplate } = await import("../src/place/vm/lima.js");
      const disk = diskName(storeName("shop"));
      assert.match(disk, /^lfs[0-9a-f]{8}$/);
      assert.ok(`lima-${disk}`.length <= 16);
      const first = calls(state);
      // ① a raw Lima disk (vz needs raw) of the configured size, made before the instance
      const made = first.find((argv) => argv[0] === "disk" && argv[1] === "create");
      assert.deepEqual(made, ["disk", "create", disk, "--size", "20GiB", "--format", "raw"]);
      const create = first.findIndex((argv) => argv[0] === "create");
      assert.ok(create > first.indexOf(made), "the Store disk is made before the instance");
      assert.deepEqual(first[create], ["create", "--name", vmName("shop"), "--tty=false", "-"], "the template comes on stdin");
      const { template } = JSON.parse(readFileSync(join(state, "vms", `${vmName("shop")}.json`), "utf8"));
      assert.deepEqual([template.vmType, template.cpus, template.memory, template.mountType], ["vz", 2, "2GiB", "virtiofs"]);
      assert.deepEqual(template.mounts, [{ location: home, mountPoint: "/layerfs/home", writable: true }]);
      assert.deepEqual(template.additionalDisks, [{ name: disk, format: true, fsType: "ext4" }], "Lima formats the Store disk ext4 on first attach");
      assert.deepEqual(template.param, { LAYERFS_STORE: storeName("shop"), LAYERFS_HOME: home }, "the instance keeps the Store volume's name and its home");
      // ④ the one forwarded port, private to this host's loopback; every other guest port is ignored
      assert.deepEqual(template.portForwards, [
        { guestPort: 7070, hostIP: "127.0.0.1", hostPort: endpoint.port },
        { guestIP: "0.0.0.0", guestPortRange: [1, 65535], ignore: true },
      ]);
      // the OCI default does not describe a Lima disk: the Ubuntu cloud image is booted, and Node.js is installed checked
      assert.match(template.images[0].location, /ubuntu-24\.04-server-cloudimg-arm64\.img$/);
      assert.match(template.provision[0].script, /nodejs\.org\/dist\/v22\.[0-9.]+\/SHASUMS256\.txt/);
      assert.match(template.provision[0].script, /sha256sum -c/);
      // every guest command is root through the instance's own ssh.config
      const remote = first.filter((argv) => argv[0] === "-F").map((argv) => [argv[1], argv[2], remoteOf(argv)]);
      assert.ok(remote.length > 0 && remote.every(([config, target, argv]) => config === join(state, "lima-home", vmName("shop"), "ssh.config") && target === `lima-${vmName("shop")}` && argv.slice(0, 3).join(" ") === "sudo -n --"));
      const scripts = remote.map(([, , argv]) => argv.join(" "));
      assert.ok(scripts.some((line) => line.includes("mount --bind") && line.endsWith(`/mnt/lima-${disk}`)), "the Store disk is bind-mounted at the Store directory");
      assert.ok(scripts.some((line) => /max_user_namespaces/.test(line)), "step ③ blocked user namespaces");
      assert.ok(scripts.some((line) => /nohup node/.test(line)), "step ③ started the worker");
      // step ② through the home the instance mounts: a private staging folder the guest copies from, then removed
      assert.ok(scripts.some((line) => /cp -Rp/.test(line) && /\/layerfs\/home\/\.vm-copy-[^/ ]+\/c \/layerfs\/bundle$/.test(line)), "the guest copied the staged bundle onto its own disk");
      assert.deepEqual(readdirSync(home).filter((entry) => entry.startsWith(".vm-copy-")), [], "no staging folder is left in the home");
      assert.equal(existsSync(guestPath(state, vmName("shop"), `/layerfs/bundle/images/${"c".repeat(64)}.tar`)), true, "with its Sandbox image");
      assert.ok(!JSON.stringify(first).includes(endpoint.secret), "no call carried the secret in its arguments");
      assert.match(readFileSync(guestPath(state, vmName("shop"), "/layerfs/state/worker.env"), "utf8"), new RegExp(`^LAYERFS_WORKER_SECRET=${endpoint.secret}$`, "m"));
      // a settings image that names a Lima disk is booted as it is
      assert.deepEqual(JSON.parse(limaTemplate({ name: "x", image: "https://example.com/a.img", cpus: 1, memory: "512M", home, storeName: "d", hostPort: 1, workerPort: 7070 })).images, [{ location: "https://example.com/a.img" }]);
      // ⑤ stop keeps the instance and its disk; a start boots it and attaches the disk again
      assert.equal((await deadline(30_000, "vm.stop", runtime.vm.stop("shop"))).state, "stopped");
      const again = await deadline(60_000, "worker.start after stop", runtime.worker.start("shop"));
      assert.deepEqual([again.vm.state, again.worker.state], ["running", "running"]);
      assert.equal(calls(state).filter((argv) => argv[0] === "create").length, 1, "started, not created again");
      assert.equal(calls(state).filter((argv) => remoteOf(argv)?.join(" ").includes("mount --bind")).length, 2, "the bind mount a reboot drops is made again");
      assert.equal(endpointAt("shop").port, endpoint.port, "the instance keeps its forwarded port");
      // V8, then a delete that keeps the disk, then one that discards it
      project("shop");
      await assert.rejects(runtime.vm.delete("shop", { confirm: "delete" }), { code: "Unsupported", message: /never destroys a Store that was not pushed/ });
      forget("shop");
      assert.equal((await runtime.vm.delete("shop", { confirm: "delete" })).store, "kept");
      assert.ok(calls(state).some((argv) => argv.join(" ") === `delete --force ${vmName("shop")}`));
      assert.ok(!calls(state).some((argv) => argv[0] === "disk" && argv[1] === "delete"), "Lima keeps the disk when its instance goes");
      await runtime.worker.start("shop");
      assert.equal(calls(state).filter((argv) => argv[0] === "disk" && argv[1] === "create").length, 1, "the kept disk is attached to the new instance, not made again");
      assert.equal((await runtime.vm.delete("shop", { confirm: "delete", discardStore: true })).store, "discarded");
      assert.ok(calls(state).some((argv) => argv.join(" ") === `disk delete ${disk}`));
    } finally {
      restore();
      await runtime.close();
      restoreWorker(work);
    }
  } else if (selected === "firecracker") {
    makeBundle();
    const state = join(work, "lima");
    mkdirSync(join(state, "vms"), { recursive: true });
    const restore = limaStandIn(state);
    // the Firecracker host is a Lima vz instance the user made and named
    process.env.LAYERFS_FIRECRACKER_LIMA = "lfs-vm-test-fc-host";
    process.env.LAYERFS_FIRECRACKER_STATE_DIR = join(state, "sessions");
    const runtime = await LayerFSRuntime.open({ home, bundle, placement: "vm:firecracker", containerPrefix: prefix, platform: "macos", sweep: false });
    try {
      await assert.rejects(runtime.worker.start("shop"), /no Lima instance lfs-vm-test-fc-host to run Firecracker in/);
      writeFileSync(join(state, "vms", "lfs-vm-test-fc-host.json"), JSON.stringify({ status: "Running", config: { vmType: "vz", nestedVirtualization: true } }));
      // a host without /dev/kvm is refused by fact, before any disk or VM
      process.env.FAKE_FC_NO_KVM = "1";
      await assert.rejects(runtime.worker.start("shop"), /there is no \/dev\/kvm/);
      delete process.env.FAKE_FC_NO_KVM;
      assert.equal(existsSync(join(state, "fc")), false, "nothing was made on a host that cannot run it");

      const started = await deadline(60_000, "worker.start", runtime.worker.start("shop"));
      assert.deepEqual([started.vm.state, started.worker.state, started.vm.store, started.vm.home], ["running", "running", storeName("shop"), home]);
      const blog = await deadline(60_000, "worker.start blog", runtime.worker.start("blog"));
      assert.equal(blog.worker.state, "running");
      const marked = (op) => calls(state).filter((argv) => remoteOf(argv)?.[5]?.startsWith(`# layerfs-firecracker:${op}`));
      // every host step is root in the named Lima host, in its folder
      assert.ok(marked("create").every((argv) => argv[2] === "lima-lfs-vm-test-fc-host" && remoteOf(argv)[7] === "/var/lib/layerfs-firecracker"));
      assert.deepEqual(marked("create").map((argv) => remoteOf(argv).slice(8)), [
        [vmName("shop"), `/var/lib/layerfs-firecracker/disks/${storeName("shop")}.ext4`, "2", "2048"],
        [vmName("blog"), `/var/lib/layerfs-firecracker/disks/${storeName("blog")}.ext4`, "2", "2048"],
      ], "one microVM per project, each on its own Store disk file, sized from settings.vm");
      assert.equal(marked("disk").length, 2);
      assert.equal(marked("boot").length, 2);
      // ④ a forward from this host's loopback to each microVM's own address
      const shopEndpoint = endpointAt("shop"), blogEndpoint = endpointAt("blog");
      const forwards = calls(state).filter((argv) => argv.includes("-N")).map((argv) => argv[argv.indexOf("-L") + 1]);
      assert.deepEqual(forwards, [`127.0.0.1:${shopEndpoint.port}:172.30.1.2:7070`, `127.0.0.1:${blogEndpoint.port}:172.30.2.2:7070`]);
      assert.ok(calls(state).filter((argv) => argv.includes("-N")).every((argv) => argv.includes("ExitOnForwardFailure=yes") && argv.includes("ControlPath=none")), "its own connection: a forward handed to Lima's control master would outlive its record");
      assert.notEqual(shopEndpoint.secret, blogEndpoint.secret);
      assert.ok(!JSON.stringify(calls(state)).includes(shopEndpoint.secret), "no call carried a secret in its arguments");
      // ② ③ in the microVM, through the host's guest script
      const guestCalls = marked("guest").map((argv) => remoteOf(argv).slice(9).join(" "));
      assert.ok(guestCalls.some((line) => /max_user_namespaces/.test(line)));
      assert.ok(guestCalls.some((line) => /nohup node/.test(line)));
      assert.ok(guestCalls.some((line) => line.includes("tar -C") && line.endsWith("/layerfs/bundle")));
      assert.equal(existsSync(guestPath(state, vmName("shop"), "/layerfs/bundle/.sealed")), true, "the bundle reached the microVM");
      assert.equal(readFileSync(guestPath(state, vmName("shop"), "/proc/sys/user/max_user_namespaces"), "utf8").trim(), "0");
      // a forward that ended reads as a stopped VM; start opens it again without a boot
      process.kill(JSON.parse(readFileSync(join(state, "sessions", `${vmName("shop")}.json`), "utf8")).pid);
      await new Promise((resolve) => setTimeout(resolve, 200));
      assert.equal((await runtime.vm.status("shop")).state, "stopped");
      const reopened = await deadline(60_000, "worker.start after the forward ended", runtime.worker.start("shop"));
      assert.deepEqual([reopened.vm.state, reopened.worker.state], ["running", "running"]);
      assert.equal(marked("boot").length, 2, "the microVM still ran: no second boot");
      assert.equal(endpointAt("shop").port, shopEndpoint.port, "the same port");
      // ⑤ stop resets the microVM and ends the forward; start boots it again
      assert.equal((await runtime.vm.stop("shop")).state, "stopped");
      assert.equal(marked("stop").length, 1);
      assert.equal(existsSync(join(state, "sessions", `${vmName("shop")}.json`)), false, "its forward record is gone");
      assert.equal((await deadline(60_000, "worker.start after stop", runtime.worker.start("shop"))).worker.state, "running");
      assert.equal(marked("boot").length, 3);
      // V8 and the delete that discards the Store disk file
      project("shop");
      await assert.rejects(runtime.vm.delete("shop", { confirm: "delete" }), { code: "Unsupported", message: /never destroys a Store that was not pushed/ });
      forget("shop");
      assert.equal((await runtime.vm.delete("shop", { confirm: "delete", discardStore: true })).store, "discarded");
      assert.equal(existsSync(join(state, "fc", "vms", vmName("shop"))), false, "the VM's folder is gone");
      assert.equal(existsSync(join(state, "fc", "disks", `${storeName("shop")}.json`)), false, "and its Store disk");
      assert.equal(existsSync(join(state, "fc", "disks", `${storeName("blog")}.json`)), true, "the other project's disk stays");
      assert.equal((await runtime.vm.status("blog")).worker.state, "running");
    } finally {
      restore();
      await runtime.close();
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
      const container = (await import("../src/place/vm/apple-container.js")).default;
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
      const msb = (await import("../src/place/vm/microsandbox.js")).default;
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
    throw new Error("select settings, flow, two, bundle, delete, hosts, stores, apple-container, ssh, lima, firecracker or cleanup");
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
  for (const provider of ["msb", "container", "ssh", "lima"]) {
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
          if (session.forwardPid ?? session.pid) process.kill(session.forwardPid ?? session.pid);
        } catch {}
      }
    }
  }
}

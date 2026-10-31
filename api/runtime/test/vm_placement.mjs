// @layerfs/runtime's `vm:<provider>` placement (#515 §6, V9) with a stand-in
// provider CLI and the real worker: no microVM, no Store, no daemon. The
// fixture keeps one VM record, maps the guest's absolute paths under a temp
// folder, and starts the adapter's bundle worker so step ④'s readiness is real.
//   node api/runtime/test/vm_placement.mjs settings|flow|bundle|delete|hosts|apple-container
import assert from "node:assert/strict";
import { chmodSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const FAKE = fileURLToPath(new URL("../../agent/test/fixture/fake-native.cjs", import.meta.url));
const SOURCE = fileURLToPath(new URL("../src", import.meta.url));
const MSB = fileURLToPath(new URL("fixture/fake-msb.cjs", import.meta.url));
const CONTAINER = fileURLToPath(new URL("fixture/fake-container.cjs", import.meta.url));
process.env.LAYERFS_NATIVE = FAKE;
const { LayerFSRuntime } = await import("@layerfs/runtime");
const { resolveSettings, missing, callHome, GUEST } = await import("../src/settings.js");

const selected = process.argv[2];
const work = mkdtempSync(join(tmpdir(), "layerfs-vm-"));
const home = join(work, "home"), bundle = join(work, "bundle"), prefix = "lfs-vm-test-";

/** A stand-in provider CLI on PATH: `name` shells out to the fixture with this state folder. */
function standIn(name, fixture, state) {
  const bin = join(work, "bin");
  mkdirSync(bin, { recursive: true });
  const file = join(bin, name);
  writeFileSync(file, `#!/bin/sh\nexec "${process.execPath}" "${fixture}" "$@"\n`);
  chmodSync(file, 0o755);
  const saved = process.env.PATH;
  process.env.PATH = `${bin}:${saved}`;
  process.env[`FAKE_${name === "msb" ? "MSB" : "CONTAINER"}_STATE`] = state;
  process.env[`FAKE_${name === "msb" ? "MSB" : "CONTAINER"}_NATIVE`] = FAKE;
  return () => { process.env.PATH = saved; };
}

/** A sealed Linux bundle with the adapter's worker and the real SDK packages it imports. */
function makeBundle(tag = "build-one") {
  rmSync(bundle, { recursive: true, force: true });
  const api = join(bundle, "api");
  cpSync(join(SOURCE, "worker"), join(api, "runtime/src/worker"), { recursive: true });
  cpSync(join(SOURCE, "../../node_modules/@layerfs"), join(api, "node_modules/@layerfs"), { recursive: true, dereference: true });
  writeFileSync(join(bundle, "manifest.json"), JSON.stringify({ version: 1, status: "sealed", platform: "linux", build_id: tag, files: {} }));
  return bundle;
}

const calls = (state) => (existsSync(join(state, "calls.jsonl")) ? readFileSync(join(state, "calls.jsonl"), "utf8").trim().split("\n").filter(Boolean).map((line) => JSON.parse(line).argv) : []);
const guestPath = (state, path) => join(state, "guest", path.replace(/^\//, ""));
const project = (name) => { mkdirSync(join(home, name), { recursive: true }); writeFileSync(join(home, name, "project.record"), "x"); };

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
    assert.throws(() => resolveSettings({ home, platform: "linux", placement: "cloud" }), { code: "InvalidInput", message: /local.*vm:<provider>/ });
    assert.throws(() => resolveSettings({ home, platform: "linux", placement: "vm:microsandbox", vm: { memory: "8" } }), { code: "InvalidInput", message: /memory/ });
    delete process.env.LAYERFS_PLACEMENT;
    delete process.env.LAYERFS_VM_CPUS;
    delete process.env.LAYERFS_VM_STORE_SIZE;
  } else if (selected === "flow") {
    makeBundle();
    const state = join(work, "msb");
    mkdirSync(state);
    const restore = standIn("msb", MSB, state);
    const runtime = await LayerFSRuntime.open({ home, bundle, placement: "vm:microsandbox", containerPrefix: prefix, platform: "linux", sweep: false });
    try {
      assert.equal(existsSync(join(home, "worker", "endpoint.json")), false);
      const started = await runtime.worker.start();
      assert.equal(started.worker.state, "running");
      assert.deepEqual([started.vm.state, started.vm.store], ["running", `${prefix}vm-store`]);
      const endpoint = JSON.parse(readFileSync(join(home, "worker", "endpoint.json"), "utf8"));
      assert.equal(endpoint.home, GUEST.home, "the endpoint names the VM's home");
      assert.equal(endpoint.vm.name, `${prefix}vm`);
      assert.deepEqual(Object.keys(endpoint.vm).sort(), ["name", "provider"]);
      assert.equal(process.env.LAYERFS_WORKER_ENDPOINT, undefined, "the endpoint is handed over in-process");

      const first = calls(state);
      const create = first.find((argv) => argv[0] === "create");
      assert.ok(create, "step ① created the VM");
      assert.ok(create.includes("--mount-named"), "the Store is a named disk volume");
      assert.equal(create[create.indexOf("--mount-named") + 1], `${prefix}vm-store:/layerfs/store:kind=disk,size=20G`);
      assert.equal(create[create.indexOf("-v") + 1], `${home}:/layerfs/home`);
      assert.equal(create[create.indexOf("-p") + 1], `127.0.0.1:${endpoint.port}:7070`);
      assert.equal(create.at(-1), "node:22-trixie-slim");
      assert.ok(first.some((argv) => argv[0] === "copy" && argv[2] === `${prefix}vm:/layerfs/bundle`), "step ② copied the bundle in");
      assert.ok(first.some((argv) => argv[0] === "exec" && /max_user_namespaces/.test(argv.join(" "))), "step ③ blocked user namespaces");
      assert.ok(first.some((argv) => argv[0] === "exec" && /nohup node/.test(argv.join(" "))), "step ③ started the worker");
      assert.equal(readFileSync(guestPath(state, "/layerfs/state/worker.env"), "utf8").includes(endpoint.secret), true, "the secret reached the VM in a file, not in a command line");
      assert.ok(!JSON.stringify(first).includes(endpoint.secret), "no call carried the secret in its arguments");

      // the same home again: the VM and its bundle are reused, and only the VM is read
      const counted = () => calls(state).filter((argv) => argv[0] === "create" || argv[0] === "copy" || argv[0] === "exec").length;
      const before = counted();
      await runtime.worker.start();
      assert.equal(counted(), before, "a running worker with its bundle is reused");

      // a stopped VM is started, not created
      await runtime.vm.stop();
      assert.equal(JSON.parse(readFileSync(join(state, "vm.json"), "utf8")).state, "stopped");
      const restarted = await runtime.worker.start();
      assert.equal(restarted.worker.state, "running");
      assert.equal(calls(state).filter((argv) => argv[0] === "create").length, 1, "the VM was started, not created again");

      // status: host Docker is not this home's, the VM carries its own state
      const state_ = await runtime.status();
      assert.deepEqual([state_.placement, state_.docker.level], ["vm:microsandbox", "not-used"]);
      assert.equal(state_.vm.state, "running");
      assert.ok(state_.disk.volumes > 0, "the VM's Store disk has free bytes");
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
      await runtime.worker.start();
      const copies = () => calls(state).filter((argv) => argv[0] === "copy").length;
      assert.equal(copies(), 1);
      // the same bundle hash: step ② is not repeated
      await runtime.worker.start();
      assert.equal(copies(), 1);
      // a changed bundle is copied again, and the worker is restarted onto it
      makeBundle("build-two");
      const restarted = await runtime.worker.start();
      assert.equal(copies(), 2, "a changed bundle hash is copied once more");
      assert.equal(restarted.worker.build, "build-two");
      assert.equal(readFileSync(guestPath(state, "/layerfs/bundle/.sealed"), "utf8").trim().length, 64);
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
      await runtime.worker.start();
      // V8: a Store that lives only on the VM's disk is never destroyed by a delete
      project("shop");
      await assert.rejects(runtime.vm.delete(), { code: "InvalidInput", message: /confirm/ });
      await assert.rejects(runtime.vm.delete({ confirm: "delete" }), { code: "Unsupported", message: /never destroys a Store that was not pushed/ });
      assert.equal(existsSync(join(state, "vm.json")), true, "the VM is untouched by the refusal");
      // with no project left, the VM record goes and its Store disk stays unless asked
      rmSync(join(home, "shop"), { recursive: true, force: true });
      assert.deepEqual(await runtime.vm.delete({ confirm: "delete" }), { name: `${prefix}vm`, state: "deleted", store: "kept" });
      assert.equal(existsSync(join(state, "vm.json")), false);
      assert.equal(calls(state).some((argv) => argv[0] === "volume"), false, "the Store disk is kept by default");
      await runtime.vm.delete({ confirm: "delete", discardStore: true });
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
      await runtime.worker.start();
      // host Docker describes another machine: the Docker-shaped helpers refuse by name
      await assert.rejects(runtime.sandbox.list(), { code: "Unsupported", message: /runtime\.sandbox\.list reads host Docker.*project\.list_sandboxes/ });
      await assert.rejects(runtime.sandbox.list_leftovers(), { code: "Unsupported", message: /Linux namespaces inside the VM/ });
      await assert.rejects(runtime.volume.list(), { code: "Unsupported", message: /directories on the VM's disk/ });
      assert.deepEqual(await runtime.sandbox.sweep(), { reclaimed: {}, failed: [], host: false });
      assert.equal(runtime.sandbox.sweep !== undefined, true);
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
      const started = await runtime.worker.start();
      assert.equal(started.vm.state, "running");
      const run = calls(state).find((argv) => argv[0] === "run");
      assert.ok(run, "the VM was created with `container run`");
      const capabilities = run.flatMap((value, index) => (value === "--cap-add" ? [run[index + 1]] : []));
      assert.deepEqual(capabilities, ["SYS_ADMIN", "SYS_RESOURCE", "NET_ADMIN"], "#512's capabilities for FUSE, cgroups and loopback");
      assert.ok(run.includes("--publish"));
      assert.ok(calls(state).some((argv) => argv[0] === "exec" && /nohup node/.test(argv.join(" "))));
    } finally {
      restore();
      await runtime.close();
      restoreWorker(work);
    }
  } else {
    throw new Error("select settings, flow, bundle, delete, hosts or apple-container");
  }
} finally {
  rmSync(work, { recursive: true, force: true });
}
console.log(`PASS vm_placement/${selected}`);

/** The stand-in's worker outlives its CLI: end it from the pid the fixture wrote. */
function restoreWorker(folder) {
  const pids = [join(folder, "msb", "guest", "layerfs/state/worker.pid"), join(folder, "container", "guest", "layerfs/state/worker.pid")];
  for (const file of pids) {
    if (!existsSync(file)) continue;
    try { process.kill(Number(readFileSync(file, "utf8").trim())); } catch { /* already gone */ }
  }
}

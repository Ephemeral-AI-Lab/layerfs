// A Sandbox's platform (#529 §4–§5): which platforms each home allows, its
// default, the refusals (before any call), the deployment each platform
// starts with, and the read-time default of a record from before #529.
// Pure functions over a stand-in client; no binding call is made.
//   node api/agent/test/platform.mjs
import assert from "node:assert/strict";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { STATE } from "../src/client.js";
import { PLATFORMS, checkedPlatform, homeOf, platformDeployment, platformOf } from "../src/platform.js";
import { workerClient } from "../src/transport/worker_client.js";

const IMAGE = `sha256:${"a".repeat(64)}`;
const DOCKER = { docker: "/var/run/docker.sock", image: IMAGE, volume: "vol", daemon: "/d/layerfs-daemon" };
const LOCAL = { localDir: "/var/lib/lfs", imageArchive: "/b/images/a.tar" };
const client = (deploy, transport = {}) => ({ options: { deploy }, [STATE]: { transport } });
const refused = (code, pattern) => (error) => error.code === code && pattern.test(error.message);
for (const name of ["LAYERFS_SANDBOX", "LAYERFS_SANDBOX_BACKEND", "LAYERFS_IMAGE", "LAYERFS_LOCAL_DIR", "LAYERFS_IMAGE_ARCHIVE"]) delete process.env[name];

assert.deepEqual(PLATFORMS, ["docker", "mac", "linux", "apple-container", "microsandbox", "lima", "firecracker", "ssh"]);

// homes (§4)
const linux = client({ ...DOCKER, ...LOCAL });
assert.deepEqual(homeOf(linux, "linux"), { name: "Linux local", allowed: ["docker", "linux"], default: "docker" });
assert.deepEqual(homeOf(client({ ...DOCKER, ...LOCAL, sandbox: "namespaces" }), "linux").default, "linux");
assert.deepEqual(homeOf(client({ ...LOCAL, sandbox: "native", daemon: "/d" }), "linux").default, "linux");
assert.deepEqual(homeOf(client(DOCKER), "darwin"), { name: "macOS local", allowed: ["docker"], default: "docker" });
assert.deepEqual(homeOf(client({ sandbox: "native", daemon: "/d", localDir: "/l" }), "darwin"), { name: "macOS local (native)", allowed: ["mac"], default: "mac" });
assert.deepEqual(homeOf(client(DOCKER), "win32").allowed, ["docker"]);

// workers: a vm: endpoint allows its provider; another worker Docker; none running is Failed
const dir = mkdtempSync(join(tmpdir(), "lfs-platform-"));
try {
  const vmFile = join(dir, "vm.json"), macFile = join(dir, "mac.json");
  writeFileSync(vmFile, JSON.stringify({ home: "/layerfs/home", port: 9, secret: "a".repeat(64), vm: { provider: "lima", name: "lfs-x", project: "x" } }));
  writeFileSync(macFile, JSON.stringify({ home: "/h", port: 9, secret: "b".repeat(64) }));
  const vm = client({ sandbox: "namespaces", ...LOCAL, image: IMAGE, daemon: "/layerfs/bundle/layerfs-daemon" }, workerClient(vmFile));
  assert.deepEqual(homeOf(vm, "darwin"), { name: "vm:lima", allowed: ["lima"], default: "lima" });
  assert.deepEqual(homeOf(client(DOCKER, workerClient(macFile)), "darwin"), { name: "macOS local (worker)", allowed: ["docker"], default: "docker" });
  assert.deepEqual(homeOf(client(DOCKER, workerClient(macFile)), "win32").name, "Windows local (worker)");
  assert.throws(() => homeOf(client(DOCKER, workerClient(join(dir, "none.json"))), "darwin"), refused("Failed", /worker is not running/));
  // a vm: home's default is its provider, a namespaces deployment made in its Linux VM
  const { home, platform } = checkedPlatform(vm, undefined, "darwin");
  assert.equal(platform, "lima");
  const vmDeploy = platformDeployment(vm, home, platform, undefined, "darwin");
  assert.equal(vmDeploy.sandbox, "namespaces");
  // its image is the one its archive holds (the binding reads it), unless the caller gives one
  assert.deepEqual([vmDeploy.image, "image" in vmDeploy, vmDeploy.imageArchive], [undefined, false, LOCAL.imageArchive]);
  assert.equal(platformDeployment(vm, home, platform, `sha256:${"c".repeat(64)}`, "darwin").image, `sha256:${"c".repeat(64)}`);
  for (const other of ["docker", "mac", "linux", "ssh"]) assert.throws(() => checkedPlatform(vm, other, "darwin"), refused("Unsupported", /not available on this vm:lima home, which allows lima/));
  // a record from before #529 on a vm: home reads as its provider
  assert.equal(platformOf({ runtime: "namespaces" }, home, "darwin"), "lima");
  // so does a native one: a vm: home allows its provider alone (§4)
  assert.equal(platformOf({ runtime: "native" }, home, "darwin"), "lima");
  assert.equal(platformOf({ runtime: "native" }, home, "linux"), "lima");
} finally {
  rmSync(dir, { recursive: true, force: true });
}

// refusals: every disallowed known platform is Unsupported, any other value InvalidInput
const homes = [[linux, "linux"], [client(DOCKER), "darwin"], [client({ sandbox: "native", daemon: "/d", localDir: "/l" }), "darwin"], [client(DOCKER), "win32"]];
for (const [one, os] of homes) {
  const { allowed } = homeOf(one, os);
  for (const name of PLATFORMS.filter((p) => !allowed.includes(p))) assert.throws(() => checkedPlatform(one, name, os), refused("Unsupported", new RegExp(`platform ${name} is not available`)), `${os} ${name}`);
  for (const bad of ["windows", "native", "namespaces", "podman", "", 7, null]) assert.throws(() => checkedPlatform(one, bad, os), refused("InvalidInput", /platform is one of docker, mac, linux, apple-container, microsandbox, lima, firecracker, ssh/), `${os} ${bad}`);
}
// a default the home does not allow (namespaces on macOS) is Unsupported too
assert.throws(() => checkedPlatform(client({ sandbox: "namespaces", daemon: "/d", localDir: "/l" }), undefined, "darwin"), refused("Unsupported", /platform linux is not available on this macOS local home.*default/));

// deployments: a Linux home's docker default keeps its settings; linux names namespaces
{
  const home = homeOf(linux, "linux");
  const docker = platformDeployment(linux, home, "docker", undefined, "linux");
  assert.equal(docker.sandbox, undefined);
  assert.equal(docker.volume, "vol");
  assert.equal(docker.image, IMAGE, "docker keeps LAYERFS_IMAGE");
  const own = platformDeployment(linux, home, "linux", `sha256:${"b".repeat(64)}`, "linux");
  assert.deepEqual([own.sandbox, own.sandboxBackend, own.image, own.localDir, own.imageArchive], ["namespaces", undefined, `sha256:${"b".repeat(64)}`, LOCAL.localDir, LOCAL.imageArchive]);
  // no image given: linux leaves Docker's LAYERFS_IMAGE and takes the archive's own (#529)
  const derived = platformDeployment(linux, home, "linux", undefined, "linux");
  assert.deepEqual([derived.sandbox, "image" in derived, derived.imageArchive], ["namespaces", false, LOCAL.imageArchive]);
  // a namespaces home's own default too; with no archive namespaces is refused, as before
  const nsHome = client({ ...DOCKER, ...LOCAL, sandbox: "namespaces" });
  assert.equal("image" in platformDeployment(nsHome, homeOf(nsHome, "linux"), "linux", undefined, "linux"), false);
  process.env.LAYERFS_IMAGE = IMAGE;
  try {
    const noArchive = client({ ...DOCKER, localDir: "/l", sandbox: "namespaces" });
    assert.throws(() => platformDeployment(noArchive, homeOf(noArchive, "linux"), "linux", undefined, "linux"), refused("InvalidInput", /imageArchive is not configured/));
  } finally {
    delete process.env.LAYERFS_IMAGE;
  }
  // a native Linux home's linux is native, and its docker needs Docker's settings
  const nativeHome = client({ sandbox: "native", daemon: "/d", localDir: "/l" });
  assert.equal(platformDeployment(nativeHome, homeOf(nativeHome, "linux"), "linux", undefined, "linux").sandbox, "native");
  assert.throws(() => platformDeployment(nativeHome, homeOf(nativeHome, "linux"), "docker", undefined, "linux"), refused("InvalidInput", /deploy\.(image|volume) is not configured/));
  // the former pair (native + namespaces) is the default as it is; docker drops it
  const former = client({ ...DOCKER, ...LOCAL, sandbox: "native", sandboxBackend: "namespaces" });
  const formerHome = homeOf(former, "linux");
  assert.equal(formerHome.default, "linux");
  assert.equal(platformDeployment(former, formerHome, "linux", undefined, "linux").sandboxBackend, "namespaces");
  const toDocker = platformDeployment(former, formerHome, "docker", undefined, "linux");
  assert.deepEqual([toDocker.sandbox, toDocker.sandboxBackend], [undefined, undefined]);
}

// read-time default of a record from before #529 (no platform)
assert.equal(platformOf({ platform: "linux", runtime: "namespaces" }, homeOf(linux, "linux"), "linux"), "linux");
assert.equal(platformOf({ runtime: "docker" }, homeOf(linux, "linux"), "linux"), "docker");
assert.equal(platformOf({}, homeOf(linux, "linux"), "linux"), "docker");
assert.equal(platformOf({ runtime: "native" }, homeOf(linux, "linux"), "linux"), "linux");
assert.equal(platformOf({ runtime: "namespaces" }, homeOf(linux, "linux"), "linux"), "linux");
const macHome = client({ sandbox: "native", daemon: "/d", localDir: "/l" });
assert.equal(platformOf({ runtime: "native" }, homeOf(macHome, "darwin"), "darwin"), "mac");
console.log("platform: ok");

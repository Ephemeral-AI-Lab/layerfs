// Which runtime a Sandbox uses. No real addon, Store or Docker (the
// stand-in binding answers the client's calls):
//   node api/agent/sdk/test/local_deployment.mjs
// LAYERFS_SANDBOX unset or `docker` gives exactly the Docker deployment it
// always gave; `local` needs only the daemon and LAYERFS_LOCAL_DIR (no Docker
// setting); any other value is refused rather than taken as either.
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { deployment } from "../src/client.js";

const FAKE = fileURLToPath(new URL("./fixture/fake-native.cjs", import.meta.url));
const native = createRequire(import.meta.url)(FAKE);

const clear = () => {
  for (const name of Object.keys(process.env)) {
    if (name.startsWith("LAYERFS_") || name.startsWith("DSH_LAYERFS_")) delete process.env[name];
  }
  process.env.LAYERFS_NATIVE = FAKE;
};
const docker = { docker: "/run/docker.sock", image: `sha256:${"a".repeat(64)}`, volume: "lfs-v", daemon: "/opt/layerfs-daemon" };
const setDocker = () => {
  process.env.LAYERFS_DOCKER = docker.docker;
  process.env.LAYERFS_IMAGE = docker.image;
  process.env.LAYERFS_VOLUME = docker.volume;
  process.env.LAYERFS_DAEMON = docker.daemon;
};
const store = () => {
  process.env.LAYERFS_HOME = "/private/local-home";
  process.env.LAYERFS_STORE_PATH = "/fake/store.sqlite";
};

// unset: the Docker deployment, field for field
clear();
setDocker();
assert.deepEqual(deployment(), docker);
// `docker` is the same deployment
process.env.LAYERFS_SANDBOX = "docker";
assert.deepEqual(deployment(), docker);
// the Docker settings are still required there
delete process.env.LAYERFS_IMAGE;
assert.throws(() => deployment(), (e) => e.code === "InvalidInput" && /LAYERFS_IMAGE/.test(e.message));

// local: daemon + localDir, no Docker setting read or required
clear();
process.env.LAYERFS_SANDBOX = "local";
process.env.LAYERFS_DAEMON = "/opt/layerfs-daemon";
process.env.LAYERFS_LOCAL_DIR = "/store/local";
assert.deepEqual(deployment(), { sandbox: "local", daemon: "/opt/layerfs-daemon", localDir: "/store/local" });
// an image (sandbox.create's option) does not change a local deployment
assert.deepEqual(deployment({ image: docker.image }), { sandbox: "local", daemon: "/opt/layerfs-daemon", localDir: "/store/local" });
// the container prefix names local Sandbox directories too
process.env.LAYERFS_CONTAINER_PREFIX = "ci-";
assert.equal(deployment().containerPrefix, "ci-");
delete process.env.LAYERFS_CONTAINER_PREFIX;
// each required setting is named when missing
delete process.env.LAYERFS_LOCAL_DIR;
assert.throws(() => deployment(), (e) => e.code === "InvalidInput" && /LAYERFS_LOCAL_DIR/.test(e.message));
process.env.LAYERFS_LOCAL_DIR = "/store/local";
delete process.env.LAYERFS_DAEMON;
assert.throws(() => deployment(), (e) => e.code === "InvalidInput" && /LAYERFS_DAEMON/.test(e.message));
// since #428 the SDK reads LAYERFS_ names only: a DSH_LAYERFS_ twin selects nothing
clear();
setDocker();
process.env.DSH_LAYERFS_SANDBOX = "local";
process.env.DSH_LAYERFS_LOCAL_DIR = "/l";
assert.deepEqual(deployment(), docker);

// anything else is refused, never silently Docker
for (const typo of ["Local", "lokal", "LOCAL", "podman"]) {
  clear();
  setDocker();
  process.env.LAYERFS_SANDBOX = typo;
  assert.throws(() => deployment(), (e) => e.code === "InvalidInput" && e.message.includes(`docker or local, not ${typo}`));
}

// a client reads LAYERFS_SANDBOX and LAYERFS_LOCAL_DIR once, and
// sandbox.create sends the local deployment (no Docker setting)
clear();
store();
process.env.LAYERFS_SANDBOX = "local";
process.env.LAYERFS_DAEMON = "/opt/layerfs-daemon";
process.env.LAYERFS_LOCAL_DIR = "/store/local";
const { LayerFS } = await import("../src/index.js");
const client = await LayerFS.open("website");
assert.equal(client.options.deploy.sandbox, "local");
assert.equal(client.options.deploy.localDir, "/store/local");
const before = native.deploys.length;
await client.sandbox.create();
assert.deepEqual(native.deploys.slice(before), [{ sandbox: "local", daemon: "/opt/layerfs-daemon", localDir: "/store/local" }]);
// an image given to sandbox.create does not turn it into a Docker deployment
await client.sandbox.create({ image: docker.image });
assert.deepEqual(native.deploys.at(-1), { sandbox: "local", daemon: "/opt/layerfs-daemon", localDir: "/store/local" });
await client.close();
// unset: the client carries no new key
clear();
store();
setDocker();
const plain = await LayerFS.open("website");
assert.equal("sandbox" in plain.options.deploy, false);
assert.equal("localDir" in plain.options.deploy, false);
await plain.close();
console.log("local_deployment: ok");

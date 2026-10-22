// Which runtime a Workspace's Sandbox uses. No addon, Store or Docker:
//   node sdk/js/test/local_deployment.mjs
// LAYERFS_SANDBOX unset or `docker` gives exactly the Docker deployment it
// always gave; `local` needs only the daemon and LAYERFS_LOCAL_DIR (no Docker
// setting); any other value is refused rather than taken as either.
import assert from "node:assert/strict";
import { LayerFS } from "@layerfs/sdk";
import { deployment } from "../src/client.js";

const clear = () => {
  for (const name of Object.keys(process.env)) {
    if (name.startsWith("LAYERFS_") || name.startsWith("DSH_LAYERFS_")) delete process.env[name];
  }
};
const docker = { docker: "/run/docker.sock", image: `sha256:${"a".repeat(64)}`, volume: "lfs-v", daemon: "/opt/layerfs-daemon" };
const setDocker = () => {
  process.env.LAYERFS_DOCKER = docker.docker;
  process.env.LAYERFS_IMAGE = docker.image;
  process.env.LAYERFS_VOLUME = docker.volume;
  process.env.LAYERFS_DAEMON = docker.daemon;
};

// unset: the Docker deployment, field for field, and the client options carry no new key
clear();
setDocker();
assert.deepEqual(deployment(), docker);
assert.deepEqual(new LayerFS("website").options.deploy, docker);
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
const client = new LayerFS("website");
assert.equal(client.options.deploy.sandbox, "local");
assert.equal(client.options.deploy.localDir, "/store/local");
assert.deepEqual(deployment(client.options.deploy), { sandbox: "local", daemon: "/opt/layerfs-daemon", localDir: "/store/local" });
// an image (workspace.mount's option) does not change a local deployment
assert.deepEqual(deployment({ ...client.options.deploy, image: docker.image }), { sandbox: "local", daemon: "/opt/layerfs-daemon", localDir: "/store/local" });
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
// the DSH_ twins work as for every other setting
clear();
process.env.DSH_LAYERFS_SANDBOX = "local";
process.env.DSH_LAYERFS_DAEMON = "/opt/d";
process.env.DSH_LAYERFS_LOCAL_DIR = "/l";
assert.deepEqual(deployment(), { sandbox: "local", daemon: "/opt/d", localDir: "/l" });

// anything else is refused, never silently Docker
for (const typo of ["Local", "lokal", "LOCAL", "podman"]) {
  clear();
  setDocker();
  process.env.LAYERFS_SANDBOX = typo;
  assert.throws(() => deployment(), (e) => e.code === "InvalidInput" && e.message.includes(`docker or local, not ${typo}`));
}
console.log("local_deployment: ok");

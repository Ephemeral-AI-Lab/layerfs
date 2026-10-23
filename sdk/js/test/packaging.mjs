// Package admission checks. Each case is one command and creates no Store.
import assert from "node:assert/strict";
import { LayerFS } from "@layerfs/sdk";
import { LayerFSOperator } from "@layerfs/sdk/operator";
import { native } from "../src/native.js";

const selected = process.argv[2];
if (selected === "lazy") {
  process.env.LAYERFS_NATIVE = "/nonexistent-layerfs-addon";
  process.env.LAYERFS_HOME = "/private/configured-home";
  process.env.LAYERFS_DOCKER = "/configured/docker.sock";
  process.env.LAYERFS_IMAGE = "configured-image";
  process.env.LAYERFS_VOLUME = "configured-volume";
  process.env.LAYERFS_DAEMON = "/configured/daemon";
  const client = new LayerFS("website");
  const operator = new LayerFSOperator(client);
  assert.equal(client.projectName, "website");
  assert.equal(client.sandbox, undefined);
  assert.equal(client.service, undefined);
  assert.equal(client.options.home, "/private/configured-home");
  assert.equal(client.options.deploy.docker, "/configured/docker.sock");
  assert.equal(client.options.deploy.image, "configured-image");
  assert.equal(client.options.deploy.volume, "configured-volume");
  assert.equal(client.options.deploy.daemon, "/configured/daemon");
  assert.equal(client.inspect, undefined);
  for (const method of ["mount", "exec", "status", "commit", "unmount"]) {
    assert.equal(typeof client.workspace[method], "function", method);
  }
  assert.equal(client.workspace.cancel, undefined);
  assert.equal(client.project.list_overwritten, undefined);
  assert.equal(typeof operator.store.overwritten, "function");
  assert.equal(typeof operator.inspect.snapshot, "function");
  assert.equal(typeof operator.process.cancel, "function");
  await assert.rejects(client.call("backend.store.project.get_history", {}), { code: "UnknownMethod" });
  assert.throws(() => new LayerFS("../foreign"), { code: "InvalidInput" });
} else if (selected === "dispatch") {
  const binding = native();
  assert.equal(typeof binding.call, "function");
  assert.equal(typeof binding.backendCall, "function");
  for (const method of ["backend.store.project.get_history", "backend.daemon.process.list", "runtime.sandbox.reclaim"]) {
    assert.equal((await binding.call(method, {})).refused.code, "UnknownMethod");
  }
  assert.equal((await binding.backendCall("project.init", {})).refused.code, "UnknownMethod");
  assert.equal((await binding.backendCall("backend.missing", {})).refused.code, "UnknownMethod");
} else {
  throw new Error("select lazy or dispatch");
}
console.log(`PASS packaging/${selected}`);

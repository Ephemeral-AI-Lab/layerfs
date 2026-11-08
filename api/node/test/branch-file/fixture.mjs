// Disposable, mountless fixtures driven only through the public SDK. A caller
// selects one real route per command; a PostgreSQL database must be private.
import assert from "node:assert/strict";
import { mkdtempSync, chmodSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { client, close } from "../../src/index.js";

export async function fixture() {
  const route = process.env.LAYERFS_BRANCH_FILE_ROUTE ?? "native";
  assert.ok(["native", "postgresql", "worker"].includes(route), `Unknown route ${route}`);
  const home = mkdtempSync(join(tmpdir(), "layerfs-branch-file-home-"));
  chmodSync(home, 0o700);
  const folder = mkdtempSync(join(tmpdir(), "layerfs-branch-file-src-"));
  const lfs = client({ home });
  const project = "sdk/api";
  // Print custody before the first operation, so failed runs remain diagnosable.
  console.log(JSON.stringify({ route, home, folder, project }));
  writeFileSync(join(folder, "base.txt"), "base\nkept\n");
  writeFileSync(join(folder, "remove.txt"), "delete me\n");
  if (route === "postgresql") {
    const url = process.env.LAYERFS_BRANCH_FILE_PG_URL;
    assert.ok(url, "Set LAYERFS_BRANCH_FILE_PG_URL to a fresh private PostgreSQL database");
    const store = await lfs.store.connect("sdk", url, { synchronous_commit: "off" });
    assert.equal(store.synchronous_commit, "off");
  } else {
    assert.equal((await lfs.store.create("sdk")).profile, "disposable");
  }
  const init = await lfs.project.init(project, folder);
  await lfs.branch.fork(project, "left", "main");
  await lfs.branch.fork(project, "right", "main");
  const left = await lfs.file.write(project, "left", "/base.txt", "alpha\nkept\n", { message: "left independently changes line one" });
  const right = await lfs.file.write(project, "right", "/base.txt", "alpha\nkept\n", { message: "right independently changes line one" });
  const published = await lfs.branch.publish(project, "left");
  await lfs.file.write(project, "left", "/new.txt", "added\n", { message: "add a path" });
  await lfs.file.rm(project, "left", "/remove.txt", { message: "remove a path" });
  await lfs.file.write(project, "left", "/blob.bin", Buffer.from([0, 255, 1]), { message: "binary" });
  await lfs.file.write(project, "left", "/long.txt", `${"x".repeat(70 * 1024)}\n`, { message: "single line exceeds a reply page" });
  await lfs.file.write(project, "left", "/base.txt", "alpha\nkept\n", { mode: 0o600, message: "same bytes different mode" });
  const head = (await lfs.branch.get(project, "left")).head;
  if (route === "worker") {
    assert.ok(process.env.LAYERFS_BUNDLE, "Set LAYERFS_BUNDLE to the matching sealed Linux worker bundle");
    await lfs.machine.update("docker", { image: process.env.LAYERFS_BRANCH_FILE_IMAGE ?? "layerfs-352-runtime:node24-https-apt-20261024" });
    await lfs.machine.start("docker");
    await lfs.store.push("sdk", "docker");
    assert.equal((await lfs.store.get("sdk")).machine, "docker");
  }
  return {
    route, home, folder, lfs, project, init: init.layer, head,
    source: left.head, other: right.head, publication: published.published_layer.id,
    async cleanup() {
      // Never conceal a failed assertion by destroying its Store. The caller
      // invokes cleanup only after all assertions pass.
      if (route === "worker") {
        await lfs.store.remove("sdk");
        await lfs.machine.stop("docker");
      }
      close();
      rmSync(home, { recursive: true, force: true });
      rmSync(folder, { recursive: true, force: true });
    },
  };
}

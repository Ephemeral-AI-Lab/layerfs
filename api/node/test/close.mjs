// `layerfs.close()` over the real addon: one top-level export, outside the
// call table, that closes the Stores this process holds open and so drops
// their leases; a later call opens its Store again.
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, writeFileSync, chmodSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import layerfs, { client, close, table } from "../src/index.js";

const temp = (tag) => {
  const dir = mkdtempSync(join(tmpdir(), `layerfs-node-${tag}-`));
  chmodSync(dir, 0o700);
  return dir;
};

test("close drops this process's leases, and a later call opens its Store again", async () => {
  assert.equal(layerfs.close, close);
  // outside the table: its families keep only their own calls (sandbox.close is one)
  assert.ok(table.every((spec) => spec.family !== "close"));
  const home = temp("close");
  const folder = temp("close-folder");
  writeFileSync(join(folder, "README.md"), "hello close\n");
  const lfs = client({ home });
  await lfs.store.create("shop");
  await lfs.project.init("shop/api", folder);
  await lfs.file.read("shop/api", "main", "/README.md");
  const lease = join(home, "store", "shop", "open", String(process.pid));
  assert.ok(existsSync(lease), "an open Store holds this process's lease");
  assert.ok(close() >= 1);
  assert.ok(!existsSync(lease), "close left the lease");
  assert.equal(close(), 0, "nothing is open any more");
  const read = await lfs.file.read("shop/api", "main", "/README.md");
  assert.equal(read.text, "hello close\n");
  assert.ok(existsSync(lease), "the next call holds the lease again");
  close();
  rmSync(home, { recursive: true, force: true });
  rmSync(folder, { recursive: true, force: true });
});

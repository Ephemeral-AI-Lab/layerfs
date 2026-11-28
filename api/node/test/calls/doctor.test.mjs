// The doctors over the real addon (call surface 2.0): `home.doctor()` and
// its five siblings are table calls that report and change nothing;
// `home.leftovers()` and `home.prune()` are gone (spec v20: LayerFS removes
// leftovers itself).
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, mkdirSync, writeFileSync, chmodSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import layerfs, { table, LayerFSError } from "../../src/index.js";

test("home.doctor reports a home without making it, as a table call", async () => {
  const names = table.filter((spec) => spec.call === "doctor").map((spec) => `${spec.family}.${spec.call}`);
  assert.deepEqual(names.sort(), ["backup.doctor", "home.doctor", "machine.doctor", "project.doctor", "sandbox.doctor", "store.doctor"]);
  const parent = mkdtempSync(join(tmpdir(), "layerfs-node-doctor-"));
  const home = join(parent, "home");
  const report = await layerfs.client({ root: parent }).home.doctor({ detail: true });
  assert.deepEqual(Object.keys(report), ["ok", "findings", "not_checked", "status", "counts", "observed_at", "tree"]);
  assert.equal(report.tree.coverage, "shallow");
  assert.ok(!existsSync(home), "the doctor made the home");
  rmSync(parent, { recursive: true, force: true });
});

test("home.doctor names a record that does not parse and its fix", async () => {
  const root = mkdtempSync(join(tmpdir(), "layerfs-node-doctor-"));
  chmodSync(root, 0o700);
  const home = join(root, "home");
  const lfs = layerfs.client({ root });
  await lfs.store.list();
  await assert.rejects(lfs.machine.doctor("missing"), (error) => error instanceof LayerFSError && error.code === "NotFound");
  mkdirSync(join(home, "store", "bad"), { recursive: true, mode: 0o700 });
  writeFileSync(join(home, "store", "bad", "record.json"), "{not json", { mode: 0o600 });
  const report = await lfs.home.doctor();
  assert.equal(report.ok, undefined);
  const finding = report.findings.find((found) => found.store === "bad");
  assert.equal(finding.check, "home.records");
  assert.equal(finding.severity, "error");
  assert.match(finding.message, /by hand/);
  assert.equal(finding.fix, undefined);
  rmSync(root, { recursive: true, force: true });
});

test("home.leftovers and home.prune are gone (spec v20: LayerFS removes leftovers itself)", () => {
  const home = table.filter((spec) => spec.family === "home").map((spec) => spec.call);
  assert.deepEqual(home, ["init", "get", "log", "clear_log", "doctor", "move"]);
  assert.equal(layerfs.home.leftovers, undefined);
  assert.equal(layerfs.home.prune, undefined);
  const lfs = layerfs.client({ root: join(tmpdir(), "layerfs-node-doctor-gone") });
  assert.equal(lfs.home.leftovers, undefined);
  assert.equal(lfs.home.prune, undefined);
});

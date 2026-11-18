// The doctors over the real addon (call surface 2.0): `home.doctor()` and
// its five siblings are table calls that report and change nothing; and
// `home.leftovers()` and `home.prune(leftover)`, which list what earlier
// runs left and remove one of them (layerfs-project#180).
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, mkdirSync, writeFileSync, chmodSync, rmSync, utimesSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import layerfs, { table, LayerFSError } from "../src/index.js";

test("home.doctor reports a home without making it, as a table call", async () => {
  const names = table.filter((spec) => spec.call === "doctor").map((spec) => `${spec.family}.${spec.call}`);
  assert.deepEqual(names.sort(), ["backup.doctor", "home.doctor", "machine.doctor", "project.doctor", "sandbox.doctor", "store.doctor"]);
  const parent = mkdtempSync(join(tmpdir(), "layerfs-node-doctor-"));
  const home = join(parent, "home");
  const report = await layerfs.client({ home }).home.doctor({ detail: true });
  assert.deepEqual(Object.keys(report), ["ok", "findings", "not_checked", "status", "counts", "observed_at", "tree"]);
  assert.equal(report.tree.coverage, "shallow");
  assert.ok(!existsSync(home), "the doctor made the home");
  rmSync(parent, { recursive: true, force: true });
});

test("home.doctor names a record that does not parse and its fix", async () => {
  const home = mkdtempSync(join(tmpdir(), "layerfs-node-doctor-"));
  chmodSync(home, 0o700);
  const lfs = layerfs.client({ home });
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
  rmSync(home, { recursive: true, force: true });
});

test("home.prune removes a leftover home.leftovers listed, on the package and on a client", async () => {
  const home = mkdtempSync(join(tmpdir(), "layerfs-node-doctor-"));
  chmodSync(home, 0o700);
  const lfs = layerfs.client({ home });
  await lfs.store.list();
  // an unpack's temporary folder, its image's lock, and a partly received
  // image (no leftover since spec v19: `machine prune` owns it); each folder
  // 0700, whatever the umask
  let images = home;
  for (const part of ["machine", "native", "run", "images"]) {
    images = join(images, part);
    mkdirSync(images, { recursive: true, mode: 0o700 });
    chmodSync(images, 0o700);
  }
  const digest = "ab".repeat(32);
  const tmp = join(images, `.tmp-${digest}.2-ab12`);
  mkdirSync(join(tmp, "usr", "bin"), { recursive: true, mode: 0o700 });
  for (const folder of [tmp, join(tmp, "usr"), join(tmp, "usr", "bin")]) chmodSync(folder, 0o700);
  writeFileSync(join(tmp, "usr", "bin", "tool"), "x".repeat(50));
  const part = join(images, `.recv-${digest}.part`);
  writeFileSync(part, "x".repeat(50));
  const then = Date.now() / 1000 - 7200;
  utimesSync(part, then, then);
  writeFileSync(join(images, `${digest}.2.lock`), "");

  // the list names the temporary by its id; the lock and the part are no
  // leftover
  const id = `native:image_tmp:${digest}.2-ab12`;
  const listed = await lfs.home.leftovers({ machine: "native" });
  assert.deepEqual(listed.items.map((item) => item.leftover), [id]);

  // the same function on the package and on a client; refusals carry a code
  assert.equal(typeof layerfs.home.prune, "function");
  assert.equal(typeof lfs.home.prune, "function");
  assert.equal(layerfs.doctor, undefined, "the doctor family is gone");
  const refused = (code) => (error) => error instanceof LayerFSError && error.code === code;
  await assert.rejects(lfs.home.prune(`native:image_lock:${digest}.2`), refused("NotFound"));
  await assert.rejects(lfs.home.prune("native:image_tmp:../../x"), refused("NotFound"));
  await assert.rejects(lfs.home.prune(`native:image_part:${digest}`), refused("NotFound"));
  await assert.rejects(lfs.home.prune(), refused("InvalidInput"));
  const removed = await lfs.home.prune(id, { detail: true });
  // a temporary is removed without a count
  assert.deepEqual(removed, { ok: true, freed_bytes: null, outcome: "removed", leftover: id, kind: "image_tmp", machine: "native", path: tmp });
  assert.ok(!existsSync(tmp));
  assert.ok(existsSync(part), "the part stays for `machine prune`");
  assert.ok(existsSync(join(images, `${digest}.2.lock`)), "the lock file stays");
  await assert.rejects(lfs.home.prune(id), refused("NotFound"));
  rmSync(home, { recursive: true, force: true });
});

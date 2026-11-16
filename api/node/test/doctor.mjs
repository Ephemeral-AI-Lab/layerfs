// `layerfs.doctor()` over the real addon (#536 H7): one top-level export,
// outside the call table, that reports and changes nothing; and
// `doctor.fix(id)`, the `doctor` family's one call, which removes a leftover
// the report listed (layerfs-project#180).
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, mkdirSync, writeFileSync, chmodSync, rmSync, utimesSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import layerfs, { doctor, table, LayerFSError } from "../src/index.js";

test("doctor reports a home without making it, outside the call table", async () => {
  assert.equal(layerfs.doctor, doctor);
  assert.ok(!table.some((spec) => spec.call === "doctor"));
  const parent = mkdtempSync(join(tmpdir(), "layerfs-node-doctor-"));
  const home = join(parent, "home");
  assert.deepEqual(await doctor({ home }), { ok: true, findings: [] });
  assert.ok(!existsSync(home), "the doctor made the home");
  rmSync(parent, { recursive: true, force: true });
});

test("doctor names a record that does not parse and its fix", async () => {
  const home = mkdtempSync(join(tmpdir(), "layerfs-node-doctor-"));
  chmodSync(home, 0o700);
  await layerfs.client({ home }).store.list();
  mkdirSync(join(home, "store", "bad"), { recursive: true, mode: 0o700 });
  writeFileSync(join(home, "store", "bad", "record.json"), "{not json", { mode: 0o600 });
  const report = await doctor({ home });
  assert.equal(report.ok, false);
  assert.equal(report.findings.length, 1);
  const [finding] = report.findings;
  assert.equal(finding.severity, "error");
  assert.deepEqual(finding.entity, { kind: "store", name: "bad" });
  assert.match(finding.fix, /by hand/);
  await assert.rejects(doctor({ home: "relative" }), (error) => error instanceof LayerFSError && error.code === "InvalidInput");
  rmSync(home, { recursive: true, force: true });
});

test("doctor lists a leftover and doctor.fix removes it, on the package and on a client", async () => {
  const home = mkdtempSync(join(tmpdir(), "layerfs-node-doctor-"));
  chmodSync(home, 0o700);
  const lfs = layerfs.client({ home });
  await lfs.store.list();
  assert.deepEqual((await doctor({ home })).leftovers, [{ machine: "native", complete: true, image_locks: 0, unmount_markers: 0 }]);
  // a partly received image, last written two hours ago, and its lock
  // each folder 0700, whatever the umask
  let images = home;
  for (const part of ["machine", "native", "run", "images"]) {
    images = join(images, part);
    mkdirSync(images, { recursive: true, mode: 0o700 });
    chmodSync(images, 0o700);
  }
  const digest = "ab".repeat(32);
  const part = join(images, `.recv-${digest}.part`);
  writeFileSync(part, "x".repeat(50));
  const then = Date.now() / 1000 - 7200;
  utimesSync(part, then, then);
  writeFileSync(join(images, `${digest}.2.lock`), "");

  const report = await doctor({ home });
  assert.equal(report.ok, true, "a leftover is a warning");
  assert.equal(report.findings.length, 1);
  const [finding] = report.findings;
  assert.deepEqual(finding.entity, { kind: "machine", name: "native" });
  const id = `native:image-part:${digest}`;
  assert.equal(finding.leftover.id, id);
  assert.equal(finding.leftover.kind, "image-part");
  assert.equal(finding.leftover.path, part);
  assert.equal(finding.leftover.bytes, 50);
  assert.ok(finding.leftover.age >= 7190, String(finding.leftover.age));
  assert.equal(finding.leftover.fixable, true);
  assert.ok(finding.fix.includes(`layerfs doctor fix ${id}`), finding.fix);
  assert.deepEqual(report.leftovers, [{ machine: "native", complete: true, image_locks: 1, unmount_markers: 0 }]);

  // the same function on the package and on a client; refusals carry a code
  assert.equal(typeof layerfs.doctor.fix, "function");
  assert.equal(typeof doctor.fix, "function");
  const refused = (code) => (error) => error instanceof LayerFSError && error.code === code;
  await assert.rejects(lfs.doctor.fix(`native:image-lock:${digest}.2`), refused("InvalidInput"));
  await assert.rejects(lfs.doctor.fix("native:image-part:../x"), refused("NotFound"));
  await assert.rejects(lfs.doctor.fix(), refused("InvalidInput"));
  const removed = await lfs.doctor.fix(id);
  assert.deepEqual(removed, { kind: "image-part", path: part, removed: true, bytes: 50, machine: "native", id });
  assert.ok(!existsSync(part));
  assert.ok(existsSync(join(images, `${digest}.2.lock`)), "the lock file stays");
  assert.deepEqual((await doctor({ home })).findings, []);
  await assert.rejects(lfs.doctor.fix(id), refused("NotFound"));
  rmSync(home, { recursive: true, force: true });
});

// The doctors over the real addon (call surface 2.0): `home.doctor()` and
// its five siblings are table calls that report and change nothing; and
// `doctor.fix(id)`, the `doctor` family's one call, which removes one
// leftover (layerfs-project#180).
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, mkdirSync, writeFileSync, chmodSync, rmSync, utimesSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import layerfs, { doctor, table, LayerFSError } from "../src/index.js";

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

test("doctor.fix removes a leftover by its id, on the package and on a client", async () => {
  const home = mkdtempSync(join(tmpdir(), "layerfs-node-doctor-"));
  chmodSync(home, 0o700);
  const lfs = layerfs.client({ home });
  await lfs.store.list();
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

  // no doctor's report lists a leftover: its id is the leftover walk's
  const id = `native:image-part:${digest}`;

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
  await assert.rejects(lfs.doctor.fix(id), refused("NotFound"));
  rmSync(home, { recursive: true, force: true });
});

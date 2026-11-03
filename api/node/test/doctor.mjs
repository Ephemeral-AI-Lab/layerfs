// `layerfs.doctor()` over the real addon (#536 H7): one top-level export,
// outside the call table, that reports and changes nothing.
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, mkdirSync, writeFileSync, chmodSync, rmSync } from "node:fs";
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

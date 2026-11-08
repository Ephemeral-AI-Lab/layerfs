// Explicit diagnostics use the real addon and leave an absent home absent.
import test from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, existsSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
import { doctor, LayerFSError } from "../src/index.js";

test("host target returns explicit coverage without creating a home", async () => {
  const parent = mkdtempSync(join(tmpdir(), "layerfs-doctor-target-"));
  const home = join(parent, "absent");
  try {
    const report = await doctor({ home, target: "host", timeout: "2s" });
    assert.equal(report.target.kind, "host");
    assert.equal(report.operation, "diagnose");
    assert.equal(typeof report.complete, "boolean");
    assert.ok(Array.isArray(report.checks));
    assert.ok(report.checks.some((check) => check.id === "host.layout" && check.outcome === "skipped"));
    assert.equal(existsSync(home), false);
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test("invalid target options are refused before home effects", async () => {
  const parent = mkdtempSync(join(tmpdir(), "layerfs-doctor-invalid-"));
  const home = join(parent, "absent");
  try {
    for (const options of [
      { target: "machine" }, { target: "host", quick: true },
      { target: "host", timeout: "0s" }, { target: "host", unexpected: true },
    ]) {
      await assert.rejects(doctor({ home, ...options }), (error) => error instanceof LayerFSError && error.code === "InvalidInput");
      assert.equal(existsSync(home), false);
    }
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

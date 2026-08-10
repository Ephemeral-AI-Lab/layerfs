import assert from "node:assert/strict";
import { test } from "node:test";
import { execFileSync } from "node:child_process";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "../..");
test("M0 architecture and exports are locked", () => {
  execFileSync(process.execPath, ["scripts/check-architecture.mjs"], { cwd: root });
  execFileSync(process.execPath, ["scripts/check-exports.mjs"], { cwd: root });
  assert.ok(true);
});


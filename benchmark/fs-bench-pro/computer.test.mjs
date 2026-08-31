#!/usr/bin/env node

import assert from "node:assert/strict";
import { createCipheriv } from "node:crypto";

import {
  AFTER_EDITS_SHA256,
  CANDIDATE,
  EDIT_COUNT,
  FILE_BYTES,
  FINAL_BYTES,
  FINAL_SHA256,
  INITIAL_SHA256,
  PREPEND_ONLY_SHA256,
  SCHEMA,
  aggregateOperations,
  applyEdits,
  editPlan,
  prependBytes,
  sha256,
  validateSummaryShape,
} from "./computer.mjs";

const cipher = createCipheriv("aes-256-ctr", Buffer.alloc(32, 7), Buffer.alloc(16, 3));
const fixture = Buffer.concat([cipher.update(Buffer.alloc(FILE_BYTES)), cipher.final()]);
assert.equal(fixture.length, FILE_BYTES);
assert.equal(sha256(fixture), INITIAL_SHA256);

const plan = editPlan();
assert.equal(plan.length, EDIT_COUNT);
assert.deepEqual(plan[0], { id: "edit-01", offset: 3_636_423, marker: "E000000001" });
assert.deepEqual(plan.at(-1), { id: "edit-16", offset: 24_628_346, marker: "E000000016" });

const edited = applyEdits(fixture, plan);
assert.equal(sha256(edited), AFTER_EDITS_SHA256);
const final = prependBytes(edited);
assert.equal(final.length, FINAL_BYTES);
assert.equal(sha256(final), FINAL_SHA256);

const ids = ["create", ...plan.map(({ id }) => id), "prepend", "read"];
const acknowledgement = { crash_durable: false, journal_mode: "memory", synchronous: 0 };
const operations = ids.map((id, index) => ({
  id,
  comparable_ns: index + 1,
  acknowledgement,
}));
const aggregates = aggregateOperations(operations);
assert.deepEqual(aggregates, {
  create_ns: 1,
  sixteen_edits_sum_ns: 152,
  prepend_ns: 18,
  read_ns: 19,
});

assert.equal(
  validateSummaryShape({
    schema: SCHEMA,
    candidate: CANDIDATE,
    status: "PASS",
    workload: { container_prewarm: false },
    setup: Object.fromEntries(
      ["cold_create", "edit16", "prepend", "read"].map((name) => [
        name,
        { helper_invocations: 0, shell_invocations: 0 },
      ]),
    ),
    operations,
    aggregates,
    verification: {
      cold_create_sha256: INITIAL_SHA256,
      edit16_sha256: AFTER_EDITS_SHA256,
      prepend_sha256: PREPEND_ONLY_SHA256,
      read_sha256: INITIAL_SHA256,
      reopen_passed: true,
    },
  }),
  true,
);

assert.throws(() => aggregateOperations(operations.slice(1)), /operation matrix mismatch/);
assert.throws(
  () =>
    validateSummaryShape({
      schema: SCHEMA,
      candidate: CANDIDATE,
      status: "PASS",
      workload: { container_prewarm: false },
      setup: Object.fromEntries(
        ["cold_create", "edit16", "prepend", "read"].map((name) => [
          name,
          { helper_invocations: 0, shell_invocations: 0 },
        ]),
      ),
      operations,
      aggregates: { ...aggregates, read_ns: 0 },
      verification: {
        cold_create_sha256: INITIAL_SHA256,
        edit16_sha256: AFTER_EDITS_SHA256,
        prepend_sha256: PREPEND_ONLY_SHA256,
        read_sha256: INITIAL_SHA256,
        reopen_passed: true,
      },
    }),
  /aggregates/,
);
assert.throws(
  () =>
    validateSummaryShape({
      schema: SCHEMA,
      candidate: CANDIDATE,
      status: "PASS",
      workload: { container_prewarm: true },
      setup: Object.fromEntries(
        ["cold_create", "edit16", "prepend", "read"].map((name) => [
          name,
          { helper_invocations: name === "edit16" ? 1 : 0, shell_invocations: 0 },
        ]),
      ),
      operations,
      aggregates,
      verification: {
        cold_create_sha256: INITIAL_SHA256,
        edit16_sha256: AFTER_EDITS_SHA256,
        prepend_sha256: PREPEND_ONLY_SHA256,
        read_sha256: INITIAL_SHA256,
        reopen_passed: true,
      },
    }),
  /prewarmed/,
);

console.log("PASS computer fs-benchmark-pro workload/oracle/schema self-check");

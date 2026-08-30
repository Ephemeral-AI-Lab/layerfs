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
const operations = ids.map((id, index) => ({ id, comparable_ns: index + 1 }));
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
    operations,
    aggregates,
    verification: { final_bytes: FINAL_BYTES, final_sha256: FINAL_SHA256, reopen_passed: true },
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
      operations,
      aggregates: { ...aggregates, read_ns: 0 },
      verification: { final_bytes: FINAL_BYTES, final_sha256: FINAL_SHA256, reopen_passed: true },
    }),
  /aggregates/,
);

console.log("PASS computer fs-benchmark-pro workload/oracle/schema self-check");

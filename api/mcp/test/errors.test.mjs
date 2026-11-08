// Error mapping and progress coalescing: the two places where a claim about
// uncertainty or volume could mislead a client.
import { test } from "node:test";
import assert from "node:assert/strict";
import { LayerFSError } from "layerfs";
import { UNCERTAIN, errorResult } from "../src/errors.js";
import { isUncertain } from "../src/values.js";
import { progressReporter } from "../src/progress.js";

const body = (result) => JSON.parse(result.content[0].text);

test("a refusal is an isError result with { code, message, detail }", () => {
  const result = errorResult(new LayerFSError("NoSuchBranch", "no Branch nope", { path: "/x" }));
  assert.equal(result.isError, true);
  assert.equal(result.content.length, 1);
  assert.deepEqual(body(result), { code: "NoSuchBranch", message: "no Branch nope", detail: { path: "/x" } });
});

test("an uncertain outcome says so; a known Failed does not", () => {
  const unknown = errorResult(new LayerFSError("Unknown", "the worker's answer was lost", null));
  assert.equal(unknown.isError, true);
  assert.equal(unknown.content.length, 2);
  assert.equal(unknown.content[1].text, UNCERTAIN);
  assert.match(unknown.content[1].text, /^outcome uncertain: read state before acting again/);

  const attempted = errorResult(new LayerFSError("Failed", "SDK operation: Exchange { attempted: true }", null));
  assert.equal(attempted.content.length, 2);
  assert.equal(isUncertain({ code: "Failed", message: "", detail: { attempted: true } }), true);

  const known = errorResult(new LayerFSError("Failed", "nothing was attempted", { attempted: false }));
  assert.equal(known.content.length, 1);
  assert.equal(isUncertain({ code: "Failed", message: "attempted: false", detail: { attempted: false } }), false);
});

test("progress sends the first and last events, coalescing the rest", async () => {
  const sent = [];
  const reporter = progressReporter(async (notification) => sent.push(notification), "token-1", 100);
  for (let index = 0; index < 50; index += 1) reporter.on_progress({ phase: "export", bytes: index, total: 100 });
  assert.equal(sent.length, 0);
  await Promise.resolve();
  assert.equal(sent.length, 1, "the first event is sent at once");
  await reporter.finish();
  assert.equal(sent.length, 2, "50 events inside one interval are two notifications: first and last");
  assert.deepEqual(sent[0].params.progressToken, "token-1");
  assert.equal(sent[0].params.progress, 1);
  assert.equal(sent[0].params.total, 100);
  assert.match(sent[0].params.message, /"bytes":0/);
  assert.equal(sent[1].params.progress, 2);
  assert.match(sent[1].params.message, /"bytes":49/);

  const noToken = progressReporter(async () => {}, undefined);
  assert.equal(noToken, null);

  const noTotal = [];
  const second = progressReporter(async (notification) => noTotal.push(notification), "token-2", 100);
  second.on_progress({ phase: "prepare", total: null });
  await second.finish();
  assert.equal(noTotal.length, 1);
  assert.equal(noTotal[0].params.total, undefined);
  assert.equal(noTotal[0].params.progress, 1);
});

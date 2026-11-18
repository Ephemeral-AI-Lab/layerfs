// Error mapping and progress coalescing: the two places where a claim about
// uncertainty or volume could mislead a client.
import { test } from "node:test";
import assert from "node:assert/strict";
import { LayerFSError } from "@layerfs/node";
import { UNCERTAIN, errorResult } from "../src/errors.js";
import { isUncertain } from "../src/values.js";
import { progressReporter } from "../src/progress.js";

const body = (result) => JSON.parse(result.content[0].text);

test("a refusal is an isError result with { error, message }, its fix, its fields and its detail", () => {
  // the short form: the library answered no detail and no fix
  const short = errorResult(LayerFSError.from({ error: "NotFound", message: "no Branch nope" }));
  assert.equal(short.isError, true);
  assert.equal(short.content.length, 1);
  assert.deepEqual(body(short), { error: "NotFound", message: "no Branch nope" });
  // a fix, and the fields the next call needs, flat beside it
  const conflict = errorResult(LayerFSError.from({ error: "Conflict", message: "the head moved", fix: "layerfs branch get shop/api main", commit: "12d1" }));
  assert.deepEqual(body(conflict), { error: "Conflict", message: "the head moved", fix: "layerfs branch get shop/api main", commit: "12d1" });
  // the long form's detail, when the call asked for it
  const long = errorResult(LayerFSError.from({ error: "NotFound", message: "no Branch nope", fix: null, detail: { code: "NoSuchBranch", path: "/x" } }));
  assert.deepEqual(body(long), { error: "NotFound", message: "no Branch nope", detail: { code: "NoSuchBranch", path: "/x" } });
});

test("an uncertain outcome says so; a known Failed does not", () => {
  const unknown = errorResult(new LayerFSError("Unknown", "the worker's answer was lost", null));
  assert.equal(unknown.isError, true);
  assert.equal(unknown.content.length, 2);
  assert.equal(unknown.content[1].text, UNCERTAIN);
  assert.match(unknown.content[1].text, /^outcome uncertain: read state before acting again/);

  const unresolved = errorResult(new LayerFSError("Unresolved", "an earlier Unknown stands", null));
  assert.equal(unresolved.content.length, 2);
  assert.equal(isUncertain({ code: "Unresolved" }), true);

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

import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { BranchError, EphemeralFS } from "../../packages/fs/dist/index.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

async function setup(filename = ":memory:") { const database = await openNodeSqlite({ filename }); const filesystem = await EphemeralFS.open({ database }); return { database, filesystem }; }

test("branch reads a frozen base and publishes one durable revision", async () => {
  const { database, filesystem } = await setup();
  await filesystem.writeFile("/file", "base"); const branch = await filesystem.branches.create("feature"); await filesystem.writeFile("/file", "main-after-base");
  assert.equal(await branch.readFile("/file", { encoding: "utf8" }), "base"); await branch.writeFile("/file", "branch-value"); assert.equal(await filesystem.readFile("/file", { encoding: "utf8" }), "main-after-base");
  const conflict = await branch.publish({ operationId: "publish-feature" }); assert.equal(conflict.outcome, "conflict"); assert.equal(await branch.readFile("/file", { encoding: "utf8" }), "branch-value"); assert.equal(await filesystem.readFile("/file", { encoding: "utf8" }), "main-after-base");
  assert.deepEqual(await filesystem.branches.replay("publish-feature", "feature"), conflict); await branch.close(); await filesystem.close(); database.close();
});

test("fifty independent writers form one parent chain", async () => {
  const { database, filesystem } = await setup(); const branches = [];
  for (let index = 0; index < 50; index += 1) { const branch = await filesystem.branches.create(`independent-${index}`); await branch.writeFile(`/file-${index}`, `value-${index}`); branches.push(branch); }
  let previous = "0";
  for (let index = 0; index < branches.length; index += 1) { const result = await branches[index].publish(); assert.equal(result.outcome, "merged"); assert.equal(result.parentRevision, previous); previous = result.revision; await branches[index].close(); }
  for (let index = 0; index < 50; index += 1) assert.equal(await filesystem.readFile(`/file-${index}`, { encoding: "utf8" }), `value-${index}`);
  await filesystem.close(); database.close();
});

test("fifty same-inode writers yield one merge and 49 explicit conflicts", async () => {
  const { database, filesystem } = await setup(); await filesystem.writeFile("/shared", "base"); const branches = [];
  for (let index = 0; index < 50; index += 1) { const branch = await filesystem.branches.create(`same-${index}`); await branch.writeFile("/shared", `writer-${index}`); branches.push(branch); }
  const outcomes = []; for (let index = 0; index < branches.length; index += 1) outcomes.push(await branches[index].publish({ operationId: `same-op-${index}` }));
  assert.equal(outcomes.filter((result) => result.outcome === "merged").length, 1); assert.equal(outcomes.filter((result) => result.outcome === "conflict").length, 49); assert.equal(await filesystem.readFile("/shared", { encoding: "utf8" }), "writer-0");
  for (const branch of branches) await branch.close(); await filesystem.close(); database.close();
});

test("lost-response replay survives physical reopen and operation IDs cannot cross branches", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-branch-")); const filename = path.join(directory, "filesystem.db");
  try {
    let { database, filesystem } = await setup(filename); const first = await filesystem.branches.create("first"); await first.writeFile("/durable", "yes"); const published = await first.publish({ operationId: "durable-op" }); assert.equal(published.outcome, "merged"); await first.close(); await filesystem.close(); database.close();
    ({ database, filesystem } = await setup(filename)); assert.deepEqual(await filesystem.branches.replay("durable-op", "first"), published); const second = await filesystem.branches.create("second"); await second.writeFile("/other", "no"); await assert.rejects(second.publish({ operationId: "durable-op" }), (error) => error instanceof BranchError && error.code === "OperationBranchMismatch"); await second.close(); await filesystem.close(); database.close();
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("branch stream is immutable across later edit and discard", async () => {
  const { database, filesystem } = await setup(); await filesystem.writeFile("/stream", "base"); const branch = await filesystem.branches.create("stream-branch"); await branch.writeFile("/stream", "snapshot"); const stream = await branch.readStream("/stream"); await branch.writeFile("/stream", "later"); await branch.discard(); let text = ""; for await (const chunk of stream) text += new TextDecoder().decode(chunk); assert.equal(text, "snapshot"); assert.equal((await branch.info()).state, "discarded"); await branch.close(); await filesystem.close(); database.close();
});


import assert from "node:assert/strict";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifest } from "../../packages/fs/dist/operations/full-rebuild.js";
import { constrainStorageLimits } from "../../packages/fs/dist/resources/limits.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

async function fixture(options = {}) {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const filesystem = await EphemeralFS.open({ database, ...options });
  return { database, filesystem };
}

async function finishCollection(maintenance, runId) {
  let result;
  for (let index = 0; index < 1000; index += 1) {
    result = await maintenance.collectGarbage({ runId, maxBatches: 1 });
    if (result.state === "complete") return result;
  }
  throw new Error("collection did not finish within bounded iterations");
}

test("mark and sweep resume in bounded batches and preserve every required root", async () => {
  const { database, filesystem } = await fixture({ storage: { maxGcBatchSize: 2 } });
  await filesystem.writeFile("/main", "reachable-main");
  const branch = await filesystem.branches.create("gc-branch");
  await branch.writeFile("/branch-only", "reachable-branch");
  const orphan = buildManifest(new TextEncoder().encode("orphan-payload"), {
    minimum: 32_768,
    average: 131_072,
    maximum: 524_288,
  });
  const limits = constrainStorageLimits(undefined, database.capabilities);
  database.transaction("write", (tx) => {
    const repository = new ContentRepository(tx, limits);
    for (const [hash, bytes] of orphan.objects)
      repository.putObject(Buffer.from(hash, "hex"), bytes);
    for (const node of orphan.nodes.values())
      repository.putManifestNode(node.hash, node.encoded);
    repository.putManifestRoot(orphan.rootHash, orphan.root);
  });
  const paused = await filesystem.maintenance.collectGarbage({
    runId: "resumable",
    maxBatches: 1,
  });
  assert.equal(paused.state, "paused");
  const completed = await finishCollection(filesystem.maintenance, "resumable");
  assert.equal(completed.state, "complete");
  assert.ok(completed.deletedManifestRootCount >= 1);
  assert.equal(
    await filesystem.readFile("/main", { encoding: "utf8" }),
    "reachable-main",
  );
  assert.equal(
    await branch.readFile("/branch-only", { encoding: "utf8" }),
    "reachable-branch",
  );
  await branch.discard();
  await finishCollection(filesystem.maintenance, "after-discard");
  await branch.close();
  await filesystem.close();
  database.close();
});

test("verification is cursor-bounded, resumable, and detects reachable corruption", async () => {
  const { database, filesystem } = await fixture({ storage: { maxGcBatchSize: 2 } });
  for (let index = 0; index < 10; index += 1)
    await filesystem.writeFile(`/file-${index}`, `value-${index}`);
  let cursor;
  let checked = 0;
  for (let index = 0; index < 1000; index += 1) {
    const result = await filesystem.maintenance.verify({ cursor, maxEntities: 2 });
    checked += result.checkedEntities;
    cursor = result.nextCursor ?? undefined;
    if (result.complete) break;
  }
  assert.ok(checked >= 10);
  assert.equal(cursor, undefined);
  const hash = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT hash FROM efs_cas_objects ORDER BY hash LIMIT 1", [], {
        maxRows: 1,
        maxBytes: 100,
      })[0].hash,
  );
  database.transaction("write", (tx) =>
    tx.run("UPDATE efs_cas_objects SET bytes=zeroblob(size) WHERE hash=?", [hash]),
  );
  await assert.rejects(async () => {
    let next;
    for (let index = 0; index < 1000; index += 1) {
      const result = await filesystem.maintenance.verify({
        cursor: next,
        maxEntities: 10,
      });
      next = result.nextCursor ?? undefined;
      if (result.complete) break;
    }
  }, /digest mismatch/);
  await filesystem.close();
  database.close();
});

test("storage snapshot reports exact durable counters and physical pages", async () => {
  const { database, filesystem } = await fixture();
  await filesystem.writeFile("/one", "1111");
  await filesystem.link("/one", "/alias");
  const snapshot = await filesystem.maintenance.snapshotStorage();
  assert.equal(snapshot.mainLogicalBytes, 4);
  assert.ok(snapshot.objectCount >= 1);
  assert.ok(snapshot.manifestCount >= 2);
  assert.ok(snapshot.physical.mainFileBytes > 0);
  assert.equal(snapshot.revisionCount, 3);
  await filesystem.close();
  database.close();
});

test(
  "100,000-row orphan fixture collects with keyset-sized durable batches",
  { timeout: 120_000 },
  async () => {
    const { database, filesystem } = await fixture({
      storage: { maxGcBatchSize: 1000 },
    });
    const count = 100_000;
    database.transaction("write", (tx) => {
      for (let index = 0; index < count; index += 1) {
        const bytes = new Uint8Array(4);
        new DataView(bytes.buffer).setUint32(0, index, true);
        tx.run(
          "INSERT INTO efs_cas_objects(hash,size,bytes,allocation_sequence) VALUES(?,?,?,?)",
          [sha256(bytes), 4, bytes, index + 1],
        );
      }
      tx.run("UPDATE efs_meta SET next_allocation_sequence=? WHERE singleton=1", [
        count + 1,
      ]);
      tx.run(
        "UPDATE efs_usage SET object_count=?,object_bytes=?,charged_metadata_bytes=charged_metadata_bytes+? WHERE singleton=1",
        [count, count * 4, count * 96],
      );
    });
    const result = await filesystem.maintenance.collectGarbage();
    assert.equal(result.state, "complete");
    assert.equal(result.deletedObjectCount, count);
    const remaining = database.transaction(
      "read",
      (tx) =>
        tx.all("SELECT count(*) count FROM efs_cas_objects", [], {
          maxRows: 1,
          maxBytes: 100,
        })[0].count,
    );
    assert.equal(remaining, 0);
    await filesystem.close();
    database.close();
  },
);

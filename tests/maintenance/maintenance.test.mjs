import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifest } from "../../packages/fs/dist/operations/full-rebuild.js";
import { constrainStorageLimits } from "../../packages/fs/dist/resources/limits.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import {
  CHARGED_ROW_BYTES,
  UsageRepository,
} from "../../packages/fs/dist/sqlite/usage-repository.js";
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

test("sweep reconciles a post-mark root generation before deleting newly reachable data", async () => {
  const { database, filesystem } = await fixture({ storage: { maxGcBatchSize: 1 } });
  const bytes = new TextEncoder().encode("rescued-after-mark");
  const orphan = buildManifest(bytes, {
    minimum: 32_768,
    average: 131_072,
    maximum: 524_288,
  });
  const limits = constrainStorageLimits(undefined, database.capabilities);
  database.transaction("write", (tx) => {
    const repository = new ContentRepository(tx, limits);
    for (const [hash, object] of orphan.objects)
      repository.putObject(Buffer.from(hash, "hex"), object);
    for (const node of orphan.nodes.values())
      repository.putManifestNode(node.hash, node.encoded);
    repository.putManifestRoot(orphan.rootHash, orphan.root);
  });
  let state = 0;
  for (let index = 0; index < 100 && state === 0; index += 1) {
    await filesystem.maintenance.collectGarbage({
      runId: "generation-race",
      maxBatches: 1,
    });
    state = database.transaction(
      "read",
      (tx) =>
        tx.all("SELECT state FROM efs_gc_runs WHERE id='generation-race'", [], {
          maxRows: 1,
          maxBytes: 128,
        })[0].state,
    );
  }
  assert.equal(state, 1);
  await filesystem.writeFile("/rescued", bytes);
  const completed = await finishCollection(filesystem.maintenance, "generation-race");
  assert.equal(completed.state, "complete");
  assert.deepEqual(await filesystem.readFile("/rescued"), bytes);
  const retained = database.transaction("read", (tx) => ({
    roots: tx.all("SELECT count(*) count FROM efs_manifest_roots", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].count,
    nodes: tx.all("SELECT count(*) count FROM efs_manifest_nodes", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].count,
  }));
  assert.ok(retained.roots >= 1);
  assert.ok(retained.nodes >= 1);
  await filesystem.close();
  database.close();
});

test("terminal collection cleanup removes marks and prior run rows in bounded batches", async () => {
  const { database, filesystem } = await fixture({
    storage: { maxGcBatchSize: 1, maxFinalTransactionRows: 64 },
  });
  await filesystem.writeFile("/kept", "kept");
  await finishCollection(filesystem.maintenance, "first-run");
  await finishCollection(filesystem.maintenance, "second-run");
  const state = database.transaction("read", (tx) => ({
    marks: tx.all("SELECT count(*) count FROM efs_gc_marks", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].count,
    first: tx.all("SELECT count(*) count FROM efs_gc_runs WHERE id='first-run'", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].count,
    second: tx.all(
      "SELECT count(*) count FROM efs_gc_runs WHERE id='second-run' AND state=7",
      [],
      { maxRows: 1, maxBytes: 128 },
    )[0].count,
  }));
  assert.deepEqual(state, { marks: 0, first: 0, second: 1 });
  database.transaction("read", (tx) =>
    new UsageRepository(
      tx,
      constrainStorageLimits(
        { maxGcBatchSize: 1, maxFinalTransactionRows: 64 },
        database.capabilities,
      ),
    ).verifyDerivedUsage(),
  );
  await filesystem.close();
  database.close();
});

test("root-journal cleanup resumes after physical reopen with one keyset row per batch", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-gc-journal-"));
  const filename = path.join(directory, "filesystem.db");
  const storageOptions = { maxGcBatchSize: 1, maxFinalTransactionRows: 64 };
  let database;
  let filesystem;
  try {
    database = await openNodeSqlite({ filename });
    filesystem = await EphemeralFS.open({ database, storage: storageOptions });
    const limits = constrainStorageLimits(storageOptions, database.capabilities);
    database.transaction("write", (tx) => {
      tx.run("UPDATE efs_meta SET root_mutation_generation=10020 WHERE singleton=1");
      for (let generation = 1; generation <= 6; generation += 1)
        tx.run("INSERT INTO efs_root_journal(generation,kind,root_id) VALUES(?,0,?)", [
          generation,
          Uint8Array.of(generation),
        ]);
      new UsageRepository(tx, limits).apply({
        maintenance_bytes: 6 * (CHARGED_ROW_BYTES + 1),
      });
    });
    let state = 0;
    for (let batches = 0; batches < 100 && state !== 5; batches += 1) {
      await filesystem.maintenance.collectGarbage({
        runId: "journal-resume",
        maxBatches: 1,
      });
      state = database.transaction(
        "read",
        (tx) =>
          tx.all("SELECT state FROM efs_gc_runs WHERE id='journal-resume'", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0].state,
      );
    }
    assert.equal(state, 5);
    await filesystem.maintenance.collectGarbage({
      runId: "journal-resume",
      maxBatches: 1,
    });
    assert.equal(
      database.transaction(
        "read",
        (tx) =>
          tx.all("SELECT count(*) count FROM efs_root_journal", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0].count,
      ),
      5,
    );
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({ filename, create: false });
    filesystem = await EphemeralFS.open({ database, storage: storageOptions });
    const completed = await finishCollection(filesystem.maintenance, "journal-resume");
    assert.equal(completed.state, "complete");
    assert.deepEqual(
      database.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT (SELECT count(*) FROM efs_root_journal) journals,(SELECT count(*) FROM efs_gc_marks) marks,(SELECT state FROM efs_gc_runs WHERE id='journal-resume') state",
            [],
            { maxRows: 1, maxBytes: 128 },
          )[0],
      ),
      { journals: 0, marks: 0, state: 7 },
    );
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;
  } finally {
    try {
      await filesystem?.close();
    } catch {}
    try {
      database?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("garbage collection rejects unbounded run identifiers and no-progress row profiles", async () => {
  const { database, filesystem } = await fixture();
  const before = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT count(*) count FROM efs_gc_runs", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].count,
  );
  await assert.rejects(
    filesystem.maintenance.collectGarbage({ runId: "x".repeat(257) }),
    /runId must encode to at most 256 bytes/,
  );
  const after = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT count(*) count FROM efs_gc_runs", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].count,
  );
  assert.equal(after, before);
  await filesystem.close();
  database.close();

  const low = await openNodeSqlite({ filename: ":memory:" });
  await assert.rejects(
    EphemeralFS.open({ database: low, storage: { maxFinalTransactionRows: 63 } }),
    /at least 64/,
  );
  low.close();
});

test("verification is cursor-bounded, resumable, and detects reachable corruption", async () => {
  const { database, filesystem } = await fixture({ storage: { maxGcBatchSize: 2 } });
  for (let index = 0; index < 10; index += 1)
    await filesystem.writeFile(`/file-${index}`, `value-${index}`);
  let cursor;
  let firstCursor;
  let checked = 0;
  for (let index = 0; index < 1000; index += 1) {
    const result = await filesystem.maintenance.verify({ cursor, maxEntities: 2 });
    checked += result.checkedEntities;
    cursor = result.nextCursor ?? undefined;
    firstCursor ??= cursor;
    if (result.complete) break;
  }
  assert.ok(checked >= 10);
  assert.equal(cursor, undefined);
  assert.ok(firstCursor);
  const replacement = firstCursor.endsWith("0") ? "1" : "0";
  await assert.rejects(
    filesystem.maintenance.verify({
      cursor: firstCursor.slice(0, -1) + replacement,
      maxEntities: 2,
    }),
    (error) => error?.code === "EINVAL",
  );

  database.transaction("write", (tx) =>
    tx.run(
      "INSERT INTO efs_operation_ids(id,branch_id,generation,created_at_ms) VALUES('unaccounted','branch',0,0)",
    ),
  );
  await assert.rejects(async () => {
    let next;
    for (let index = 0; index < 1000; index += 1) {
      const result = await filesystem.maintenance.verify({
        scopes: ["metadata"],
        cursor: next,
        maxEntities: 2,
      });
      next = result.nextCursor ?? undefined;
      if (result.complete) break;
    }
  }, /authoritative usage differs from the bounded durable recount/);
  database.transaction("write", (tx) =>
    tx.run("DELETE FROM efs_operation_ids WHERE id='unaccounted'"),
  );

  const pausedUsage = await filesystem.maintenance.verify({
    scopes: ["metadata"],
    maxEntities: 1,
  });
  assert.ok(pausedUsage.nextCursor);
  await filesystem.writeFile("/after-verification-start", "new value");
  await assert.rejects(
    filesystem.maintenance.verify({
      scopes: ["metadata"],
      cursor: pausedUsage.nextCursor,
      maxEntities: 1,
    }),
    (error) => error?.code === "EBUSY",
  );
  const hash = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT hash FROM efs_cas_objects ORDER BY hash LIMIT 1", [], {
        maxRows: 1,
        maxBytes: 100,
      })[0].hash,
  );
  database.transaction("write", (tx) => {
    const size = tx.all("SELECT size FROM efs_cas_objects WHERE hash=?", [hash], {
      maxRows: 1,
      maxBytes: 128,
    })[0].size;
    tx.run("UPDATE efs_cas_objects SET bytes=? WHERE hash=?", [
      new Uint8Array(size),
      hash,
    ]);
  });
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
    const limits = constrainStorageLimits(undefined, database.capabilities);
    const count = 100_000;
    const insertBatchSize = 1000;
    for (let start = 0; start < count; start += insertBatchSize) {
      const end = Math.min(count, start + insertBatchSize);
      database.transaction("write", (tx) => {
        for (let index = start; index < end; index += 1) {
          const bytes = new Uint8Array(4);
          new DataView(bytes.buffer).setUint32(0, index, true);
          tx.run(
            "INSERT INTO efs_cas_objects(hash,size,bytes,allocation_sequence) VALUES(?,?,?,?)",
            [sha256(bytes), 4, bytes, index + 1],
          );
        }
        tx.run("UPDATE efs_meta SET next_allocation_sequence=? WHERE singleton=1", [
          end + 1,
        ]);
        new UsageRepository(tx, limits).apply({
          object_count: end - start,
          object_bytes: (end - start) * 4,
          charged_metadata_bytes: (end - start) * CHARGED_ROW_BYTES,
        });
      });
    }
    let verificationCursor;
    let verifiedRows = 0;
    let verificationBatches = 0;
    for (; verificationBatches < 1000; verificationBatches += 1) {
      const verification = await filesystem.maintenance.verify({
        scopes: ["metadata"],
        cursor: verificationCursor,
        maxEntities: insertBatchSize,
      });
      assert.ok(verification.checkedEntities <= insertBatchSize);
      verifiedRows += verification.checkedEntities;
      verificationCursor = verification.nextCursor ?? undefined;
      if (verification.complete) break;
    }
    assert.ok(verifiedRows >= count);
    assert.equal(verificationCursor, undefined);
    assert.ok(verificationBatches >= Math.floor(count / insertBatchSize) - 1);
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

import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifest } from "../../packages/fs/dist/operations/full-rebuild.js";
import {
  MIN_MAINTENANCE_BYTES,
  MAX_CONTENT_OBJECT_BYTES,
  constrainStorageLimits,
} from "../../packages/fs/dist/resources/limits.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import { MaintenanceRepository } from "../../packages/fs/dist/sqlite/maintenance-repository.js";
import { initializeOrValidateSchema } from "../../packages/fs/dist/sqlite/schema.js";
import { StagingRepository } from "../../packages/fs/dist/sqlite/staging-repository.js";
import {
  CHARGED_ROW_BYTES,
  GC_MARK_RESERVATION_BYTES,
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

test("GC root seeding and sweep reference probes use hash-leading indexes", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(database);
  const digest = new Uint8Array(32);
  const plans = database.transaction("read", (tx) => {
    const plan = (sql, bindings) =>
      tx
        .all(`EXPLAIN QUERY PLAN ${sql}`, bindings, {
          maxRows: 64,
          maxBytes: 64 * 1024,
        })
        .map((row) => row.detail)
        .join(" | ");
    return [
      [
        plan(
          "SELECT DISTINCT manifest_hash FROM efs_inodes WHERE manifest_hash IS NOT NULL AND manifest_hash>? ORDER BY manifest_hash LIMIT ?",
          [digest, 8],
        ),
        "efs_inodes_manifest_hash",
      ],
      [
        plan(
          "SELECT DISTINCT manifest_hash FROM efs_revision_manifest_roots WHERE manifest_hash>? ORDER BY manifest_hash LIMIT ?",
          [digest, 8],
        ),
        "efs_revision_manifest_hash",
      ],
      [
        plan(
          "SELECT DISTINCT manifest_hash FROM efs_branch_manifest_roots WHERE manifest_hash>? ORDER BY manifest_hash LIMIT ?",
          [digest, 8],
        ),
        "efs_branch_manifest_hash",
      ],
      [
        plan(
          "SELECT DISTINCT manifest_hash FROM efs_lease_manifests WHERE manifest_hash>? ORDER BY manifest_hash LIMIT ?",
          [digest, 8],
        ),
        "efs_lease_manifest_hash",
      ],
      [
        plan(
          "SELECT DISTINCT manifest_hash FROM efs_lease_staged_manifests WHERE kind=0 AND manifest_hash>? ORDER BY manifest_hash LIMIT ?",
          [digest, 8],
        ),
        "efs_lease_staged_manifest_hash",
      ],
      [
        plan(
          "SELECT 1 FROM efs_staging_reused_subtrees WHERE source_manifest_hash=? LIMIT 1",
          [digest],
        ),
        "efs_staging_reused_source_manifest_hash",
      ],
      [
        plan("SELECT 1 FROM efs_staging_level_records WHERE node_hash=? LIMIT 1", [
          digest,
        ]),
        "efs_staging_level_node_hash",
      ],
      [
        plan("SELECT 1 FROM efs_staging_reused_subtrees WHERE node_hash=? LIMIT 1", [
          digest,
        ]),
        "efs_staging_reused_node_hash",
      ],
      [
        plan("SELECT 1 FROM efs_lease_objects WHERE object_hash=? LIMIT 1", [digest]),
        "efs_lease_object_hash",
      ],
      [
        plan("SELECT 1 FROM efs_staging_entries WHERE object_hash=? LIMIT 1", [digest]),
        "efs_staging_entry_object_hash",
      ],
      [
        plan(
          "SELECT kind,hash FROM efs_gc_marks WHERE run_id=? AND processed=0 ORDER BY kind,hash LIMIT ?",
          ["run", 8],
        ),
        "efs_gc_marks_pending",
      ],
      [
        plan(
          "SELECT sequence FROM efs_staging_reconciliation_queue WHERE lease_id=? AND processed=0 ORDER BY sequence LIMIT ?",
          ["lease", 8],
        ),
        "efs_staging_reconciliation_pending",
      ],
      [
        plan(
          "SELECT path FROM efs_staging_manifest_validation_queue WHERE lease_id=? AND processed=0 ORDER BY path LIMIT ?",
          ["lease", 8],
        ),
        "efs_staging_manifest_validation_pending",
      ],
    ];
  });
  for (const [plan, expectedIndex] of plans)
    assert.match(plan, new RegExp(`USING (?:COVERING )?INDEX ${expectedIndex}`));
  database.close();
});

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

test("branch root attachment advances generation before GC can sweep its old closure", async () => {
  const { database, filesystem } = await fixture({ storage: { maxGcBatchSize: 1 } });
  const branch = await filesystem.branches.create("generation-branch");
  const bytes = new TextEncoder().encode("branch-rescued-after-mark");
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
      runId: "branch-generation-race",
      maxBatches: 1,
    });
    state = database.transaction(
      "read",
      (tx) =>
        tx.all("SELECT state FROM efs_gc_runs WHERE id='branch-generation-race'", [], {
          maxRows: 1,
          maxBytes: 128,
        })[0].state,
    );
  }
  assert.equal(state, 1);
  const generationBefore = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT root_mutation_generation value FROM efs_meta", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].value,
  );
  await branch.writeFile("/rescued", bytes);
  const generationAfter = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT root_mutation_generation value FROM efs_meta", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].value,
  );
  assert.ok(generationAfter > generationBefore);
  await finishCollection(filesystem.maintenance, "branch-generation-race");
  assert.deepEqual(await branch.readFile("/rescued"), bytes);
  await branch.discard();
  await branch.close();
  await filesystem.close();
  database.close();
});

test("garbage collection preserves every manifest member of an active staging lease", async () => {
  const { database, filesystem } = await fixture({ storage: { maxGcBatchSize: 1 } });
  const manifest = buildManifest(new Uint8Array(), {
    minimum: 32_768,
    average: 131_072,
    maximum: 524_288,
  });
  const storage = constrainStorageLimits(undefined, database.capabilities);
  const nonce = new Uint8Array(16).fill(9);
  database.transaction("write", (tx) => {
    const content = new ContentRepository(tx, storage);
    const staging = new StagingRepository(tx, storage);
    staging.begin({
      leaseId: "active-stage-gc",
      ownerId: "owner",
      ownerNonce: nonce,
      now: 1,
      expiresAt: Number.MAX_SAFE_INTEGER,
    });
    for (const node of manifest.nodes.values())
      content.putManifestNode(node.hash, node.encoded);
    content.putManifestRoot(manifest.rootHash, manifest.root);
    staging.appendBatch("active-stage-gc", nonce, [
      ...[...manifest.nodes.values()].map((node) => ({
        kind: "manifest-node",
        hash: node.hash,
        size: node.encoded.length,
      })),
      {
        kind: "manifest-root",
        hash: manifest.rootHash,
        size: manifest.root.length,
      },
    ]);
    staging.bumpRoot(5, "active-stage-gc");
  });
  const result = await finishCollection(filesystem.maintenance, "active-stage-hold");
  assert.equal(result.deletedManifestCount, 0);
  assert.deepEqual(
    database.transaction("read", (tx) => ({
      roots: tx.all("SELECT count(*) count FROM efs_manifest_roots", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].count,
      nodes: tx.all("SELECT count(*) count FROM efs_manifest_nodes", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].count,
      members: tx.all(
        "SELECT count(*) count FROM efs_lease_staged_manifests WHERE lease_id='active-stage-gc'",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].count,
      state: tx.all("SELECT state FROM efs_leases WHERE id='active-stage-gc'", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].state,
    })),
    {
      roots: 1,
      nodes: manifest.nodes.size,
      members: manifest.nodes.size + 1,
      state: 0,
    },
  );
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

test("content admission preserves GC progress and abandoned marks clean before a new run", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-gc-emergency-"));
  const filename = path.join(directory, "filesystem.db");
  const storageOptions = {
    maxManagedPayloadBytes: 16 * 1024 * 1024,
    maxMaintenanceBytes: MIN_MAINTENANCE_BYTES,
    maintenanceReserveBytes: MIN_MAINTENANCE_BYTES,
  };
  let database;
  try {
    database = await openNodeSqlite({ filename });
    initializeOrValidateSchema(database);
    const storage = constrainStorageLimits(storageOptions, database.capabilities);
    const retained = [];
    for (let value = 1; value <= 1; value += 1) {
      const bytes = Uint8Array.of(value);
      const hash = sha256(bytes);
      retained.push(hash);
      database.transaction("write", (tx) =>
        new ContentRepository(tx, storage).putObject(hash, bytes),
      );
    }
    const rejected = Uint8Array.of(2);
    assert.throws(
      () =>
        database.transaction("write", (tx) =>
          new ContentRepository(tx, storage).putObject(sha256(rejected), rejected),
        ),
      /maintenance quota/,
    );
    database.transaction("write", (tx) => {
      const maintenance = new MaintenanceRepository(tx, storage);
      maintenance.beginRun("abandoned", 1);
      maintenance.addMark("abandoned", 0, retained[0]);
      maintenance.abandonRun("abandoned", 7, 8);
    });
    database.close();
    database = await openNodeSqlite({ filename, create: false });
    initializeOrValidateSchema(database);
    assert.throws(
      () =>
        database.transaction("write", (tx) =>
          new MaintenanceRepository(tx, storage).beginRun("new", 2),
        ),
      /another garbage-collection run is nonterminal/,
    );
    database.transaction("write", (tx) =>
      new MaintenanceRepository(tx, storage).resumeAbandonedRun("abandoned", 8, 4),
    );
    assert.equal(
      database.transaction("write", (tx) =>
        new MaintenanceRepository(tx, storage).cleanupMarks("abandoned", 1, 5),
      ),
      true,
    );
    assert.equal(
      database.transaction("write", (tx) =>
        new MaintenanceRepository(tx, storage).cleanupMarks("abandoned", 1, 5),
      ),
      false,
    );
    assert.equal(
      database.transaction("write", (tx) =>
        new MaintenanceRepository(tx, storage).cleanupRootJournal("abandoned", 1, 6),
      ),
      false,
    );
    assert.equal(
      database.transaction("write", (tx) =>
        new MaintenanceRepository(tx, storage).cleanupTerminalRuns(
          "abandoned",
          1,
          7,
          8,
          7,
        ),
      ),
      false,
    );
    database.transaction("write", (tx) =>
      new MaintenanceRepository(tx, storage).beginRun("new", 2),
    );
    assert.deepEqual(
      database.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT (SELECT count(*) FROM efs_gc_marks) marks,(SELECT state FROM efs_gc_runs WHERE id='abandoned') abandoned,(SELECT state FROM efs_gc_runs WHERE id='new') fresh",
            [],
            { maxRows: 1, maxBytes: 256 },
          )[0],
      ),
      { marks: 0, abandoned: 7, fresh: 0 },
    );
  } finally {
    try {
      database?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
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
  const OriginalTextEncoder = globalThis.TextEncoder;
  let encoded = false;
  globalThis.TextEncoder = class extends OriginalTextEncoder {
    encode(...args) {
      encoded = true;
      return super.encode(...args);
    }
  };
  try {
    await assert.rejects(
      filesystem.maintenance.collectGarbage({ runId: "x".repeat(1_000_000) }),
      /runId must encode to at most 256 bytes/,
    );
  } finally {
    globalThis.TextEncoder = OriginalTextEncoder;
  }
  assert.equal(encoded, false);
  const originalAtob = globalThis.atob;
  let decoded = false;
  globalThis.atob = (...args) => {
    decoded = true;
    return originalAtob(...args);
  };
  try {
    await assert.rejects(
      filesystem.maintenance.verify({
        cursor: "A".repeat(64 * 1024 + 1),
        maxEntities: 1,
      }),
      (error) => error?.code === "EINVAL",
    );
  } finally {
    globalThis.atob = originalAtob;
  }
  assert.equal(decoded, false);
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

test("cold verification keysets exact-size objects within one UoW result envelope", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-verify-max-object-"));
  const filename = path.join(directory, "filesystem.db");
  let database;
  let filesystem;
  try {
    database = await openNodeSqlite({ filename, durability: "relaxed-test" });
    filesystem = await EphemeralFS.open({ database });
    const storage = constrainStorageLimits(undefined, database.capabilities);
    for (const value of [41, 42]) {
      const bytes = new Uint8Array(MAX_CONTENT_OBJECT_BYTES).fill(value);
      database.transaction("write", (tx) =>
        new ContentRepository(tx, storage).putObject(sha256(bytes), bytes),
      );
    }
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = await openNodeSqlite({
      filename,
      create: false,
      durability: "relaxed-test",
    });
    filesystem = await EphemeralFS.open({ database });
    let cursor;
    let checked = 0;
    for (let batch = 0; batch < 8; batch += 1) {
      const result = await filesystem.maintenance.verify({
        scopes: ["objects"],
        cursor,
        maxEntities: 2,
      });
      checked += result.checkedEntities;
      cursor = result.nextCursor ?? undefined;
      if (result.complete) break;
    }
    assert.equal(checked, 2);
    assert.equal(cursor, undefined);
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
    const storageOptions = {
      maxGcBatchSize: 1000,
      maxMaintenanceBytes: 128 * 1024 * 1024,
      maintenanceReserveBytes: 128 * 1024 * 1024,
    };
    const { database, filesystem } = await fixture({
      storage: storageOptions,
    });
    const limits = constrainStorageLimits(storageOptions, database.capabilities);
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
          maintenance_bytes: (end - start) * GC_MARK_RESERVATION_BYTES,
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

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import {
  encodeManifestNode,
  encodeManifestRoot,
} from "../../packages/fs/dist/manifests/codec.js";
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
  UsageRepository,
} from "../../packages/fs/dist/sqlite/usage-repository.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { maintenanceFaultInjector } from "../fault/maintenance-fault-injector.mjs";

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

async function advanceSnapshotUntil(filesystem, database, predicate, maximum = 5000) {
  for (let batch = 0; batch < maximum; batch += 1) {
    await filesystem.maintenance.snapshotStorage({ maxBatches: 1 });
    const state = database.transaction(
      "read",
      (tx) =>
        tx.all("SELECT * FROM efs_storage_snapshots WHERE singleton=1", [], {
          maxRows: 1,
          maxBytes: 8192,
        })[0],
    );
    if (predicate(state)) return state;
  }
  throw new Error("storage snapshot did not reach the requested durable state");
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

test("root-journal normal capacity rejects atomically and emergency collection compacts it", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-root-journal-capacity-"));
  const filename = path.join(directory, "filesystem.db");
  const storageOptions = {
    maxGcBatchSize: 1,
    maxFinalTransactionRows: 64,
    maxMaintenanceBytes: MIN_MAINTENANCE_BYTES,
    maintenanceReserveBytes: MIN_MAINTENANCE_BYTES,
  };
  let database;
  let filesystem;
  try {
    database = await openNodeSqlite({ filename });
    filesystem = await EphemeralFS.open({ database, storage: storageOptions });
    const limits = constrainStorageLimits(storageOptions, database.capabilities);
    let appended = 0;
    for (; appended < 100; appended += 1) {
      try {
        database.transaction("write", (tx) =>
          new StagingRepository(tx, limits).bumpRoot(7, `journal-root-${appended}`),
        );
      } catch (error) {
        assert.match(String(error), /maintenance quota/);
        break;
      }
    }
    assert.ok(appended > 0 && appended < 100);
    const beforeRejectedMutation = database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT (SELECT root_mutation_generation FROM efs_meta WHERE singleton=1) generation,(SELECT count(*) FROM efs_root_journal) journals,(SELECT maintenance_bytes FROM efs_usage WHERE singleton=1) maintenance_bytes",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    );
    assert.throws(
      () =>
        database.transaction("write", (tx) =>
          new StagingRepository(tx, limits).bumpRoot(7, "journal-root-rejected"),
        ),
      /maintenance quota/,
    );
    assert.deepEqual(
      database.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT (SELECT root_mutation_generation FROM efs_meta WHERE singleton=1) generation,(SELECT count(*) FROM efs_root_journal) journals,(SELECT maintenance_bytes FROM efs_usage WHERE singleton=1) maintenance_bytes",
            [],
            { maxRows: 1, maxBytes: 256 },
          )[0],
      ),
      beforeRejectedMutation,
    );

    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;
    database = await openNodeSqlite({ filename, create: false });
    filesystem = await EphemeralFS.open({ database, storage: storageOptions });
    const completed = await finishCollection(
      filesystem.maintenance,
      "root-journal-emergency-compaction",
    );
    assert.equal(completed.state, "complete");
    assert.equal(
      database.transaction(
        "read",
        (tx) =>
          tx.all("SELECT count(*) count FROM efs_root_journal", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0].count,
      ),
      0,
    );
    database.transaction("read", (tx) =>
      new UsageRepository(tx, limits).verifyDerivedUsage(),
    );
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

test("reachable corruption aborts marking before sweep and remains restart-safe", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-gc-corruption-"));
  const filename = path.join(directory, "filesystem.db");
  const storageOptions = { maxGcBatchSize: 1, maxQueryBatchSize: 16 };
  const liveBytes = new TextEncoder().encode("reachable-corruption-value");
  const orphanBytes = new TextEncoder().encode("unreachable-must-not-be-swept");
  const liveHash = sha256(liveBytes);
  const orphanHash = sha256(orphanBytes);
  let database;
  let filesystem;
  const runUntilIntegrityFailure = async () => {
    let failure;
    for (let batch = 0; batch < 1000; batch += 1) {
      try {
        await filesystem.maintenance.collectGarbage({
          runId: "reachable-corruption",
          maxBatches: 1,
        });
      } catch (error) {
        failure = error;
        break;
      }
    }
    assert.match(String(failure), /digest mismatch|missing or invalid/);
  };
  try {
    database = await openNodeSqlite({ filename });
    filesystem = await EphemeralFS.open({ database, storage: storageOptions });
    await filesystem.writeFile("/reachable", liveBytes);
    await filesystem.writeFile("/orphan", orphanBytes);
    await filesystem.unlink("/orphan");
    database.transaction("write", (tx) =>
      tx.run("UPDATE efs_cas_objects SET bytes=? WHERE hash=?", [
        new Uint8Array(liveBytes.length),
        liveHash,
      ]),
    );

    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;
    database = await openNodeSqlite({ filename, create: false });
    filesystem = await EphemeralFS.open({ database, storage: storageOptions });

    await runUntilIntegrityFailure();
    const beforeReopen = database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT (SELECT count(*) FROM efs_cas_objects WHERE hash=?) orphan,(SELECT state FROM efs_gc_runs WHERE id='reachable-corruption') state,(SELECT deleted_objects FROM efs_gc_runs WHERE id='reachable-corruption') deleted_objects",
          [orphanHash],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    );
    assert.equal(beforeReopen.orphan, 1);
    assert.equal(beforeReopen.state, 8);
    assert.equal(beforeReopen.deleted_objects, 0);

    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;
    database = await openNodeSqlite({ filename, create: false });
    filesystem = await EphemeralFS.open({ database, storage: storageOptions });
    assert.deepEqual(
      database.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT (SELECT count(*) FROM efs_cas_objects WHERE hash=?) orphan,(SELECT state FROM efs_gc_runs WHERE id='reachable-corruption') state,(SELECT deleted_objects FROM efs_gc_runs WHERE id='reachable-corruption') deleted_objects",
            [orphanHash],
            { maxRows: 1, maxBytes: 256 },
          )[0],
      ),
      beforeReopen,
    );
    await assert.rejects(filesystem.readFile("/reachable"), /digest mismatch/);
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

test("storage snapshots pause with durable progress and compute exact branch set differences", async () => {
  const { database, filesystem } = await fixture({
    storage: { maxGcBatchSize: 1, maxQueryBatchSize: 1 },
  });
  await filesystem.writeFile("/shared", "shared-value");
  const branch = await filesystem.branches.create("snapshot-accounting");
  await branch.writeFile("/branch-only", "exclusive-value");
  const paused = await filesystem.maintenance.snapshotStorage({ maxBatches: 1 });
  assert.equal(paused.state, "paused");
  assert.notEqual(paused.phase, "complete");
  assert.equal(paused.remainingWork, null);
  assert.equal(paused.committedBatches, 1);
  assert.ok(paused.batchSize <= 1);
  const completed = await filesystem.maintenance.snapshotStorage();
  assert.equal(completed.state, "complete");
  assert.equal(completed.phase, "complete");
  assert.equal(completed.remainingWork, 0);
  assert.ok(completed.branchExclusiveObjectBytes > 0);
  assert.ok(completed.branchExclusiveManifestBytes > 0);
  assert.equal(
    completed.branchExclusivePayloadBytes,
    completed.branchPageBytes +
      completed.branchPatchBytes +
      completed.branchExclusiveObjectBytes +
      completed.branchExclusiveManifestBytes,
  );
  assert.ok(
    completed.reachableObjectPayloadBytes >= completed.branchExclusiveObjectBytes,
  );
  await branch.close();
  await filesystem.close();
  database.close();
});

test("branch-exclusive accounting includes unchanged inherited base content exactly", async () => {
  const { database, filesystem } = await fixture({
    storage: {
      maxRetainedRevisions: 1,
      maxGcBatchSize: 1,
      maxQueryBatchSize: 1,
    },
  });
  await filesystem.writeFile("/inherited", "OLD-UNIQUE-CONTENT");
  const oldRoot = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT manifest_hash FROM efs_inodes WHERE type=0 ORDER BY id LIMIT 1",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].manifest_hash,
  );
  const branch = await filesystem.branches.create("inherited-base-accounting");
  await filesystem.writeFile("/inherited", "NEW-UNIQUE-CONTENT");

  await advanceSnapshotUntil(filesystem, database, (state) => state.state === 3);
  const direct = database.transaction("read", (tx) => {
    const rootScope = tx.all(
      "SELECT scope_mask FROM efs_storage_marks WHERE kind=0 AND hash=?",
      [oldRoot],
      { maxRows: 1, maxBytes: 128 },
    )[0]?.scope_mask;
    const totals = tx.all(
      "SELECT coalesce(sum(CASE WHEN s.kind=0 THEN length(r.encoded) WHEN s.kind=1 THEN length(n.encoded) ELSE o.size END),0) total,coalesce(sum(CASE WHEN s.kind=0 THEN length(r.encoded) ELSE 0 END),0) roots,coalesce(sum(CASE WHEN s.kind=1 THEN length(n.encoded) ELSE 0 END),0) nodes,coalesce(sum(CASE WHEN s.kind=2 THEN o.size ELSE 0 END),0) objects FROM efs_storage_marks s LEFT JOIN efs_manifest_roots r ON s.kind=0 AND r.hash=s.hash LEFT JOIN efs_manifest_nodes n ON s.kind=1 AND n.hash=s.hash LEFT JOIN efs_cas_objects o ON s.kind=2 AND o.hash=s.hash WHERE (s.scope_mask&2)<>0 AND (s.scope_mask&1)=0",
      [],
      { maxRows: 1, maxBytes: 256 },
    )[0];
    return { rootScope, totals };
  });
  assert.equal(direct.rootScope & 2, 2);
  assert.equal(direct.rootScope & 1, 0);
  assert.ok(direct.totals.total > 0);

  const snapshot = await filesystem.maintenance.snapshotStorage();
  assert.equal(snapshot.branchExclusiveObjectBytes, direct.totals.objects);
  assert.equal(
    snapshot.branchExclusiveManifestBytes,
    direct.totals.roots + direct.totals.nodes,
  );
  assert.equal(
    snapshot.branchExclusivePayloadBytes,
    direct.totals.total + snapshot.branchPageBytes + snapshot.branchPatchBytes,
  );
  assert.equal(
    await branch.readFile("/inherited", { encoding: "utf8" }),
    "OLD-UNIQUE-CONTENT",
  );
  await branch.discard();
  await branch.close();
  await filesystem.close();
  database.close();
});

test("root removal rebuilds exact scopes without deleting durable mark identities", async () => {
  const { database, filesystem } = await fixture({
    storage: { maxGcBatchSize: 1, maxQueryBatchSize: 1 },
  });
  await filesystem.writeFile("/base", "base-content");
  const branch = await filesystem.branches.create("snapshot-root-removal");
  await branch.writeFile("/branch-only", "REMOVED-BRANCH-CONTENT");
  const branchRoot = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT manifest_hash FROM efs_branch_manifest_roots WHERE branch_id=? ORDER BY manifest_hash LIMIT 1",
        ["snapshot-root-removal"],
        { maxRows: 1, maxBytes: 128 },
      )[0].manifest_hash,
  );
  await advanceSnapshotUntil(filesystem, database, () =>
    database.transaction(
      "read",
      (tx) =>
        (tx.all(
          "SELECT processed FROM efs_storage_marks WHERE kind=0 AND hash=?",
          [branchRoot],
          { maxRows: 1, maxBytes: 128 },
        )[0]?.processed ?? 0) === 1,
    ),
  );
  await branch.discard();
  await advanceSnapshotUntil(filesystem, database, (state) => state.state === 3);
  const rebuilt = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT processed,accounted,scope_mask FROM efs_storage_marks WHERE kind=0 AND hash=?",
        [branchRoot],
        { maxRows: 1, maxBytes: 128 },
      )[0],
  );
  assert.deepEqual(rebuilt, { processed: 1, accounted: 0, scope_mask: 0 });
  const snapshot = await filesystem.maintenance.snapshotStorage();
  assert.equal(snapshot.branchExclusiveObjectBytes, 0);
  assert.equal(snapshot.branchExclusiveManifestBytes, 0);
  assert.ok(snapshot.reclaimablePayloadBytes > 0);
  await branch.close();
  await filesystem.close();
  database.close();
});

test("garbage collection reports exact reclaimed branch-overlay payload", async () => {
  const now = 1;
  const { database, filesystem } = await fixture({
    clock: () => now,
    storage: { maxGcBatchSize: 1 },
  });
  await filesystem.writeFile("/overlay-base", new Uint8Array(16_384).fill(1));
  const branch = await filesystem.branches.create("overlay-reclaim");
  await branch.writeRange("/overlay-base", 10, Uint8Array.of(2));
  await branch.writeFile("/overlay-base", new Uint8Array(16_384).fill(3));
  const before = await filesystem.maintenance.snapshotStorage();
  assert.ok(before.branchPageBytes > 0);
  assert.equal(
    before.branchExclusivePayloadBytes,
    before.branchPageBytes +
      before.branchPatchBytes +
      before.branchExclusiveObjectBytes +
      before.branchExclusiveManifestBytes,
  );
  assert.equal(
    before.reclaimablePayloadBytes,
    before.branchPageBytes + before.branchPatchBytes,
  );
  const collected = await finishCollection(
    filesystem.maintenance,
    "overlay-reclaimed-bytes",
  );
  assert.equal(
    collected.reclaimedBranchOverlayPayloadBytes,
    before.branchPageBytes + before.branchPatchBytes,
  );
  const after = await filesystem.maintenance.snapshotStorage();
  assert.equal(after.branchPageBytes, 0);
  assert.equal(after.branchPatchBytes, 0);
  await branch.discard();
  await branch.close();
  await filesystem.close();
  database.close();
});

test("active marking incrementally reconciles every required root class", async () => {
  const { database, filesystem } = await fixture({
    storage: {
      maxGcBatchSize: 1,
      maxQueryBatchSize: 1,
      maxMaintenanceBytes: 32 * 1024 * 1024,
      maintenanceReserveBytes: 32 * 1024 * 1024,
    },
  });
  await filesystem.writeFile("/seed", "seed-value");
  const limits = constrainStorageLimits(
    {
      maxGcBatchSize: 1,
      maxQueryBatchSize: 1,
      maxMaintenanceBytes: 32 * 1024 * 1024,
      maintenanceReserveBytes: 32 * 1024 * 1024,
    },
    database.capabilities,
  );
  const orphanManifests = [
    "lease-root",
    "staging-root",
    "checkpoint-root",
    "hold-root",
  ].map((value) =>
    buildManifest(new TextEncoder().encode(value), {
      minimum: 32_768,
      average: 131_072,
      maximum: 524_288,
    }),
  );
  database.transaction("write", (tx) => {
    const content = new ContentRepository(tx, limits);
    for (const manifest of orphanManifests) {
      for (const [hash, bytes] of manifest.objects)
        content.putObject(Buffer.from(hash, "hex"), bytes);
      for (const node of manifest.nodes.values())
        content.putManifestNode(node.hash, node.encoded);
      content.putManifestRoot(manifest.rootHash, manifest.root);
      tx.run(
        "INSERT INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,?)",
        [manifest.rootHash, 1],
      );
    }
    new UsageRepository(tx, limits).apply({
      charged_metadata_bytes: orphanManifests.length * CHARGED_ROW_BYTES,
    });
  });

  let completedMarks = 0;
  for (let batches = 0; batches < 100; batches += 1) {
    await filesystem.maintenance.snapshotStorage({ maxBatches: 1 });
    const state = database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT s.state,(SELECT count(*) FROM efs_storage_marks WHERE processed=1) completed FROM efs_storage_snapshots s WHERE singleton=1",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0],
    );
    completedMarks = state.completed;
    if (state.state === 2 && completedMarks > 0) break;
  }
  assert.ok(completedMarks > 0);
  const generationBefore = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT root_mutation_generation generation FROM efs_meta", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].generation,
  );

  // Main plus retained-revision roots.
  await filesystem.writeFile("/main-added", "main-added-value");
  // Active branch base and materialized branch root.
  const active = await filesystem.branches.create("root-add-active");
  await active.writeFile("/active-added", "active-added-value");
  // Successful publication result root.
  const publication = await filesystem.branches.create("root-add-publication");
  await publication.writeFile("/published-added", "published-added-value");
  const published = await publication.publish({ operationId: "root-add-result" });
  assert.equal(published.outcome, "merged");

  const leaseNonce = new Uint8Array(16).fill(3);
  const stagingNonce = new Uint8Array(16).fill(4);
  database.transaction("write", (tx) => {
    const staging = new StagingRepository(tx, limits);
    staging.acquireReadLease(
      "root-add-read-lease",
      "root-add-owner",
      leaseNonce,
      orphanManifests[0].rootHash,
      Number.MAX_SAFE_INTEGER,
    );
    staging.begin({
      leaseId: "root-add-staging",
      ownerId: "root-add-owner",
      ownerNonce: stagingNonce,
      now: 1,
      expiresAt: Number.MAX_SAFE_INTEGER,
    });
    const stagedMembers = [
      ...[...orphanManifests[1].objects].map(([hash, bytes]) => ({
        kind: "object",
        hash: Buffer.from(hash, "hex"),
        size: bytes.length,
      })),
      ...[...orphanManifests[1].nodes.values()].map((node) => ({
        kind: "manifest-node",
        hash: node.hash,
        size: node.encoded.length,
      })),
      {
        kind: "manifest-root",
        hash: orphanManifests[1].rootHash,
        size: orphanManifests[1].root.length,
      },
    ];
    for (const member of stagedMembers)
      staging.appendBatch("root-add-staging", stagingNonce, [member]);

    const revision = tx.all("SELECT main_revision revision FROM efs_meta", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].revision;
    tx.run(
      "INSERT INTO efs_revision_checkpoints(target_revision,state,phase,created_at_ms) VALUES(?,1,7,?)",
      [revision, 1],
    );
    tx.run(
      "INSERT INTO efs_checkpoint_manifest_roots(target_revision,inode_id,manifest_hash) VALUES(?,?,?)",
      [revision, "root-add-checkpoint", orphanManifests[2].rootHash],
    );
    new UsageRepository(tx, limits).apply({
      charged_metadata_bytes: 2 * CHARGED_ROW_BYTES,
    });
    staging.bumpRoot(2, "root-add-checkpoint", false);

    tx.run("INSERT INTO efs_root_holds(id,kind,root_id) VALUES(?,0,?)", [
      "root-add-hold",
      orphanManifests[3].rootHash,
    ]);
    new UsageRepository(tx, limits).apply({
      maintenance_bytes: CHARGED_ROW_BYTES + 32,
    });
    staging.bumpRoot(7, "root-add-hold", false);
  });
  const immediatelyRetained = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT count(*) count FROM efs_storage_marks WHERE processed=1", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].count,
  );
  assert.equal(immediatelyRetained, completedMarks);
  const generationAfter = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT root_mutation_generation generation FROM efs_meta", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].generation,
  );
  assert.ok(generationAfter > generationBefore);

  let markingFinished = false;
  for (let batches = 0; batches < 1000; batches += 1) {
    await filesystem.maintenance.snapshotStorage({ maxBatches: 1 });
    const state = database.transaction(
      "read",
      (tx) =>
        tx.all("SELECT state FROM efs_storage_snapshots", [], {
          maxRows: 1,
          maxBytes: 128,
        })[0].state,
    );
    if (state >= 3 && state <= 6) {
      markingFinished = true;
      break;
    }
  }
  assert.equal(markingFinished, true);
  const scopes = database.transaction("read", (tx) =>
    orphanManifests.map(
      (manifest) =>
        tx.all(
          "SELECT scope_mask FROM efs_storage_marks WHERE kind=0 AND hash=?",
          [manifest.rootHash],
          { maxRows: 1, maxBytes: 128 },
        )[0]?.scope_mask ?? 0,
    ),
  );
  assert.equal(scopes[0] & 4, 4, "read lease root");
  assert.equal(scopes[1] & 4, 4, "staging root");
  assert.equal(scopes[2] & 1, 1, "checkpoint root");
  assert.equal(scopes[3] & 4, 4, "held root");

  const snapshot = await filesystem.maintenance.snapshotStorage();
  assert.equal(snapshot.state, "complete");
  assert.equal(snapshot.rootMutationGeneration, generationAfter);
  assert.equal(
    await active.readFile("/active-added", { encoding: "utf8" }),
    "active-added-value",
  );
  assert.equal(
    await filesystem.readFile("/published-added", { encoding: "utf8" }),
    "published-added-value",
  );
  await publication.close();
  await active.close();
  await filesystem.close();
  database.close();
});

test("a stale completed snapshot returns EAGAIN on physical read-only reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-snapshot-read-only-"));
  const filename = path.join(directory, "filesystem.db");
  let database;
  let filesystem;
  try {
    database = await openNodeSqlite({ filename });
    filesystem = await EphemeralFS.open({ database });
    await filesystem.writeFile("/before", "before");
    await filesystem.maintenance.snapshotStorage();
    await filesystem.writeFile("/after", "after");
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({ filename, create: false, readOnly: true });
    filesystem = await EphemeralFS.open({ database });
    await assert.rejects(
      filesystem.maintenance.snapshotStorage(),
      (error) => error?.code === "EAGAIN",
    );
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({ filename, create: false });
    filesystem = await EphemeralFS.open({ database });
    const reconciled = await filesystem.maintenance.snapshotStorage();
    assert.equal(reconciled.state, "complete");
    assert.equal(reconciled.mainLogicalBytes, 11);
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

test("GC deletion invalidates cached snapshots and anonymous runs adopt after reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-gc-adoption-"));
  const filename = path.join(directory, "filesystem.db");
  let database;
  let filesystem;
  try {
    database = await openNodeSqlite({ filename });
    filesystem = await EphemeralFS.open({ database });
    const limits = constrainStorageLimits(undefined, database.capabilities);
    database.transaction("write", (tx) => {
      new ContentRepository(tx, limits).putObject(
        sha256(new TextEncoder().encode("anonymous-orphan")),
        new TextEncoder().encode("anonymous-orphan"),
      );
    });
    const before = await filesystem.maintenance.snapshotStorage();
    assert.equal(before.storedObjectPayloadBytes, 16);
    const started = await filesystem.maintenance.collectGarbage({ maxBatches: 1 });
    assert.equal(started.state, "paused");
    const durableRunId = started.runId;
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({ filename, create: false });
    filesystem = await EphemeralFS.open({ database });
    let collected;
    for (let batch = 0; batch < 1000; batch += 1) {
      collected = await filesystem.maintenance.collectGarbage({ maxBatches: 1 });
      assert.equal(collected.runId, durableRunId);
      if (collected.state === "complete") break;
    }
    assert.equal(collected.state, "complete");
    assert.equal(collected.reclaimedObjectPayloadBytes, 16);
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({ filename, create: false, readOnly: true });
    filesystem = await EphemeralFS.open({ database });
    await assert.rejects(
      filesystem.maintenance.snapshotStorage(),
      (error) => error?.code === "EAGAIN",
    );
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({ filename, create: false });
    filesystem = await EphemeralFS.open({ database });
    const after = await filesystem.maintenance.snapshotStorage();
    assert.equal(after.storedObjectPayloadBytes, 0);
    assert.equal(after.reclaimablePayloadBytes, 0);
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

test("caller-supplied lease time permits rollback without revival or early expiry", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-lease-clock-"));
  const filename = path.join(directory, "filesystem.db");
  let database;
  try {
    database = await openNodeSqlite({ filename });
    initializeOrValidateSchema(database);
    const limits = constrainStorageLimits(undefined, database.capabilities);
    const manifest = buildManifest(new TextEncoder().encode("lease-clock-value"), {
      minimum: 32_768,
      average: 131_072,
      maximum: 524_288,
    });
    const nonce = new Uint8Array(16).fill(5);
    const wrongNonce = new Uint8Array(16).fill(6);
    database.transaction("write", (tx) => {
      const content = new ContentRepository(tx, limits);
      for (const [hash, bytes] of manifest.objects)
        content.putObject(Buffer.from(hash, "hex"), bytes);
      for (const node of manifest.nodes.values())
        content.putManifestNode(node.hash, node.encoded);
      content.putManifestRoot(manifest.rootHash, manifest.root);
      tx.run(
        "INSERT INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,?)",
        [manifest.rootHash, 1],
      );
      new UsageRepository(tx, limits).apply({
        charged_metadata_bytes: CHARGED_ROW_BYTES,
      });
      new StagingRepository(tx, limits).acquireReadLease(
        "clock-lease",
        "clock-owner",
        nonce,
        manifest.rootHash,
        100,
      );
    });
    assert.equal(
      database.transaction("write", (tx) =>
        new StagingRepository(tx, limits).renewReadLease(
          "clock-lease",
          "clock-owner",
          wrongNonce,
          100,
          50,
          150,
        ),
      ),
      false,
    );
    assert.equal(
      database.transaction("write", (tx) =>
        new StagingRepository(tx, limits).renewReadLease(
          "clock-lease",
          "clock-owner",
          nonce,
          100,
          50,
          150,
        ),
      ),
      true,
    );
    // A deliberately independent simulated clock may move backward. Renewal
    // still extends from the persisted prior expiry and never shortens it.
    assert.equal(
      database.transaction("write", (tx) =>
        new StagingRepository(tx, limits).renewReadLease(
          "clock-lease",
          "clock-owner",
          nonce,
          150,
          40,
          200,
        ),
      ),
      true,
    );
    database.close();
    database = await openNodeSqlite({ filename, create: false });
    assert.equal(
      database.transaction("write", (tx) =>
        new StagingRepository(tx, limits).expireBatch(199, 10),
      ),
      0,
    );
    assert.equal(
      database.transaction("write", (tx) =>
        new StagingRepository(tx, limits).expireBatch(200, 10),
      ),
      1,
    );
    assert.equal(
      database.transaction("write", (tx) =>
        new StagingRepository(tx, limits).renewReadLease(
          "clock-lease",
          "clock-owner",
          nonce,
          200,
          200,
          300,
        ),
      ),
      false,
    );
  } finally {
    try {
      database?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("snapshot reconciliation evaluates newly acquired leases after clock rollback", async () => {
  let now = 100;
  const { database, filesystem } = await fixture({
    clock: () => now,
    storage: { maxGcBatchSize: 1, maxQueryBatchSize: 1 },
  });
  const limits = constrainStorageLimits(undefined, database.capabilities);
  const manifest = buildManifest(new TextEncoder().encode("rollback-root"), {
    minimum: 32_768,
    average: 131_072,
    maximum: 524_288,
  });
  database.transaction("write", (tx) => {
    const content = new ContentRepository(tx, limits);
    for (const [hash, bytes] of manifest.objects)
      content.putObject(Buffer.from(hash, "hex"), bytes);
    for (const node of manifest.nodes.values())
      content.putManifestNode(node.hash, node.encoded);
    content.putManifestRoot(manifest.rootHash, manifest.root);
    tx.run(
      "INSERT INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,1)",
      [manifest.rootHash],
    );
    new UsageRepository(tx, limits).apply({
      charged_metadata_bytes: CHARGED_ROW_BYTES,
    });
  });
  await advanceSnapshotUntil(filesystem, database, (state) => state.state === 2);
  now = 40;
  const nonce = new Uint8Array(16).fill(9);
  database.transaction("write", (tx) =>
    new StagingRepository(tx, limits).acquireReadLease(
      "rollback-snapshot-lease",
      "rollback-snapshot-owner",
      nonce,
      manifest.rootHash,
      80,
    ),
  );
  const snapshot = await filesystem.maintenance.snapshotStorage();
  assert.equal(snapshot.state, "complete");
  assert.equal(snapshot.reachableObjectPayloadBytes, 13);
  assert.ok(snapshot.reachableManifestPayloadBytes > 0);
  assert.equal(snapshot.reclaimablePayloadBytes, 0);
  database.transaction("write", (tx) =>
    new StagingRepository(tx, limits).releaseReadLease(
      "rollback-snapshot-lease",
      "rollback-snapshot-owner",
      nonce,
    ),
  );
  await filesystem.close();
  database.close();
});

test("content admission reserves enough space for exact snapshot marks", async () => {
  const { database, filesystem } = await fixture({
    storage: {
      maxMaintenanceBytes: 7500,
      maintenanceReserveBytes: 5000,
    },
  });
  await filesystem.writeFile("/quota-live", "abc");
  const first = await filesystem.maintenance.snapshotStorage();
  assert.equal(first.state, "complete");
  assert.equal(first.reclaimablePayloadBytes, 0);
  await filesystem.close();
  database.close();
});

test("storage snapshots expose exact operation-result payload bytes", async () => {
  const { database, filesystem } = await fixture({
    storage: {
      maxRetainedRevisions: 1,
      maxGcBatchSize: 1,
      maxQueryBatchSize: 1,
    },
  });
  const branch = await filesystem.branches.create("result-byte-accounting");
  await branch.writeFile("/published-result", "published");
  const published = await branch.publish({ operationId: "result-byte-operation" });
  assert.equal(published.outcome, "merged");
  const resultRoot = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT manifest_hash FROM efs_inodes WHERE type=0 ORDER BY id LIMIT 1",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].manifest_hash,
  );
  await filesystem.writeFile("/published-result", "replacement");
  const expectedResultBytes = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT result_bytes FROM efs_usage WHERE singleton=1", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].result_bytes,
  );
  await advanceSnapshotUntil(filesystem, database, (state) => state.state === 3);
  const resultScope = database.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT scope_mask FROM efs_storage_marks WHERE kind=0 AND hash=?",
        [resultRoot],
        { maxRows: 1, maxBytes: 128 },
      )[0]?.scope_mask ?? 0,
  );
  assert.equal(resultScope & 1, 1);
  const second = await filesystem.maintenance.snapshotStorage();
  assert.ok(expectedResultBytes > 0);
  assert.equal(second.operationResultPayloadBytes, expectedResultBytes);
  assert.equal(second.includesOperationResults, true);
  await branch.close();
  await filesystem.close();
  database.close();
});

test("current-main and retained-revision roots receive exact main scope", async () => {
  const { database, filesystem } = await fixture({
    storage: {
      maxRetainedRevisions: 2,
      maxGcBatchSize: 1,
      maxQueryBatchSize: 1,
    },
  });
  await filesystem.writeFile("/retained", "retained-old");
  const retainedRoot = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT manifest_hash FROM efs_inodes WHERE type=0", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].manifest_hash,
  );
  await filesystem.writeFile("/retained", "current-main");
  const mainRoot = database.transaction(
    "read",
    (tx) =>
      tx.all("SELECT manifest_hash FROM efs_inodes WHERE type=0", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].manifest_hash,
  );
  await advanceSnapshotUntil(filesystem, database, (state) => state.state === 3);
  const scopes = database.transaction("read", (tx) =>
    [retainedRoot, mainRoot].map(
      (hash) =>
        tx.all(
          "SELECT scope_mask FROM efs_storage_marks WHERE kind=0 AND hash=?",
          [hash],
          { maxRows: 1, maxBytes: 128 },
        )[0]?.scope_mask ?? 0,
    ),
  );
  assert.equal(scopes[0] & 1, 1, "retained revision root");
  assert.equal(scopes[1] & 1, 1, "current main root");
  await filesystem.maintenance.snapshotStorage();
  await filesystem.close();
  database.close();
});

test("metadata quota failure is exact across reopen and later maintenance", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-metadata-pressure-"));
  const filename = path.join(directory, "filesystem.db");
  let database;
  let filesystem;
  try {
    const probe = await openNodeSqlite({ filename: ":memory:" });
    const probeFilesystem = await EphemeralFS.open({ database: probe });
    const baseline = probe.transaction(
      "read",
      (tx) =>
        tx.all("SELECT charged_metadata_bytes FROM efs_usage WHERE singleton=1", [], {
          maxRows: 1,
          maxBytes: 128,
        })[0].charged_metadata_bytes,
    );
    await probeFilesystem.close();
    probe.close();

    const metadataLimit = baseline + 16 * 1024;
    database = await openNodeSqlite({ filename });
    filesystem = await EphemeralFS.open({
      database,
      storage: { maxChargedMetadataBytes: metadataLimit },
    });
    const limits = constrainStorageLimits(
      { maxChargedMetadataBytes: metadataLimit },
      database.capabilities,
    );
    const committed = [];
    let failedPath;
    for (let index = 0; index < 100; index += 1) {
      const candidate = `/metadata-${index}`;
      const before = database.transaction(
        "read",
        (tx) =>
          tx.all("SELECT * FROM efs_usage WHERE singleton=1", [], {
            maxRows: 1,
            maxBytes: 4096,
          })[0],
      );
      try {
        await filesystem.symlink("metadata-target", candidate);
        committed.push(candidate);
      } catch (error) {
        assert.equal(error?.code, "ENOSPC");
        failedPath = candidate;
        const after = database.transaction(
          "read",
          (tx) =>
            tx.all("SELECT * FROM efs_usage WHERE singleton=1", [], {
              maxRows: 1,
              maxBytes: 4096,
            })[0],
        );
        assert.deepEqual(after, before);
        break;
      }
    }
    assert.ok(committed.length > 0);
    assert.ok(failedPath);
    database.transaction("read", (tx) =>
      new UsageRepository(tx, limits).verifyDerivedUsage(),
    );
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({ filename, create: false });
    filesystem = await EphemeralFS.open({
      database,
      storage: { maxChargedMetadataBytes: metadataLimit },
    });
    assert.equal(
      await filesystem.readlink(committed[0], { encoding: "utf8" }),
      "metadata-target",
    );
    assert.equal((await filesystem.maintenance.snapshotStorage()).state, "complete");
    assert.equal(
      (await filesystem.maintenance.collectGarbage({ runId: "metadata-pressure" }))
        .state,
      "complete",
    );
    database.transaction("read", (tx) =>
      new UsageRepository(tx, limits).verifyDerivedUsage(),
    );
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

test("pinned WAL pressure rejects one filesystem mutation and recovers after reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-wal-pressure-"));
  const filename = path.join(directory, "filesystem.db");
  let database;
  let filesystem;
  let pinned;
  try {
    database = await openNodeSqlite({
      filename,
      busyTimeoutMs: 0,
      maxJournalBytes: 8 * 1024 * 1024,
    });
    filesystem = await EphemeralFS.open({
      database,
      storage: {
        maxJournalBytes: database.capabilities.maxJournalBytes,
        maxQueryBatchSize: 5000,
      },
    });
    await filesystem.writeFile("/kept", "wal-kept");
    database.checkpoint("truncate");
    pinned = new DatabaseSync(filename);
    pinned.exec("BEGIN");
    pinned.prepare("SELECT root_mutation_generation FROM efs_meta").get();

    const committed = [];
    let failedPath;
    for (let index = 0; index < 1000; index += 1) {
      const candidate = `/wal-${index}`;
      const before = database.transaction(
        "read",
        (tx) =>
          tx.all("SELECT * FROM efs_usage WHERE singleton=1", [], {
            maxRows: 1,
            maxBytes: 4096,
          })[0],
      );
      try {
        await filesystem.symlink("wal-target", candidate);
        committed.push(candidate);
      } catch (error) {
        assert.match(String(error), /ENOSPC.*WAL|WAL.*backpressure/i);
        failedPath = candidate;
        const after = database.transaction(
          "read",
          (tx) =>
            tx.all("SELECT * FROM efs_usage WHERE singleton=1", [], {
              maxRows: 1,
              maxBytes: 4096,
            })[0],
        );
        assert.deepEqual(after, before);
        break;
      }
    }
    assert.ok(committed.length > 0);
    assert.ok(failedPath);
    assert.ok((database.physicalStorage().walBytes ?? 0) > 0);
    pinned.exec("COMMIT");
    pinned.close();
    pinned = undefined;
    const checkpoint = database.checkpoint("truncate");
    assert.equal(checkpoint.busy, 0);
    assert.equal(checkpoint.walBytes, 0);
    await filesystem.symlink("wal-target", failedPath);
    database.transaction("read", (tx) =>
      new UsageRepository(
        tx,
        constrainStorageLimits(
          {
            maxJournalBytes: database.capabilities.maxJournalBytes,
            maxQueryBatchSize: 5000,
          },
          database.capabilities,
        ),
      ).verifyDerivedUsage(),
    );
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({
      filename,
      create: false,
      maxJournalBytes: 8 * 1024 * 1024,
    });
    filesystem = await EphemeralFS.open({
      database,
      storage: {
        maxJournalBytes: database.capabilities.maxJournalBytes,
        maxQueryBatchSize: 5000,
      },
    });
    assert.equal(
      await filesystem.readlink(failedPath, { encoding: "utf8" }),
      "wal-target",
    );
    assert.equal(await filesystem.readFile("/kept", { encoding: "utf8" }), "wal-kept");
    assert.equal((await filesystem.maintenance.snapshotStorage()).state, "complete");
    assert.equal(
      (await filesystem.maintenance.collectGarbage({ runId: "wal-pressure" })).state,
      "complete",
    );
  } finally {
    try {
      pinned?.exec("ROLLBACK");
    } catch {}
    try {
      pinned?.close();
    } catch {}
    try {
      await filesystem?.close();
    } catch {}
    try {
      database?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("database page exhaustion preserves exact filesystem state and later maintenance", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-page-pressure-"));
  const filename = path.join(directory, "filesystem.db");
  let database;
  let filesystem;
  try {
    const pageLimit = 2 * 1024 * 1024;
    const journalLimit = 8 * 1024 * 1024;
    database = await openNodeSqlite({
      filename,
      maxPhysicalDatabaseBytes: pageLimit,
      maxJournalBytes: journalLimit,
    });
    filesystem = await EphemeralFS.open({
      database,
      storage: {
        maxPhysicalDatabaseBytes: database.capabilities.maxPhysicalDatabaseBytes,
        maxJournalBytes: database.capabilities.maxJournalBytes,
        maxQueryBatchSize: 5000,
      },
    });
    await filesystem.writeFile("/kept", "page-kept");
    database.checkpoint("truncate");
    const failedPath = "/page-overflow";
    let failedDurablePayloadBytes = 0;
    const before = database.transaction(
      "read",
      (tx) =>
        tx.all("SELECT * FROM efs_usage WHERE singleton=1", [], {
          maxRows: 1,
          maxBytes: 4096,
        })[0],
    );
    const oversized = new Uint8Array(3 * 1024 * 1024);
    let randomState = 0x9e3779b9;
    for (let index = 0; index < oversized.length; index += 1) {
      randomState ^= randomState << 13;
      randomState ^= randomState >>> 17;
      randomState ^= randomState << 5;
      oversized[index] = randomState & 0xff;
    }
    await assert.rejects(filesystem.writeFile(failedPath, oversized), (error) =>
      /ENOSPC|full/i.test(String(error)),
    );
    const after = database.transaction(
      "read",
      (tx) =>
        tx.all("SELECT * FROM efs_usage WHERE singleton=1", [], {
          maxRows: 1,
          maxBytes: 4096,
        })[0],
    );
    assert.ok(after.object_count - before.object_count >= 0);
    assert.ok(after.object_count - before.object_count <= 64);
    failedDurablePayloadBytes = after.object_bytes - before.object_bytes;
    assert.ok(failedDurablePayloadBytes >= 0);
    assert.ok(failedDurablePayloadBytes <= 3 * 1024 * 1024);
    assert.equal(after.staging_bytes, before.staging_bytes);
    assert.equal(after.result_bytes, before.result_bytes);
    await assert.rejects(
      filesystem.stat(failedPath),
      (error) => error?.code === "ENOENT",
    );
    database.transaction("read", (tx) =>
      new UsageRepository(
        tx,
        constrainStorageLimits(
          {
            maxPhysicalDatabaseBytes: pageLimit,
            maxJournalBytes: journalLimit,
            maxQueryBatchSize: 5000,
          },
          database.capabilities,
        ),
      ).verifyDerivedUsage(),
    );
    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({
      filename,
      create: false,
      maxPhysicalDatabaseBytes: pageLimit,
      maxJournalBytes: journalLimit,
    });
    filesystem = await EphemeralFS.open({
      database,
      storage: {
        maxPhysicalDatabaseBytes: database.capabilities.maxPhysicalDatabaseBytes,
        maxJournalBytes: database.capabilities.maxJournalBytes,
        maxQueryBatchSize: 5000,
      },
    });
    assert.equal(await filesystem.readFile("/kept", { encoding: "utf8" }), "page-kept");
    const collection = await filesystem.maintenance.collectGarbage({
      runId: "page-pressure",
    });
    assert.equal(collection.state, "complete");
    assert.ok(collection.reclaimedObjectPayloadBytes >= failedDurablePayloadBytes);
    assert.equal((await filesystem.maintenance.snapshotStorage()).state, "complete");
    database.transaction("read", (tx) =>
      new UsageRepository(
        tx,
        constrainStorageLimits(
          {
            maxPhysicalDatabaseBytes: pageLimit,
            maxJournalBytes: journalLimit,
            maxQueryBatchSize: 5000,
          },
          database.capabilities,
        ),
      ).verifyDerivedUsage(),
    );
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

test("metadata-only page exhaustion is atomic and recovers from freed pages", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-metadata-page-pressure-"));
  const filename = path.join(directory, "filesystem.db");
  const pageLimit = 1024 * 1024;
  const journalLimit = 8 * 1024 * 1024;
  let database;
  let filesystem;
  try {
    database = await openNodeSqlite({
      filename,
      maxPhysicalDatabaseBytes: pageLimit,
      maxJournalBytes: journalLimit,
    });
    const storageOptions = {
      maxPhysicalDatabaseBytes: database.capabilities.maxPhysicalDatabaseBytes,
      maxJournalBytes: database.capabilities.maxJournalBytes,
      maxQueryBatchSize: 10_000,
    };
    filesystem = await EphemeralFS.open({ database, storage: storageOptions });
    await filesystem.mkdir("/metadata");
    const baselineObjects = database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT object_count,object_bytes FROM efs_usage WHERE singleton=1",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0],
    );
    const committed = [];
    let failedPath;
    for (let index = 0; index < 10_000; index += 1) {
      const candidate = `/metadata/link-${index.toString().padStart(5, "0")}`;
      try {
        await filesystem.symlink(`target-${"x".repeat(3500)}-${index}`, candidate);
        committed.push(candidate);
      } catch (error) {
        assert.match(String(error), /ENOSPC|full/i);
        failedPath = candidate;
        break;
      }
    }
    assert.ok(committed.length > 50);
    assert.ok(failedPath);
    await assert.rejects(
      filesystem.lstat(failedPath),
      (error) => error?.code === "ENOENT",
    );
    const afterFailure = database.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT object_count,object_bytes FROM efs_usage WHERE singleton=1",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0],
    );
    assert.deepEqual(afterFailure, baselineObjects);
    assert.ok(database.physicalStorage().mainFileBytes <= pageLimit);
    assert.ok((database.physicalStorage().walBytes ?? 0) <= journalLimit);

    for (const pathToFree of committed.slice(0, Math.ceil(committed.length / 2)))
      await filesystem.unlink(pathToFree);
    const checkpoint = database.checkpoint("truncate");
    assert.equal(checkpoint.busy, 0);
    assert.equal(checkpoint.walBytes, 0);
    await filesystem.symlink("recovered-target", failedPath);
    assert.equal(await filesystem.readlink(failedPath), "recovered-target");
    database.transaction("read", (tx) =>
      new UsageRepository(
        tx,
        constrainStorageLimits(storageOptions, database.capabilities),
      ).verifyDerivedUsage(),
    );

    await filesystem.close();
    filesystem = undefined;
    database.close();
    database = undefined;
    database = await openNodeSqlite({
      filename,
      create: false,
      maxPhysicalDatabaseBytes: pageLimit,
      maxJournalBytes: journalLimit,
    });
    filesystem = await EphemeralFS.open({
      database,
      storage: {
        maxPhysicalDatabaseBytes: database.capabilities.maxPhysicalDatabaseBytes,
        maxJournalBytes: database.capabilities.maxJournalBytes,
        maxQueryBatchSize: 10_000,
      },
    });
    assert.equal(await filesystem.readlink(failedPath), "recovered-target");
    assert.equal(
      (await filesystem.maintenance.collectGarbage({ runId: "metadata-pages" })).state,
      "complete",
    );
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

test(
  "100,000 reachable object, namespace, manifest-node, and mark rows stay cursor-bounded",
  { timeout: 600_000 },
  async (t) => {
    const count = 100_000;
    const baselineScaleCount = 10_240;
    const storageOptions = {
      maxGcBatchSize: 256,
      maxQueryBatchSize: 256,
      maxJournalBytes: 512 * 1024 * 1024,
      maxMaintenanceBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 256 * 1024 * 1024,
    };
    const runtimeOptions = {
      // This is the smallest round-MiB admission limit above the mandatory
      // 97.5 MiB progress working set for one maximum-size content object.
      maxManagedResidentBytes: 104 * 1024 * 1024,
      maxCacheBytes: 4 * 1024 * 1024,
      maxQueryBatchBytes: 256 * 1024,
    };
    const insertBatchSize = 256;
    const directory = await mkdtemp(path.join(tmpdir(), "efs-scale-maintenance-"));
    const filename = path.join(directory, "filesystem.db");
    const encoder = new TextEncoder();
    const objectBytes = (index) => {
      const bytes = new Uint8Array(4);
      new DataView(bytes.buffer).setUint32(0, index, true);
      return bytes;
    };
    const manifestRecord = (index) => {
      const bytes = objectBytes(index);
      const objectHash = sha256(bytes);
      const node = encodeManifestNode({
        kind: "leaf",
        span: bytes.length,
        entryCount: 1,
        entries: [{ hash: objectHash, length: bytes.length }],
      });
      const nodeHash = sha256(node);
      const root = encodeManifestRoot({
        parameters: { minimum: 1, average: 2, maximum: 4 },
        fileSize: bytes.length,
        entryCount: 1,
        rootNodeHash: nodeHash,
      });
      return Object.freeze({
        bytes,
        objectHash,
        node,
        nodeHash,
        root,
        rootHash: sha256(root),
      });
    };
    const baselineMemory = process.memoryUsage();
    let peakHeapBytes = baselineMemory.heapUsed;
    let peakRssBytes = baselineMemory.rss;
    let maxWalBytes = 0;
    let peakManagedResidentBytes = 0;
    let instancePeakManagedResidentBytes = 0;
    let baselineScaleManagedPeak = 0;
    let maxMaintenanceBatchMs = 0;
    const observeMemory = () => {
      const current = process.memoryUsage();
      peakHeapBytes = Math.max(peakHeapBytes, current.heapUsed);
      peakRssBytes = Math.max(peakRssBytes, current.rss);
    };
    const metricParts = [];
    let database;
    let injector;
    let filesystem;
    let limits;
    const open = async (create) => {
      instancePeakManagedResidentBytes = 0;
      database = await openNodeSqlite({
        filename,
        create,
        maxJournalBytes: storageOptions.maxJournalBytes,
      });
      injector = maintenanceFaultInjector(database, { captureTrace: false });
      filesystem = await EphemeralFS.open({
        database: injector.driver,
        storage: storageOptions,
        runtime: runtimeOptions,
        observer(event) {
          peakManagedResidentBytes = Math.max(
            peakManagedResidentBytes,
            event.counters.peakManagedResidentBytes ?? 0,
          );
          instancePeakManagedResidentBytes = Math.max(
            instancePeakManagedResidentBytes,
            event.counters.peakManagedResidentBytes ?? 0,
          );
        },
      });
      limits = constrainStorageLimits(storageOptions, database.capabilities);
      injector.arm({ afterStatement: Number.MAX_SAFE_INTEGER });
    };
    const close = async () => {
      if (injector) {
        metricParts.push(injector.metrics());
        injector.disarm();
      }
      await filesystem?.close();
      filesystem = undefined;
      database?.close();
      database = undefined;
      injector = undefined;
    };
    const markCount = (table, where = "") =>
      injector.driver.transaction(
        "read",
        (tx) =>
          tx.all(`SELECT count(*) count FROM ${table} ${where}`, [], {
            maxRows: 1,
            maxBytes: 128,
          })[0].count,
      );
    const updatePhysicalPeak = () => {
      const physical = injector.driver.physicalStorage();
      maxWalBytes = Math.max(maxWalBytes, physical.walBytes ?? 0);
    };
    const maintenanceCall = async (callback) => {
      const started = performance.now();
      const result = await callback();
      maxMaintenanceBatchMs = Math.max(
        maxMaintenanceBatchMs,
        performance.now() - started,
      );
      peakManagedResidentBytes = Math.max(
        peakManagedResidentBytes,
        result.peakManagedResidentBytes ?? 0,
      );
      instancePeakManagedResidentBytes = Math.max(
        instancePeakManagedResidentBytes,
        result.peakManagedResidentBytes ?? 0,
      );
      return result;
    };
    const completeSnapshot = async (afterFirstBatch) => {
      let result;
      let peakMarks = 0;
      let calls = 0;
      for (; calls < 2000; calls += 1) {
        result = await maintenanceCall(() =>
          filesystem.maintenance.snapshotStorage({ maxBatches: 8 }),
        );
        peakMarks = Math.max(peakMarks, markCount("efs_storage_marks"));
        observeMemory();
        updatePhysicalPeak();
        if (calls === 0 && afterFirstBatch) {
          assert.equal(result.state, "paused");
          await afterFirstBatch();
        }
        if (result.state === "complete") break;
      }
      assert.equal(result.state, "complete");
      assert.ok(calls < 2000);
      return { result, peakMarks };
    };
    try {
      await open(true);
      const fixtureDigest = createHash("sha256");
      fixtureDigest.update("efs-m5-scale-fixture-v2\0");
      const rootInode = injector.driver.transaction(
        "read",
        (tx) =>
          tx.all("SELECT root_inode FROM efs_meta WHERE singleton=1", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0].root_inode,
      );
      for (let start = 0; start < count; start += insertBatchSize) {
        const end = Math.min(count, start + insertBatchSize);
        const records = Array.from({ length: end - start }, (_, offset) =>
          manifestRecord(start + offset),
        );
        for (let index = start; index < end; index += 1) {
          const record = records[index - start];
          const name = `scale-file-${index.toString().padStart(6, "0")}`;
          fixtureDigest.update(record.objectHash);
          fixtureDigest.update(record.nodeHash);
          fixtureDigest.update(record.rootHash);
          fixtureDigest.update(encoder.encode(name));
        }
        injector.driver.transaction("write", (tx) => {
          const content = new ContentRepository(tx, limits);
          content.putFreshObjectsBatch(
            records.map((record) => ({ hash: record.objectHash, bytes: record.bytes })),
          );
          content.putFreshManifestNodesBatch(
            records.map((record) => ({ hash: record.nodeHash, encoded: record.node })),
          );
          for (const record of records) {
            content.putFreshManifestRoot(record.rootHash, record.root);
            tx.run(
              "INSERT INTO efs_manifest_validations(manifest_hash,tree_depth) VALUES(?,1)",
              [record.rootHash],
            );
          }
          let variableBytes = 0;
          for (let index = start; index < end; index += 1) {
            const record = records[index - start];
            const name = `scale-file-${index.toString().padStart(6, "0")}`;
            const nameSort = encoder.encode(name);
            const inodeId = `scale-inode-${index.toString().padStart(6, "0")}`;
            tx.run(
              "INSERT INTO efs_inodes(id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token) VALUES(?,0,420,1,1,1,1,?,?,NULL,0)",
              [inodeId, record.bytes.length, record.rootHash],
            );
            tx.run(
              "INSERT INTO efs_entries(parent_inode,name_sort,name,inode_id,token) VALUES(?,?,?,?,0)",
              [rootInode, nameSort, name, inodeId],
            );
            variableBytes += nameSort.length * 2;
          }
          new UsageRepository(tx, limits).apply({
            charged_metadata_bytes:
              (end - start) * CHARGED_ROW_BYTES * 3 + variableBytes,
          });
        });
        observeMemory();
        updatePhysicalPeak();
        if (end === baselineScaleCount) {
          const baselineCheckpoint = injector.driver.checkpoint("truncate");
          assert.equal(baselineCheckpoint.busy, 0);
          assert.equal(baselineCheckpoint.walBytes, 0);
          await close();
          await open(false);
          assert.deepEqual(
            await filesystem.readFile("/scale-file-000000"),
            objectBytes(0),
          );
          assert.deepEqual(
            await filesystem.readFile("/scale-file-010239"),
            objectBytes(baselineScaleCount - 1),
          );
          const baselineSnapshot = await completeSnapshot(() =>
            filesystem.writeFile("/scale-file-000000", objectBytes(0)),
          );
          assert.equal(baselineSnapshot.result.objectCount, baselineScaleCount);
          assert.ok(baselineSnapshot.peakMarks >= baselineScaleCount * 3);
          baselineScaleManagedPeak = instancePeakManagedResidentBytes;
          assert.ok(baselineScaleManagedPeak > 8 * 1024 * 1024);
          assert.ok(baselineScaleManagedPeak < 16 * 1024 * 1024);
          await close();
          await open(false);
        }
      }
      const scaleFixtureDigest = fixtureDigest.digest("hex");
      assert.match(scaleFixtureDigest, /^[0-9a-f]{64}$/u);
      injector.driver.transaction("write", (tx) => {
        new StagingRepository(tx, limits).bumpRoot(0, "scale-manifest-attachment");
      });
      const seeded = injector.driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT (SELECT count(*) FROM efs_cas_objects) objects,(SELECT count(*) FROM efs_manifest_roots) roots,(SELECT count(*) FROM efs_manifest_nodes) nodes,(SELECT count(*) FROM efs_entries) entries",
            [],
            { maxRows: 1, maxBytes: 256 },
          )[0],
      );
      assert.deepEqual(seeded, {
        objects: count,
        roots: count + 1,
        nodes: count,
        entries: count,
      });
      const expectedSupersededRootBytes = manifestRecord(0).root.length;
      const setupCheckpoint = injector.driver.checkpoint("truncate");
      assert.equal(setupCheckpoint.busy, 0);
      assert.equal(setupCheckpoint.walBytes, 0);
      await close();
      await open(false);
      assert.deepEqual(await filesystem.readFile("/scale-file-000000"), objectBytes(0));
      assert.deepEqual(
        await filesystem.readFile("/scale-file-099999"),
        objectBytes(count - 1),
      );
      observeMemory();

      const maintenanceStarted = performance.now();
      const firstSnapshotStep = await maintenanceCall(() =>
        filesystem.maintenance.snapshotStorage({ maxBatches: 8 }),
      );
      assert.equal(firstSnapshotStep.state, "paused");
      let peakStorageMarks = markCount("efs_storage_marks");
      observeMemory();
      updatePhysicalPeak();
      await filesystem.writeFile("/concurrent", "writer-after-snapshot-start");
      let snapshot = firstSnapshotStep;
      let snapshotCalls = 1;
      for (
        ;
        snapshot.state !== "complete" && snapshotCalls < 2000;
        snapshotCalls += 1
      ) {
        snapshot = await maintenanceCall(() =>
          filesystem.maintenance.snapshotStorage({ maxBatches: 8 }),
        );
        peakStorageMarks = Math.max(peakStorageMarks, markCount("efs_storage_marks"));
        observeMemory();
        updatePhysicalPeak();
      }
      assert.equal(snapshot.state, "complete");
      assert.ok(snapshotCalls < 2000);
      assert.ok(peakStorageMarks >= count * 3);
      assert.equal(snapshot.reclaimablePayloadBytes, expectedSupersededRootBytes);
      const fullScaleManagedPeak = instancePeakManagedResidentBytes;
      assert.ok(fullScaleManagedPeak > 0);
      assert.ok(fullScaleManagedPeak < 16 * 1024 * 1024);
      assert.ok(
        fullScaleManagedPeak <= baselineScaleManagedPeak + 512 * 1024,
        JSON.stringify({ baselineScaleManagedPeak, fullScaleManagedPeak }),
      );

      const exact = injector.driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT u.object_count,u.object_bytes,u.manifest_root_count,u.manifest_root_bytes,u.manifest_node_count,u.manifest_node_bytes,(SELECT count(*) FROM efs_entries) entry_count,(SELECT count(*) FROM efs_inodes) inode_count FROM efs_usage u WHERE singleton=1",
            [],
            { maxRows: 1, maxBytes: 1024 },
          )[0],
      );
      assert.equal(exact.entry_count, count + 1);
      assert.equal(exact.inode_count, count + 2);
      assert.equal(exact.object_count, seeded.objects + 1);
      assert.equal(exact.manifest_root_count, seeded.roots + 1);
      assert.equal(exact.manifest_node_count, seeded.nodes + 1);
      assert.equal(snapshot.objectCount, exact.object_count);
      assert.equal(snapshot.storedObjectPayloadBytes, exact.object_bytes);
      assert.equal(snapshot.reachableObjectPayloadBytes, exact.object_bytes);
      assert.equal(
        snapshot.storedManifestPayloadBytes,
        exact.manifest_root_bytes + exact.manifest_node_bytes,
      );

      let verificationCursor;
      let verification;
      let verifiedRows = 0;
      let verificationBatches = 0;
      for (; verificationBatches < 25_000; verificationBatches += 1) {
        verification = await maintenanceCall(() =>
          filesystem.maintenance.verify({
            cursor: verificationCursor,
            maxEntities: insertBatchSize,
          }),
        );
        assert.ok(verification.checkedEntities <= insertBatchSize);
        verifiedRows += verification.checkedEntities;
        verificationCursor = verification.nextCursor ?? undefined;
        observeMemory();
        if (verification.complete) break;
      }
      assert.equal(
        verification.complete,
        true,
        JSON.stringify({ verificationBatches, verifiedRows, verification }),
      );
      assert.equal(verificationCursor, undefined);
      assert.ok(verifiedRows >= count * 4);
      const beforeCheckpoint = injector.driver.physicalStorage();
      maxWalBytes = Math.max(maxWalBytes, beforeCheckpoint.walBytes ?? 0);
      const checkpoint = injector.driver.checkpoint("truncate");
      assert.equal(checkpoint.busy, 0);
      assert.equal(checkpoint.walBytes, 0);
      await close();

      await open(false);
      assert.equal(
        await filesystem.readFile("/concurrent", { encoding: "utf8" }),
        "writer-after-snapshot-start",
      );
      const orphanBytes = encoder.encode("scale-orphan");
      injector.driver.transaction("write", (tx) =>
        new ContentRepository(tx, limits).putObject(sha256(orphanBytes), orphanBytes),
      );
      let collection = await maintenanceCall(() =>
        filesystem.maintenance.collectGarbage({
          runId: "scale-reachable-collection",
          maxBatches: 25,
        }),
      );
      assert.equal(collection.state, "paused");
      let peakGcMarks = markCount(
        "efs_gc_marks",
        "WHERE run_id='scale-reachable-collection'",
      );
      updatePhysicalPeak();
      await close();

      await open(false);
      let collectionCalls = 1;
      for (; collectionCalls < 2000; collectionCalls += 1) {
        collection = await maintenanceCall(() =>
          filesystem.maintenance.collectGarbage({
            runId: "scale-reachable-collection",
            maxBatches: 8,
          }),
        );
        peakGcMarks = Math.max(
          peakGcMarks,
          markCount("efs_gc_marks", "WHERE run_id='scale-reachable-collection'"),
        );
        observeMemory();
        updatePhysicalPeak();
        if (collection.state === "complete") break;
      }
      assert.equal(collection.state, "complete");
      assert.ok(collectionCalls < 2000);
      assert.ok(peakGcMarks >= count * 3);
      assert.equal(collection.deletedObjectCount, 1);
      assert.equal(collection.reclaimedObjectPayloadBytes, orphanBytes.length);
      assert.equal(collection.deletedManifestRootCount, 1);
      assert.equal(
        collection.reclaimedManifestPayloadBytes,
        expectedSupersededRootBytes,
      );
      assert.equal(
        markCount("efs_gc_marks", "WHERE run_id='scale-reachable-collection'"),
        0,
      );
      assert.deepEqual(
        await filesystem.readFile("/scale-file-099999"),
        objectBytes(count - 1),
      );
      const expectedPostCollectionUsage = {
        object_count: exact.object_count,
        manifest_root_count: exact.manifest_root_count - 1,
        manifest_node_count: exact.manifest_node_count,
      };
      const usageAfterCollection = injector.driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT object_count,manifest_root_count,manifest_node_count FROM efs_usage WHERE singleton=1",
            [],
            { maxRows: 1, maxBytes: 256 },
          )[0],
      );
      assert.deepEqual(usageAfterCollection, expectedPostCollectionUsage);
      const finalCheckpoint = injector.driver.checkpoint("truncate");
      assert.equal(finalCheckpoint.busy, 0);
      assert.equal(finalCheckpoint.walBytes, 0);
      const maintenanceDurationMs = performance.now() - maintenanceStarted;
      // The older edge-only fixture completed inside 180 seconds. This stricter
      // fixture enumerates 100,000 distinct roots and nodes in addition to the
      // original objects and namespace rows; keep the job finite without
      // widening any per-query or per-transaction envelope.
      assert.ok(maintenanceDurationMs < 480_000);
      await close();

      await open(false);
      const postReopenUsage = injector.driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT object_count,manifest_root_count,manifest_node_count FROM efs_usage WHERE singleton=1",
            [],
            { maxRows: 1, maxBytes: 256 },
          )[0],
      );
      assert.deepEqual(postReopenUsage, expectedPostCollectionUsage);
      assert.deepEqual(await filesystem.readFile("/scale-file-000000"), objectBytes(0));
      observeMemory();

      const metrics = metricParts.reduce(
        (total, part) => ({
          transactions: total.transactions + part.transactions,
          executedStatements: total.executedStatements + part.executedStatements,
          durableStatements: total.durableStatements + part.durableStatements,
          committedBatches: total.committedBatches + part.committedBatches,
          maxBatchStatements: Math.max(
            total.maxBatchStatements,
            part.maxBatchStatements,
          ),
        }),
        {
          transactions: 0,
          executedStatements: 0,
          durableStatements: 0,
          committedBatches: 0,
          maxBatchStatements: 0,
        },
      );
      assert.ok(metrics.durableStatements > count * 3);
      assert.ok(metrics.maxBatchStatements <= 4096);
      const heapDeltaBytes = peakHeapBytes - baselineMemory.heapUsed;
      const rssDeltaBytes = peakRssBytes - baselineMemory.rss;
      t.diagnostic(
        `100k process memory: heapPeak=${peakHeapBytes}, rssPeak=${peakRssBytes}, heapDelta=${heapDeltaBytes}, rssDelta=${rssDeltaBytes}, managedPeak=${peakManagedResidentBytes}`,
      );
      assert.ok(peakHeapBytes < 512 * 1024 * 1024);
      assert.ok(peakRssBytes < 768 * 1024 * 1024);
      assert.ok(peakManagedResidentBytes < 16 * 1024 * 1024);
      assert.ok(maxWalBytes > 0);
      assert.ok(maxWalBytes <= storageOptions.maxJournalBytes);
      assert.ok(maxMaintenanceBatchMs < 5000);
      t.diagnostic(
        `100k evidence: fixtureDigest=${scaleFixtureDigest}, baselineRows=${baselineScaleCount}, baselineManagedPeak=${baselineScaleManagedPeak}, namespaceRows=${exact.entry_count}, reachableObjects=${snapshot.objectCount}, manifestRootRows=${exact.manifest_root_count}, manifestNodeRows=${exact.manifest_node_count}, peakStorageMarks=${peakStorageMarks}, peakGcMarks=${peakGcMarks}, verifiedRows=${verifiedRows}, heapPeak=${peakHeapBytes}, rssPeak=${peakRssBytes}, managedPeak=${peakManagedResidentBytes}, fullScaleManagedPeak=${fullScaleManagedPeak}, maxWal=${maxWalBytes}, maxMaintenanceBatchMs=${maxMaintenanceBatchMs.toFixed(1)}, maintenanceMs=${maintenanceDurationMs.toFixed(1)}, transactions=${metrics.transactions}, statements=${metrics.executedStatements}, durableStatements=${metrics.durableStatements}, maxBatchStatements=${metrics.maxBatchStatements}`,
      );
    } finally {
      try {
        await close();
      } catch {}
      await rm(directory, { recursive: true, force: true });
    }
  },
);

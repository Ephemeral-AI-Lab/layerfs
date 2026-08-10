import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifest } from "../../packages/fs/dist/operations/full-rebuild.js";
import {
  DEFAULT_FILESYSTEM_LIMITS,
  MAX_CONTENT_OBJECT_BYTES,
  constrainStorageLimits,
} from "../../packages/fs/dist/resources/limits.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import { NamespaceRepository } from "../../packages/fs/dist/sqlite/namespace-repository.js";
import { StagingRepository } from "../../packages/fs/dist/sqlite/staging-repository.js";
import { UsageRepository } from "../../packages/fs/dist/sqlite/usage-repository.js";
import {
  EFS_APPLICATION_ID,
  EFS_SCHEMA_VERSION,
  initializeOrValidateSchema,
} from "../../packages/fs/dist/sqlite/schema.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { createV1Schema } from "../fixtures/schema-v1.mjs";
import { createV3Schema } from "../fixtures/schema-v3.mjs";

test("schema initialization is deterministic, persisted, and read-only reopen-safe", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-schema-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    const driver = await openNodeSqlite({ filename });
    const created = initializeOrValidateSchema(driver, {
      cowPageBytes: 4096,
      now: 1234,
    });
    const identity = driver.transaction("read", (tx) => ({
      applicationId: tx.all(
        "SELECT application_id value FROM pragma_application_id",
        [],
        { maxRows: 1, maxBytes: 1024 },
      )[0].value,
      userVersion: tx.all("SELECT user_version value FROM pragma_user_version", [], {
        maxRows: 1,
        maxBytes: 1024,
      })[0].value,
    }));
    assert.deepEqual(identity, {
      applicationId: EFS_APPLICATION_ID,
      userVersion: EFS_SCHEMA_VERSION,
    });
    driver.close();
    const readOnly = await openNodeSqlite({ filename, readOnly: true });
    const reopened = initializeOrValidateSchema(readOnly, { cowPageBytes: 4096 });
    assert.deepEqual(reopened, created);
    assert.throws(
      () => initializeOrValidateSchema(readOnly, { cowPageBytes: 8192 }),
      /ESCHEMA/,
    );
    readOnly.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

function migrationDriver(base, failAt, count) {
  return {
    kind: "sqlite",
    readOnly: base.readOnly,
    capabilities: base.capabilities,
    close: () => base.close(),
    transaction(mode, callback) {
      return base.transaction(mode, (tx) => {
        if (mode !== "exclusive") return callback(tx);
        const invoke =
          (fn) =>
          (...args) => {
            count.value += 1;
            if (count.value === failAt) throw new Error(`migration fault ${failAt}`);
            return fn(...args);
          };
        return callback({ scope: tx.scope, run: invoke(tx.run), all: invoke(tx.all) });
      });
    },
  };
}

test("schema v1 migrates data to the current schema and every migration-statement fault rolls back", async () => {
  const probe = await openNodeSqlite({ filename: ":memory:" });
  createV1Schema(probe);
  probe.transaction("write", (tx) => {
    tx.run("INSERT INTO efs_branch_ids VALUES('b',1)");
    tx.run("INSERT INTO efs_branches VALUES('b',0,0,7,1,NULL)");
    tx.run("INSERT INTO efs_cow_pages VALUES('b','inode',2,7,?)", [
      new Uint8Array(4096).fill(3),
    ]);
    tx.run("INSERT INTO efs_patches VALUES('b','inode',0,9,2,?)", [
      Uint8Array.of(4, 5, 6),
    ]);
  });
  const count = { value: 0 };
  initializeOrValidateSchema(migrationDriver(probe, Number.POSITIVE_INFINITY, count), {
    cowPageBytes: 4096,
  });
  const migrated = probe.transaction("read", (tx) => ({
    version: tx.all("SELECT user_version value FROM pragma_user_version", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].value,
    page: tx.all(
      "SELECT page_index,generation,length(bytes) size FROM efs_cow_page_versions",
      [],
      { maxRows: 1, maxBytes: 128 },
    )[0],
    patch: tx.all("SELECT offset,delete_length,insert_length FROM efs_patches", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0],
    segment: tx.all("SELECT segment_index,bytes FROM efs_patch_segments", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0],
  }));
  assert.equal(migrated.version, EFS_SCHEMA_VERSION);
  assert.deepEqual(migrated.page, { page_index: 2, generation: 7, size: 4096 });
  assert.deepEqual(migrated.patch, { offset: 9, delete_length: 2, insert_length: 3 });
  assert.equal(migrated.segment.segment_index, 0);
  assert.deepEqual(migrated.segment.bytes, Uint8Array.of(4, 5, 6));
  probe.close();
  assert.ok(count.value > 20);
  for (let failAt = 1; failAt <= count.value; failAt += 1) {
    const base = await openNodeSqlite({ filename: ":memory:" });
    createV1Schema(base);
    const faultCount = { value: 0 };
    assert.throws(
      () => initializeOrValidateSchema(migrationDriver(base, failAt, faultCount)),
      new RegExp(`migration fault ${failAt}`),
    );
    const state = base.transaction("read", (tx) => ({
      version: tx.all("SELECT user_version value FROM pragma_user_version", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].value,
      meta: tx.all("SELECT schema_version FROM efs_meta", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].schema_version,
      oldPages: tx.all(
        "SELECT count(*) count FROM sqlite_schema WHERE name='efs_cow_pages'",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].count,
      newPages: tx.all(
        "SELECT count(*) count FROM sqlite_schema WHERE name='efs_cow_page_versions'",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].count,
    }));
    assert.deepEqual(state, { version: 1, meta: 1, oldPages: 1, newPages: 0 });
    base.close();
  }
});

test("schema v3 migrates forward to v4 and every v4 statement fault preserves usable v3", async () => {
  const probe = await openNodeSqlite({ filename: ":memory:" });
  createV3Schema(probe);
  probe.transaction("write", (tx) => {
    tx.run("INSERT INTO efs_root_journal VALUES(1,1,?)", [Uint8Array.of(1, 2, 3)]);
    tx.run(
      "INSERT INTO efs_gc_runs(id,state,high_water,root_generation,cursor_kind,cursor_value,created_at_ms) VALUES('run',0,0,0,0,NULL,1)",
    );
    tx.run("INSERT INTO efs_gc_marks VALUES('run',0,?,0)", [new Uint8Array(32)]);
  });
  const count = { value: 0 };
  initializeOrValidateSchema(migrationDriver(probe, Number.POSITIVE_INFINITY, count));
  const migrated = probe.transaction("read", (tx) => ({
    userVersion: tx.all("SELECT user_version value FROM pragma_user_version", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].value,
    metaVersion: tx.all("SELECT schema_version value FROM efs_meta", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0].value,
    usage: tx.all("SELECT mutation_sequence,maintenance_bytes FROM efs_usage", [], {
      maxRows: 1,
      maxBytes: 128,
    })[0],
    v4Tables: tx.all(
      "SELECT count(*) count FROM sqlite_schema WHERE type='table' AND name IN ('efs_lease_cleanups','efs_staging_workspaces','efs_staging_reused_subtrees')",
      [],
      { maxRows: 1, maxBytes: 128 },
    )[0].count,
  }));
  assert.deepEqual(migrated, {
    userVersion: 4,
    metaVersion: 4,
    usage: { mutation_sequence: 0, maintenance_bytes: 291 },
    v4Tables: 3,
  });
  assert.ok(count.value >= 8);
  probe.close();

  for (let failAt = 1; failAt <= count.value; failAt += 1) {
    const base = await openNodeSqlite({ filename: ":memory:" });
    createV3Schema(base);
    const faultCount = { value: 0 };
    assert.throws(
      () => initializeOrValidateSchema(migrationDriver(base, failAt, faultCount)),
      new RegExp(`migration fault ${failAt}`),
    );
    const state = base.transaction("read", (tx) => ({
      userVersion: tx.all("SELECT user_version value FROM pragma_user_version", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].value,
      metaVersion: tx.all("SELECT schema_version value FROM efs_meta", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].value,
      mutationColumn: tx.all(
        "SELECT count(*) count FROM pragma_table_info('efs_usage') WHERE name='mutation_sequence'",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].count,
      cleanupTable: tx.all(
        "SELECT count(*) count FROM sqlite_schema WHERE name='efs_lease_cleanups'",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].count,
    }));
    assert.deepEqual(state, {
      userVersion: 3,
      metaVersion: 3,
      mutationColumn: 0,
      cleanupTable: 0,
    });
    base.close();
  }
});

test("one usage authority enforces aggregate and category quotas transactionally", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const limits = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 1_000,
      maintenanceReserveBytes: 100,
      maxBranchOverlayBytes: 200,
      maxStagingPayloadBytes: 200,
      maxMaintenanceBytes: 100,
      maxPermanentIdentifiers: 1,
    },
    driver.capabilities,
  );
  const apply = (delta) =>
    driver.transaction("write", (tx) =>
      new UsageRepository(tx, limits).apply(delta, "quota test"),
    );
  apply({ object_bytes: 400 });
  apply({ page_bytes: 200 });
  apply({ staging_bytes: 200 });
  apply({ result_bytes: 100 });
  apply({ maintenance_bytes: 100 });
  apply({ permanent_identifiers: 1 });
  for (const delta of [
    { result_bytes: 1 },
    { patch_bytes: 1 },
    { staging_bytes: 1 },
    { maintenance_bytes: 1 },
    { permanent_identifiers: 1 },
  ])
    assert.throws(() => apply(delta), /ENOSPC/);
  const usage = driver.transaction("read", (tx) =>
    new UsageRepository(tx, limits).snapshot(),
  );
  assert.equal(usage.object_bytes, 400);
  assert.equal(usage.page_bytes, 200);
  assert.equal(usage.patch_bytes, 0);
  assert.equal(usage.staging_bytes, 200);
  assert.equal(usage.result_bytes, 100);
  assert.equal(usage.maintenance_bytes, 100);
  assert.equal(usage.permanent_identifiers, 1);
  assert.equal(usage.mutation_sequence, 6);
  driver.close();
});

test("namespace root journals reserve maintenance quota before changing the head", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const tight = constrainStorageLimits(
    { maxMaintenanceBytes: 96 },
    driver.capabilities,
  );
  assert.throws(
    () =>
      driver.transaction("write", (tx) =>
        new NamespaceRepository(
          tx,
          DEFAULT_FILESYSTEM_LIMITS,
          tight,
          "test",
        ).nextRevision(2, 1),
      ),
    /maintenance quota/,
  );
  assert.deepEqual(
    driver.transaction("read", (tx) => ({
      meta: tx.all("SELECT main_revision,root_mutation_generation FROM efs_meta", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0],
      revisions: tx.all("SELECT count(*) count FROM efs_revisions", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].count,
      journals: tx.all("SELECT count(*) count FROM efs_root_journal", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].count,
      maintenance: tx.all("SELECT maintenance_bytes FROM efs_usage", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].maintenance_bytes,
    })),
    {
      meta: { main_revision: 0, root_mutation_generation: 0 },
      revisions: 1,
      journals: 0,
      maintenance: 0,
    },
  );
  const admitted = constrainStorageLimits(
    { maxMaintenanceBytes: 97 },
    driver.capabilities,
  );
  assert.equal(
    driver.transaction("write", (tx) =>
      new NamespaceRepository(
        tx,
        DEFAULT_FILESYSTEM_LIMITS,
        admitted,
        "test",
      ).nextRevision(2, 1),
    ),
    1,
  );
  assert.deepEqual(
    driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT u.maintenance_bytes,m.main_revision,m.root_mutation_generation,(SELECT count(*) FROM efs_root_journal) journals FROM efs_usage u JOIN efs_meta m ON m.singleton=u.singleton",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    ),
    {
      maintenance_bytes: 97,
      main_revision: 1,
      root_mutation_generation: 1,
      journals: 1,
    },
  );
  driver.close();
});

test("two connections serialize quota admission against the authoritative usage row", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-usage-race-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    const first = await openNodeSqlite({ filename });
    initializeOrValidateSchema(first);
    const second = await openNodeSqlite({ filename });
    initializeOrValidateSchema(second);
    const limits = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 1_000,
        maintenanceReserveBytes: 100,
      },
      first.capabilities,
    );
    first.transaction("write", (tx) =>
      new UsageRepository(tx, limits).apply({ object_bytes: 600 }, "first writer"),
    );
    assert.throws(
      () =>
        second.transaction("write", (tx) =>
          new UsageRepository(tx, limits).apply(
            { staging_bytes: 301 },
            "second writer",
          ),
        ),
      /aggregate managed payload quota/,
    );
    const usage = second.transaction("read", (tx) =>
      new UsageRepository(tx, limits).snapshot(),
    );
    assert.equal(usage.object_bytes, 600);
    assert.equal(usage.staging_bytes, 0);
    assert.equal(usage.mutation_sequence, 1);
    second.close();
    first.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("two connections serialize staging metadata admission without an orphan row", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-stage-meta-race-"));
  const filename = path.join(directory, "filesystem.db");
  let first;
  let second;
  try {
    first = await openNodeSqlite({ filename });
    initializeOrValidateSchema(first);
    second = await openNodeSqlite({ filename });
    initializeOrValidateSchema(second);
    const limits = constrainStorageLimits(
      {
        maxChargedMetadataBytes: 448,
        maxManagedPayloadBytes: 1024 * 1024,
        maintenanceReserveBytes: 1024,
      },
      first.capabilities,
    );
    first.transaction("write", (tx) =>
      new StagingRepository(tx, limits).begin({
        leaseId: "first",
        ownerId: "owner",
        ownerNonce: new Uint8Array(16).fill(1),
        now: 1,
        expiresAt: 100,
      }),
    );
    assert.throws(
      () =>
        second.transaction("write", (tx) =>
          new StagingRepository(tx, limits).begin({
            leaseId: "second",
            ownerId: "owner",
            ownerNonce: new Uint8Array(16).fill(2),
            now: 1,
            expiresAt: 100,
          }),
        ),
      /charged metadata quota/,
    );
    assert.deepEqual(
      second.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT charged_metadata_bytes,(SELECT count(*) FROM efs_leases) leases,(SELECT count(*) FROM efs_staging_certificates) certificates FROM efs_usage",
            [],
            { maxRows: 1, maxBytes: 256 },
          )[0],
      ),
      { charged_metadata_bytes: 448, leases: 1, certificates: 1 },
    );
    second.close();
    first.close();
  } finally {
    try {
      second?.close();
    } catch {}
    try {
      first?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("CAS and segmented manifests persist with verified deduplication and exact usage", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const limits = constrainStorageLimits(
    { maxManagedPayloadBytes: 256 * 1024 * 1024, maintenanceReserveBytes: 1024 },
    driver.capabilities,
  );
  const bytes = Uint8Array.from(
    { length: 2 * 1024 * 1024 },
    (_, index) => (index * 131 + index ** 2) & 0xff,
  );
  const manifest = buildManifest(bytes, {
    minimum: 32_768,
    average: 131_072,
    maximum: 524_288,
  });
  driver.transaction("write", (tx) => {
    const repo = new ContentRepository(tx, limits);
    for (const [hash, object] of manifest.objects)
      assert.equal(repo.putObject(Buffer.from(hash, "hex"), object), true);
    for (const node of manifest.nodes.values())
      assert.equal(repo.putManifestNode(node.hash, node.encoded), true);
    assert.equal(repo.putManifestRoot(manifest.rootHash, manifest.root), true);
  });
  driver.transaction("write", (tx) => {
    const repo = new ContentRepository(tx, limits);
    for (const [hash, object] of manifest.objects)
      assert.equal(repo.putObject(Buffer.from(hash, "hex"), object), false);
    assert.deepEqual(repo.getManifestRoot(manifest.rootHash), manifest.root);
  });
  const usage = driver.transaction(
    "read",
    (tx) =>
      tx.all(
        "SELECT object_count,object_bytes,manifest_root_count,manifest_node_count FROM efs_usage",
        [],
        { maxRows: 1, maxBytes: 1024 },
      )[0],
  );
  const uniqueObjectBytes = [...manifest.objects.values()].reduce(
    (sum, value) => sum + value.byteLength,
    0,
  );
  assert.equal(usage.object_count, manifest.objects.size);
  assert.equal(usage.object_bytes, uniqueObjectBytes);
  assert.equal(usage.manifest_root_count, 1);
  assert.equal(usage.manifest_node_count, manifest.nodes.size);
  const firstHash = Buffer.from(manifest.objects.keys().next().value, "hex");
  driver.transaction("write", (tx) =>
    tx.run("UPDATE efs_cas_objects SET bytes=zeroblob(size) WHERE hash=?", [firstHash]),
  );
  assert.throws(
    () =>
      driver.transaction("read", (tx) =>
        new ContentRepository(tx, limits).getObject(firstHash),
      ),
    /digest mismatch/,
  );
  driver.close();
});

test("the exact supported content-object bound persists and bound plus one rolls back", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-object-envelope-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    let driver = await openNodeSqlite({ filename, durability: "relaxed-test" });
    initializeOrValidateSchema(driver);
    let storage = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 128 * 1024 * 1024,
        maintenanceReserveBytes: 1024 * 1024,
      },
      driver.capabilities,
    );
    const exact = new Uint8Array(MAX_CONTENT_OBJECT_BYTES).fill(71);
    const exactHash = sha256(exact);
    assert.equal(
      driver.transaction("write", (tx) =>
        new ContentRepository(tx, storage).putObject(exactHash, exact),
      ),
      true,
    );
    const over = new Uint8Array(MAX_CONTENT_OBJECT_BYTES + 1).fill(72);
    const overHash = sha256(over);
    assert.throws(
      () =>
        driver.transaction("write", (tx) =>
          new ContentRepository(tx, storage).putObject(overHash, over),
        ),
      /object exceeds configured limit/,
    );
    driver.close();

    driver = await openNodeSqlite({
      filename,
      create: false,
      durability: "relaxed-test",
    });
    initializeOrValidateSchema(driver);
    storage = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 128 * 1024 * 1024,
        maintenanceReserveBytes: 1024 * 1024,
      },
      driver.capabilities,
    );
    const reopened = driver.transaction("read", (tx) =>
      new ContentRepository(tx, storage).getObject(exactHash, MAX_CONTENT_OBJECT_BYTES),
    );
    assert.equal(reopened.byteLength, MAX_CONTENT_OBJECT_BYTES);
    assert.deepEqual(sha256(reopened), exactHash);
    assert.deepEqual(
      driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT object_count,object_bytes,(SELECT count(*) FROM efs_cas_objects) rows FROM efs_usage",
            [],
            { maxRows: 1, maxBytes: 128 },
          )[0],
      ),
      {
        object_count: 1,
        object_bytes: MAX_CONTENT_OBJECT_BYTES,
        rows: 1,
      },
    );
    driver.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

function failingDriver(base, occurrence) {
  return {
    kind: "sqlite",
    readOnly: base.readOnly,
    capabilities: base.capabilities,
    close: () => base.close(),
    transaction(mode, callback) {
      return base.transaction(mode, (tx) => {
        let statements = 0;
        const invoke =
          (fn) =>
          (...args) => {
            statements += 1;
            if (statements === occurrence)
              throw new Error(`fault after statement ${occurrence}`);
            return fn(...args);
          };
        return callback({ scope: tx.scope, run: invoke(tx.run), all: invoke(tx.all) });
      });
    },
  };
}

test("failure at every content write statement leaves the complete old state", async () => {
  const bytes = new TextEncoder().encode("atomic-content");
  const hash = sha256(bytes);
  for (let occurrence = 1; occurrence <= 6; occurrence += 1) {
    const base = await openNodeSqlite({ filename: ":memory:" });
    initializeOrValidateSchema(base);
    const driver = failingDriver(base, occurrence);
    const limits = constrainStorageLimits(
      { maxManagedPayloadBytes: 1024 * 1024, maintenanceReserveBytes: 1024 },
      driver.capabilities,
    );
    assert.throws(
      () =>
        driver.transaction("write", (tx) =>
          new ContentRepository(tx, limits).putObject(hash, bytes),
        ),
      /fault after statement/,
    );
    const state = base.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT (SELECT count(*) FROM efs_cas_objects) objects,object_count,object_bytes FROM efs_usage",
          [],
          { maxRows: 1, maxBytes: 1024 },
        )[0],
    );
    assert.deepEqual(state, { objects: 0, object_count: 0, object_bytes: 0 });
    base.close();
  }
});

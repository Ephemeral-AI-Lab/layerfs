import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifest } from "../../packages/fs/dist/operations/full-rebuild.js";
import {
  decodeManifestRoot,
  encodeManifestNode,
  encodeManifestRoot,
} from "../../packages/fs/dist/manifests/codec.js";
import {
  DEFAULT_FILESYSTEM_LIMITS,
  DEFAULT_FASTCDC_MAXIMUM_BYTES,
  DEFAULT_RUNTIME_LIMITS,
  MAX_CONTENT_OBJECT_BYTES,
  MIN_MAINTENANCE_BYTES,
  AdmissionController,
  constrainStorageLimits,
} from "../../packages/fs/dist/resources/limits.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import { ContentCache } from "../../packages/fs/dist/cache/content-cache.js";
import { NamespaceRepository } from "../../packages/fs/dist/sqlite/namespace-repository.js";
import { StagingRepository } from "../../packages/fs/dist/sqlite/staging-repository.js";
import {
  CHARGED_ROW_BYTES,
  UsageRepository,
} from "../../packages/fs/dist/sqlite/usage-repository.js";
import {
  EFS_APPLICATION_ID,
  EFS_SCHEMA_VERSION,
  initializeOrValidateSchema,
} from "../../packages/fs/dist/sqlite/schema.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { createSqliteOperationsStorage } from "../../packages/fs/dist/sqlite/operations-storage.js";
import { prepareContentEntriesStreaming } from "../../packages/fs/dist/operations/streaming-prepare.js";
import { createV1Schema } from "../fixtures/schema-v1.mjs";
import { createV2Schema } from "../fixtures/schema-v2.mjs";
import { createV3Schema } from "../fixtures/schema-v3.mjs";

function admittedRepository(tx, storage, managedBytes = 128 * 1024 * 1024) {
  const admission = new AdmissionController(managedBytes);
  const cache = new ContentCache(1, admission);
  return new ContentRepository(tx, storage, cache);
}

function insertLegacyV3Manifest(driver, manifest, inodeId = "legacy-file") {
  driver.transaction("write", (tx) => {
    let sequence = 1;
    for (const [hash, bytes] of manifest.objects) {
      tx.run(
        "INSERT INTO efs_cas_objects(hash,size,bytes,allocation_sequence) VALUES(?,?,?,?)",
        [Buffer.from(hash, "hex"), bytes.byteLength, bytes, sequence++],
      );
    }
    for (const node of manifest.nodes.values())
      tx.run(
        "INSERT INTO efs_manifest_nodes(hash,kind,logical_bytes,entry_count,encoded,allocation_sequence) VALUES(?,?,?,?,?,?)",
        [
          node.hash,
          node.node.kind === "leaf" ? 0 : 1,
          node.node.span,
          node.node.entryCount,
          node.encoded,
          sequence++,
        ],
      );
    const root = decodeManifestRoot(manifest.root, manifest.rootHash);
    tx.run(
      "INSERT INTO efs_manifest_roots(hash,root_node_hash,file_size,entry_count,chunk_min,chunk_avg,chunk_max,encoded,allocation_sequence) VALUES(?,?,?,?,?,?,?,?,?)",
      [
        manifest.rootHash,
        root.rootNodeHash,
        root.fileSize,
        root.entryCount,
        root.parameters.minimum,
        root.parameters.average,
        root.parameters.maximum,
        manifest.root,
        sequence++,
      ],
    );
    tx.run(
      "INSERT INTO efs_inodes(id,type,mode,birthtime_ms,mtime_ms,ctime_ms,nlink,size,manifest_hash,symlink_target,token) VALUES(?,0,420,0,0,0,1,?,?,NULL,1)",
      [inodeId, root.fileSize, manifest.rootHash],
    );
    tx.run("UPDATE efs_meta SET next_allocation_sequence=? WHERE singleton=1", [
      sequence,
    ]);
  });
}

function unbalancedLegacyManifest() {
  const object = Uint8Array.of(3);
  const objectHash = sha256(object);
  const encoded = (node) => {
    const value = encodeManifestNode(node);
    return Object.freeze({ hash: sha256(value), encoded: value, node });
  };
  const left = encoded(
    Object.freeze({
      kind: "leaf",
      span: 256,
      entryCount: 256,
      entries: Object.freeze(
        Array.from({ length: 256 }, () =>
          Object.freeze({ hash: objectHash, length: 1 }),
        ),
      ),
    }),
  );
  const rightLeaf = encoded(
    Object.freeze({
      kind: "leaf",
      span: 1,
      entryCount: 1,
      entries: Object.freeze([Object.freeze({ hash: objectHash, length: 1 })]),
    }),
  );
  const rightWrapper = encoded(
    Object.freeze({
      kind: "internal",
      span: 1,
      entryCount: 1,
      children: Object.freeze([
        Object.freeze({ hash: rightLeaf.hash, span: 1, entryCount: 1 }),
      ]),
    }),
  );
  const rootNode = encoded(
    Object.freeze({
      kind: "internal",
      span: 257,
      entryCount: 257,
      children: Object.freeze([
        Object.freeze({ hash: left.hash, span: 256, entryCount: 256 }),
        Object.freeze({ hash: rightWrapper.hash, span: 1, entryCount: 1 }),
      ]),
    }),
  );
  const root = encodeManifestRoot(
    Object.freeze({
      parameters: Object.freeze({ minimum: 1, average: 1, maximum: 1 }),
      fileSize: 257,
      entryCount: 257,
      rootNodeHash: rootNode.hash,
    }),
  );
  return Object.freeze({
    root,
    rootHash: sha256(root),
    objects: new Map([[Buffer.from(objectHash).toString("hex"), object]]),
    nodes: new Map(
      [left, rightLeaf, rightWrapper, rootNode].map((node) => [
        Buffer.from(node.hash).toString("hex"),
        node,
      ]),
    ),
  });
}

test("one OperationsStorage transaction rejects mixed quota profiles", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const port = createSqliteOperationsStorage(driver);
  port.initialize();
  const first = constrainStorageLimits(undefined, driver.capabilities);
  const second = Object.freeze({
    ...first,
    maxStagingPayloadBytes: first.maxStagingPayloadBytes - 1,
  });
  assert.throws(
    () =>
      port.transaction("write", { maxRows: 64, maxBytes: 64 * 1024 }, (tx) => {
        tx.content(first);
        tx.staging(second);
      }),
    /cannot mix storage limit profiles/,
  );
  await port.close();
});

test("writer filesystem, storage, and branch limits persist across connections", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-writer-profile-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  let filesystem;
  try {
    driver = await openNodeSqlite({ filename });
    filesystem = await EphemeralFS.open({
      database: driver,
      storage: {
        maxStagingPayloadBytes: 64 * 1024 * 1024,
      },
      branch: { maxActiveBranches: 17 },
    });
    assert.ok(
      filesystem.capabilities.effectiveLimits.some(
        (limit) =>
          limit.domain === "branch" &&
          limit.name === "maxActiveBranches" &&
          limit.value === 17 &&
          limit.scope === "persisted",
      ),
    );
    await filesystem.close();
    filesystem = undefined;
    driver.close();
    driver = undefined;

    driver = await openNodeSqlite({ filename, create: false });
    await assert.rejects(
      EphemeralFS.open({ database: driver }),
      /persisted writer limit profile differs/,
    );
    driver.close();
    driver = undefined;
  } finally {
    try {
      await filesystem?.close();
    } catch {}
    try {
      driver?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("invalid writer profiles reject before creating schema state", async () => {
  for (const options of [
    { filesystem: { maxPathBytes: Number.NaN } },
    { runtime: { maxPendingWriteBytes: 1 } },
    { branch: { maxActiveBranches: 0 } },
  ]) {
    const driver = await openNodeSqlite({ filename: ":memory:" });
    await assert.rejects(
      EphemeralFS.open({ database: driver, ...options }),
      RangeError,
    );
    assert.deepEqual(
      driver.transaction("read", (tx) => ({
        applicationId: tx.all(
          "SELECT application_id value FROM pragma_application_id",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0].value,
        objects: tx.all(
          "SELECT count(*) count FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0].count,
      })),
      { applicationId: 0, objects: 0 },
    );
    driver.close();
  }
});

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

test("current schema recovery authority is revalidated after physical reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-schema-recovery-"));
  try {
    const corruptions = [
      {
        name: "usage",
        mutate: (tx) => tx.run("DELETE FROM efs_usage WHERE singleton=1"),
        expected: /missing usage singleton/,
      },
      {
        name: "metadata",
        mutate: (tx) =>
          tx.run(
            "UPDATE efs_usage SET charged_metadata_bytes=charged_metadata_bytes+96 WHERE singleton=1",
          ),
        expected: /usage integrity token mismatch/,
      },
      {
        name: "payload-counters",
        mutate: (tx) =>
          tx.run(
            "UPDATE efs_usage SET object_count=7,object_bytes=123,page_bytes=41,maintenance_bytes=77 WHERE singleton=1",
          ),
        expected: /usage integrity token mismatch/,
      },
      {
        name: "root-generation",
        mutate: (tx) =>
          tx.run("UPDATE efs_meta SET root_mutation_generation=-1 WHERE singleton=1"),
        expected: /invalid persisted filesystem metadata/,
      },
      {
        name: "allocation-sequence",
        mutate: (tx) =>
          tx.run("UPDATE efs_meta SET next_allocation_sequence=0 WHERE singleton=1"),
        expected: /invalid persisted filesystem metadata/,
      },
      {
        name: "gc-run-state",
        mutate: (tx) =>
          tx.run(
            "INSERT INTO efs_gc_runs(id,state,high_water,root_generation,cursor_kind,cursor_value,created_at_ms) VALUES('corrupt-run',999,0,0,0,NULL,0)",
          ),
        expected: /invalid retained garbage-collection state/,
      },
      {
        name: "trigger",
        mutate: (tx) => tx.run("DROP TRIGGER efs_sealed_certificate_delete"),
        expected: /required schema-v4 table, index, or trigger is missing/,
      },
      {
        name: "extra-trigger",
        mode: "exclusive",
        mutate: (tx) =>
          tx.run(
            "CREATE TRIGGER efs_unexpected_object_delete AFTER INSERT ON efs_cas_objects BEGIN DELETE FROM efs_cas_objects WHERE hash=NEW.hash; END",
          ),
        expected: /unexpected trigger mutates an owned filesystem table/,
      },
    ];
    for (const corruption of corruptions) {
      const filename = path.join(directory, `${corruption.name}.db`);
      let driver = await openNodeSqlite({ filename });
      initializeOrValidateSchema(driver);
      driver.transaction(corruption.mode ?? "write", corruption.mutate);
      driver.close();
      driver = await openNodeSqlite({ filename, create: false });
      assert.throws(() => initializeOrValidateSchema(driver), corruption.expected);
      driver.close();
    }
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

function populateV1MigrationRows(driver) {
  driver.transaction("write", (tx) => {
    tx.run("INSERT INTO efs_branch_ids VALUES('b',1)");
    tx.run("INSERT INTO efs_branches VALUES('b',0,0,7,1,NULL)");
    tx.run("INSERT INTO efs_cow_pages VALUES('b','inode',2,7,?)", [
      new Uint8Array(4096).fill(3),
    ]);
    tx.run("INSERT INTO efs_patches VALUES('b','inode',0,9,2,?)", [
      Uint8Array.of(4, 5, 6),
    ]);
  });
}

test("schema v1 migrates data to the current schema and every migration-statement fault rolls back", async () => {
  const probe = await openNodeSqlite({ filename: ":memory:" });
  createV1Schema(probe);
  populateV1MigrationRows(probe);
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
  const faultDirectory = await mkdtemp(path.join(tmpdir(), "efs-schema-v1-fault-"));
  try {
    for (let failAt = 1; failAt <= count.value; failAt += 1) {
      const filename = path.join(faultDirectory, `fault-${failAt}.db`);
      let base = await openNodeSqlite({ filename });
      createV1Schema(base);
      populateV1MigrationRows(base);
      const faultCount = { value: 0 };
      assert.throws(
        () => initializeOrValidateSchema(migrationDriver(base, failAt, faultCount)),
        new RegExp(`migration fault ${failAt}`),
      );
      base.close();
      base = await openNodeSqlite({ filename, create: false });
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
  } finally {
    await rm(faultDirectory, { recursive: true, force: true });
  }
});

test("released schema v2 migrates through v3 to v4 and file-backed faults reopen as intact v2", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-schema-v2-"));
  try {
    const filename = path.join(directory, "normal.db");
    let driver = await openNodeSqlite({ filename });
    createV2Schema(driver);
    driver.close();
    const readOnly = await openNodeSqlite({ filename, readOnly: true });
    assert.throws(
      () => initializeOrValidateSchema(readOnly),
      /schema v2 requires a writable migration/,
    );
    readOnly.close();
    driver = await openNodeSqlite({ filename, create: false });
    initializeOrValidateSchema(driver);
    assert.deepEqual(
      driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT (SELECT user_version FROM pragma_user_version) version,(SELECT schema_version FROM efs_meta) meta,(SELECT count(*) FROM efs_staging_reconciliations) reconciliations,(SELECT count(*) FROM pragma_table_info('efs_gc_marks') WHERE name='edge_cursor') edge_cursor",
            [],
            { maxRows: 1, maxBytes: 256 },
          )[0],
      ),
      { version: 4, meta: 4, reconciliations: 0, edge_cursor: 1 },
    );
    driver.close();

    const probe = await openNodeSqlite({ filename: ":memory:" });
    createV2Schema(probe);
    const count = { value: 0 };
    initializeOrValidateSchema(migrationDriver(probe, Number.POSITIVE_INFINITY, count));
    probe.close();
    assert.ok(count.value > 10);

    for (let failAt = 1; failAt <= count.value; failAt += 1) {
      const faultFile = path.join(directory, `fault-${failAt}.db`);
      let base = await openNodeSqlite({ filename: faultFile });
      createV2Schema(base);
      const faultCount = { value: 0 };
      assert.throws(
        () => initializeOrValidateSchema(migrationDriver(base, failAt, faultCount)),
        new RegExp(`migration fault ${failAt}`),
      );
      base.close();
      base = await openNodeSqlite({ filename: faultFile, create: false });
      assert.deepEqual(
        base.transaction(
          "read",
          (tx) =>
            tx.all(
              "SELECT (SELECT user_version FROM pragma_user_version) version,(SELECT schema_version FROM efs_meta) meta,(SELECT count(*) FROM sqlite_schema WHERE name='efs_staging_reconciliations') v3,(SELECT count(*) FROM sqlite_schema WHERE name='efs_lease_cleanups') v4",
              [],
              { maxRows: 1, maxBytes: 256 },
            )[0],
        ),
        { version: 2, meta: 2, v3: 0, v4: 0 },
      );
      base.close();
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("schema v3 migrates forward to v4 and every v4 statement fault preserves usable v3", async () => {
  const probe = await openNodeSqlite({ filename: ":memory:" });
  createV3Schema(probe);
  probe.transaction("write", (tx) => {
    tx.run(
      "UPDATE efs_usage SET object_count=7,object_bytes=123,manifest_root_count=5,manifest_root_bytes=99,manifest_node_count=4,manifest_node_bytes=88,page_count=3,page_bytes=77,patch_count=2,patch_bytes=66,staging_bytes=55,result_bytes=44,maintenance_bytes=33,permanent_identifiers=22,charged_metadata_bytes=11",
    );
    tx.run("INSERT INTO efs_root_journal VALUES(1,1,?)", [Uint8Array.of(1, 2, 3)]);
    tx.run(
      "INSERT INTO efs_gc_runs(id,state,high_water,root_generation,cursor_kind,cursor_value,created_at_ms) VALUES('run',0,0,0,0,NULL,1)",
    );
    tx.run("INSERT INTO efs_gc_marks VALUES('run',0,?,0)", [new Uint8Array(32)]);
  });
  const count = { value: 0 };
  initializeOrValidateSchema(migrationDriver(probe, Number.POSITIVE_INFINITY, count));
  const migrated = probe.transaction("read", (tx) => {
    new UsageRepository(
      tx,
      constrainStorageLimits(undefined, probe.capabilities),
    ).verifyDerivedUsage();
    return {
      userVersion: tx.all("SELECT user_version value FROM pragma_user_version", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].value,
      metaVersion: tx.all("SELECT schema_version value FROM efs_meta", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].value,
      usage: tx.all(
        "SELECT mutation_sequence,object_count,object_bytes,page_count,page_bytes,result_bytes,permanent_identifiers,maintenance_bytes FROM efs_usage",
        [],
        { maxRows: 1, maxBytes: 1024 },
      )[0],
      v4Tables: tx.all(
        "SELECT count(*) count FROM sqlite_schema WHERE type='table' AND name IN ('efs_lease_cleanups','efs_staging_workspaces','efs_staging_reused_subtrees')",
        [],
        { maxRows: 1, maxBytes: 128 },
      )[0].count,
    };
  });
  assert.deepEqual(migrated, {
    userVersion: 4,
    metaVersion: 4,
    usage: {
      mutation_sequence: 0,
      object_count: 0,
      object_bytes: 0,
      page_count: 0,
      page_bytes: 0,
      result_bytes: 0,
      permanent_identifiers: 0,
      maintenance_bytes: 1033,
    },
    v4Tables: 3,
  });
  assert.ok(count.value >= 8);
  probe.close();

  const faultDirectory = await mkdtemp(path.join(tmpdir(), "efs-schema-v3-fault-"));
  try {
    for (let failAt = 1; failAt <= count.value; failAt += 1) {
      const filename = path.join(faultDirectory, `fault-${failAt}.db`);
      let base = await openNodeSqlite({ filename });
      createV3Schema(base);
      const faultCount = { value: 0 };
      assert.throws(
        () => initializeOrValidateSchema(migrationDriver(base, failAt, faultCount)),
        new RegExp(`migration fault ${failAt}`),
      );
      base.close();
      base = await openNodeSqlite({ filename, create: false });
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
  } finally {
    await rm(faultDirectory, { recursive: true, force: true });
  }
});

test("populated multi-height v3 manifests certify and remain readable after physical reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-v3-manifest-"));
  const filename = path.join(directory, "filesystem.db");
  const parameters = Object.freeze({ minimum: 1, average: 1, maximum: 1 });
  const bytes = new Uint8Array(257).fill(7);
  const manifest = buildManifest(bytes, parameters);
  const manifestLimits = {
    maxManifestEntries: 1024,
    maxManifestDepth: 4,
    maxFileBytes: 4096,
    maxContentObjectBytes: 524_288,
  };
  let driver;
  try {
    driver = await openNodeSqlite({ filename });
    createV3Schema(driver);
    insertLegacyV3Manifest(driver, manifest);
    initializeOrValidateSchema(driver, manifestLimits);
    assert.deepEqual(
      driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT tree_depth FROM efs_manifest_validations WHERE manifest_hash=?",
            [manifest.rootHash],
            { maxRows: 1, maxBytes: 128 },
          )[0],
      ),
      { tree_depth: 2 },
    );
    driver.close();
    driver = await openNodeSqlite({ filename, create: false });
    initializeOrValidateSchema(driver, manifestLimits);
    const storage = constrainStorageLimits(
      {
        maxManifestEntries: manifestLimits.maxManifestEntries,
        maxManifestDepth: manifestLimits.maxManifestDepth,
        maxFileBytes: manifestLimits.maxFileBytes,
        maxManagedPayloadBytes: 16 * 1024 * 1024,
        maintenanceReserveBytes: 4096,
      },
      driver.capabilities,
    );
    const actual = driver.transaction("read", (tx) => {
      const cursor = admittedRepository(tx, storage).openManifestCursor(
        manifest.rootHash,
        0,
      );
      try {
        const output = new Uint8Array(bytes.length);
        assert.equal(cursor.readInto(output, 0, output.length), output.length);
        return output;
      } finally {
        cursor.close();
      }
    });
    assert.deepEqual(actual, bytes);
  } finally {
    try {
      driver?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("a released v3 database containing one exact-bound object migrates and reopens", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-v3-max-object-"));
  const filename = path.join(directory, "filesystem.db");
  const bytes = new Uint8Array(MAX_CONTENT_OBJECT_BYTES).fill(11);
  const hash = sha256(bytes);
  let driver;
  try {
    driver = await openNodeSqlite({ filename });
    createV3Schema(driver);
    driver.transaction("write", (tx) => {
      tx.run(
        "INSERT INTO efs_cas_objects(hash,size,bytes,allocation_sequence) VALUES(?,?,?,1)",
        [hash, bytes.length, bytes],
      );
      tx.run("UPDATE efs_meta SET next_allocation_sequence=2 WHERE singleton=1");
    });
    driver.close();
    driver = await openNodeSqlite({ filename, create: false });
    initializeOrValidateSchema(driver, {
      maxManifestEntries: 1024,
      maxManifestDepth: 4,
      maxFileBytes: 4096,
      maxContentObjectBytes: MAX_CONTENT_OBJECT_BYTES,
    });
    driver.close();
    driver = await openNodeSqlite({ filename, create: false });
    initializeOrValidateSchema(driver, {
      maxManifestEntries: 1024,
      maxManifestDepth: 4,
      maxFileBytes: 4096,
      maxContentObjectBytes: MAX_CONTENT_OBJECT_BYTES,
    });
    assert.deepEqual(
      driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT size,length(bytes) stored FROM efs_cas_objects WHERE hash=?",
            [hash],
            { maxRows: 1, maxBytes: 128 },
          )[0],
      ),
      { size: MAX_CONTENT_OBJECT_BYTES, stored: MAX_CONTENT_OBJECT_BYTES },
    );
  } finally {
    try {
      driver?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("legacy certification rolls back corrupt, unbalanced, and unwritable manifests", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-v3-invalid-manifest-"));
  try {
    for (const kind of ["missing-node", "unbalanced", "unwritable-maximum"]) {
      const filename = path.join(directory, `${kind}.db`);
      let driver = await openNodeSqlite({ filename });
      createV3Schema(driver);
      const manifest =
        kind === "unbalanced"
          ? unbalancedLegacyManifest()
          : buildManifest(
              new Uint8Array(kind === "missing-node" ? 257 : 1).fill(9),
              kind === "missing-node"
                ? Object.freeze({ minimum: 1, average: 1, maximum: 1 })
                : Object.freeze({
                    minimum: 1,
                    average: 1_048_576,
                    maximum: 1_048_576,
                  }),
            );
      insertLegacyV3Manifest(driver, manifest, `legacy-${kind}`);
      if (kind === "missing-node") {
        const root = decodeManifestRoot(manifest.root, manifest.rootHash);
        driver.transaction("write", (tx) =>
          tx.run("DELETE FROM efs_manifest_nodes WHERE hash=?", [root.rootNodeHash]),
        );
      }
      assert.throws(
        () =>
          initializeOrValidateSchema(driver, {
            maxManifestEntries: 1024,
            maxManifestDepth: 4,
            maxFileBytes: 4096,
            maxContentObjectBytes: 524_288,
          }),
        /legacy manifest|manifest node|missing|unbalanced|ECORRUPT/,
      );
      driver.close();
      driver = await openNodeSqlite({ filename, create: false });
      assert.deepEqual(
        driver.transaction(
          "read",
          (tx) =>
            tx.all(
              "SELECT (SELECT user_version FROM pragma_user_version) version,(SELECT schema_version FROM efs_meta) meta,(SELECT count(*) FROM sqlite_schema WHERE name='efs_manifest_validations') validations",
              [],
              { maxRows: 1, maxBytes: 256 },
            )[0],
        ),
        { version: 3, meta: 3, validations: 0 },
      );
      driver.close();
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("v4 migration refuses an unbounded atomic recount before changing v3", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-schema-v4-cap-"));
  const filename = path.join(directory, "filesystem.db");
  let driver = await openNodeSqlite({
    filename,
    maxJournalBytes: 4 * 1024 ** 3,
  });
  createV3Schema(driver);
  driver.close();
  const fixture = new DatabaseSync(filename);
  fixture.exec(
    "WITH RECURSIVE n(value) AS (VALUES(0) UNION ALL SELECT value+1 FROM n WHERE value<100000) INSERT INTO efs_operation_ids(id,branch_id,generation,created_at_ms) SELECT 'op-'||value,'branch',0,0 FROM n",
  );
  fixture.close();
  try {
    driver = await openNodeSqlite({
      filename,
      create: false,
      maxJournalBytes: 4 * 1024 ** 3,
    });
    assert.throws(
      () => initializeOrValidateSchema(driver),
      /atomic usage recount exceeds 100000 rows/,
    );
    assert.deepEqual(
      driver.transaction(
        "read",
        (tx) =>
          tx.all(
            "SELECT (SELECT user_version FROM pragma_user_version) version,(SELECT schema_version FROM efs_meta) meta,(SELECT count(*) FROM sqlite_schema WHERE name='efs_lease_cleanups') v4",
            [],
            { maxRows: 1, maxBytes: 128 },
          )[0],
      ),
      { version: 3, meta: 3, v4: 0 },
    );
    driver.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("v1 transformed BLOB bytes admit the exact envelope and reject plus one row", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-schema-v1-bytes-"));
  try {
    for (const patchCount of [32, 33]) {
      const filename = path.join(directory, `patches-${patchCount}.db`);
      let driver = await openNodeSqlite({ filename });
      createV1Schema(driver);
      const payload = new Uint8Array(MAX_CONTENT_OBJECT_BYTES).fill(17);
      driver.transaction("write", (tx) => {
        tx.run(
          "INSERT INTO efs_branches(id,base_revision,state,generation,created_at_ms,terminal_at_ms) VALUES('legacy-branch',0,0,0,0,NULL)",
        );
        for (let index = 0; index < patchCount; index += 1)
          tx.run(
            "INSERT INTO efs_patches(branch_id,inode_id,sequence,offset,delete_length,insert_bytes) VALUES('legacy-branch','legacy-inode',?,0,0,?)",
            [index, index === 0 ? payload : new Uint8Array()],
          );
      });
      if (patchCount === 32) {
        initializeOrValidateSchema(driver);
        assert.equal(
          driver.transaction(
            "read",
            (tx) =>
              tx.all("SELECT user_version value FROM pragma_user_version", [], {
                maxRows: 1,
                maxBytes: 128,
              })[0].value,
          ),
          4,
        );
      } else {
        assert.throws(
          () => initializeOrValidateSchema(driver),
          /legacy transformed payload exceeds/,
        );
        driver.close();
        driver = await openNodeSqlite({ filename, create: false });
        assert.deepEqual(
          driver.transaction("read", (tx) => ({
            version: tx.all("SELECT user_version value FROM pragma_user_version", [], {
              maxRows: 1,
              maxBytes: 128,
            })[0].value,
            legacyPatches: tx.all(
              "SELECT count(*) count FROM pragma_table_info('efs_patches') WHERE name='insert_bytes'",
              [],
              { maxRows: 1, maxBytes: 128 },
            )[0].count,
          })),
          { version: 1, legacyPatches: 1 },
        );
      }
      driver.close();
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("one usage authority enforces aggregate and category quotas transactionally", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const limits = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 10_000,
      maintenanceReserveBytes: 3_000,
      maxBranchOverlayBytes: 200,
      maxStagingPayloadBytes: 200,
      maxMaintenanceBytes: 3_000,
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
  apply({ result_bytes: 6_200 });
  apply({ maintenance_bytes: 3_000 });
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
  assert.equal(usage.result_bytes, 6_200);
  assert.equal(usage.maintenance_bytes, 3_000);
  assert.equal(usage.permanent_identifiers, 1);
  assert.equal(usage.mutation_sequence, 6);
  driver.close();
});

test("staging identities and nonces are intrinsically bounded before durable admission", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const limits = constrainStorageLimits(undefined, driver.capabilities);
  const baselineMetadata = driver.transaction(
    "read",
    (tx) => new UsageRepository(tx, limits).snapshot().charged_metadata_bytes,
  );
  for (const options of [
    {
      leaseId: "x".repeat(129),
      ownerId: "owner",
      ownerNonce: new Uint8Array(16),
    },
    {
      leaseId: "lease",
      ownerId: "x".repeat(129),
      ownerNonce: new Uint8Array(16),
    },
    {
      leaseId: "lease",
      ownerId: "owner",
      ownerNonce: new Uint8Array(17),
    },
  ])
    assert.throws(
      () =>
        driver.transaction("write", (tx) =>
          new StagingRepository(tx, limits).begin({
            ...options,
            now: 1,
            expiresAt: 2,
          }),
        ),
      /1\.\.128 UTF-8 bytes|owner nonce is invalid/,
    );
  assert.deepEqual(
    driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT (SELECT count(*) FROM efs_leases) leases,(SELECT count(*) FROM efs_staging_certificates) certificates,charged_metadata_bytes FROM efs_usage",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    ),
    {
      leases: 0,
      certificates: 0,
      charged_metadata_bytes: baselineMetadata,
    },
  );
  driver.close();
});

test("namespace root journals reserve maintenance quota before changing the head", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  assert.throws(
    () =>
      constrainStorageLimits(
        {
          maxMaintenanceBytes: MIN_MAINTENANCE_BYTES - 1,
          maintenanceReserveBytes: MIN_MAINTENANCE_BYTES - 1,
        },
        driver.capabilities,
      ),
    /bounded progress/,
  );
  const admitted = constrainStorageLimits(
    {
      maxMaintenanceBytes: MIN_MAINTENANCE_BYTES,
      maintenanceReserveBytes: MIN_MAINTENANCE_BYTES,
    },
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
      maintenance_bytes: CHARGED_ROW_BYTES + 1,
      main_revision: 1,
      root_mutation_generation: 1,
      journals: 1,
    },
  );
  driver.close();
});

test("transaction row profiles keep every derived statement budget safe", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const maximumRows = Math.floor(Number.MAX_SAFE_INTEGER / 4);
  assert.equal(
    constrainStorageLimits(
      { maxFinalTransactionRows: maximumRows },
      driver.capabilities,
    ).maxFinalTransactionRows,
    maximumRows,
  );
  assert.throws(
    () =>
      constrainStorageLimits(
        { maxFinalTransactionRows: maximumRows + 1 },
        driver.capabilities,
      ),
    /safe derived statement envelope/,
  );
  driver.close();
});

test("storage profiles reject an adapter that cannot persist default FastCDC chunks", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  assert.throws(
    () =>
      constrainStorageLimits(undefined, {
        ...driver.capabilities,
        maxBlobBytes: DEFAULT_FASTCDC_MAXIMUM_BYTES - 1,
      }),
    /cannot persist the default FastCDC maximum/,
  );
  assert.equal(
    constrainStorageLimits(undefined, {
      ...driver.capabilities,
      maxBlobBytes: DEFAULT_FASTCDC_MAXIMUM_BYTES,
    }).maxFinalTransactionBytes,
    DEFAULT_FASTCDC_MAXIMUM_BYTES + 16 * 1024,
  );
  driver.close();
});

test("namespace variable metadata deltas match a bounded direct recount across reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-usage-variable-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  let filesystem;
  try {
    driver = await openNodeSqlite({ filename });
    filesystem = await EphemeralFS.open({ database: driver });
    await filesystem.mkdir("/目录");
    await filesystem.symlink("x".repeat(4096), "/目录/链接");
    await filesystem.rename("/目录/链接", "/目录/重命名");
    const limits = constrainStorageLimits(undefined, driver.capabilities);
    const before = driver.transaction("read", (tx) => {
      const usage = new UsageRepository(tx, limits);
      usage.verifyDerivedUsage();
      return {
        actual: usage.snapshot().charged_metadata_bytes,
        direct: usage.directChargedMetadataBytes(),
      };
    });
    assert.equal(before.actual, before.direct);
    await filesystem.close();
    filesystem = undefined;
    driver.close();
    driver = undefined;
    driver = await openNodeSqlite({ filename, create: false });
    const after = driver.transaction("read", (tx) => {
      const usage = new UsageRepository(tx, limits);
      return {
        actual: usage.snapshot().charged_metadata_bytes,
        direct: usage.directChargedMetadataBytes(),
      };
    });
    assert.deepEqual(after, before);
    driver.close();
    driver = undefined;
  } finally {
    try {
      await filesystem?.close();
    } catch {}
    try {
      driver?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("direct usage recount refuses before scanning beyond its configured row envelope", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const limits = constrainStorageLimits(
    { maxQueryBatchSize: 256 },
    driver.capabilities,
  );
  driver.transaction("write", (tx) => {
    for (let index = 0; index < 257; index += 1)
      tx.run(
        "INSERT INTO efs_operation_ids(id,branch_id,generation,created_at_ms) VALUES(?,?,0,0)",
        [`operation-${index}`, "branch"],
      );
  });
  assert.throws(
    () =>
      driver.transaction("read", (tx) =>
        new UsageRepository(tx, limits).directChargedMetadataBytes(),
      ),
    /bounded row envelope/,
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
        maxManagedPayloadBytes: 10_000,
        maintenanceReserveBytes: 3_000,
      },
      first.capabilities,
    );
    first.transaction("write", (tx) =>
      new UsageRepository(tx, limits).apply({ object_bytes: 6_000 }, "first writer"),
    );
    assert.throws(
      () =>
        second.transaction("write", (tx) =>
          new UsageRepository(tx, limits).apply(
            { staging_bytes: 1_001 },
            "second writer",
          ),
        ),
      /aggregate managed payload quota/,
    );
    const usage = second.transaction("read", (tx) =>
      new UsageRepository(tx, limits).snapshot(),
    );
    assert.equal(usage.object_bytes, 6_000);
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
    const defaults = constrainStorageLimits(undefined, first.capabilities);
    const baselineMetadata = first.transaction(
      "read",
      (tx) => new UsageRepository(tx, defaults).snapshot().charged_metadata_bytes,
    );
    const limits = constrainStorageLimits(
      {
        maxChargedMetadataBytes: baselineMetadata + 2 * CHARGED_ROW_BYTES,
        maxManagedPayloadBytes: 1024 * 1024,
        maintenanceReserveBytes: 4096,
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
      {
        charged_metadata_bytes: baselineMetadata + 2 * CHARGED_ROW_BYTES,
        leases: 1,
        certificates: 1,
      },
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
    { maxManagedPayloadBytes: 256 * 1024 * 1024, maintenanceReserveBytes: 4096 },
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
    const admission = new AdmissionController(128 * 1024 * 1024);
    const cache = new ContentCache(32 * 1024 * 1024, admission);
    const repo = new ContentRepository(tx, limits, cache);
    for (const [hash, object] of manifest.objects)
      assert.equal(repo.putObject(Buffer.from(hash, "hex"), object), false);
    assert.deepEqual(
      repo.withManifestRoot(manifest.rootHash, (encoded) => Uint8Array.from(encoded)),
      manifest.root,
    );
    cache.clear();
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
  driver.transaction("write", (tx) => {
    const size = tx.all("SELECT size FROM efs_cas_objects WHERE hash=?", [firstHash], {
      maxRows: 1,
      maxBytes: 128,
    })[0].size;
    tx.run("UPDATE efs_cas_objects SET bytes=? WHERE hash=?", [
      new Uint8Array(size),
      firstHash,
    ]);
  });
  assert.throws(
    () =>
      driver.transaction("read", (tx) =>
        admittedRepository(tx, limits).verifyObject(firstHash),
      ),
    /digest mismatch/,
  );
  driver.close();
});

test("the exact supported content-object bound persists and bound plus one rolls back", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-object-envelope-"));
  const filename = path.join(directory, "filesystem.db");
  let driver;
  let port;
  try {
    driver = await openNodeSqlite({ filename, durability: "relaxed-test" });
    port = createSqliteOperationsStorage(driver);
    port.initialize();
    let storage = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 128 * 1024 * 1024,
        maintenanceReserveBytes: 1024 * 1024,
      },
      driver.capabilities,
    );
    const exact = new Uint8Array(MAX_CONTENT_OBJECT_BYTES).fill(71);
    const exactHash = sha256(exact);
    const parameters = Object.freeze({
      minimum: MAX_CONTENT_OBJECT_BYTES,
      average: MAX_CONTENT_OBJECT_BYTES,
      maximum: MAX_CONTENT_OBJECT_BYTES,
    });
    const admission = new AdmissionController(
      DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
    );
    const prepared = await prepareContentEntriesStreaming(
      port,
      [{ hash: exactHash, length: exact.length, bytes: exact }],
      parameters,
      exact.length,
      storage,
      DEFAULT_RUNTIME_LIMITS,
      admission,
    );
    assert.equal(prepared.size, MAX_CONTENT_OBJECT_BYTES);
    const over = new Uint8Array(MAX_CONTENT_OBJECT_BYTES + 1).fill(72);
    const overHash = sha256(over);
    await assert.rejects(
      prepareContentEntriesStreaming(
        port,
        [{ hash: overHash, length: over.length, bytes: over }],
        parameters,
        over.length,
        storage,
        DEFAULT_RUNTIME_LIMITS,
        admission,
      ),
      /invalid staged manifest entry|object exceeds configured limit/,
    );
    let producerWork = 0;
    const unwritable = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 128 * 1024 * 1024,
        maintenanceReserveBytes: 1024 * 1024,
        maxFinalTransactionBytes: 1024 * 1024,
      },
      driver.capabilities,
    );
    await assert.rejects(
      prepareContentEntriesStreaming(
        port,
        {
          *[Symbol.iterator]() {
            producerWork += 1;
            yield { hash: exactHash, length: exact.length, bytes: exact };
          },
        },
        parameters,
        exact.length,
        unwritable,
        DEFAULT_RUNTIME_LIMITS,
        admission,
      ),
      /FastCDC maximum exceeds the durable object transaction envelope/,
    );
    assert.equal(producerWork, 0);
    assert.equal(admission.usedBytes, 0);
    await port.close();

    driver = await openNodeSqlite({
      filename,
      create: false,
      durability: "relaxed-test",
    });
    port = createSqliteOperationsStorage(driver);
    port.initialize();
    storage = constrainStorageLimits(
      {
        maxManagedPayloadBytes: 128 * 1024 * 1024,
        maintenanceReserveBytes: 1024 * 1024,
      },
      driver.capabilities,
    );
    const reopened = port.transaction(
      "read",
      { maxRows: 32, maxBytes: storage.maxFinalTransactionBytes },
      (tx) => {
        const output = new Uint8Array(MAX_CONTENT_OBJECT_BYTES);
        assert.equal(
          tx
            .content(
              storage,
              new ContentCache(1, new AdmissionController(64 * 1024 * 1024)),
            )
            .readObjectInto(
              exactHash,
              MAX_CONTENT_OBJECT_BYTES,
              0,
              output,
              0,
              MAX_CONTENT_OBJECT_BYTES,
            ),
          true,
        );
        return output;
      },
    );
    assert.equal(reopened.byteLength, MAX_CONTENT_OBJECT_BYTES);
    assert.deepEqual(sha256(reopened), exactHash);
    const usage = driver.transaction(
      "read",
      (tx) =>
        tx.all("SELECT object_count,object_bytes FROM efs_usage", [], {
          maxRows: 1,
          maxBytes: 128,
        })[0],
    );
    assert.equal(usage.object_count, 1);
    assert.equal(usage.object_bytes, MAX_CONTENT_OBJECT_BYTES);
    await port.close();
    port = undefined;
    driver = undefined;
  } finally {
    try {
      await port?.close();
    } catch {}
    try {
      driver?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("bulk content envelopes reject before hashing or manifest decoding", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 16 * 1024 * 1024,
      maintenanceReserveBytes: 4096,
      maxFinalTransactionBytes: DEFAULT_FASTCDC_MAXIMUM_BYTES + 16 * 1024,
    },
    driver.capabilities,
  );
  class HostileBytes extends Uint8Array {
    get byteLength() {
      return 0;
    }
  }
  const objectBytes = new HostileBytes(16 * 1024);
  const invalidHash = new HostileBytes(32);
  assert.throws(
    () =>
      driver.transaction("write", (tx) =>
        new ContentRepository(tx, storage).putObjectsBatch(
          Array.from({ length: 34 }, () => ({
            hash: invalidHash,
            bytes: objectBytes,
          })),
        ),
      ),
    /transaction byte limit/,
  );
  const invalidNode = new HostileBytes(10 * 1024);
  assert.throws(
    () =>
      driver.transaction("write", (tx) =>
        new ContentRepository(tx, storage).putManifestNodesBatch(
          Array.from({ length: 54 }, () => ({
            hash: invalidHash,
            encoded: invalidNode,
          })),
        ),
      ),
    /transaction byte limit/,
  );
  driver.close();
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
      { maxManagedPayloadBytes: 1024 * 1024, maintenanceReserveBytes: 4096 },
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

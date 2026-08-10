import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifest } from "../../packages/fs/dist/operations/full-rebuild.js";
import { constrainStorageLimits } from "../../packages/fs/dist/resources/limits.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import { EFS_APPLICATION_ID, EFS_SCHEMA_VERSION, initializeOrValidateSchema } from "../../packages/fs/dist/sqlite/schema.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { createV1Schema } from "../fixtures/schema-v1.mjs";

test("schema initialization is deterministic, persisted, and read-only reopen-safe", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-schema-")); const filename = path.join(directory, "filesystem.db");
  try {
    const driver = await openNodeSqlite({ filename }); const created = initializeOrValidateSchema(driver, { cowPageBytes: 4096, now: 1234 });
    const identity = driver.transaction("read", (tx) => ({
      applicationId: tx.all("SELECT application_id value FROM pragma_application_id", [], { maxRows: 1, maxBytes: 1024 })[0].value,
      userVersion: tx.all("SELECT user_version value FROM pragma_user_version", [], { maxRows: 1, maxBytes: 1024 })[0].value,
    }));
    assert.deepEqual(identity, { applicationId: EFS_APPLICATION_ID, userVersion: EFS_SCHEMA_VERSION }); driver.close();
    const readOnly = await openNodeSqlite({ filename, readOnly: true }); const reopened = initializeOrValidateSchema(readOnly, { cowPageBytes: 4096 });
    assert.deepEqual(reopened, created); assert.throws(() => initializeOrValidateSchema(readOnly, { cowPageBytes: 8192 }), /ESCHEMA/); readOnly.close();
  } finally { await rm(directory, { recursive: true, force: true }); }
});

function migrationDriver(base, failAt, count) {
  return { kind: "sqlite", readOnly: base.readOnly, capabilities: base.capabilities, close: () => base.close(), transaction(mode, callback) { return base.transaction(mode, (tx) => {
    if (mode !== "exclusive") return callback(tx);
    const invoke = (fn) => (...args) => { count.value += 1; if (count.value === failAt) throw new Error(`migration fault ${failAt}`); return fn(...args); };
    return callback({ scope: tx.scope, run: invoke(tx.run), all: invoke(tx.all) });
  }); } };
}

test("schema v1 migrates data to the current schema and every migration-statement fault rolls back", async () => {
  const probe = await openNodeSqlite({ filename: ":memory:" }); createV1Schema(probe);
  probe.transaction("write", (tx) => { tx.run("INSERT INTO efs_branch_ids VALUES('b',1)"); tx.run("INSERT INTO efs_branches VALUES('b',0,0,7,1,NULL)"); tx.run("INSERT INTO efs_cow_pages VALUES('b','inode',2,7,?)", [new Uint8Array(4096).fill(3)]); tx.run("INSERT INTO efs_patches VALUES('b','inode',0,9,2,?)", [Uint8Array.of(4, 5, 6)]); });
  const count = { value: 0 }; initializeOrValidateSchema(migrationDriver(probe, Number.POSITIVE_INFINITY, count), { cowPageBytes: 4096 });
  const migrated = probe.transaction("read", (tx) => ({ version: tx.all("SELECT user_version value FROM pragma_user_version", [], { maxRows: 1, maxBytes: 128 })[0].value, page: tx.all("SELECT page_index,generation,length(bytes) size FROM efs_cow_page_versions", [], { maxRows: 1, maxBytes: 128 })[0], patch: tx.all("SELECT offset,delete_length,insert_length FROM efs_patches", [], { maxRows: 1, maxBytes: 128 })[0], segment: tx.all("SELECT segment_index,bytes FROM efs_patch_segments", [], { maxRows: 1, maxBytes: 128 })[0] }));
  assert.equal(migrated.version, EFS_SCHEMA_VERSION); assert.deepEqual(migrated.page, { page_index: 2, generation: 7, size: 4096 }); assert.deepEqual(migrated.patch, { offset: 9, delete_length: 2, insert_length: 3 }); assert.equal(migrated.segment.segment_index, 0); assert.deepEqual(migrated.segment.bytes, Uint8Array.of(4, 5, 6)); probe.close();
  assert.ok(count.value > 20);
  for (let failAt = 1; failAt <= count.value; failAt += 1) {
    const base = await openNodeSqlite({ filename: ":memory:" }); createV1Schema(base); const faultCount = { value: 0 };
    assert.throws(() => initializeOrValidateSchema(migrationDriver(base, failAt, faultCount)), new RegExp(`migration fault ${failAt}`));
    const state = base.transaction("read", (tx) => ({ version: tx.all("SELECT user_version value FROM pragma_user_version", [], { maxRows: 1, maxBytes: 128 })[0].value, meta: tx.all("SELECT schema_version FROM efs_meta", [], { maxRows: 1, maxBytes: 128 })[0].schema_version, oldPages: tx.all("SELECT count(*) count FROM sqlite_schema WHERE name='efs_cow_pages'", [], { maxRows: 1, maxBytes: 128 })[0].count, newPages: tx.all("SELECT count(*) count FROM sqlite_schema WHERE name='efs_cow_page_versions'", [], { maxRows: 1, maxBytes: 128 })[0].count }));
    assert.deepEqual(state, { version: 1, meta: 1, oldPages: 1, newPages: 0 }); base.close();
  }
});

test("CAS and segmented manifests persist with verified deduplication and exact usage", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" }); initializeOrValidateSchema(driver);
  const limits = constrainStorageLimits({ maxManagedPayloadBytes: 256 * 1024 * 1024, maintenanceReserveBytes: 1024 }, driver.capabilities);
  const bytes = Uint8Array.from({ length: 2 * 1024 * 1024 }, (_, index) => (index * 131 + index ** 2) & 0xff);
  const manifest = buildManifest(bytes, { minimum: 32_768, average: 131_072, maximum: 524_288 });
  driver.transaction("write", (tx) => {
    const repo = new ContentRepository(tx, limits);
    for (const [hash, object] of manifest.objects) assert.equal(repo.putObject(Buffer.from(hash, "hex"), object), true);
    for (const node of manifest.nodes.values()) assert.equal(repo.putManifestNode(node.hash, node.encoded), true);
    assert.equal(repo.putManifestRoot(manifest.rootHash, manifest.root), true);
  });
  driver.transaction("write", (tx) => {
    const repo = new ContentRepository(tx, limits);
    for (const [hash, object] of manifest.objects) assert.equal(repo.putObject(Buffer.from(hash, "hex"), object), false);
    assert.deepEqual(repo.getManifestRoot(manifest.rootHash), manifest.root);
  });
  const usage = driver.transaction("read", (tx) => tx.all("SELECT object_count,object_bytes,manifest_root_count,manifest_node_count FROM efs_usage", [], { maxRows: 1, maxBytes: 1024 })[0]);
  const uniqueObjectBytes = [...manifest.objects.values()].reduce((sum, value) => sum + value.byteLength, 0);
  assert.equal(usage.object_count, manifest.objects.size); assert.equal(usage.object_bytes, uniqueObjectBytes); assert.equal(usage.manifest_root_count, 1); assert.equal(usage.manifest_node_count, manifest.nodes.size);
  const firstHash = Buffer.from(manifest.objects.keys().next().value, "hex");
  driver.transaction("write", (tx) => tx.run("UPDATE efs_cas_objects SET bytes=zeroblob(size) WHERE hash=?", [firstHash]));
  assert.throws(() => driver.transaction("read", (tx) => new ContentRepository(tx, limits).getObject(firstHash)), /digest mismatch/);
  driver.close();
});

function failingDriver(base, occurrence) {
  return {
    kind: "sqlite", readOnly: base.readOnly, capabilities: base.capabilities, close: () => base.close(),
    transaction(mode, callback) {
      return base.transaction(mode, (tx) => {
        let statements = 0; const invoke = (fn) => (...args) => { statements += 1; if (statements === occurrence) throw new Error(`fault after statement ${occurrence}`); return fn(...args); };
        return callback({ scope: tx.scope, run: invoke(tx.run), all: invoke(tx.all) });
      });
    },
  };
}

test("failure at every content write statement leaves the complete old state", async () => {
  const bytes = new TextEncoder().encode("atomic-content"); const hash = sha256(bytes);
  for (let occurrence = 1; occurrence <= 6; occurrence += 1) {
    const base = await openNodeSqlite({ filename: ":memory:" }); initializeOrValidateSchema(base);
    const driver = failingDriver(base, occurrence); const limits = constrainStorageLimits({ maxManagedPayloadBytes: 1024 * 1024, maintenanceReserveBytes: 1024 }, driver.capabilities);
    assert.throws(() => driver.transaction("write", (tx) => new ContentRepository(tx, limits).putObject(hash, bytes)), /fault after statement/);
    const state = base.transaction("read", (tx) => tx.all("SELECT (SELECT count(*) FROM efs_cas_objects) objects,object_count,object_bytes FROM efs_usage", [], { maxRows: 1, maxBytes: 1024 })[0]);
    assert.deepEqual(state, { objects: 0, object_count: 0, object_bytes: 0 }); base.close();
  }
});

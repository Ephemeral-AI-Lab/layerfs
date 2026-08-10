import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifest } from "../../packages/fs/dist/manifests/builder.js";
import { constrainStorageLimits } from "../../packages/fs/dist/resources/limits.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import { EFS_APPLICATION_ID, initializeOrValidateSchema } from "../../packages/fs/dist/sqlite/schema.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

test("schema initialization is deterministic, persisted, and read-only reopen-safe", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-schema-")); const filename = path.join(directory, "filesystem.db");
  try {
    const driver = await openNodeSqlite({ filename }); const created = initializeOrValidateSchema(driver, { cowPageBytes: 4096, now: 1234 });
    const identity = driver.transaction("read", (tx) => ({
      applicationId: tx.all("SELECT application_id value FROM pragma_application_id", [], { maxRows: 1, maxBytes: 1024 })[0].value,
      userVersion: tx.all("SELECT user_version value FROM pragma_user_version", [], { maxRows: 1, maxBytes: 1024 })[0].value,
    }));
    assert.deepEqual(identity, { applicationId: EFS_APPLICATION_ID, userVersion: 1 }); driver.close();
    const readOnly = await openNodeSqlite({ filename, readOnly: true }); const reopened = initializeOrValidateSchema(readOnly, { cowPageBytes: 4096 });
    assert.deepEqual(reopened, created); assert.throws(() => initializeOrValidateSchema(readOnly, { cowPageBytes: 8192 }), /ESCHEMA/); readOnly.close();
  } finally { await rm(directory, { recursive: true, force: true }); }
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

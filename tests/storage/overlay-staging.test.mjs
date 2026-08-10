import assert from "node:assert/strict";
import { test } from "node:test";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import { buildManifest } from "../../packages/fs/dist/manifests/builder.js";
import { AdmissionController, DEFAULT_RUNTIME_LIMITS, constrainStorageLimits } from "../../packages/fs/dist/resources/limits.js";
import { ContentCache } from "../../packages/fs/dist/resources/content-cache.js";
import { prepareContent } from "../../packages/fs/dist/operations/manifest-io.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import { OverlayRepository } from "../../packages/fs/dist/sqlite/overlay-repository.js";
import { initializeOrValidateSchema } from "../../packages/fs/dist/sqlite/schema.js";
import { StagingRepository } from "../../packages/fs/dist/sqlite/staging-repository.js";
import { runUnitOfWork } from "../../packages/fs/dist/sqlite/unit-of-work.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

function limits(driver) { return constrainStorageLimits({ maxManagedPayloadBytes: 256 * 1024 * 1024, maintenanceReserveBytes: 1024 * 1024, maxBranchOverlayBytes: 32 * 1024 * 1024 }, driver.capabilities); }
function createBranch(driver, id = "branch") { driver.transaction("write", (tx) => { tx.run("INSERT INTO efs_branch_ids(id,created_at_ms) VALUES(?,0)", [id]); tx.run("INSERT INTO efs_branches(id,base_revision,state,generation,created_at_ms,terminal_at_ms) VALUES(?,0,0,0,0,NULL)", [id]); }); }

test("immutable COW heads retain one current page and atomically cross boundaries at every page size", async () => {
  for (const pageBytes of [4096, 8192, 16384]) {
    const driver = await openNodeSqlite({ filename: ":memory:" }); initializeOrValidateSchema(driver, { cowPageBytes: pageBytes }); createBranch(driver); const storage = limits(driver);
    for (let iteration = 0; iteration < 1000; iteration += 1) driver.transaction("write", (tx) => new OverlayRepository(tx, storage, pageBytes).writePages("branch", "inode", pageBytes * 2, [{ index: 0, bytes: new Uint8Array(pageBytes).fill(iteration & 0xff) }], iteration));
    let state = driver.transaction("read", (tx) => ({ versions: tx.all("SELECT count(*) count FROM efs_cow_page_versions", [], { maxRows: 1, maxBytes: 100 })[0].count, heads: tx.all("SELECT count(*) count FROM efs_cow_page_heads", [], { maxRows: 1, maxBytes: 100 })[0].count, usage: tx.all("SELECT page_count,page_bytes FROM efs_usage", [], { maxRows: 1, maxBytes: 100 })[0] }));
    assert.equal(state.versions, 1); assert.equal(state.heads, 1); assert.deepEqual(state.usage, { page_count: 1, page_bytes: pageBytes });
    driver.transaction("write", (tx) => new OverlayRepository(tx, storage, pageBytes).writePages("branch", "crossing", pageBytes + 17, [{ index: 0, bytes: new Uint8Array(pageBytes).fill(1) }, { index: 1, bytes: new Uint8Array(17).fill(2) }], 1001));
    state = driver.transaction("read", (tx) => ({ versions: tx.all("SELECT count(*) count FROM efs_cow_page_versions WHERE inode_id='crossing'", [], { maxRows: 1, maxBytes: 100 })[0].count, heads: tx.all("SELECT count(*) count FROM efs_cow_page_heads WHERE inode_id='crossing'", [], { maxRows: 1, maxBytes: 100 })[0].count }));
    assert.deepEqual(state, { versions: 2, heads: 2 });
    assert.throws(() => driver.transaction("write", (tx) => new OverlayRepository(tx, storage, pageBytes).writePages("branch", "bad", pageBytes + 1, [{ index: 1, bytes: new Uint8Array(pageBytes) }], 1002)), /exact logical length/);
    driver.close();
  }
});

test("structural patches are segmented, ordered, bounded, and exact", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" }); const metadata = initializeOrValidateSchema(driver); createBranch(driver); const storage = limits(driver);
  driver.transaction("write", (tx) => new OverlayRepository(tx, storage, metadata.cowPageBytes).appendPatch("branch", "inode", 10, 4, 2, [Uint8Array.of(1, 2), Uint8Array.of(3)]));
  driver.transaction("write", (tx) => new OverlayRepository(tx, storage, metadata.cowPageBytes).appendPatch("branch", "inode", 11, 11, 0, []));
  const patches = driver.transaction("read", (tx) => new OverlayRepository(tx, storage, metadata.cowPageBytes).patches("branch", "inode"));
  assert.deepEqual(patches.map((patch) => ({ sequence: patch.sequence, offset: patch.offset, deleteLength: patch.deleteLength, insertLength: patch.insertLength, segments: patch.segments.map((value) => [...value]) })), [{ sequence: 0, offset: 4, deleteLength: 2, insertLength: 3, segments: [[1, 2], [3]] }, { sequence: 1, offset: 11, deleteLength: 0, insertLength: 0, segments: [] }]);
  assert.throws(() => driver.transaction("write", (tx) => new OverlayRepository(tx, storage, metadata.cowPageBytes).appendPatch("branch", "inode", 11, 12, 0, [])), /outside/); driver.close();
});

test("byte-weighted cache verifies once, remains bounded, and eviction preserves integrity checks", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" }); initializeOrValidateSchema(driver); const storage = limits(driver); const admission = new AdmissionController(1024 * 1024); const cache = new ContentCache(128 * 1024, admission);
  const bytes = Uint8Array.from({ length: 32 * 1024 }, (_, index) => index & 0xff); const hash = sha256(bytes);
  driver.transaction("write", (tx) => assert.equal(new ContentRepository(tx, storage, cache).putObject(hash, bytes), true));
  assert.deepEqual(driver.transaction("read", (tx) => new ContentRepository(tx, storage, cache).getObject(hash, bytes.length)), bytes);
  assert.deepEqual(driver.transaction("read", (tx) => new ContentRepository(tx, storage, cache).getObject(hash, bytes.length)), bytes);
  assert.ok(cache.metrics().hits >= 1); assert.ok(cache.metrics().bytes <= 128 * 1024); cache.clear(); assert.equal(admission.usedBytes, 0);
  driver.transaction("write", (tx) => tx.run("UPDATE efs_cas_objects SET bytes=zeroblob(size) WHERE hash=?", [hash]));
  assert.throws(() => driver.transaction("read", (tx) => new ContentRepository(tx, storage, cache).getObject(hash, bytes.length)), /digest mismatch/); cache.clear(); driver.close();
});

test("partial write-admission failure removes its staging lease and releases every reservation", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" }); initializeOrValidateSchema(driver); const storage = limits(driver); const admission = new AdmissionController(DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes);
  const releasePressure = admission.reserve(120 * 1024 * 1024);
  await assert.rejects(prepareContent(driver, Uint8Array.of(1), storage, DEFAULT_RUNTIME_LIMITS, admission), /managed resident memory limit/);
  releasePressure(); assert.equal(admission.usedBytes, 0);
  const active = driver.transaction("read", (tx) => tx.all("SELECT count(*) count FROM efs_leases", [], { maxRows: 1, maxBytes: 128 })[0].count);
  assert.equal(active, 0); driver.close();
});

test("a 100001-object staging closure seals and final-validates with constant-row work", { timeout: 120_000 }, async () => {
  const driver = await openNodeSqlite({ filename: ":memory:", durability: "relaxed-test" }); initializeOrValidateSchema(driver); const storage = limits(driver); const leaseId = "large-stage"; const nonce = Uint8Array.from({ length: 16 }, (_, index) => index + 1); const budget = { maxRows: storage.maxFinalTransactionRows, maxBytes: storage.maxFinalTransactionBytes };
  runUnitOfWork(driver, "write", budget, (tx) => new StagingRepository(tx).begin({ leaseId, ownerId: "test", ownerNonce: nonce, now: 1, expiresAt: 1_000_000 }));
  const total = 100_001; const batchSize = 128;
  for (let start = 0; start < total; start += batchSize) {
    const batch = []; for (let index = start; index < Math.min(total, start + batchSize); index += 1) { const bytes = new Uint8Array(8); new DataView(bytes.buffer).setBigUint64(0, BigInt(index), true); batch.push({ hash: sha256(bytes), bytes }); }
    runUnitOfWork(driver, "write", budget, (tx) => { new ContentRepository(tx, storage).putObjectsBatch(batch); new StagingRepository(tx).appendBatch(leaseId, nonce, batch.map((item) => ({ kind: "object", hash: item.hash, size: item.bytes.length }))); });
  }
  const empty = buildManifest(new Uint8Array(), { minimum: 32768, average: 131072, maximum: 524288 });
  const certificate = runUnitOfWork(driver, "write", budget, (tx) => { const content = new ContentRepository(tx, storage); for (const node of empty.nodes.values()) content.putManifestNode(node.hash, node.encoded); content.putManifestRoot(empty.rootHash, empty.root); const staging = new StagingRepository(tx); staging.appendBatch(leaseId, nonce, [...empty.nodes.values()].map((node) => ({ kind: "manifest-node", hash: node.hash, size: node.encoded.length }))); staging.appendBatch(leaseId, nonce, [{ kind: "manifest-root", hash: empty.rootHash, size: empty.root.length }]); const snapshot = staging.snapshot(leaseId, nonce); const value = { ...snapshot, manifestHash: empty.rootHash }; staging.seal(value); return value; });
  let statements = 0;
  const counted = { ...driver, transaction(mode, callback) { return driver.transaction(mode, (tx) => callback({ scope: tx.scope, run(...args) { statements += 1; return tx.run(...args); }, all(...args) { statements += 1; return tx.all(...args); } })); } };
  runUnitOfWork(counted, "read", budget, (tx) => new StagingRepository(tx).validateSealed(certificate, 2));
  assert.equal(certificate.objectCount, total); assert.equal(certificate.membershipCount, total + empty.nodes.size + 1); assert.equal(statements, 1); driver.close();
});

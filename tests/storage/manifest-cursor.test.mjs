import assert from "node:assert/strict";
import { test } from "node:test";
import { sha256 } from "../../packages/fs/dist/cas/sha256.js";
import {
  encodeManifestNode,
  encodeManifestRoot,
} from "../../packages/fs/dist/manifests/codec.js";
import { buildManifest } from "../../packages/fs/dist/operations/full-rebuild.js";
import {
  readManifestInto,
  readManifestRange as readManifestRangeUnadmitted,
} from "../../packages/fs/dist/operations/manifest-io.js";
import {
  AdmissionController,
  constrainStorageLimits,
} from "../../packages/fs/dist/resources/limits.js";
import { ContentRepository } from "../../packages/fs/dist/sqlite/content-repository.js";
import { initializeOrValidateSchema } from "../../packages/fs/dist/sqlite/schema.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

function limits(driver, overrides = {}) {
  return constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024,
      ...overrides,
    },
    driver.capabilities,
  );
}

function readManifestRange(repository, hash, offset, length) {
  return readManifestRangeUnadmitted(
    repository,
    hash,
    offset,
    length,
    new AdmissionController(256 * 1024 * 1024),
  );
}

function persistBuilt(tx, storage, manifest) {
  const content = new ContentRepository(tx, storage);
  for (const [hash, bytes] of manifest.objects)
    content.putObject(Buffer.from(hash, "hex"), bytes);
  for (const node of manifest.nodes.values())
    content.putManifestNode(node.hash, node.encoded);
  content.putManifestRoot(manifest.rootHash, manifest.root);
}

function persistManual(tx, storage, object, nodes, root) {
  const content = new ContentRepository(tx, storage);
  content.putObject(object.hash, object.bytes);
  for (const node of nodes) content.putManifestNode(node.hash, node.encoded);
  content.putManifestRoot(root.hash, root.encoded);
}

function node(value) {
  const encoded = encodeManifestNode(value);
  return { hash: sha256(encoded), encoded };
}

function root(parameters, fileSize, entryCount, rootNodeHash) {
  const encoded = encodeManifestRoot({
    parameters,
    fileSize,
    entryCount,
    rootNodeHash,
  });
  return { hash: sha256(encoded), encoded };
}

test("SQLite manifest cursor returns bounded ranges through authenticated M1 paths", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const storage = limits(driver);
  const bytes = Uint8Array.from(
    { length: 2 * 1024 * 1024 },
    (_, index) => (index * 17 + index ** 2) & 0xff,
  );
  const manifest = buildManifest(bytes, {
    minimum: 32_768,
    average: 131_072,
    maximum: 524_288,
  });
  driver.transaction("write", (tx) => persistBuilt(tx, storage, manifest));
  const offset = 510_123;
  const length = 300_007;
  const actual = driver.transaction("read", (tx) =>
    readManifestRange(
      new ContentRepository(tx, storage),
      manifest.rootHash,
      offset,
      length,
    ),
  );
  assert.deepEqual(actual, bytes.slice(offset, offset + length));

  class HostileDestination extends Uint8Array {
    set() {
      throw new Error("subclass set must not be called");
    }
    get byteLength() {
      throw new Error("subclass byteLength must not be read");
    }
  }
  const destination = new HostileDestination(length + 8).fill(0xaa);
  const written = driver.transaction("read", (tx) =>
    readManifestInto(
      new ContentRepository(tx, storage),
      manifest.rootHash,
      offset,
      destination,
      4,
      length,
    ),
  );
  assert.equal(written, length);
  assert.deepEqual(
    new Uint8Array(destination.buffer, 4, length),
    bytes.slice(offset, offset + length),
  );
  driver.close();
});

test("cursor rejects unsupported parameters and root totals before exposing bytes", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const storage = limits(driver);
  const bytes = Uint8Array.of(9);
  const hash = sha256(bytes);
  const leaf = node({
    kind: "leaf",
    span: 1,
    entryCount: 1,
    entries: [{ hash, length: 1 }],
  });
  const unsupported = root(
    { minimum: 1, average: 1, maximum: 32 * 1024 * 1024 },
    1,
    1,
    leaf.hash,
  );
  driver.transaction("write", (tx) =>
    persistManual(tx, storage, { hash, bytes }, [leaf], unsupported),
  );
  assert.throws(
    () =>
      driver.transaction("read", (tx) =>
        readManifestRange(new ContentRepository(tx, storage), unsupported.hash, 0, 1),
      ),
    /effective content-object limit/,
  );
  const wrongTotals = root({ minimum: 1, average: 1, maximum: 1 }, 2, 1, leaf.hash);
  driver.transaction("write", (tx) =>
    new ContentRepository(tx, storage).putManifestRoot(
      wrongTotals.hash,
      wrongTotals.encoded,
    ),
  );
  assert.throws(
    () =>
      driver.transaction("read", (tx) =>
        readManifestRange(new ContentRepository(tx, storage), wrongTotals.hash, 0, 1),
      ),
    /root totals mismatch/,
  );
  driver.close();
});

test("cursor validates child totals, canonical grouping, and configured depth", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const storage = limits(driver);
  const bytes = Uint8Array.of(3);
  const hash = sha256(bytes);
  const fullLeaf = node({
    kind: "leaf",
    span: 256,
    entryCount: 256,
    entries: Array.from({ length: 256 }, () => ({ hash, length: 1 })),
  });
  const finalLeaf = node({
    kind: "leaf",
    span: 1,
    entryCount: 1,
    entries: [{ hash, length: 1 }],
  });
  const mismatchedInternal = node({
    kind: "internal",
    span: 256,
    entryCount: 257,
    children: [
      { hash: fullLeaf.hash, span: 255, entryCount: 256 },
      { hash: finalLeaf.hash, span: 1, entryCount: 1 },
    ],
  });
  const mismatchedRoot = root(
    { minimum: 1, average: 1, maximum: 1 },
    256,
    257,
    mismatchedInternal.hash,
  );
  driver.transaction("write", (tx) =>
    persistManual(
      tx,
      storage,
      { hash, bytes },
      [fullLeaf, finalLeaf, mismatchedInternal],
      mismatchedRoot,
    ),
  );
  assert.throws(
    () =>
      driver.transaction("read", (tx) =>
        readManifestRange(
          new ContentRepository(tx, storage),
          mismatchedRoot.hash,
          0,
          1,
        ),
      ),
    /child totals mismatch/,
  );

  const shortLeaf = finalLeaf;
  const noncanonicalInternal = node({
    kind: "internal",
    span: 2,
    entryCount: 2,
    children: [
      { hash: shortLeaf.hash, span: 1, entryCount: 1 },
      { hash: finalLeaf.hash, span: 1, entryCount: 1 },
    ],
  });
  const noncanonicalRoot = root(
    { minimum: 1, average: 1, maximum: 1 },
    2,
    2,
    noncanonicalInternal.hash,
  );
  driver.transaction("write", (tx) => {
    const content = new ContentRepository(tx, storage);
    content.putManifestNode(noncanonicalInternal.hash, noncanonicalInternal.encoded);
    content.putManifestRoot(noncanonicalRoot.hash, noncanonicalRoot.encoded);
  });
  assert.throws(
    () =>
      driver.transaction("read", (tx) =>
        readManifestRange(
          new ContentRepository(tx, storage),
          noncanonicalRoot.hash,
          0,
          1,
        ),
      ),
    /canonical boundary/,
  );

  const validInternal = node({
    kind: "internal",
    span: 257,
    entryCount: 257,
    children: [
      { hash: fullLeaf.hash, span: 256, entryCount: 256 },
      { hash: finalLeaf.hash, span: 1, entryCount: 1 },
    ],
  });
  const deepRoot = root(
    { minimum: 1, average: 1, maximum: 1 },
    257,
    257,
    validInternal.hash,
  );
  driver.transaction("write", (tx) => {
    const content = new ContentRepository(tx, storage);
    content.putManifestNode(validInternal.hash, validInternal.encoded);
    content.putManifestRoot(deepRoot.hash, deepRoot.encoded);
  });
  const shallow = limits(driver, { maxManifestDepth: 1 });
  assert.throws(
    () =>
      driver.transaction("read", (tx) =>
        readManifestRange(new ContentRepository(tx, shallow), deepRoot.hash, 0, 1),
      ),
    /depth exceeds/,
  );
  driver.close();
});

test("CAS corruption is rejected before destination bytes are changed", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  initializeOrValidateSchema(driver);
  const storage = limits(driver);
  const bytes = Uint8Array.of(1, 2, 3, 4);
  const hash = sha256(bytes);
  const leaf = node({
    kind: "leaf",
    span: 4,
    entryCount: 1,
    entries: [{ hash, length: 4 }],
  });
  const manifest = root({ minimum: 4, average: 4, maximum: 4 }, 4, 1, leaf.hash);
  driver.transaction("write", (tx) =>
    persistManual(tx, storage, { hash, bytes }, [leaf], manifest),
  );
  driver.transaction("write", (tx) =>
    tx.run("UPDATE efs_cas_objects SET bytes=? WHERE hash=?", [
      Uint8Array.of(4, 3, 2, 1),
      hash,
    ]),
  );
  const destination = new Uint8Array(4).fill(0xaa);
  assert.throws(
    () =>
      driver.transaction("read", (tx) =>
        readManifestInto(
          new ContentRepository(tx, storage),
          manifest.hash,
          0,
          destination,
          0,
          4,
        ),
      ),
    /digest mismatch/,
  );
  assert.deepEqual(destination, new Uint8Array(4).fill(0xaa));
  driver.close();
});

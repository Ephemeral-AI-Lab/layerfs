import assert from "node:assert/strict";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { DEFAULT_FASTCDC } from "../../packages/fs/dist/cdc/fastcdc.js";
import { MAX_DIAGNOSTIC_CONTENT_BYTES } from "../../packages/fs/dist/operations/full-rebuild.js";
import { prepareDurableEditedContent } from "../../packages/fs/dist/operations/durable-edit-prepare.js";
import {
  prepareContent,
  readManifestRange,
} from "../../packages/fs/dist/operations/manifest-io.js";
import {
  AdmissionController,
  DEFAULT_RUNTIME_LIMITS,
  constrainStorageLimits,
} from "../../packages/fs/dist/resources/limits.js";
import { createSqliteOperationsStorage } from "../../packages/fs/dist/sqlite/operations-storage.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

function bytePattern(length) {
  const output = new Uint8Array(length);
  let state = 0x6d2b79f5;
  for (let index = 0; index < length; index += 1) {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    output[index] = state & 0xff;
  }
  return output;
}

function insertion(bytes) {
  return Object.freeze({
    offset: 9 * 1024 * 1024 + 37,
    deleteLength: 9,
    insertLength: bytes.byteLength,
    readInsert(offset, length) {
      return bytes.slice(offset, offset + length);
    },
  });
}

test("durable large-manifest edits path-copy authenticated entries and stream on stale boundaries", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const port = createSqliteOperationsStorage(driver);
  const storage = constrainStorageLimits(
    {
      maxManagedPayloadBytes: 256 * 1024 * 1024,
      maintenanceReserveBytes: 1024 * 1024,
    },
    driver.capabilities,
  );
  port.initialize({ now: 1000 });
  const admission = new AdmissionController(
    DEFAULT_RUNTIME_LIMITS.maxManagedResidentBytes,
  );
  const original = bytePattern(MAX_DIAGNOSTIC_CONTENT_BYTES + 4 * 1024 * 1024);
  const old = await prepareContent(
    port,
    original,
    storage,
    DEFAULT_RUNTIME_LIMITS,
    admission,
    undefined,
    undefined,
    () => 1001,
  );
  let sourceBytesRead = 0;
  const source = Object.freeze({
    size: old.size,
    parameters: DEFAULT_FASTCDC,
    read(offset, length) {
      sourceBytesRead += length;
      return port.transaction(
        "read",
        { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 },
        (tx) => readManifestRange(tx.content(storage), old.hash, offset, length),
      );
    },
    entries(offset, limit) {
      return port.transaction(
        "read",
        { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 },
        (tx) => {
          const cursor = tx.content(storage).openManifestCursor(old.hash, offset);
          const rows = [];
          for (let index = 0; index < limit; index += 1) {
            const row = cursor.nextEntry();
            if (!row) break;
            rows.push(row);
          }
          return Object.freeze(rows);
        },
      );
    },
  });
  const inserted = Uint8Array.of(201, 202, 203, 204, 205);
  const edit = insertion(inserted);
  const prepared = await prepareDurableEditedContent(
    port,
    source,
    edit,
    storage,
    DEFAULT_RUNTIME_LIMITS,
    admission,
    undefined,
    () => 1002,
  );
  assert.equal(prepared.mode, "durable-path-copy");
  assert.ok(
    sourceBytesRead <= DEFAULT_FASTCDC.maximum * 3,
    `path-copy read ${sourceBytesRead} source bytes`,
  );
  assert.equal(prepared.size, original.byteLength - 9 + inserted.byteLength);
  const actual = port.transaction(
    "read",
    { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 },
    (tx) =>
      readManifestRange(
        tx.content(storage),
        prepared.hash,
        edit.offset - 16,
        16 + inserted.byteLength + 16,
      ),
  );
  const expected = new Uint8Array(16 + inserted.byteLength + 16);
  expected.set(original.slice(edit.offset - 16, edit.offset), 0);
  expected.set(inserted, 16);
  expected.set(original.slice(edit.offset + edit.deleteLength, edit.offset + 25), 21);
  assert.deepEqual(actual, expected);

  let staleInjected = false;
  const staleSource = Object.freeze({
    ...source,
    entries(offset, limit) {
      const rows = source.entries(offset, limit);
      if (!staleInjected && offset === 0 && rows.length) {
        staleInjected = true;
        return Object.freeze([
          Object.freeze({ ...rows[0], offset: rows[0].offset + 1 }),
          ...rows.slice(1),
        ]);
      }
      return rows;
    },
  });
  const fallback = await prepareDurableEditedContent(
    port,
    staleSource,
    edit,
    storage,
    DEFAULT_RUNTIME_LIMITS,
    admission,
    undefined,
    () => 1003,
  );
  assert.equal(staleInjected, true);
  assert.equal(fallback.mode, "streamed-fallback");
  assert.match(fallback.pathCopyReason, /stale derived boundary/);
  assert.deepEqual(
    port.transaction("read", { maxRows: 10_000, maxBytes: 4 * 1024 * 1024 }, (tx) =>
      readManifestRange(
        tx.content(storage),
        fallback.hash,
        edit.offset - 16,
        expected.byteLength,
      ),
    ),
    expected,
  );
  assert.equal(admission.usedBytes, 0);
  await port.close();
});

test("filesystem range mutations and streamed preparation own hostile byte views", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  const filesystem = await EphemeralFS.open({ database: driver });
  class HostileBytes extends Uint8Array {
    get byteLength() {
      throw new Error("subclass byteLength must not be observed");
    }
    slice() {
      throw new Error("subclass slice must not be called");
    }
    subarray() {
      throw new Error("subclass subarray must not be called");
    }
  }
  try {
    const initial = new HostileBytes([97, 98, 99, 100, 101, 102]);
    await filesystem.writeFile("/data", initial);
    initial.fill(0);
    await filesystem.writeRange("/data", 8, new HostileBytes([88]));
    assert.deepEqual(
      await filesystem.readFile("/data"),
      Uint8Array.of(97, 98, 99, 100, 101, 102, 0, 0, 88),
    );
    await filesystem.replaceRange("/data", 2, 4, new HostileBytes([49, 50, 51]));
    assert.deepEqual(
      await filesystem.readFile("/data"),
      Uint8Array.of(97, 98, 49, 50, 51, 0, 0, 88),
    );
    await filesystem.truncate("/data", 3);
    assert.deepEqual(await filesystem.readFile("/data"), Uint8Array.of(97, 98, 49));
    await filesystem.truncate("/data", 1024 * 1024);
    assert.deepEqual(
      await filesystem.readRange("/data", { offset: 1024 * 1024 - 4, length: 4 }),
      new Uint8Array(4),
    );

    const streamParts = [
      new HostileBytes([1, 2]),
      new HostileBytes([3]),
      new HostileBytes([4, 5, 6]),
    ];
    await filesystem.writeFile(
      "/stream",
      new ReadableStream({
        pull(controller) {
          const part = streamParts.shift();
          if (part) controller.enqueue(part);
          else controller.close();
        },
      }),
    );
    assert.deepEqual(
      await filesystem.readFile("/stream"),
      Uint8Array.of(1, 2, 3, 4, 5, 6),
    );
  } finally {
    await filesystem.close();
    driver.close();
  }
});

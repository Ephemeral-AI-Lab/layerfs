import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { openNodeVfs } from "../../packages/node-vfs/dist/index.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

test("hidden staging does not advance visible state and direct reads fill caller buffers", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const handle = await openNodeVfs({ database });
  const provider = handle.provider;
  provider.mkdirSync("/workspace", { recursive: true });
  const session = provider.openFileSync("/workspace/file", {
    writable: true,
    create: true,
  });
  session.writeSync(new TextEncoder().encode("hello world"), 0);
  assert.equal(new TextDecoder().decode(session.readRangeSync(0, 11)), "hello world");
  session.stagePrefixSync();
  assert.equal(provider.existsSync("/workspace/file"), false);
  session.commitVisibleSync();
  const destination = new Uint8Array(32).fill(0xff);
  assert.equal(session.readIntoSync(destination, 5, 6, 5), 5);
  assert.equal(new TextDecoder().decode(destination.subarray(5, 10)), "world");
  assert.equal(
    new TextDecoder().decode(provider.readRangeSync("/workspace/file", 0, 11)),
    "hello world",
  );
  session.closeSync();
  const metrics = provider.metrics.snapshot();
  assert.equal(metrics.openSessions, 0);
  assert.equal(metrics.dirtySessions, 0);
  assert.equal(metrics.residentWriteBytes, 0);
  assert.ok(metrics.directReadBytes >= 16);
  await handle.close();
  database.close();
});

test("three sessions on one inode preserve every commit order without lost updates", async () => {
  const orders = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
  ];
  for (const order of orders) {
    const database = await openNodeSqlite({ filename: ":memory:" });
    const handle = await openNodeVfs({ database });
    const provider = handle.provider;
    let session = provider.openFileSync("/file", { writable: true, create: true });
    session.writeSync(new TextEncoder().encode("000"), 0);
    session.commitVisibleSync();
    session.closeSync();
    const sessions = [0, 1, 2].map(() =>
      provider.openFileSync("/file", { writable: true }),
    );
    sessions[0].writeSync(Uint8Array.of(65), 0);
    sessions[1].writeSync(Uint8Array.of(66), 1);
    sessions[2].writeSync(Uint8Array.of(67), 2);
    for (const index of order) sessions[index].commitVisibleSync();
    assert.equal(
      new TextDecoder().decode(provider.readRangeSync("/file", 0, 3)),
      "ABC",
    );
    for (const value of sessions) value.closeSync();
    await handle.close();
    database.close();
  }
});

test("shared backpressure bounds 64 sessions and rejects excess bytes", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const handle = await openNodeVfs({
    database,
    runtime: {
      maxManagedResidentBytes: 1024 * 1024,
      maxPendingWriteBytes: 32 * 1024,
      maxWriteSessionBytes: 1024,
      maxOpenNodeVfsSessions: 64,
    },
  });
  const sessions = [];
  for (let index = 0; index < 64; index += 1) {
    const session = handle.provider.openFileSync(`/file-${index}`, {
      writable: true,
      create: true,
    });
    session.writeSync(new Uint8Array(512), 0);
    sessions.push(session);
  }
  assert.throws(
    () => sessions[0].writeSync(Uint8Array.of(1), 512),
    (error) => error.code === "EAGAIN",
  );
  const peak = handle.provider.metrics.snapshot().peakManagedResidentBytes;
  assert.ok(peak <= handle.provider.capabilities.runtime.maxManagedResidentBytes);
  for (const session of sessions) session.abortSync();
  assert.equal(handle.provider.metrics.snapshot().residentWriteBytes, 0);
  await handle.close();
  database.close();
});

test("flush, close, physical restart, and remount preserve the digest", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-vfs-"));
  const filename = path.join(directory, "filesystem.db");
  const expected = Uint8Array.from(
    { length: 1024 * 1024 },
    (_, index) => (index * 17) & 0xff,
  );
  try {
    let database = await openNodeSqlite({ filename });
    let handle = await openNodeVfs({ database });
    const session = handle.provider.openFileSync("/large", {
      writable: true,
      create: true,
    });
    for (let offset = 0; offset < expected.length; offset += 64 * 1024)
      session.writeSync(expected.subarray(offset, offset + 64 * 1024), offset);
    session.flushSync({ dataOnly: true });
    session.closeSync();
    await handle.close();
    database.close();
    database = await openNodeSqlite({ filename });
    handle = await openNodeVfs({ database });
    const actual = handle.provider.readRangeSync("/large", 0, expected.length);
    assert.deepEqual(actual, expected);
    await handle.close();
    database.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

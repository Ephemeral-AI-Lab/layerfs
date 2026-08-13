import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { openNodeVfs } from "../../packages/node-vfs/dist/index.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import {
  createStatementFaultController,
  runNodeVfsConformance,
} from "../../packages/testkit/dist/index.js";

test("shared Node VFS conformance", async () => {
  const cases = await runNodeVfsConformance({
    async create(options = {}) {
      const directory = await mkdtemp(path.join(tmpdir(), "efs-vfs-conformance-"));
      const database = await openNodeSqlite({
        filename: path.join(directory, "fs.db"),
      });
      if (options.cowPageBytes !== undefined) {
        const initialized = await EphemeralFS.open({
          database,
          format: { cowPageBytes: options.cowPageBytes },
        });
        await initialized.close();
      }
      const handle = await openNodeVfs({
        database,
        ...(options.runtime === undefined ? {} : { runtime: options.runtime }),
      });
      let closed = false;
      return {
        ...handle,
        async close() {
          if (closed) return;
          closed = true;
          await handle.close();
          database.close();
          await rm(directory, { recursive: true, force: true });
        },
      };
    },
  });
  assert.deepEqual(cases, [
    "pinned-direct-reads",
    "irregular-range-writes",
    "three-session-orders",
    "pending-namespace",
    "hidden-staging",
    "flush-close-abort",
    "session-backpressure",
  ]);
  console.log(
    JSON.stringify({
      schema: "efs-m7-conformance-v1",
      cases,
      commitCloseOrders: 36,
      sessionCounts: [1, 16, 64],
    }),
  );
});

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
  assert.equal(provider.existsSync("/workspace/file"), true);
  await assert.rejects(handle.filesystem.stat("/workspace/file"), {
    code: "ENOENT",
  });
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
  assert.equal(metrics.stagedLogicalBytes, 0);
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

test("default 64 MiB pending-write budget backpressures 64 sessions exactly", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const handle = await openNodeVfs({
    database,
    runtime: {
      maxManagedResidentBytes: 128 * 1024 * 1024,
      maxPendingWriteBytes: 64 * 1024 * 1024,
      maxWriteSessionBytes: 16 * 1024 * 1024,
      maxOpenNodeVfsSessions: 64,
    },
  });
  const sessions = [];
  for (let index = 0; index < 64; index += 1) {
    const session = handle.provider.openFileSync(`/file-${index}`, {
      writable: true,
      create: true,
    });
    session.writeSync(new Uint8Array(1024 * 1024), 0);
    sessions.push(session);
  }
  assert.equal(sessions[0].writeSync(Uint8Array.of(1), 1024 * 1024), 1);
  assert.ok(handle.provider.metrics.snapshot().forcedFlushCount >= 1);
  const peak = handle.provider.metrics.snapshot().peakManagedResidentBytes;
  assert.ok(peak <= handle.provider.capabilities.runtime.maxManagedResidentBytes);
  console.log(
    JSON.stringify({
      schema: "efs-m7-default-pressure-v1",
      sessions: 64,
      residentBoundaryBytes: 64 * 1024 * 1024,
      aggregateLimitBytes: 128 * 1024 * 1024,
      peakManagedResidentBytes: peak,
    }),
  );
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

test("all persisted COW page formats report immutable effective capabilities", async () => {
  for (const cowPageBytes of [4096, 8192, 16384]) {
    const database = await openNodeSqlite({ filename: ":memory:" });
    const initialized = await EphemeralFS.open({
      database,
      format: { cowPageBytes },
    });
    await initialized.close();
    const handle = await openNodeVfs({ database });
    assert.equal(handle.provider.capabilities.cowPageBytes, cowPageBytes);
    assert.equal(Object.isFrozen(handle.provider.capabilities), true);
    assert.equal(Object.isFrozen(handle.provider.capabilities.runtime), true);
    assert.equal(handle.provider.capabilities.supportsDataSync, false);
    const session = handle.provider.openFileSync("/page-format", {
      writable: true,
      create: true,
    });
    session.writeSync(Uint8Array.of(1, 2, 3), cowPageBytes - 1);
    session.closeSync();
    assert.deepEqual(
      handle.provider.readRangeSync("/page-format", cowPageBytes - 1, 3),
      Uint8Array.of(1, 2, 3),
    );
    await handle.close();
    database.close();
  }
});

test("one callback larger than the session budget streams without resident whole-file state", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const handle = await openNodeVfs({ database });
  const bytes = Uint8Array.from(
    { length: 20 * 1024 * 1024 },
    (_, index) => (index * 31) & 0xff,
  );
  const expected = bytes.slice(0, 64);
  const session = handle.provider.openFileSync("/large-callback", {
    writable: true,
    create: true,
  });
  assert.equal(session.writeSync(bytes, 0), bytes.byteLength);
  bytes.fill(0);
  const staged = handle.provider.metrics.snapshot();
  assert.equal(staged.residentWriteBytes, 0);
  assert.equal(staged.stagedLogicalBytes, 20 * 1024 * 1024);
  assert.ok(staged.peakManagedResidentBytes < 20 * 1024 * 1024);
  session.flushSync();
  assert.deepEqual(session.readRangeSync(0, 64), expected);
  session.closeSync();
  assert.equal(handle.provider.metrics.snapshot().residentControlBytes, 0);
  assert.equal(handle.provider.metrics.snapshot().stagedLogicalBytes, 0);
  await handle.close();
  database.close();
});

test("1,000 one-byte overwrites of a 100 MiB file stay on bounded core COW paths", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-vfs-cow-"));
  const database = await openNodeSqlite({ filename: path.join(directory, "fs.db") });
  try {
    const handle = await openNodeVfs({ database });
    const initial = handle.provider.openFileSync("/cow-large", {
      writable: true,
      create: true,
    });
    const block = new Uint8Array(1024 * 1024);
    for (let offset = 0; offset < 100 * 1024 * 1024; offset += block.byteLength) {
      const blockIndex = offset / block.byteLength;
      for (let index = 0; index < block.length; index += 1)
        block[index] = (index * 131 + blockIndex * 17 + 29) & 0xff;
      initial.writeSync(block, offset);
    }
    initial.closeSync();
    const before = handle.provider.metrics.snapshot();
    const expected = new Map();
    for (let index = 0; index < 1000; index += 1) {
      const position = (index * 104_729 + 11) % (100 * 1024 * 1024);
      const value = (index * 37 + 0x5a) & 0xff;
      const edit = handle.provider.openFileSync("/cow-large", { writable: true });
      edit.writeSync(Uint8Array.of(value), position);
      edit.closeSync();
      expected.set(position, value);
    }
    const after = handle.provider.metrics.snapshot();
    assert.equal(after.cowEditCount - before.cowEditCount, 1000);
    assert.ok(
      after.cowEditSourceBytes - before.cowEditSourceBytes < 1000 * 8 * 1024 * 1024,
    );
    for (const [position, value] of expected)
      assert.deepEqual(
        handle.provider.readRangeSync("/cow-large", position, 1),
        Uint8Array.of(value),
      );
    const actualDigest = createHash("sha256");
    const expectedDigest = createHash("sha256");
    for (let offset = 0; offset < 100 * 1024 * 1024; offset += 256 * 1024) {
      const length = Math.min(256 * 1024, 100 * 1024 * 1024 - offset);
      const actual = handle.provider.readRangeSync("/cow-large", offset, length);
      const generated = new Uint8Array(length);
      for (let index = 0; index < length; index += 1) {
        const absolute = offset + index;
        const blockIndex = Math.floor(absolute / (1024 * 1024));
        const blockOffset = absolute % (1024 * 1024);
        generated[index] = (blockOffset * 131 + blockIndex * 17 + 29) & 0xff;
      }
      for (const [position, value] of expected)
        if (position >= offset && position < offset + length)
          generated[position - offset] = value;
      assert.deepEqual(actual, generated);
      actualDigest.update(actual);
      expectedDigest.update(generated);
    }
    const fixtureDigest = actualDigest.digest("hex");
    assert.equal(fixtureDigest, expectedDigest.digest("hex"));
    assert.ok(
      after.peakManagedResidentBytes <=
        handle.provider.capabilities.runtime.maxManagedResidentBytes,
    );
    console.log(
      JSON.stringify({
        schema: "efs-m7-cow-resource-v1",
        fixtureBytes: 100 * 1024 * 1024,
        fixtureDigest,
        edits: 1000,
        cowEditCount: after.cowEditCount - before.cowEditCount,
        sourceBytesRead: after.cowEditSourceBytes - before.cowEditSourceBytes,
        peakManagedResidentBytes: after.peakManagedResidentBytes,
      }),
    );
    await handle.close();
  } finally {
    database.close();
    await rm(directory, { recursive: true, force: true });
  }
});

test("100 one-MiB files commit and read without aggregate-budget leakage", async () => {
  const database = await openNodeSqlite({ filename: ":memory:" });
  const handle = await openNodeVfs({ database });
  const content = Uint8Array.from(
    { length: 1024 * 1024 },
    (_, index) => (index * 47 + 13) & 0xff,
  );
  for (let index = 0; index < 100; index += 1) {
    const session = handle.provider.openFileSync(`/many-${index}`, {
      writable: true,
      create: true,
    });
    session.writeSync(content, 0);
    session.closeSync();
  }
  for (const index of [0, 49, 99])
    assert.deepEqual(
      handle.provider.readRangeSync(`/many-${index}`, 0, content.byteLength),
      content,
    );
  const metrics = handle.provider.metrics.snapshot();
  assert.equal(metrics.residentWriteBytes, 0);
  assert.equal(metrics.stagedLogicalBytes, 0);
  assert.ok(
    metrics.peakManagedResidentBytes <=
      handle.provider.capabilities.runtime.maxManagedResidentBytes,
  );
  await handle.close();
  database.close();
});

test("provider close rejects dirty state and failed session close remains retryable", async () => {
  const raw = await openNodeSqlite({ filename: ":memory:" });
  const faults = createStatementFaultController();
  const database = faults.wrap(raw);
  const handle = await openNodeVfs({ database });
  const session = handle.provider.openFileSync("/retry-close", {
    writable: true,
    create: true,
  });
  session.writeSync(new TextEncoder().encode("retryable"), 0);
  assert.throws(() => handle.provider.closeSync(), { code: "EBUSY" });
  faults.arm("after-sql-statement", 1);
  assert.throws(() => session.closeSync(), { code: "EIO" });
  assert.equal(new TextDecoder().decode(session.readRangeSync(0, 9)), "retryable");
  assert.equal(handle.provider.metrics.snapshot().dirtySessions, 1);
  faults.clear();
  session.closeSync();
  assert.equal(handle.provider.metrics.snapshot().dirtySessions, 0);
  await handle.close();
  raw.close();
});

test("every observed staging and visible-commit statement fault stays readable and retryable", async () => {
  const expected = Uint8Array.from({ length: 16 * 1024 }, (_, index) => index & 0xff);
  const runPhase = async (phase) => {
    for (let occurrence = 1; occurrence <= 512; occurrence += 1) {
      const raw = await openNodeSqlite({ filename: ":memory:" });
      const faults = createStatementFaultController();
      const database = faults.wrap(raw);
      const handle = await openNodeVfs({ database });
      const session = handle.provider.openFileSync(`/faulted-${phase}`, {
        writable: true,
        create: true,
      });
      session.writeSync(expected, 0);
      if (phase === "commit") session.stagePrefixSync();
      faults.arm("after-sql-statement", occurrence);
      let failed = false;
      try {
        if (phase === "stage") session.stagePrefixSync();
        else session.flushSync();
      } catch (error) {
        failed = true;
        assert.equal(error.code, "EIO");
        assert.deepEqual(session.readRangeSync(0, expected.byteLength), expected);
        assert.equal(handle.provider.metrics.snapshot().dirtySessions, 1);
        faults.clear();
        if (phase === "stage") session.stagePrefixSync();
        else session.flushSync();
      }
      faults.clear();
      if (phase === "stage") session.flushSync();
      session.closeSync();
      assert.deepEqual(
        handle.provider.readRangeSync(`/faulted-${phase}`, 0, expected.byteLength),
        expected,
      );
      assert.equal(handle.provider.metrics.snapshot().stagedLogicalBytes, 0);
      await handle.close();
      raw.close();
      if (!failed) return occurrence - 1;
    }
    throw new Error(`${phase} fault matrix exceeded its finite position cap`);
  };
  const stagingPositions = await runPhase("stage");
  const commitPositions = await runPhase("commit");
  assert.ok(stagingPositions > 20 && stagingPositions < 512);
  assert.ok(commitPositions > 20 && commitPositions < 512);
  console.log(
    JSON.stringify({
      schema: "efs-m7-fault-matrix-v1",
      faultPoint: "after-sql-statement",
      stagingPositions,
      commitPositions,
    }),
  );
});

test("process restart discards unflushed memory and keeps hidden staging invisible", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-vfs-crash-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    let database = await openNodeSqlite({ filename });
    let handle = await openNodeVfs({ database });
    let session = handle.provider.openFileSync("/restart", {
      writable: true,
      create: true,
    });
    session.writeSync(new TextEncoder().encode("committed"), 0);
    session.closeSync();
    await handle.close();
    database.close();

    database = await openNodeSqlite({ filename });
    handle = await openNodeVfs({ database });
    session = handle.provider.openFileSync("/restart", { writable: true });
    session.writeSync(new TextEncoder().encode("hidden"), 0);
    session.truncateSync(6);
    session.stagePrefixSync();
    database.close();

    const reopenedDatabase = await openNodeSqlite({ filename });
    const reopened = await openNodeVfs({ database: reopenedDatabase });
    assert.equal(
      new TextDecoder().decode(reopened.provider.readRangeSync("/restart", 0, 9)),
      "committed",
    );
    await reopened.close();
    reopenedDatabase.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

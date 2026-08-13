import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { setTimeout as delay } from "node:timers/promises";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { openNodeVfs } from "../../packages/node-vfs/dist/index.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { createStatementFaultController } from "../../packages/testkit/dist/index.js";

const encoder = new TextEncoder();
const decoder = new TextDecoder();

function text(value) {
  return encoder.encode(value);
}

function decoded(value) {
  return decoder.decode(value);
}

function expectCode(operation, expected) {
  assert.throws(operation, (error) => {
    assert.equal(error?.code, expected);
    return true;
  });
}

async function inMemory(callback, options = {}) {
  const raw = await openNodeSqlite({ filename: ":memory:" });
  const database = options.wrap ? options.wrap(raw) : raw;
  const handle = await openNodeVfs({
    database,
    ...(options.runtime === undefined ? {} : { runtime: options.runtime }),
  });
  try {
    return await callback(handle, raw);
  } finally {
    try {
      await handle.close();
    } catch {}
    raw.close();
  }
}

function writeText(provider, path, value, options = {}) {
  const session = provider.openFileSync(path, {
    writable: true,
    create: true,
    ...options,
  });
  session.writeSync(text(value), 0);
  session.closeSync();
}

async function missing(filesystem, path) {
  try {
    await filesystem.stat(path);
  } catch (error) {
    if (error?.code === "ENOENT") return true;
    throw error;
  }
  return false;
}

test("an empty exclusive create is durable after successful close and physical reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-vfs-empty-create-"));
  const filename = path.join(directory, "fs.db");
  let database;
  let handle;
  try {
    database = await openNodeSqlite({ filename });
    handle = await openNodeVfs({ database });
    const created = handle.provider.openFileSync("/empty", {
      writable: true,
      create: true,
      exclusive: true,
      mode: 0o640,
    });
    assert.equal(handle.provider.existsSync("/empty"), true);
    created.closeSync();
    assert.equal(handle.provider.existsSync("/empty"), true);
    assert.equal(handle.provider.statSync("/empty").size, 0);
    assert.equal(handle.provider.statSync("/empty").mode, 0o640);
    await handle.close();
    handle = undefined;
    database.close();
    database = undefined;

    database = await openNodeSqlite({ filename });
    handle = await openNodeVfs({ database });
    assert.equal(handle.provider.existsSync("/empty"), true);
    assert.equal(handle.provider.statSync("/empty").size, 0);
    await handle.close();
    handle = undefined;
    database.close();
    database = undefined;
  } finally {
    try {
      await handle?.close();
    } catch {}
    try {
      database?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("renaming a parent directory updates dirty descendants atomically or returns EBUSY", async () => {
  await inMemory(async ({ provider }) => {
    provider.mkdirSync("/source");
    writeText(provider, "/source/file", "old");
    const dirty = provider.openFileSync("/source/file", { writable: true });
    dirty.writeSync(Uint8Array.of("X".charCodeAt(0)), 0);

    let renamed = false;
    try {
      provider.renameSync("/source", "/destination");
      renamed = true;
    } catch (error) {
      assert.equal(error?.code, "EBUSY");
    }

    if (renamed) {
      assert.equal(dirty.path, "/destination/file");
      dirty.closeSync();
      assert.equal(provider.existsSync("/source/file"), false);
      assert.equal(decoded(provider.readRangeSync("/destination/file", 0, 3)), "Xld");
    } else {
      assert.equal(dirty.path, "/source/file");
      dirty.closeSync();
      assert.equal(decoded(provider.readRangeSync("/source/file", 0, 3)), "Xld");
      assert.equal(provider.existsSync("/destination"), false);
    }
    assert.equal(provider.metrics.snapshot().dirtySessions, 0);
  });
});

test("exclusive create resolves every injected commit outcome before returning and remains retryable", async () => {
  const retryFailures = [];
  const reportedFailureAfterVisibility = [];
  let injectedFailures = 0;
  let statementBoundary;

  for (let occurrence = 1; occurrence <= 512; occurrence += 1) {
    const raw = await openNodeSqlite({ filename: ":memory:" });
    const faults = createStatementFaultController();
    const database = faults.wrap(raw);
    const handle = await openNodeVfs({ database });
    const session = handle.provider.openFileSync("/exclusive", {
      writable: true,
      create: true,
      exclusive: true,
    });
    try {
      session.writeSync(text("exclusive-content"), 0);
      session.stagePrefixSync();
      faults.arm("after-sql-statement", occurrence);
      let firstError;
      try {
        session.flushSync();
      } catch (error) {
        firstError = error;
      }
      faults.clear();
      if (!firstError) {
        statementBoundary = occurrence - 1;
        session.closeSync();
        break;
      }

      injectedFailures += 1;
      if (!(await missing(handle.filesystem, "/exclusive")))
        reportedFailureAfterVisibility.push({ occurrence, code: firstError.code });
      try {
        session.flushSync();
      } catch (error) {
        retryFailures.push({ occurrence, code: error?.code, message: error?.message });
      }
      if (!session.dirty) {
        assert.equal(
          decoded(handle.provider.readRangeSync("/exclusive", 0, 17)),
          "exclusive-content",
        );
        session.closeSync();
      }
    } finally {
      faults.clear();
      session.abortSync();
      await handle.close();
      raw.close();
    }
  }

  assert.ok(injectedFailures > 0, "fault injection did not exercise commit work");
  assert.ok(statementBoundary > 20, "commit statement boundary was not discovered");
  assert.deepEqual(
    { reportedFailureAfterVisibility, retryFailures },
    { reportedFailureAfterVisibility: [], retryFailures: [] },
    "exclusive-create commit outcomes were not resolved or retryable",
  );
});

test("all namespace operations consistently observe a pending inode", async () => {
  await inMemory(async ({ provider, filesystem }) => {
    const pending = provider.openFileSync("/pending", {
      writable: true,
      create: true,
      exclusive: true,
    });
    pending.writeSync(text("pending"), 0);

    assert.equal(provider.readdirSync("/").includes("pending"), true);
    expectCode(() => provider.mkdirSync("/pending"), "EEXIST");
    expectCode(() => provider.symlinkSync("target", "/pending"), "EEXIST");
    expectCode(
      () =>
        provider.openFileSync("/pending/child", {
          writable: true,
          create: true,
        }),
      "ENOTDIR",
    );

    provider.symlinkSync("/pending", "/pending-link");
    assert.equal(provider.existsSync("/pending-link"), true);
    assert.equal(provider.statSync("/pending-link").id, pending.statSync().id);
    assert.equal(decoded(provider.readRangeSync("/pending-link", 0, 7)), "pending");
    assert.equal(await missing(filesystem, "/pending"), true);

    pending.closeSync();
    assert.equal(decoded(provider.readRangeSync("/pending-link", 0, 7)), "pending");
  });
});

test("rename enforces the complete type, root, nonempty, and ancestry matrix", async () => {
  await inMemory(async ({ provider }) => {
    writeText(provider, "/file-source", "source");
    writeText(provider, "/file-target", "target");
    provider.renameSync("/file-source", "/file-target");
    assert.equal(decoded(provider.readRangeSync("/file-target", 0, 6)), "source");
    assert.equal(provider.existsSync("/file-source"), false);

    provider.mkdirSync("/empty-source");
    provider.mkdirSync("/empty-target");
    provider.renameSync("/empty-source", "/empty-target");
    assert.equal(provider.statSync("/empty-target").isDirectory(), true);

    provider.mkdirSync("/directory-source");
    provider.mkdirSync("/nonempty-target");
    writeText(provider, "/nonempty-target/child", "retained");
    expectCode(
      () => provider.renameSync("/directory-source", "/nonempty-target"),
      "ENOTEMPTY",
    );
    assert.equal(provider.statSync("/directory-source").isDirectory(), true);
    assert.equal(
      decoded(provider.readRangeSync("/nonempty-target/child", 0, 8)),
      "retained",
    );

    writeText(provider, "/file-over-directory", "file");
    provider.mkdirSync("/directory-destination");
    expectCode(
      () => provider.renameSync("/file-over-directory", "/directory-destination"),
      "EISDIR",
    );

    provider.mkdirSync("/directory-over-file");
    writeText(provider, "/file-destination", "file");
    expectCode(
      () => provider.renameSync("/directory-over-file", "/file-destination"),
      "ENOTDIR",
    );

    provider.mkdirSync("/ancestor/child", { recursive: true });
    expectCode(
      () => provider.renameSync("/ancestor", "/ancestor/child/inside"),
      "EINVAL",
    );
    expectCode(() => provider.renameSync("/", "/renamed-root"), "EPERM");
    expectCode(() => provider.renameSync("/ancestor", "/"), "EPERM");
  });
});

test("file and directory modes use portable defaults and reject invalid numbers", async () => {
  await inMemory(async ({ provider }) => {
    const file = provider.openFileSync("/default-file", {
      writable: true,
      create: true,
    });
    file.writeSync(Uint8Array.of(1), 0);
    file.closeSync();
    provider.mkdirSync("/default-directory");
    assert.equal(provider.statSync("/default-file").mode, 0o644);
    assert.equal(provider.statSync("/default-directory").mode, 0o755);

    const masked = provider.openFileSync("/masked-file", {
      writable: true,
      create: true,
      mode: 0o17_640,
    });
    masked.writeSync(Uint8Array.of(1), 0);
    masked.closeSync();
    provider.mkdirSync("/masked-directory", { mode: 0o17_750 });
    assert.equal(provider.statSync("/masked-file").mode, 0o7640);
    assert.equal(provider.statSync("/masked-directory").mode, 0o7750);

    for (const [index, invalid] of [
      -1,
      1.5,
      Number.NaN,
      Number.POSITIVE_INFINITY,
    ].entries()) {
      expectCode(
        () =>
          provider.openFileSync(`/invalid-file-${index}`, {
            writable: true,
            create: true,
            mode: invalid,
          }),
        "EINVAL",
      );
      assert.equal(provider.existsSync(`/invalid-file-${index}`), false);
      expectCode(
        () => provider.mkdirSync(`/invalid-directory-${index}`, { mode: invalid }),
        "EINVAL",
      );
      assert.equal(provider.existsSync(`/invalid-directory-${index}`), false);
    }
  });
});

test("symlink targets are validated before namespace mutation", async () => {
  await inMemory(async ({ provider }) => {
    for (const [index, target] of ["", "nul\0target", "\ud800"].entries()) {
      expectCode(
        () => provider.symlinkSync(target, `/invalid-link-${index}`),
        "EINVAL",
      );
      assert.equal(provider.existsSync(`/invalid-link-${index}`), false);
    }
    expectCode(() => provider.symlinkSync("target", "/"), "EPERM");
  });
});

test("hard-link coordinators retain inode identity, exact nlink, and stable timestamps", async () => {
  await inMemory(async ({ provider }) => {
    writeText(provider, "/hard-a", "abc");
    provider.linkSync("/hard-a", "/hard-b");
    const initialA = provider.statSync("/hard-a");
    const initialB = provider.statSync("/hard-b");
    assert.equal(initialA.id, initialB.id);
    assert.equal(initialA.nlink, 2);
    assert.equal(initialB.nlink, 2);

    const opened = provider.openFileSync("/hard-a", { writable: true });
    assert.equal(opened.statSync().id, initialA.id);
    assert.equal(opened.statSync().nlink, 2);
    assert.equal(provider.statSync("/hard-b").id, initialA.id);
    assert.equal(provider.statSync("/hard-b").nlink, 2);
    assert.equal(opened.statSync().birthtimeMs, initialA.birthtimeMs);

    opened.writeSync(Uint8Array.of("X".charCodeAt(0)), 1);
    const firstDirty = opened.statSync();
    await delay(5);
    const secondDirty = opened.statSync();
    assert.equal(secondDirty.mtimeMs, firstDirty.mtimeMs);
    assert.equal(secondDirty.ctimeMs, firstDirty.ctimeMs);
    assert.equal(decoded(provider.readRangeSync("/hard-b", 0, 3)), "aXc");
    opened.closeSync();

    const committedA = provider.statSync("/hard-a");
    const committedB = provider.statSync("/hard-b");
    assert.equal(committedA.id, initialA.id);
    assert.equal(committedB.id, initialA.id);
    assert.equal(committedA.nlink, 2);
    assert.equal(committedB.nlink, 2);
  });
});

function requiredMetric(metrics, alternatives, label) {
  for (const name of alternatives)
    if (Object.hasOwn(metrics, name)) return metrics[name];
  assert.fail(`${label} metric is absent (expected one of ${alternatives.join(", ")})`);
}

test("metrics exactly account callbacks, contiguous runs, session peaks, and flush reasons", async () => {
  await inMemory(async ({ provider }) => {
    const first = provider.openFileSync("/metrics-a", {
      writable: true,
      create: true,
    });
    const second = provider.openFileSync("/metrics-b", {
      writable: true,
      create: true,
    });
    first.writeSync(text("abc"), 0);
    first.writeSync(text("defgh"), 3);
    first.writeSync(text("ij"), 8);
    assert.deepEqual(
      {
        openSessions: provider.metrics.snapshot().openSessions,
        dirtySessions: provider.metrics.snapshot().dirtySessions,
        residentWriteBytes: provider.metrics.snapshot().residentWriteBytes,
        admittedWriteBytes: provider.metrics.snapshot().admittedWriteBytes,
      },
      {
        openSessions: 2,
        dirtySessions: 1,
        residentWriteBytes: 10,
        admittedWriteBytes: 10,
      },
    );

    first.stagePrefixSync();
    assert.equal(provider.metrics.snapshot().residentWriteBytes, 0);
    assert.equal(provider.metrics.snapshot().stagedLogicalBytes, 10);
    first.flushSync();
    first.readIntoSync(new Uint8Array(10), 0, 0, 10);
    first.closeSync();
    second.abortSync();
    const metrics = provider.metrics.snapshot();
    assert.deepEqual(
      {
        openSessions: metrics.openSessions,
        dirtySessions: metrics.dirtySessions,
        residentWriteBytes: metrics.residentWriteBytes,
        residentControlBytes: metrics.residentControlBytes,
        stagedLogicalBytes: metrics.stagedLogicalBytes,
        admittedWriteBytes: metrics.admittedWriteBytes,
        flushedWriteBytes: metrics.flushedWriteBytes,
        flushCount: metrics.flushCount,
        directReadBytes: metrics.directReadBytes,
      },
      {
        openSessions: 0,
        dirtySessions: 0,
        residentWriteBytes: 0,
        residentControlBytes: 0,
        stagedLogicalBytes: 0,
        admittedWriteBytes: 10,
        flushedWriteBytes: 10,
        flushCount: 1,
        directReadBytes: 10,
      },
    );

    const peakSessions = requiredMetric(
      metrics,
      ["peakSessions", "peakOpenSessions"],
      "peak session count",
    );
    assert.ok(peakSessions >= 2);
    assert.ok(
      requiredMetric(
        metrics,
        ["callbackSizeDistribution", "callbackSizes"],
        "callback-size distribution",
      ),
    );
    assert.ok(
      requiredMetric(
        metrics,
        ["maxContiguousRunBytes", "contiguousRunLength", "contiguousRunBytes"],
        "contiguous-run length",
      ),
    );
    assert.ok(
      requiredMetric(metrics, ["flushReasons", "flushReasonCounts"], "flush reason"),
    );
  });
});

function countBlobReads(driver) {
  const counter = { bytes: 0 };
  const countRows = (rows) => {
    for (const row of rows)
      for (const value of Object.values(row))
        if (ArrayBuffer.isView(value)) counter.bytes += value.byteLength;
  };
  const wrapped = Object.freeze({
    kind: driver.kind,
    readOnly: driver.readOnly,
    capabilities: driver.capabilities,
    ...(driver.hashBytes === undefined
      ? {}
      : { hashBytes: driver.hashBytes.bind(driver) }),
    ...(driver.hashBytesAsync === undefined
      ? {}
      : { hashBytesAsync: driver.hashBytesAsync.bind(driver) }),
    transaction(mode, callback) {
      return driver.transaction(mode, (transaction) =>
        callback(
          Object.freeze({
            scope: transaction.scope,
            run: transaction.run.bind(transaction),
            all(sql, bindings, budget) {
              const rows = transaction.all(sql, bindings, budget);
              countRows(rows);
              return rows;
            },
          }),
        ),
      );
    },
    physicalStorage: () => driver.physicalStorage?.() ?? Object.freeze({}),
    ...(driver.checkpoint === undefined
      ? {}
      : { checkpoint: driver.checkpoint.bind(driver) }),
    close: () => driver.close(),
  });
  return { counter, wrapped };
}

test("several edits in one large-file session stay on bounded COW paths", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-vfs-multi-cow-"));
  const filename = path.join(directory, "fs.db");
  const fixtureBytes = 32 * 1024 * 1024;
  const positions = [7 * 1024 * 1024 + 13, 25 * 1024 * 1024 + 29];
  let raw;
  let handle;
  try {
    raw = await openNodeSqlite({ filename });
    const counted = countBlobReads(raw);
    handle = await openNodeVfs({ database: counted.wrapped });
    const initial = handle.provider.openFileSync("/large", {
      writable: true,
      create: true,
    });
    const block = new Uint8Array(1024 * 1024);
    let state = 0x9e37_79b9;
    for (let offset = 0; offset < fixtureBytes; offset += block.byteLength) {
      for (let index = 0; index < block.length; index += 1) {
        state ^= state << 13;
        state ^= state >>> 17;
        state ^= state << 5;
        block[index] = state & 0xff;
      }
      initial.writeSync(block, offset);
    }
    initial.closeSync();
    await handle.close();
    handle = undefined;

    handle = await openNodeVfs({ database: counted.wrapped });
    counted.counter.bytes = 0;
    const before = handle.provider.metrics.snapshot();
    const edited = handle.provider.openFileSync("/large", { writable: true });
    edited.writeSync(Uint8Array.of(0xa5), positions[0]);
    edited.writeSync(Uint8Array.of(0x5a), positions[1]);
    edited.closeSync();
    const after = handle.provider.metrics.snapshot();

    assert.equal(handle.provider.statSync("/large").size, fixtureBytes);
    assert.deepEqual(
      handle.provider.readRangeSync("/large", positions[0], 1),
      Uint8Array.of(0xa5),
    );
    assert.deepEqual(
      handle.provider.readRangeSync("/large", positions[1], 1),
      Uint8Array.of(0x5a),
    );
    assert.deepEqual(
      {
        usedCowPath: after.cowEditCount - before.cowEditCount >= 1,
        sourceReadWasBounded: counted.counter.bytes < fixtureBytes / 2,
      },
      { usedCowPath: true, sourceReadWasBounded: true },
      `multi-edit commit read ${counted.counter.bytes} BLOB bytes for a ${fixtureBytes}-byte file`,
    );
    assert.ok(
      after.peakManagedResidentBytes <=
        handle.provider.capabilities.runtime.maxManagedResidentBytes,
    );
    await handle.close();
    handle = undefined;
    raw.close();
    raw = undefined;
  } finally {
    try {
      await handle?.close();
    } catch {}
    try {
      raw?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

function instrumentOwnedByteAllocations() {
  const OriginalUint8Array = globalThis.Uint8Array;
  const OriginalArrayBuffer = globalThis.ArrayBuffer;
  const originalBufferAlloc = Buffer.alloc;
  const originalBufferAllocUnsafe = Buffer.allocUnsafe;
  const sizes = [];
  const record = (size) => {
    if (Number.isSafeInteger(size) && size >= 0) sizes.push(size);
  };
  globalThis.Uint8Array = new Proxy(OriginalUint8Array, {
    construct(target, argumentsList) {
      if (typeof argumentsList[0] === "number") record(argumentsList[0]);
      return Reflect.construct(target, argumentsList, target);
    },
  });
  globalThis.ArrayBuffer = new Proxy(OriginalArrayBuffer, {
    construct(target, argumentsList) {
      record(argumentsList[0]);
      return Reflect.construct(target, argumentsList, target);
    },
  });
  Buffer.alloc = function alloc(size, ...rest) {
    record(size);
    return Reflect.apply(originalBufferAlloc, Buffer, [size, ...rest]);
  };
  Buffer.allocUnsafe = function allocUnsafe(size) {
    record(size);
    return Reflect.apply(originalBufferAllocUnsafe, Buffer, [size]);
  };
  return {
    sizes,
    restore() {
      globalThis.Uint8Array = OriginalUint8Array;
      globalThis.ArrayBuffer = OriginalArrayBuffer;
      Buffer.alloc = originalBufferAlloc;
      Buffer.allocUnsafe = originalBufferAllocUnsafe;
    },
  };
}

test("readIntoSync uses caller storage without an equal-sized owned allocation at every page size", async () => {
  for (const cowPageBytes of [4096, 8192, 16384]) {
    const database = await openNodeSqlite({ filename: ":memory:" });
    const initialized = await EphemeralFS.open({
      database,
      format: { cowPageBytes },
    });
    await initialized.close();
    const handle = await openNodeVfs({ database });
    try {
      const writer = handle.provider.openFileSync(`/direct-${randomUUID()}`, {
        writable: true,
        create: true,
      });
      const block = Uint8Array.from(
        { length: 256 * 1024 },
        (_, index) => (index * 131 + cowPageBytes / 4096) & 0xff,
      );
      for (let offset = 0; offset < 2 * 1024 * 1024; offset += block.byteLength)
        writer.writeSync(block, offset);
      const writerPath = writer.path;
      writer.closeSync();

      const reader = handle.provider.openFileSync(writerPath);
      const length = 768 * 1024 + 123;
      const destination = Buffer.alloc(length + 38, 0xa5);
      const allocations = instrumentOwnedByteAllocations();
      try {
        assert.equal(reader.readIntoSync(destination, 19, 12345, length), length);
      } finally {
        allocations.restore();
      }
      assert.equal(
        destination.subarray(0, 19).every((byte) => byte === 0xa5),
        true,
      );
      assert.equal(
        destination.subarray(19 + length).every((byte) => byte === 0xa5),
        true,
      );
      assert.equal(
        allocations.sizes.some((size) => size >= length),
        false,
        `readIntoSync allocated an owned ${Math.max(0, ...allocations.sizes)}-byte value`,
      );
      reader.closeSync();
    } finally {
      await handle.close();
      database.close();
    }
  }
});

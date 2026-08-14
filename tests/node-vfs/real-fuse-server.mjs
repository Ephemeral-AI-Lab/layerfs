import fs from "node:fs";
import { createRequire } from "node:module";
import os from "node:os";
import readline from "node:readline";
import { openNodeVfs } from "../../packages/node-vfs/dist/index.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

if (process.platform !== "linux") throw new Error("real FUSE host requires Linux");
const [databaseFilename, mountpoint] = process.argv.slice(2);
if (!databaseFilename || !mountpoint)
  throw new Error("usage: real-fuse-server.mjs <database> <mountpoint>");

const require = createRequire(import.meta.url);
const Fuse = require("fuse-native");
const fuseVersion = require("fuse-native/package.json").version;
const rawDatabase = await openNodeSqlite({ filename: databaseFilename });
let transactionCount = 0;
const database = {
  kind: rawDatabase.kind,
  readOnly: rawDatabase.readOnly,
  capabilities: rawDatabase.capabilities,
  hashBytes: rawDatabase.hashBytes,
  transaction(mode, callback) {
    transactionCount += 1;
    return rawDatabase.transaction(mode, callback);
  },
  physicalStorage: () => rawDatabase.physicalStorage(),
  checkpoint: (mode) => rawDatabase.checkpoint(mode),
  close: () => rawDatabase.close(),
};
const handle = await openNodeVfs({ database });
const sessions = new Map();
let nextFileHandle = 1;
let peakRssBytes = process.memoryUsage().rss;
let mountedPayloadOneByteWriteCallbacks = 0;
let countPayloadOneByteWriteCallbacks = false;
let payloadEditMetricsStart;
let stopping = false;
let controlPending = Promise.resolve();

function sampleMemory() {
  peakRssBytes = Math.max(peakRssBytes, process.memoryUsage().rss);
}

function writeMessage(value) {
  process.stdout.write(`${JSON.stringify(value)}\n`);
}

function errno(error) {
  const code =
    error && typeof error === "object" && "code" in error ? error.code : "EIO";
  return Fuse[code] ?? Fuse.EIO;
}

function call(callback, operation, value) {
  try {
    const result = operation();
    sampleMemory();
    if (value) value(callback, result);
    else callback(0);
  } catch (error) {
    callback(errno(error));
  }
}

function fileStat(path) {
  const value = handle.provider.lstatSync(path);
  const type = value.isDirectory()
    ? 0o040000
    : value.isSymbolicLink()
      ? 0o120000
      : 0o100000;
  return {
    mode: type | value.mode,
    size: value.isDirectory() ? 4096 : value.size,
    nlink: value.nlink,
    uid: process.getuid?.() ?? 0,
    gid: process.getgid?.() ?? 0,
    mtime: new Date(value.mtimeMs),
    ctime: new Date(value.ctimeMs),
    atime: new Date(value.mtimeMs),
  };
}

function session(fileHandle) {
  const selected = sessions.get(fileHandle);
  if (!selected) {
    const error = new Error("unknown FUSE file handle");
    error.code = "EBADF";
    throw error;
  }
  return selected;
}

const operations = {
  access(path, mode, callback) {
    void mode;
    call(callback, () => {
      if (!handle.provider.existsSync(path)) {
        const error = new Error("missing path");
        error.code = "ENOENT";
        throw error;
      }
    });
  },
  getattr(path, callback) {
    call(
      callback,
      () => fileStat(path),
      (done, value) => done(0, value),
    );
  },
  // fuse-native dispatches an advertised fgetattr through getattr, but the
  // marker is required so non-root file-handle stats are enabled.
  fgetattr() {},
  statfs(path, callback) {
    void path;
    callback(0, {
      bsize: 4096,
      frsize: 4096,
      blocks: 1024 * 1024,
      bfree: 512 * 1024,
      bavail: 512 * 1024,
      files: 1024 * 1024,
      ffree: 512 * 1024,
      favail: 512 * 1024,
      fsid: 0x45504653,
      flag: 0,
      namemax: 255,
    });
  },
  readdir(path, callback) {
    call(
      callback,
      () => handle.provider.readdirSync(path),
      (done, names) => done(0, names),
    );
  },
  open(path, flags, callback) {
    call(
      callback,
      () => {
        const writable = (flags & 3) !== fs.constants.O_RDONLY;
        const opened = handle.provider.openFileSync(path, {
          writable,
          truncate: writable && (flags & fs.constants.O_TRUNC) !== 0,
        });
        const fileHandle = nextFileHandle++;
        sessions.set(fileHandle, opened);
        return fileHandle;
      },
      (done, fileHandle) => done(0, fileHandle),
    );
  },
  create(path, mode, callback) {
    call(
      callback,
      () => {
        const opened = handle.provider.openFileSync(path, {
          writable: true,
          create: true,
          exclusive: true,
          mode,
        });
        const fileHandle = nextFileHandle++;
        sessions.set(fileHandle, opened);
        return fileHandle;
      },
      (done, fileHandle) => done(0, fileHandle),
    );
  },
  read(path, fileHandle, buffer, length, position, callback) {
    void path;
    call(
      callback,
      () => session(fileHandle).readIntoSync(buffer, 0, position, length),
      (done, bytesRead) => done(bytesRead),
    );
  },
  write(path, fileHandle, buffer, length, position, callback) {
    call(
      callback,
      () => {
        const written = session(fileHandle).writeSync(
          buffer.subarray(0, length),
          position,
        );
        if (
          countPayloadOneByteWriteCallbacks &&
          path === "/smoke/payload" &&
          length === 1 &&
          written === 1
        )
          mountedPayloadOneByteWriteCallbacks += 1;
        return written;
      },
      (done, bytesWritten) => done(bytesWritten),
    );
  },
  flush(path, fileHandle, callback) {
    void path;
    call(callback, () => {
      const opened = session(fileHandle);
      if (opened.writable) opened.flushSync();
    });
  },
  fsync(path, dataOnly, fileHandle, callback) {
    void path;
    call(callback, () => {
      const opened = session(fileHandle);
      if (opened.writable) opened.flushSync({ dataOnly });
    });
  },
  ftruncate(path, fileHandle, size, callback) {
    void path;
    call(callback, () => session(fileHandle).truncateSync(size));
  },
  truncate(path, size, callback) {
    call(callback, () => {
      const opened = handle.provider.openFileSync(path, { writable: true });
      try {
        opened.truncateSync(size);
        opened.closeSync();
      } catch (error) {
        try {
          opened.abortSync();
        } catch {}
        throw error;
      }
    });
  },
  release(path, fileHandle, callback) {
    void path;
    call(callback, () => {
      const opened = session(fileHandle);
      opened.closeSync();
      sessions.delete(fileHandle);
    });
  },
  mkdir(path, mode, callback) {
    call(callback, () => handle.provider.mkdirSync(path, { mode }));
  },
  rmdir(path, callback) {
    call(callback, () => handle.provider.rmdirSync(path));
  },
  unlink(path, callback) {
    call(callback, () => handle.provider.unlinkSync(path));
  },
  rename(source, destination, callback) {
    call(callback, () => handle.provider.renameSync(source, destination));
  },
  link(source, destination, callback) {
    call(callback, () => handle.provider.linkSync(source, destination));
  },
  symlink(target, path, callback) {
    call(callback, () => handle.provider.symlinkSync(target, path));
  },
  readlink(path, callback) {
    call(
      callback,
      () => handle.provider.readlinkSync(path),
      (done, target) => done(0, target),
    );
  },
  chmod(path, mode, callback) {
    call(callback, () => handle.provider.chmodSync(path, mode));
  },
  utimens(path, atime, mtime, callback) {
    void path;
    void atime;
    void mtime;
    callback(0);
  },
  opendir(path, flags, callback) {
    void path;
    void flags;
    callback(0, 0);
  },
  releasedir(path, fileHandle, callback) {
    void path;
    void fileHandle;
    callback(0);
  },
  fsyncdir(path, dataOnly, fileHandle, callback) {
    void path;
    void fileHandle;
    void dataOnly;
    call(callback, () => handle.provider.syncSync());
  },
};

const fuse = new Fuse(mountpoint, operations, {
  mkdir: true,
  force: true,
  autoUnmount: true,
  defaultPermissions: true,
  timeout: 10_000,
});

function mount() {
  return new Promise((resolve, reject) =>
    fuse.mount((error) => (error ? reject(error) : resolve())),
  );
}

function unmount() {
  return new Promise((resolve, reject) =>
    fuse.unmount((error) => (error ? reject(error) : resolve())),
  );
}

async function verifyAll() {
  let cursor;
  let checkedEntities = 0;
  for (let batch = 0; batch < 100_000; batch += 1) {
    const result = await handle.filesystem.maintenance.verify({
      ...(cursor === undefined ? {} : { cursor }),
      maxEntities: 32,
    });
    checkedEntities += result.checkedEntities;
    cursor = result.nextCursor ?? undefined;
    if (result.complete) return { complete: true, checkedEntities };
  }
  throw new Error("real FUSE bounded verification did not complete");
}

function durableState() {
  return database.transaction("read", (tx) => {
    const active = tx.all(
      "SELECT (SELECT count(*) FROM efs_leases WHERE state IN (0,1)) leases,(SELECT count(*) FROM efs_staging_certificates) staging,(SELECT count(*) FROM efs_operation_results WHERE outcome=-1 AND length(encoded)=0) reservations",
      [],
      { maxRows: 1, maxBytes: 512 },
    )[0];
    const leases = tx.all(
      "SELECT id,kind,owner_id,branch_id,state FROM efs_leases WHERE state IN (0,1) ORDER BY id",
      [],
      { maxRows: 256, maxBytes: 65_536 },
    );
    const usage = tx.all("SELECT * FROM efs_usage WHERE singleton=1", [], {
      maxRows: 1,
      maxBytes: 4096,
    })[0];
    return { active, leases, usage };
  });
}

async function control(command) {
  if (command.command === "snapshot") {
    sampleMemory();
    const state = durableState();
    return {
      metrics: handle.provider.metrics.snapshot(),
      peakRssBytes,
      physical: database.physicalStorage?.(),
      transactionCount,
      openSessionCount: sessions.size,
      activeDurableState: state.active,
      mountedPayloadOneByteWriteCallbacks,
    };
  }
  if (command.command === "retain-smoke-revision") {
    const branch = await handle.filesystem.branches.create(
      "m7-real-fuse-retention-anchor",
    );
    try {
      const publication = await branch.publish({
        operationId: "m7-real-fuse-retention-anchor-publication",
      });
      if (publication.outcome !== "merged")
        throw new Error(
          `real FUSE retention anchor did not publish (${publication.outcome})`,
        );
      return { publication };
    } finally {
      await branch.close();
    }
  }
  if (command.command === "reset-payload-write-callbacks") {
    mountedPayloadOneByteWriteCallbacks = 0;
    countPayloadOneByteWriteCallbacks = true;
    payloadEditMetricsStart = handle.provider.metrics.snapshot();
    return {
      mountedPayloadOneByteWriteCallbacks,
      metrics: payloadEditMetricsStart,
    };
  }
  if (command.command === "stop-payload-write-callbacks") {
    countPayloadOneByteWriteCallbacks = false;
    if (!payloadEditMetricsStart)
      throw new Error("real FUSE payload edit metrics were not started");
    const metrics = handle.provider.metrics.snapshot();
    const editBatchProof = {
      callbackCount: mountedPayloadOneByteWriteCallbacks,
      flushCountDelta: metrics.flushCount - payloadEditMetricsStart.flushCount,
      failedFlushCountDelta:
        metrics.failedFlushCount - payloadEditMetricsStart.failedFlushCount,
      cowEditCountDelta: metrics.cowEditCount - payloadEditMetricsStart.cowEditCount,
      cowEditSourceBytesDelta:
        metrics.cowEditSourceBytes - payloadEditMetricsStart.cowEditSourceBytes,
      coreBatchCountDelta:
        metrics.coreBatchCount - payloadEditMetricsStart.coreBatchCount,
    };
    payloadEditMetricsStart = undefined;
    return { mountedPayloadOneByteWriteCallbacks, editBatchProof };
  }
  if (command.command === "collect-start") {
    handle.provider.syncSync();
    const collection = await handle.filesystem.maintenance.collectGarbage({
      runId: "m7-real-fuse-interrupted-collection",
      maxBatches: 1,
    });
    if (collection.state !== "paused")
      throw new Error(`real FUSE collection was not interrupted (${collection.state})`);
    return { collection };
  }
  if (command.command === "resume-collection") {
    handle.provider.syncSync();
    let collection = await handle.filesystem.maintenance.collectGarbage({
      runId: "m7-real-fuse-interrupted-collection",
      maxBatches: 8,
    });
    for (let call = 0; call < 5_000 && collection.state !== "complete"; call += 1)
      collection = await handle.filesystem.maintenance.collectGarbage({
        runId: "m7-real-fuse-interrupted-collection",
        maxBatches: 8,
      });
    if (collection.state !== "complete")
      throw new Error(
        `real FUSE collection did not resume to completion ${JSON.stringify(collection)}`,
      );
    sampleMemory();
    return { collection, metrics: handle.provider.metrics.snapshot(), peakRssBytes };
  }
  if (command.command === "final-verify") {
    for (const opened of sessions.values()) opened.closeSync();
    sessions.clear();
    handle.provider.syncSync();
    const collection = await handle.filesystem.maintenance.collectGarbage({
      runId: "m7-real-fuse-interrupted-collection",
      maxBatches: 0,
    });
    if (collection.state !== "complete")
      throw new Error("real FUSE final verification lost the completed collection");
    let finalCollection = await handle.filesystem.maintenance.collectGarbage({
      runId: "m7-real-fuse-final-collection",
      maxBatches: 8,
    });
    for (let call = 0; call < 5_000 && finalCollection.state !== "complete"; call += 1)
      finalCollection = await handle.filesystem.maintenance.collectGarbage({
        runId: "m7-real-fuse-final-collection",
        maxBatches: 8,
      });
    if (finalCollection.state !== "complete")
      throw new Error("real FUSE final collection did not complete");
    const verification = await verifyAll();
    const storage = await handle.filesystem.maintenance.snapshotStorage();
    if (storage.state !== "complete")
      throw new Error("real FUSE storage snapshot did not complete");
    const state = durableState();
    if (
      state.active.leases !== 0 ||
      state.active.staging !== 0 ||
      state.active.reservations !== 0
    )
      throw new Error(`real FUSE durable state leaked ${JSON.stringify(state)}`);
    sampleMemory();
    return {
      collection,
      finalCollection,
      verification,
      storage,
      activeDurableState: state.active,
      usage: state.usage,
      usageVerified: true,
      metrics: handle.provider.metrics.snapshot(),
      peakRssBytes,
      physical: database.physicalStorage?.(),
      transactionCount,
    };
  }
  throw new Error(`unknown real FUSE control command ${command.command}`);
}

async function stop() {
  if (stopping) return;
  stopping = true;
  for (const opened of sessions.values()) opened.closeSync();
  sessions.clear();
  await unmount();
  const metrics = handle.provider.metrics.snapshot();
  const physical = database.physicalStorage?.();
  await handle.close();
  database.close();
  writeMessage({
    kind: "stopped",
    metrics,
    peakRssBytes,
    physical,
    transactionCount,
    mountedPayloadOneByteWriteCallbacks,
  });
}

await mount();
const identity = database.transaction(
  "read",
  (tx) =>
    tx.all(
      "SELECT sqlite_version() sqlite,m.schema_version schemaVersion FROM efs_meta m WHERE m.singleton=1",
      [],
      { maxRows: 1, maxBytes: 512 },
    )[0],
);
writeMessage({
  kind: "ready",
  pid: process.pid,
  fuseVersion,
  mountpoint,
  environment: {
    platform: process.platform,
    architecture: process.arch,
    node: process.version,
    kernel: os.release(),
    cpu: os.cpus()[0]?.model ?? "unknown",
    totalMemoryBytes: os.totalmem(),
    uid: process.getuid?.() ?? -1,
  },
  sqlite: identity.sqlite,
  schemaVersion: identity.schemaVersion,
  sqliteCapabilities: database.capabilities,
  filesystemCapabilities: handle.filesystem.capabilities,
  providerCapabilities: handle.provider.capabilities,
});

const lines = readline.createInterface({ input: process.stdin });
lines.on("line", (line) => {
  if (line.trim() === "stop") {
    void stop()
      .then(() => setTimeout(() => process.exit(0), 50))
      .catch((error) => {
        console.error(error);
        process.exit(1);
      });
    return;
  }
  let command;
  try {
    command = JSON.parse(line);
  } catch (error) {
    console.error(error);
    return;
  }
  controlPending = controlPending.then(async () => {
    try {
      writeMessage({ kind: command.id, ...(await control(command)) });
    } catch (error) {
      writeMessage({
        kind: command.id,
        error: error instanceof Error ? (error.stack ?? String(error)) : String(error),
      });
    }
  });
});
for (const signal of ["SIGINT", "SIGTERM"])
  process.on(signal, () => {
    void stop().finally(() => process.exit(1));
  });

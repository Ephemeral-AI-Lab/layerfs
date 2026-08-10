// M2 engine-level mini benchmark (see docs/benchmarks/m2-minibench.md).
//
// Measures the SQLite storage engine directly through the public EphemeralFS
// API: no FUSE, no page cache, no base filesystem. Deterministic fixtures are
// generated from a recorded seed; every cell writes one machine-readable JSON
// artifact per the efs-benchmark-result-v1 schema (docs/benchmarks/
// release-benchmarks.md section 16) and prints a summary table.
//
// Budget: the full matrix must finish under 120 seconds. The A6 edit loop and
// the whole run enforce hard wall budgets; a cell that cannot finish its full
// work within its budget records pass=false plus the deviation instead of
// silently degrading the measurement.
//
// Usage:
//   node tests/performance/mini-bench.mjs            full matrix
//   node tests/performance/mini-bench.mjs --cell A1  single cell
//   node tests/performance/mini-bench.mjs --artifacts <dir>

import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { performance } from "node:perf_hooks";
import { createHash } from "node:crypto";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { EphemeralFS } from "../../packages/fs/dist/index.js";

const ROOT = path.resolve(import.meta.dirname, "..", "..");
const MIB = 1024 * 1024;
const SEED = 0x5eed_5eed;
const BIG_SIZE = 100 * MIB;
const SMALL_COUNT = 100;
const SMALL_SIZE = MIB;
const BIG_PATH = "/big";
const SMALL_PREFIX = "/small";
const OVERALL_WALL_BUDGET_MS = 115_000;
const A6_EDIT_BUDGET_MS = 8_000;
const A6_EDIT_COUNT = 1_000;
const A6_READ_COUNT = 500;
const WRITE_STREAM_CHUNK = 4 * MIB;

function mulberry32(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = state;
    value = Math.imul(value ^ (value >>> 15), value | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    return ((value ^ (value >>> 14)) >>> 0) / 4294967296;
  };
}

function deterministicBytes(seed, length) {
  const random = mulberry32(seed);
  const bytes = new Uint8Array(length);
  for (let offset = 0; offset < length; offset += 4) {
    const value = (random() * 0x100000000) >>> 0;
    const end = Math.min(length, offset + 4);
    for (let index = offset; index < end; index += 1)
      bytes[index] = (value >>> ((index - offset) * 8)) & 0xff;
  }
  return bytes;
}

function sha256Hex(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function bytesStream(bytes, chunkBytes = WRITE_STREAM_CHUNK) {
  let offset = 0;
  return new ReadableStream({
    pull(controller) {
      if (offset >= bytes.length) {
        controller.close();
        return;
      }
      const end = Math.min(bytes.length, offset + chunkBytes);
      controller.enqueue(bytes.subarray(offset, end));
      offset = end;
    },
  });
}

function countingDriver(driver, observed) {
  return {
    kind: driver.kind,
    readOnly: driver.readOnly,
    capabilities: driver.capabilities,
    hashBytes: driver.hashBytes,
    physicalStorage: () => driver.physicalStorage() ?? {},
    checkpoint: (mode) => driver.checkpoint?.(mode),
    close: () => driver.close(),
    transaction(mode, callback) {
      observed.transactions += 1;
      return driver.transaction(mode, (tx) =>
        callback({
          scope: tx.scope,
          run(sql, bindings) {
            observed.statements += 1;
            return tx.run(sql, bindings);
          },
          all(sql, bindings, budget) {
            observed.statements += 1;
            return tx.all(sql, bindings, budget);
          },
        }),
      );
    },
  };
}

function freshObserved() {
  return { transactions: 0, statements: 0 };
}

function physicalBytes(driver) {
  const physical = driver.physicalStorage() ?? {};
  return (physical.mainFileBytes ?? 0) + (physical.walBytes ?? 0);
}

async function openDriver(filename) {
  return openNodeSqlite({
    filename,
    durability: "acknowledged",
    cacheTargetBytes: 16 * MIB,
    mmapLimitBytes: 0,
  });
}

async function openFilesystem(driver, observed, observer) {
  return EphemeralFS.open({
    database: countingDriver(driver, observed),
    observer,
    ownsDatabase: false,
  });
}

function makeObserver() {
  const state = { peakManagedBytes: 0, peakHeapBytes: 0 };
  let sampling = false;
  const timer = setInterval(() => {
    if (!sampling) return;
    const heap = process.memoryUsage().heapUsed;
    if (heap > state.peakHeapBytes) state.peakHeapBytes = heap;
  }, 10);
  timer.unref();
  return {
    state,
    begin() {
      state.peakManagedBytes = 0;
      state.peakHeapBytes = process.memoryUsage().heapUsed;
      sampling = true;
    },
    end() {
      sampling = false;
      const heap = process.memoryUsage().heapUsed;
      if (heap > state.peakHeapBytes) state.peakHeapBytes = heap;
    },
    event(event) {
      if (event.type !== "operation") return;
      if (event.counters.peakManagedResidentBytes > state.peakManagedBytes)
        state.peakManagedBytes = event.counters.peakManagedResidentBytes;
    },
  };
}

function gitHead() {
  const result = spawnSync("git", ["rev-parse", "HEAD"], { cwd: ROOT });
  if (result.status !== 0) return "unknown";
  return result.stdout.toString().trim();
}

async function collectBytes(stream) {
  let total = 0;
  const reader = stream.getReader();
  while (true) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
  }
  return total;
}

async function measureCell(cell, driver, observed, observer, run) {
  const physicalBefore = physicalBytes(driver);
  const statementsBefore = observed.statements;
  const transactionsBefore = observed.transactions;
  observer.begin();
  const started = performance.now();
  const runResult = await run();
  const wallMs = performance.now() - started;
  observer.end();
  const physicalAfter = physicalBytes(driver);
  const counters = {
    wallMs: Math.round(wallMs * 1000) / 1000,
    ...(runResult?.counters ?? {}),
    dbGrowthBytes: physicalAfter - physicalBefore,
    transactions: observed.transactions - transactionsBefore,
    statements: observed.statements - statementsBefore,
    peakManagedResidentBytes: observer.state.peakManagedBytes,
    peakHarnessHeapBytes: observer.state.peakHeapBytes,
  };
  if (runResult?.fixtureBytes !== undefined)
    counters.overheadBasisPoints = Math.round(
      ((physicalAfter - physicalBefore - runResult.fixtureBytes) /
        runResult.fixtureBytes) *
        10000,
    );
  return Object.freeze({ cell, counters, pass: runResult?.pass ?? true });
}

function mibPerSecond(bytes, wallMs) {
  return Math.round((bytes / 1048576 / (wallMs / 1000)) * 10) / 10;
}

function artifactFor(cell, result, configuration, fixtureBytes, pass) {
  return {
    schema: "efs-benchmark-result-v1",
    benchmark: cell,
    commit: gitHead(),
    engine: "ephemeral-ai-fs",
    driver: "sqlite-node",
    fixture: {
      name: `deterministic-seed-${SEED.toString(16)}`,
      sha256: fixtureBytes === undefined ? null : sha256Hex(fixtureBytes),
    },
    configuration: {
      seed: SEED,
      sizeBytes: configuration.sizeBytes,
      ...configuration.extra,
    },
    trials: 1,
    latencyMs: {
      p50: result.counters.wallMs,
      p95: result.counters.wallMs,
      p99: result.counters.wallMs,
    },
    counters: { ...result.counters },
    pass,
  };
}

async function writeArtifacts(artifacts, directory) {
  await mkdir(directory, { recursive: true });
  for (const artifact of artifacts) {
    const filename = path.join(directory, `${artifact.benchmark}.json`);
    await writeFile(filename, `${JSON.stringify(artifact, null, 2)}\n`);
  }
}

function printSummary(artifacts) {
  console.log("\nMini-bench summary (schema efs-benchmark-result-v1):\n");
  console.log(
    "cell             wallMs    MiB/s   dbGrowth     overhead%   stmts   peakManaged",
  );
  for (const artifact of artifacts) {
    const counters = artifact.counters;
    const wall =
      counters.wallMs === undefined ? "      -" : String(counters.wallMs).padStart(9);
    const mib =
      counters.mibPerSec === undefined
        ? "     -"
        : String(counters.mibPerSec).padStart(7);
    const growth =
      counters.dbGrowthBytes === undefined
        ? "        -"
        : String(counters.dbGrowthBytes).padStart(11);
    const overhead =
      counters.overheadBasisPoints === undefined
        ? "       -"
        : (counters.overheadBasisPoints / 100).toFixed(2).padStart(8);
    const stmts =
      counters.statements === undefined
        ? "     -"
        : String(counters.statements).padStart(8);
    const peak = counters.peakManagedResidentBytes ?? 0;
    console.log(
      `${artifact.benchmark.padEnd(15)}${wall}${mib}${growth}${overhead}${stmts}${String(peak).padStart(14)}`,
    );
  }
  console.log("");
}

async function closeFilesystem(filesystem, driver) {
  await filesystem.close();
  if (driver) driver.close();
}

// A. Big file - 1 x 100 MiB --------------------------------------------------

async function runABigGroup(artifacts, bigBytes) {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-minibench-a-"));
  const dbFile = path.join(directory, "a.db");
  const observed = freshObserved();
  const observer = makeObserver();
  const driver = await openDriver(dbFile);
  const filesystem = await openFilesystem(driver, observed, observer.event);
  try {
    // A1: cold first write.
    const a1 = await measureCell(
      "A1-cold-write",
      driver,
      observed,
      observer,
      async () => {
        await filesystem.writeFile(BIG_PATH, bytesStream(bigBytes), {
          maxBytes: bigBytes.length,
        });
        return { fixtureBytes: bigBytes.length };
      },
    );
    a1.counters.mibPerSec = mibPerSecond(bigBytes.length, a1.counters.wallMs);
    artifacts.push(
      artifactFor("A1-cold-write", a1, { sizeBytes: BIG_SIZE }, bigBytes, true),
    );

    // A2: rewrite identical content (dedup).
    const a2 = await measureCell(
      "A2-rewrite-identical",
      driver,
      observed,
      observer,
      async () => {
        await filesystem.writeFile(BIG_PATH, bytesStream(bigBytes), {
          maxBytes: bigBytes.length,
        });
        return { fixtureBytes: 0 };
      },
    );
    a2.counters.mibPerSec = mibPerSecond(bigBytes.length, a2.counters.wallMs);
    artifacts.push(
      artifactFor("A2-rewrite-identical", a2, { sizeBytes: BIG_SIZE }, bigBytes, true),
    );

    // A3/A4/A5/A6: fresh database, untimed write, then cold read, warm read,
    // one-byte edits, and the scattered edit/read cell.
    const coldObserved = freshObserved();
    const coldObserver = makeObserver();
    const coldDriver = await openDriver(path.join(directory, "a-cold.db"));
    const coldFs = await openFilesystem(coldDriver, coldObserved, coldObserver.event);
    try {
      await coldFs.writeFile(BIG_PATH, bytesStream(bigBytes), {
        maxBytes: bigBytes.length,
      });
      const a3 = await measureCell(
        "A3-cold-read",
        coldDriver,
        coldObserved,
        coldObserver,
        async () => {
          const bytes = await collectBytes(await coldFs.readStream(BIG_PATH));
          return { fixtureBytes: 0, counters: { bytes } };
        },
      );
      a3.counters.mibPerSec = mibPerSecond(a3.counters.bytes, a3.counters.wallMs);
      artifacts.push(
        artifactFor("A3-cold-read", a3, { sizeBytes: BIG_SIZE }, bigBytes, true),
      );

      const a4 = await measureCell(
        "A4-warm-read",
        coldDriver,
        coldObserved,
        coldObserver,
        async () => {
          const bytes = await collectBytes(await coldFs.readStream(BIG_PATH));
          return { fixtureBytes: 0, counters: { bytes } };
        },
      );
      a4.counters.mibPerSec = mibPerSecond(a4.counters.bytes, a4.counters.wallMs);
      artifacts.push(
        artifactFor("A4-warm-read", a4, { sizeBytes: BIG_SIZE }, bigBytes, true),
      );

      const a5 = await measureCell(
        "A5-one-byte-edit",
        coldDriver,
        coldObserved,
        coldObserver,
        async () => {
          const perEdit = [];
          for (const offset of [0, Math.floor(BIG_SIZE / 2), BIG_SIZE - 1]) {
            const started = performance.now();
            await coldFs.replaceRange(BIG_PATH, offset, 1, Uint8Array.of(1));
            perEdit.push(performance.now() - started);
          }
          return {
            fixtureBytes: 0,
            counters: {
              editCount: 3,
              perEditMs: perEdit.map((value) => Math.round(value * 1000) / 1000),
            },
          };
        },
      );
      artifacts.push(
        artifactFor(
          "A5-one-byte-edit",
          a5,
          { sizeBytes: BIG_SIZE, edits: 3 },
          bigBytes,
          true,
        ),
      );

      const a6 = await (async () => {
        const a6Directory = path.join(directory, "a6");
        await mkdir(a6Directory, { recursive: true });
        // Scattered one-byte edits on a fresh 100 MiB file (bounded budget).
        const editObserved = freshObserved();
        const editObserver = makeObserver();
        const editDriver = await openDriver(path.join(a6Directory, "edits.db"));
        const editFs = await openFilesystem(
          editDriver,
          editObserved,
          editObserver.event,
        );
        try {
          await editFs.writeFile(BIG_PATH, bytesStream(bigBytes), {
            maxBytes: bigBytes.length,
          });
          const measured = await measureCell(
            "A6-scattered-edits",
            editDriver,
            editObserved,
            editObserver,
            async () => {
              const scatter = mulberry32(SEED ^ 0xa6);
              let completed = 0;
              const editStarted = performance.now();
              for (let index = 0; index < A6_EDIT_COUNT; index += 1) {
                const offset = Math.floor(scatter() * BIG_SIZE);
                await editFs.replaceRange(BIG_PATH, offset, 1, Uint8Array.of(2));
                completed += 1;
                if (performance.now() - editStarted > A6_EDIT_BUDGET_MS) break;
              }
              return {
                fixtureBytes: 0,
                counters: {
                  completedEdits: completed,
                  scaledEdits:
                    completed < A6_EDIT_COUNT ? A6_EDIT_COUNT - completed : 0,
                },
              };
            },
          );
          const a6Pass = measured.counters.completedEdits >= A6_EDIT_COUNT;
          artifacts.push(
            artifactFor(
              "A6-scattered-edits",
              measured,
              { sizeBytes: BIG_SIZE, edits: A6_EDIT_COUNT },
              bigBytes,
              a6Pass,
            ),
          );
        } finally {
          await closeFilesystem(editFs, editDriver);
        }
        // Small random reads: fresh database, no prior reads, so the content
        // cache is cold and every read is verification-bound.
        const readObserved = freshObserved();
        const readObserver = makeObserver();
        const readDriver = await openDriver(path.join(a6Directory, "reads.db"));
        const readFs = await openFilesystem(
          readDriver,
          readObserved,
          readObserver.event,
        );
        try {
          await readFs.writeFile(BIG_PATH, bytesStream(bigBytes), {
            maxBytes: bigBytes.length,
          });
          const measured = await measureCell(
            "A6-small-reads",
            readDriver,
            readObserved,
            readObserver,
            async () => {
              const scatter = mulberry32(SEED ^ 0xa6);
              let readOps = 0;
              for (let index = 0; index < A6_READ_COUNT; index += 1) {
                const offset = Math.floor(scatter() * (BIG_SIZE - 4096));
                const value = await readFs.readRange(BIG_PATH, {
                  offset,
                  length: 4096,
                });
                if (value.byteLength !== 4096)
                  throw new Error("scattered read returned a partial range");
                readOps += 1;
              }
              return {
                fixtureBytes: 0,
                counters: { smallReadOps: readOps },
              };
            },
          );
          measured.counters.smallReadMsPerOp =
            Math.round((measured.counters.wallMs / A6_READ_COUNT) * 1000) / 1000;
          artifacts.push(
            artifactFor(
              "A6-small-reads",
              measured,
              { sizeBytes: BIG_SIZE, reads: A6_READ_COUNT },
              bigBytes,
              true,
            ),
          );
        } finally {
          await closeFilesystem(readFs, readDriver);
          await rm(a6Directory, { recursive: true, force: true });
        }
      })();
      await closeFilesystem(coldFs, coldDriver);
    } catch (error) {
      await closeFilesystem(coldFs, coldDriver);
      throw error;
    }

    // A7: cold materialization (reopen then read all).
    await filesystem.close();
    driver.close();
    const reopenedObserved = freshObserved();
    const reopenedObserver = makeObserver();
    const reopenedDriver = await openDriver(dbFile);
    const reopenedFs = await openFilesystem(
      reopenedDriver,
      reopenedObserved,
      reopenedObserver.event,
    );
    try {
      const a7 = await measureCell(
        "A7-materialization",
        reopenedDriver,
        reopenedObserved,
        reopenedObserver,
        async () => {
          const bytes = await collectBytes(await reopenedFs.readStream(BIG_PATH));
          return { fixtureBytes: 0, counters: { bytes } };
        },
      );
      a7.counters.mibPerSec = mibPerSecond(a7.counters.bytes, a7.counters.wallMs);
      artifacts.push(
        artifactFor("A7-materialization", a7, { sizeBytes: BIG_SIZE }, bigBytes, true),
      );
    } finally {
      await closeFilesystem(reopenedFs, reopenedDriver);
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

// B. Small files - 100 x 1 MiB ------------------------------------------------

async function runBSmallGroup(artifacts, smallFiles) {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-minibench-b-"));
  const dbFile = path.join(directory, "b.db");
  const observed = freshObserved();
  const observer = makeObserver();
  const driver = await openDriver(dbFile);
  const filesystem = await openFilesystem(driver, observed, observer.event);
  const writeAll = async () => {
    for (let index = 0; index < SMALL_COUNT; index += 1)
      await filesystem.writeFile(`${SMALL_PREFIX}${index}`, smallFiles[index]);
  };
  const readAll = async () => {
    let total = 0;
    for (let index = 0; index < SMALL_COUNT; index += 1) {
      const bytes = await filesystem.readFile(`${SMALL_PREFIX}${index}`);
      total += bytes.byteLength;
    }
    return total;
  };
  try {
    const b1 = await measureCell(
      "B1-cold-write-all",
      driver,
      observed,
      observer,
      async () => {
        await writeAll();
        return { fixtureBytes: SMALL_COUNT * SMALL_SIZE };
      },
    );
    b1.counters.mibPerSec = mibPerSecond(SMALL_COUNT * SMALL_SIZE, b1.counters.wallMs);
    artifacts.push(
      artifactFor(
        "B1-cold-write-all",
        b1,
        { sizeBytes: SMALL_COUNT * SMALL_SIZE, files: SMALL_COUNT },
        smallFiles[0],
        true,
      ),
    );

    const b2 = await measureCell(
      "B2-cold-read-all",
      driver,
      observed,
      observer,
      async () => {
        const bytes = await readAll();
        return { bytes, fixtureBytes: 0, counters: { bytes } };
      },
    );
    b2.counters.mibPerSec = mibPerSecond(b2.counters.bytes, b2.counters.wallMs);
    artifacts.push(
      artifactFor(
        "B2-cold-read-all",
        b2,
        { sizeBytes: SMALL_COUNT * SMALL_SIZE },
        smallFiles[0],
        true,
      ),
    );

    const b3 = await measureCell(
      "B3-warm-read-all",
      driver,
      observed,
      observer,
      async () => {
        const bytes = await readAll();
        return { bytes, fixtureBytes: 0, counters: { bytes } };
      },
    );
    b3.counters.mibPerSec = mibPerSecond(b3.counters.bytes, b3.counters.wallMs);
    artifacts.push(
      artifactFor(
        "B3-warm-read-all",
        b3,
        { sizeBytes: SMALL_COUNT * SMALL_SIZE },
        smallFiles[0],
        true,
      ),
    );

    const b4 = await measureCell(
      "B4-one-byte-edit-per-file",
      driver,
      observed,
      observer,
      async () => {
        for (let index = 0; index < SMALL_COUNT; index += 1)
          await filesystem.replaceRange(
            `${SMALL_PREFIX}${index}`,
            0,
            1,
            Uint8Array.of(3),
          );
        return { fixtureBytes: 0, counters: { editCount: SMALL_COUNT } };
      },
    );
    artifacts.push(
      artifactFor(
        "B4-one-byte-edit-per-file",
        b4,
        { sizeBytes: SMALL_COUNT * SMALL_SIZE, edits: SMALL_COUNT },
        smallFiles[0],
        true,
      ),
    );

    await filesystem.close();
    driver.close();
    const reopenedObserved = freshObserved();
    const reopenedObserver = makeObserver();
    const reopenedDriver = await openDriver(dbFile);
    const reopenedFs = await openFilesystem(
      reopenedDriver,
      reopenedObserved,
      reopenedObserver.event,
    );
    try {
      const b5 = await measureCell(
        "B5-materialization",
        reopenedDriver,
        reopenedObserved,
        reopenedObserver,
        async () => {
          let total = 0;
          for (let index = 0; index < SMALL_COUNT; index += 1) {
            const bytes = await reopenedFs.readFile(`${SMALL_PREFIX}${index}`);
            total += bytes.byteLength;
          }
          return { fixtureBytes: 0, counters: { bytes: total } };
        },
      );
      b5.counters.mibPerSec = mibPerSecond(b5.counters.bytes, b5.counters.wallMs);
      artifacts.push(
        artifactFor(
          "B5-materialization",
          b5,
          { sizeBytes: SMALL_COUNT * SMALL_SIZE },
          smallFiles[0],
          true,
        ),
      );
    } finally {
      await closeFilesystem(reopenedFs, reopenedDriver);
    }
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

// C. Messy workspace - mixed script -------------------------------------------

function mixedScript() {
  const big = deterministicBytes(SEED ^ 0xc1, 4 * MIB);
  const smalls = Array.from({ length: 16 }, (_, index) =>
    deterministicBytes((SEED ^ 0xc2) + index, 256 * 1024),
  );
  const scatter = mulberry32(SEED ^ 0xc3);
  const editOffsets = Array.from({ length: 20 }, () =>
    Math.floor(scatter() * (big.length - 1)),
  );
  return { big, smalls, editOffsets };
}

const C_PHASES = [
  "write-big",
  "write-smalls",
  "one-byte-edits",
  "four-kib-edits",
  "half-mib-edit",
  "range-reads",
  "small-reads",
  "rewrites",
  "full-read",
];

async function runCPhase(filesystem, script, phase) {
  if (phase === "write-big") {
    await filesystem.writeFile(BIG_PATH, bytesStream(script.big), {
      maxBytes: script.big.length,
    });
  } else if (phase === "write-smalls") {
    for (let index = 0; index < script.smalls.length; index += 1)
      await filesystem.writeFile(`${SMALL_PREFIX}${index}`, script.smalls[index]);
  } else if (phase === "one-byte-edits") {
    for (const offset of script.editOffsets)
      await filesystem.replaceRange(BIG_PATH, offset, 1, Uint8Array.of(4));
  } else if (phase === "four-kib-edits") {
    for (let index = 0; index < 4; index += 1) {
      const offset = Math.floor(((index + 1) * script.big.length) / 5);
      await filesystem.replaceRange(BIG_PATH, offset, 4096, new Uint8Array(4096));
    }
  } else if (phase === "half-mib-edit") {
    await filesystem.replaceRange(
      BIG_PATH,
      Math.floor(script.big.length / 2),
      512 * 1024,
      new Uint8Array(512 * 1024),
    );
  } else if (phase === "range-reads") {
    for (let index = 0; index < 32; index += 1) {
      const offset = Math.floor(((index * 7) % 8) * script.big.length) / 8;
      const value = await filesystem.readRange(BIG_PATH, { offset, length: 64 * 1024 });
      if (value.byteLength !== 64 * 1024) throw new Error("range read partial");
    }
  } else if (phase === "small-reads") {
    for (let index = 0; index < script.smalls.length; index += 1) {
      const bytes = await filesystem.readFile(`${SMALL_PREFIX}${index}`);
      if (bytes.byteLength !== script.smalls[index].length)
        throw new Error("small read length mismatch");
    }
  } else if (phase === "rewrites") {
    await filesystem.writeFile(BIG_PATH, bytesStream(script.big), {
      maxBytes: script.big.length,
    });
    for (let index = 0; index < 4; index += 1)
      await filesystem.writeFile(`${SMALL_PREFIX}${index}`, script.smalls[index]);
  } else if (phase === "full-read") {
    const bytes = await collectBytes(await filesystem.readStream(BIG_PATH));
    if (bytes !== script.big.length) throw new Error("full read length mismatch");
  } else {
    throw new Error(`unknown C phase: ${phase}`);
  }
}

async function runCMixedGroup(artifacts, script) {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-minibench-c-"));
  const dbFile = path.join(directory, "c.db");
  const observed = freshObserved();
  const observer = makeObserver();
  const driver = await openDriver(dbFile);
  const filesystem = await openFilesystem(driver, observed, observer.event);
  try {
    const c1 = await measureCell(
      "C1-mixed-script",
      driver,
      observed,
      observer,
      async () => {
        const phaseDbBytes = [];
        for (const phase of C_PHASES) {
          await runCPhase(filesystem, script, phase);
          phaseDbBytes.push(physicalBytes(driver));
        }
        const counters = { phaseCount: phaseDbBytes.length };
        for (let phase = 0; phase < phaseDbBytes.length; phase += 1)
          counters[`dbBytesAtPhase${phase}`] = phaseDbBytes[phase];
        return { fixtureBytes: 0, counters };
      },
    );
    const nativePayloadBytes =
      script.big.length +
      script.smalls.reduce((sum, bytes) => sum + bytes.length, 0) +
      8 * 4096 +
      512 * 1024 +
      script.big.length +
      4 * script.smalls[0].length;
    c1.counters.nativePayloadBytes = nativePayloadBytes;
    c1.counters.nativeRatioBasisPoints = Math.round(
      (c1.counters.dbGrowthBytes / nativePayloadBytes) * 10000,
    );
    artifacts.push(
      artifactFor("C1-mixed-script", c1, { sizeBytes: 4 * MIB }, undefined, true),
    );

    const c2 = await measureCell(
      "C2-mixed-script-warm",
      driver,
      observed,
      observer,
      async () => {
        for (const phase of C_PHASES) await runCPhase(filesystem, script, phase);
        return { fixtureBytes: 0 };
      },
    );
    artifacts.push(
      artifactFor("C2-mixed-script-warm", c2, { sizeBytes: 4 * MIB }, undefined, true),
    );

    const c3Counters = {
      nativePayloadBytes,
      finalDbBytes: c1.counters[`dbBytesAtPhase${C_PHASES.length - 1}`],
      ratioBasisPoints: Math.round(
        (c1.counters[`dbBytesAtPhase${C_PHASES.length - 1}`] / nativePayloadBytes) *
          10000,
      ),
    };
    for (let phase = 0; phase < c1.counters.phaseCount; phase += 1)
      c3Counters[`dbBytesAtPhase${phase}`] = c1.counters[`dbBytesAtPhase${phase}`];
    const c3 = Object.freeze({
      cell: "C3-storage-evolution",
      counters: c3Counters,
      pass: true,
    });
    artifacts.push(
      artifactFor("C3-storage-evolution", c3, { sizeBytes: 4 * MIB }, undefined, true),
    );
  } finally {
    await closeFilesystem(filesystem, driver);
    await rm(directory, { recursive: true, force: true });
  }
}

async function main() {
  const onlyCell = process.argv
    .find((value) => value.startsWith("--cell="))
    ?.slice("--cell=".length);
  const artifactsDirectory = path.resolve(
    process.argv
      .find((value) => value.startsWith("--artifacts="))
      ?.slice("--artifacts=".length) ??
      path.join(ROOT, "tests", "performance", "artifacts"),
  );
  const started = performance.now();
  const artifacts = [];

  console.log(
    `mini-bench: seed ${SEED.toString(16)}, big=${BIG_SIZE}, small=${SMALL_COUNT}x${SMALL_SIZE}`,
  );
  console.log("fixture generation...");
  const bigBytes = deterministicBytes(SEED, BIG_SIZE);
  const smallFiles = Array.from({ length: SMALL_COUNT }, (_, index) =>
    deterministicBytes((SEED ^ 0xbeef) + index, SMALL_SIZE),
  );
  const script = mixedScript();

  if (onlyCell) {
    const groups = {
      A1: () => runABigGroup(artifacts, bigBytes),
      B1: () => runBSmallGroup(artifacts, smallFiles),
      C1: () => runCMixedGroup(artifacts, script),
    };
    const runner = groups[onlyCell];
    if (!runner) throw new Error(`--cell must be one of A1, B1, C1; got ${onlyCell}`);
    await runner();
  } else {
    await runABigGroup(artifacts, bigBytes);
    await runBSmallGroup(artifacts, smallFiles);
    await runCMixedGroup(artifacts, script);
  }

  await writeArtifacts(artifacts, artifactsDirectory);
  printSummary(artifacts);
  const wallMs = Math.round(performance.now() - started);
  console.log(`mini-bench total: ${wallMs} ms (budget ${OVERALL_WALL_BUDGET_MS} ms)`);
  const overBudget = wallMs > OVERALL_WALL_BUDGET_MS;
  console.log(overBudget ? "mini-bench: OVER BUDGET" : "mini-bench: within budget");
  process.exitCode = overBudget ? 1 : 0;
}

await main();

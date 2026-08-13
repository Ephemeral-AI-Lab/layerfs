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
//   node tests/performance/mini-bench.mjs --trials 3 timed cells run 3 times
//   node tests/performance/mini-bench.mjs --cell D1  concurrency sweep (D1-D3)

import { spawnSync } from "node:child_process";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { performance } from "node:perf_hooks";
import { createHash } from "node:crypto";
import { format, resolveConfig } from "prettier";
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
// M3.2: the local-rebuild gate is 500 scattered edits in <=20 s (pass=true);
// the M2-era 8 s budget capped A6 at 2 edits and pass=false by design.
const A6_EDIT_BUDGET_MS = 20_000;
const A6_EDIT_COUNT = 500;
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
            if (observed.sqlCounts) {
              const key = sql.replace(/\s+/gu, " ").trim();
              observed.sqlCounts.set(key, (observed.sqlCounts.get(key) ?? 0) + 1);
            }
            return tx.run(sql, bindings);
          },
          all(sql, bindings, budget) {
            observed.statements += 1;
            if (observed.sqlCounts) {
              const key = sql.replace(/\s+/gu, " ").trim();
              observed.sqlCounts.set(key, (observed.sqlCounts.get(key) ?? 0) + 1);
            }
            return tx.all(sql, bindings, budget);
          },
        }),
      );
    },
  };
}

function freshObserved() {
  return {
    transactions: 0,
    statements: 0,
    ...(process.env.EFS_TRACE_SQL === "1" ? { sqlCounts: new Map() } : {}),
  };
}

function physicalBytes(driver) {
  const physical = driver.physicalStorage() ?? {};
  return (physical.mainFileBytes ?? 0) + (physical.walBytes ?? 0);
}

async function openDriver(filename) {
  return openNodeSqlite({
    filename,
    durability: "acknowledged",
    // M3.1: the 2 MiB read windows span ~520 4 KiB pages per pull; the page
    // cache must hold a working window set, aligned with the engine's 64 MiB
    // content cache. The M2-era 16 MiB profile left every pull page-cache
    // cold and capped reads near 175 MiB/s.
    cacheTargetBytes: 64 * MIB,
    mmapLimitBytes: 0,
  });
}

async function openFilesystem(driver, observed, observer, managedBytes = 192 * MIB) {
  return EphemeralFS.open({
    database: countingDriver(driver, observed),
    observer,
    ownsDatabase: false,
    // M3.1: the warm-read gate (A4 >= 1.2x A3) requires the content cache to
    // hold the whole 100 MiB fixture; the M2-era 64 MiB default evicts during
    // the cold pass and leaves the "warm" pass indistinguishable.
    runtime: {
      maxCacheBytes: 128 * MIB,
      maxManagedResidentBytes: managedBytes,
    },
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
  const rev = spawnSync("git", ["rev-parse", "HEAD"], { cwd: ROOT });
  if (rev.status !== 0) {
    // Some restricted runners deny child-process creation even though the
    // benchmark itself is allowed to run. Resolve HEAD from the ref files so
    // artifacts still identify the measured tree; the fallback is explicitly
    // dirty because status cannot be queried safely in that environment.
    try {
      const head = readFileSync(path.join(ROOT, ".git", "HEAD"), "utf8").trim();
      const ref = head.startsWith("ref: ") ? head.slice(5) : undefined;
      const commit = ref
        ? readFileSync(path.join(ROOT, ".git", ref), "utf8").trim()
        : head;
      if (/^[0-9a-f]{40}$/u.test(commit)) return { commit, dirty: true };
    } catch {}
    return { commit: "unknown", dirty: true };
  }
  const status = spawnSync("git", ["status", "--porcelain"], { cwd: ROOT });
  return {
    commit: rev.stdout.toString().trim(),
    dirty: status.status === 0 && status.stdout.toString().trim().length > 0,
  };
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

function percentile(sorted, fraction) {
  if (sorted.length === 0) return undefined;
  const index = Math.min(sorted.length - 1, Math.floor((sorted.length - 1) * fraction));
  return sorted[index];
}

async function measureCell(
  cell,
  driver,
  observed,
  observer,
  run,
  { trials = 1, beforeTrial } = {},
) {
  const trialResults = [];
  for (let trial = 0; trial < trials; trial += 1) {
    if (beforeTrial) await beforeTrial();
    observed.sqlCounts?.clear();
    const trialStatementsBefore = observed.statements;
    const trialTransactionsBefore = observed.transactions;
    const trialPhysicalBefore = physicalBytes(driver);
    observer.begin();
    const started = performance.now();
    const runResult = await run();
    if (process.env.EFS_TRACE_SQL === "1" && cell === "A6-scattered-edits") {
      const top = [...(observed.sqlCounts ?? new Map())]
        .sort((left, right) => right[1] - left[1])
        .slice(0, 100);
      console.error("A6 SQL statement counts:", JSON.stringify(top));
    }
    trialResults.push({
      wallMs: performance.now() - started,
      result: runResult,
      statements: observed.statements - trialStatementsBefore,
      transactions: observed.transactions - trialTransactionsBefore,
      physicalBefore: trialPhysicalBefore,
      physicalAfter: physicalBytes(driver),
    });
    observer.end();
  }
  const sorted = [...trialResults].sort((left, right) => left.wallMs - right.wallMs);
  const median = sorted[Math.floor(sorted.length / 2)];
  const counters = {
    wallMs: Math.round(median.wallMs * 1000) / 1000,
    ...(median.result?.counters ?? {}),
    dbGrowthBytes: median.physicalAfter - median.physicalBefore,
    transactions: median.transactions,
    statements: median.statements,
    peakManagedResidentBytes: observer.state.peakManagedBytes,
    peakHarnessHeapBytes: observer.state.peakHeapBytes,
  };
  if (median.result?.fixtureBytes !== undefined && median.result.fixtureBytes > 0)
    counters.overheadBasisPoints = Math.round(
      ((median.physicalAfter - median.physicalBefore - median.result.fixtureBytes) /
        median.result.fixtureBytes) *
        10000,
    );
  return Object.freeze({
    cell,
    counters,
    pass: median.result?.pass ?? true,
    trials,
    latencyMs: Object.freeze({
      p50: Math.round(percentile(sorted, 0.5).wallMs * 1000) / 1000,
      p95: Math.round(percentile(sorted, 0.95).wallMs * 1000) / 1000,
      p99: Math.round(percentile(sorted, 0.99).wallMs * 1000) / 1000,
    }),
  });
}

function mibPerSecond(bytes, wallMs) {
  return Math.round((bytes / 1048576 / (wallMs / 1000)) * 10) / 10;
}

function artifactFor(cell, result, configuration, fixtureBytes, pass) {
  const head = gitHead();
  return {
    schema: "efs-benchmark-result-v1",
    benchmark: cell,
    commit: head.commit,
    worktreeDirty: head.dirty,
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
    trials: result.trials,
    latencyMs: result.latencyMs,
    counters: { ...result.counters },
    pass,
  };
}

async function writeArtifacts(artifacts, directory) {
  await mkdir(directory, { recursive: true });
  const options = (await resolveConfig(path.join(ROOT, "package.json"))) ?? {};
  for (const artifact of artifacts) {
    const filename = path.join(directory, `${artifact.benchmark}.json`);
    const contents = await format(JSON.stringify(artifact), {
      ...options,
      filepath: filename,
    });
    await writeFile(filename, contents);
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

async function removeTree(target, attempts = 20) {
  let lastError;
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    try {
      await rm(target, { recursive: true, force: true });
      return;
    } catch (error) {
      lastError = error;
      if (attempt === attempts - 1) {
        try {
          const { readdir } = await import("node:fs/promises");
          console.error(`removeTree: giving up on ${target}: ${error.code}`);
          for (const name of await readdir(target)) console.error(`  remains: ${name}`);
        } catch {}
      }
      await new Promise((resolve) =>
        setTimeout(resolve, Math.min(500, 50 * 2 ** attempt)),
      );
    }
  }
  throw lastError;
}

async function closeFilesystem(filesystem, driver) {
  const errors = [];
  try {
    await filesystem.close();
  } catch (error) {
    errors.push(error);
  }
  try {
    if (driver) await driver.close();
  } catch (error) {
    errors.push(error);
  }
  if (errors.length === 1) throw errors[0];
  if (errors.length > 1)
    throw new AggregateError(errors, "filesystem and SQLite driver close failed");
}

// A. Big file - 1 x 100 MiB --------------------------------------------------

async function runABigGroup(artifacts, bigBytes, trials = 1) {
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
      { trials },
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
      { trials },
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
    let coldFs = await openFilesystem(coldDriver, coldObserved, coldObserver.event);
    const coldReopen = async () => {
      await coldFs.close();
      coldFs = await openFilesystem(coldDriver, coldObserved, coldObserver.event);
    };
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
        { trials, beforeTrial: trials > 1 ? coldReopen : undefined },
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
        { trials },
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
        { trials },
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
            { trials },
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
            { trials },
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
          await removeTree(a6Directory);
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
    let reopenedFs = await openFilesystem(
      reopenedDriver,
      reopenedObserved,
      reopenedObserver.event,
    );
    const reopenedCold = async () => {
      await reopenedFs.close();
      reopenedFs = await openFilesystem(
        reopenedDriver,
        reopenedObserved,
        reopenedObserver.event,
      );
    };
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
        { trials, beforeTrial: trials > 1 ? reopenedCold : undefined },
      );
      a7.counters.mibPerSec = mibPerSecond(a7.counters.bytes, a7.counters.wallMs);
      artifacts.push(
        artifactFor("A7-materialization", a7, { sizeBytes: BIG_SIZE }, bigBytes, true),
      );
    } finally {
      await closeFilesystem(reopenedFs, reopenedDriver);
    }
  } finally {
    // Keep the primary database out of the directory-removal race even when
    // an earlier cell or its nested cleanup fails. closeFilesystem is
    // intentionally idempotent, so this also covers the normal A7 path.
    await closeFilesystem(filesystem, driver);
    await removeTree(directory);
  }
}

// B. Small files - 100 x 1 MiB ------------------------------------------------

async function runBSmallGroup(artifacts, smallFiles, trials = 1) {
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
      { trials },
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
      { trials },
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
      { trials },
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
      { trials },
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
        { trials },
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
    await removeTree(directory);
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

async function runCMixedGroup(artifacts, script, trials = 1) {
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
      { trials },
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
      { trials },
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
    await removeTree(directory);
  }
}

// D. Concurrency sweep -------------------------------------------------------

// 100 x 1 MiB files written, read, and one-byte edited in batches of 1, 5, 10,
// or 20 concurrent operations. Each D1 cell writes fresh content so the write
// cells stay cold; D2 reads and D3 edits run against the fixture files. D3 also
// has a focused c100 cell: one Promise.all of all 100 independent edits on the
// same database, without adding c100 to the write/read memory-heavy sweep.
const CONCURRENCY_LEVELS = [1, 5, 10, 20];
const D_COUNT = SMALL_COUNT;
const D_SIZE = SMALL_SIZE;

function concurrencyBatches(count, level) {
  const batches = [];
  for (let start = 0; start < count; start += level)
    batches.push([start, Math.min(count, start + level)]);
  return batches;
}

async function runDGroup(artifacts, smallFiles, trials = 1) {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-minibench-d-"));
  const dbFile = path.join(directory, "d.db");
  const observed = freshObserved();
  const observer = makeObserver();
  const driver = await openDriver(dbFile);
  // 20 concurrent write pipelines each reserve ~18 MiB of managed memory, so
  // the concurrency sweep runs on its own larger managed envelope.
  const filesystem = await openFilesystem(driver, observed, observer.event, 1024 * MIB);
  try {
    // Untimed fixture write (the D2/D3 fixture).
    for (let index = 0; index < D_COUNT; index += 1)
      await filesystem.writeFile(`${SMALL_PREFIX}${index}`, smallFiles[index]);
    for (const level of CONCURRENCY_LEVELS) {
      const runs = concurrencyBatches(D_COUNT, level);
      const freshContent = (index) =>
        deterministicBytes((SEED ^ 0xbeef) + index + level * 0x10000, D_SIZE);
      const d1 = await measureCell(
        `D1-write-c${level}`,
        driver,
        observed,
        observer,
        async () => {
          for (const [start, end] of runs)
            await Promise.all(
              Array.from({ length: end - start }, (_, i) =>
                filesystem.writeFile(
                  `${SMALL_PREFIX}${start + i}`,
                  freshContent(start + i),
                ),
              ),
            );
          return {
            fixtureBytes: D_COUNT * D_SIZE,
            counters: { operations: D_COUNT, concurrency: level, batches: runs.length },
          };
        },
        { trials },
      );
      d1.counters.mibPerSec = mibPerSecond(D_COUNT * D_SIZE, d1.counters.wallMs);
      artifacts.push(
        artifactFor(
          `D1-write-c${level}`,
          d1,
          { sizeBytes: D_COUNT * D_SIZE, files: D_COUNT, concurrency: level },
          smallFiles[0],
          true,
        ),
      );
      const d2 = await measureCell(
        `D2-read-c${level}`,
        driver,
        observed,
        observer,
        async () => {
          let total = 0;
          for (const [start, end] of runs) {
            const values = await Promise.all(
              Array.from({ length: end - start }, (_, i) =>
                filesystem.readFile(`${SMALL_PREFIX}${start + i}`),
              ),
            );
            total += values.reduce((sum, bytes) => sum + bytes.byteLength, 0);
          }
          return {
            fixtureBytes: 0,
            counters: {
              operations: D_COUNT,
              concurrency: level,
              batches: runs.length,
              bytes: total,
            },
          };
        },
        { trials },
      );
      d2.counters.mibPerSec = mibPerSecond(D_COUNT * D_SIZE, d2.counters.wallMs);
      artifacts.push(
        artifactFor(
          `D2-read-c${level}`,
          d2,
          { sizeBytes: D_COUNT * D_SIZE, files: D_COUNT, concurrency: level },
          smallFiles[0],
          true,
        ),
      );
      const d3 = await measureCell(
        `D3-edit-c${level}`,
        driver,
        observed,
        observer,
        async () => {
          for (const [start, end] of runs)
            await Promise.all(
              Array.from({ length: end - start }, (_, i) =>
                filesystem.replaceRange(
                  `${SMALL_PREFIX}${start + i}`,
                  0,
                  1,
                  Uint8Array.of(3),
                ),
              ),
            );
          return {
            fixtureBytes: 0,
            counters: { operations: D_COUNT, concurrency: level, batches: runs.length },
          };
        },
        { trials },
      );
      artifacts.push(
        artifactFor(
          `D3-edit-c${level}`,
          d3,
          { sizeBytes: D_COUNT * D_SIZE, files: D_COUNT, concurrency: level },
          smallFiles[0],
          true,
        ),
      );
    }
    // Reset the same database outside the timed interval so c100 measures
    // actual one-byte changes rather than repeated writes of an already-set
    // byte after the c1/c5/c10/c20 cells.
    for (let index = 0; index < D_COUNT; index += 1)
      await filesystem.writeFile(`${SMALL_PREFIX}${index}`, smallFiles[index]);
    const focusedLevel = D_COUNT;
    const focused = await measureCell(
      "D3-edit-c100",
      driver,
      observed,
      observer,
      async () => {
        const started = performance.now();
        await Promise.all(
          Array.from({ length: D_COUNT }, (_, index) =>
            filesystem.replaceRange(`${SMALL_PREFIX}${index}`, 0, 1, Uint8Array.of(4)),
          ),
        );
        return {
          fixtureBytes: 0,
          counters: {
            operations: D_COUNT,
            concurrency: focusedLevel,
            batches: 1,
            batchWallMs: performance.now() - started,
          },
        };
      },
      { trials },
    );
    artifacts.push(
      artifactFor(
        "D3-edit-c100",
        focused,
        {
          sizeBytes: D_COUNT * D_SIZE,
          extra: {
            files: D_COUNT,
            concurrency: focusedLevel,
            batches: 1,
            databaseIsolation: "one-database",
            coalescing: false,
          },
        },
        smallFiles[0],
        true,
      ),
    );
  } finally {
    await closeFilesystem(filesystem, driver);
    await removeTree(directory);
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
  const trials = Number(
    process.argv
      .find((value) => value.startsWith("--trials="))
      ?.slice("--trials=".length) ?? 1,
  );
  if (!Number.isInteger(trials) || trials < 1 || trials > 5)
    throw new Error("--trials must be an integer between 1 and 5");
  const started = performance.now();
  const artifacts = [];

  console.log(
    `mini-bench: seed ${SEED.toString(16)}, big=${BIG_SIZE}, small=${SMALL_COUNT}x${SMALL_SIZE}, trials=${trials}`,
  );
  console.log("fixture generation...");
  const bigBytes = deterministicBytes(SEED, BIG_SIZE);
  const smallFiles = Array.from({ length: SMALL_COUNT }, (_, index) =>
    deterministicBytes((SEED ^ 0xbeef) + index, SMALL_SIZE),
  );
  const script = mixedScript();

  if (onlyCell) {
    const groups = {
      A1: () => runABigGroup(artifacts, bigBytes, trials),
      B1: () => runBSmallGroup(artifacts, smallFiles, trials),
      C1: () => runCMixedGroup(artifacts, script, trials),
      D1: () => runDGroup(artifacts, smallFiles, trials),
    };
    const runner = groups[onlyCell];
    if (!runner)
      throw new Error(`--cell must be one of A1, B1, C1, D1; got ${onlyCell}`);
    await runner();
  } else {
    await runABigGroup(artifacts, bigBytes, trials);
    await runBSmallGroup(artifacts, smallFiles, trials);
    await runCMixedGroup(artifacts, script, trials);
    await runDGroup(artifacts, smallFiles, trials);
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

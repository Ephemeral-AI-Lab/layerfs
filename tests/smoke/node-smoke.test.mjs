import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { performance } from "node:perf_hooks";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { DEFAULT_FASTCDC } from "../../packages/fs/dist/cdc/fastcdc.js";
import {
  DEFAULT_BRANCH_CONFIGURATION,
  DEFAULT_FILESYSTEM_LIMITS,
  DEFAULT_RUNTIME_LIMITS,
  DEFAULT_STORAGE_LIMITS,
  constrainStorageLimits,
} from "../../packages/fs/dist/resources/limits.js";
import { EFS_SCHEMA_VERSION } from "../../packages/fs/dist/sqlite/schema.js";
import { UsageRepository } from "../../packages/fs/dist/sqlite/usage-repository.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { runtimeEnvironment } from "../helpers/runtime-environment.mjs";

const MIB = 1024 * 1024;
const SEED = 0x5eed_5eed;
const PAYLOAD_BYTES = 16 * MIB;
const COW_EDITS = 5000;
const NAMESPACE_OPERATIONS = 2000;
const ACTORS_PER_KIND = 16;
const OPERATIONS_PER_ACTOR = 64;
const STORAGE_OPTIONS = Object.freeze({
  maxGcBatchSize: 64,
  maxQueryBatchSize: 256,
});

function deterministicBytes(length, seed) {
  let state = seed >>> 0;
  const bytes = new Uint8Array(length);
  for (let index = 0; index < length; index += 1) {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    bytes[index] = state & 0xff;
  }
  return bytes;
}

function digest(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

async function digestStream(stream) {
  const hash = createHash("sha256");
  const reader = stream.getReader();
  while (true) {
    const { done, value } = await reader.read();
    if (done) return hash.digest("hex");
    hash.update(value);
  }
}

async function namespaceDescriptors(filesystem, currentPath = "/") {
  const descriptors = [`${currentPath}|directory`];
  const entries = [...(await filesystem.readdir(currentPath))].sort((left, right) =>
    left.name.localeCompare(right.name),
  );
  for (const entry of entries) {
    const childPath =
      currentPath === "/" ? `/${entry.name}` : `${currentPath}/${entry.name}`;
    if (entry.isDirectory()) {
      descriptors.push(...(await namespaceDescriptors(filesystem, childPath)));
    } else if (entry.isSymbolicLink()) {
      descriptors.push(`${childPath}|symlink|${await filesystem.readlink(childPath)}`);
    } else {
      const stat = await filesystem.lstat(childPath);
      descriptors.push(
        `${childPath}|file|${stat.size}|${stat.nlink}|${await digestStream(
          await filesystem.readStream(childPath),
        )}`,
      );
    }
  }
  return descriptors;
}

function expectedNamespaceDescriptors(expectedPayload) {
  const source = new TextEncoder().encode("source");
  const result = [
    "/|directory",
    "/concurrent|directory",
    "/namespace|directory",
    `/namespace/source|file|${source.length}|251|${digest(source)}`,
    "/smoke|directory",
    `/smoke/payload|file|${expectedPayload.length}|1|${digest(expectedPayload)}`,
  ];
  for (let index = 0; index < 250; index += 1) {
    const suffix = index.toString().padStart(4, "0");
    const directoryPath = `/namespace/d-${suffix}`;
    result.push(`${directoryPath}|directory`);
    result.push(`${directoryPath}/hard|file|${source.length}|251|${digest(source)}`);
    result.push(`${directoryPath}/symbolic|symlink|../source`);
  }
  for (let writer = 0; writer < ACTORS_PER_KIND; writer += 1) {
    const bytes = new Uint8Array(OPERATIONS_PER_ACTOR);
    for (let operation = 0; operation < OPERATIONS_PER_ACTOR; operation += 1)
      bytes[operation] = (writer + operation) % 251;
    result.push(`/concurrent/w-${writer}|file|${bytes.length}|1|${digest(bytes)}`);
  }
  return result.sort();
}

function headCommit() {
  const result = spawnSync("git", ["rev-parse", "HEAD"], {
    cwd: path.resolve(import.meta.dirname, "..", ".."),
  });
  assert.equal(
    result.status,
    0,
    "smoke result must identify its implementation commit",
  );
  return result.stdout.toString().trim();
}

test(
  "Node SQLite completes the exact finite integration smoke profile within 60 seconds",
  { timeout: 65_000 },
  async (t) => {
    const started = performance.now();
    const directory = await mkdtemp(path.join(tmpdir(), "efs-node-smoke-"));
    const filename = path.join(directory, "filesystem.db");
    const payload = deterministicBytes(PAYLOAD_BYTES, SEED);
    const expected = payload.slice();
    const expectedDigest = digest(expected);
    let database;
    let filesystem;
    let restarts = 0;
    let peakManagedResidentBytes = 0;
    let phase = "initialization";
    let completedOperationCount = 0;
    let namespaceOperationCount = 0;
    const slowestOperations = [];
    const measured = async (name, callback, { namespace = false } = {}) => {
      const operationStarted = performance.now();
      try {
        const value = await callback();
        completedOperationCount += 1;
        if (namespace) namespaceOperationCount += 1;
        return value;
      } finally {
        slowestOperations.push({
          name,
          elapsedMs: Math.round((performance.now() - operationStarted) * 1000) / 1000,
        });
        slowestOperations.sort((left, right) => right.elapsedMs - left.elapsedMs);
        if (slowestOperations.length > 10) slowestOperations.length = 10;
      }
    };
    const deadlineDiagnostic = setTimeout(
      () =>
        t.diagnostic(
          JSON.stringify({
            failure: true,
            reason: "60-second-deadline",
            seed: SEED,
            phase,
            completedOperationCount,
            namespaceOperationCount,
            slowestOperations,
          }),
        ),
      60_000,
    );
    deadlineDiagnostic.unref();
    const observer = (event) => {
      peakManagedResidentBytes = Math.max(
        peakManagedResidentBytes,
        event.counters.peakManagedResidentBytes ?? 0,
      );
    };
    const open = async (create) => {
      database = await openNodeSqlite({ filename, create });
      filesystem = await EphemeralFS.open({
        database,
        storage: STORAGE_OPTIONS,
        observer,
      });
    };
    const close = async () => {
      await filesystem?.close();
      filesystem = undefined;
      database?.close();
      database = undefined;
    };
    const restart = async () => {
      await close();
      await open(false);
      restarts += 1;
    };

    try {
      phase = "initial-write-and-reopen";
      await open(true);
      await measured("mkdir-smoke", () =>
        filesystem.mkdir("/smoke", { recursive: true }),
      );
      await measured("write-16m-payload", () =>
        filesystem.writeFile("/smoke/payload", payload),
      );
      await measured("restart-after-initial-write", restart);
      assert.equal(
        await measured("digest-after-initial-reopen", async () =>
          digestStream(await filesystem.readStream("/smoke/payload")),
        ),
        expectedDigest,
      );

      phase = "cow-edits";
      const branch = await filesystem.branches.create("smoke-cow");
      for (let index = 0; index < COW_EDITS; index += 1) {
        const group = index % 3;
        const offset =
          group === 0
            ? index % 4096
            : group === 1
              ? (index * 97) % (32 * 4096)
              : (index * 7919) % PAYLOAD_BYTES;
        const value = (index * 17) & 0xff;
        expected[offset] = value;
        await measured("cow-one-byte-edit", () =>
          branch.writeRange("/smoke/payload", offset, Uint8Array.of(value)),
        );
      }
      const publication = await branch.publish({ operationId: "smoke-cow-publish" });
      assert.equal(publication.outcome, "merged");
      await branch.close();

      phase = "namespace-operations";
      await filesystem.mkdir("/namespace", { recursive: true });
      await filesystem.writeFile("/namespace/source", "source");
      for (let index = 0; index < NAMESPACE_OPERATIONS / 8; index += 1) {
        const suffix = index.toString().padStart(4, "0");
        const directoryPath = `/namespace/d-${suffix}`;
        await measured("namespace-mkdir", () => filesystem.mkdir(directoryPath), {
          namespace: true,
        });
        await measured(
          "namespace-create",
          () => filesystem.writeFile(`${directoryPath}/created`, `created-${suffix}`),
          { namespace: true },
        );
        await measured(
          "namespace-stat-created",
          () => filesystem.stat(`${directoryPath}/created`),
          { namespace: true },
        );
        await measured(
          "namespace-rename",
          () =>
            filesystem.rename(`${directoryPath}/created`, `${directoryPath}/renamed`),
          { namespace: true },
        );
        await measured(
          "namespace-hard-link",
          () => filesystem.link("/namespace/source", `${directoryPath}/hard`),
          { namespace: true },
        );
        await measured(
          "namespace-stat-hard-link",
          () => filesystem.stat(`${directoryPath}/hard`),
          { namespace: true },
        );
        await measured(
          "namespace-unlink",
          () => filesystem.unlink(`${directoryPath}/renamed`),
          { namespace: true },
        );
        await measured(
          "namespace-symbolic-link",
          () => filesystem.symlink("../source", `${directoryPath}/symbolic`),
          { namespace: true },
        );
      }
      assert.equal(namespaceOperationCount, NAMESPACE_OPERATIONS);
      await measured("restart-after-namespace", restart);

      phase = "concurrent-readers-and-writers";
      await filesystem.mkdir("/concurrent", { recursive: true });
      const writerBranches = [];
      for (let writer = 0; writer < ACTORS_PER_KIND; writer += 1) {
        await filesystem.writeFile(
          `/concurrent/w-${writer}`,
          new Uint8Array(OPERATIONS_PER_ACTOR),
        );
        writerBranches.push(await filesystem.branches.create(`smoke-writer-${writer}`));
      }
      await Promise.all([
        ...Array.from({ length: ACTORS_PER_KIND }, (_, reader) =>
          (async () => {
            for (let operation = 0; operation < OPERATIONS_PER_ACTOR; operation += 1) {
              const value = await measured("concurrent-reader", () =>
                filesystem.readRange("/namespace/source", {
                  offset: 0,
                  length: 6,
                }),
              );
              assert.equal(value.byteLength, 6, `${reader}:${operation}`);
            }
          })(),
        ),
        ...Array.from({ length: ACTORS_PER_KIND }, (_, writer) =>
          (async () => {
            for (let operation = 0; operation < OPERATIONS_PER_ACTOR; operation += 1)
              await measured("concurrent-writer", () =>
                writerBranches[writer].writeRange(
                  `/concurrent/w-${writer}`,
                  operation,
                  Uint8Array.of((writer + operation) % 251),
                ),
              );
          })(),
        ),
      ]);
      for (let writer = 0; writer < writerBranches.length; writer += 1) {
        const result = await writerBranches[writer].publish({
          operationId: `smoke-writer-publish-${writer}`,
        });
        assert.equal(result.outcome, "merged");
      }
      await Promise.all(writerBranches.map((branch) => branch.close()));

      phase = "interrupted-collection";
      await measured("write-orphan", () =>
        filesystem.writeFile("/orphan", "collect-me"),
      );
      await measured("unlink-orphan", () => filesystem.unlink("/orphan"));
      let collection = await filesystem.maintenance.collectGarbage({
        runId: "smoke-interrupted-collection",
        maxBatches: 1,
      });
      assert.equal(collection.state, "paused");
      await measured("restart-during-collection", restart);
      for (let call = 0; call < 5000 && collection.state !== "complete"; call += 1)
        collection = await filesystem.maintenance.collectGarbage({
          runId: "smoke-interrupted-collection",
          maxBatches: 1,
        });
      assert.equal(collection.state, "complete");
      assert.equal(restarts, 3);

      phase = "final-verification";
      assert.equal(
        await digestStream(await filesystem.readStream("/smoke/payload")),
        digest(expected),
      );
      assert.equal(
        (await filesystem.readFile("/concurrent/w-15"))[63],
        (15 + 63) % 251,
      );
      assert.equal(
        await filesystem.readFile("/namespace/d-0249/hard", { encoding: "utf8" }),
        "source",
      );
      const actualNamespace = (await namespaceDescriptors(filesystem)).sort();
      const expectedNamespace = expectedNamespaceDescriptors(expected);
      assert.deepEqual(actualNamespace, expectedNamespace);
      const namespaceDigest = digest(
        new TextEncoder().encode(actualNamespace.join("\n")),
      );

      let cursor;
      let verification;
      do {
        verification = await filesystem.maintenance.verify({
          cursor,
          maxEntities: 64,
        });
        cursor = verification.nextCursor ?? undefined;
      } while (!verification.complete);
      const snapshot = await filesystem.maintenance.snapshotStorage();
      assert.equal(snapshot.state, "complete");
      database.transaction("read", (tx) => {
        new UsageRepository(
          tx,
          constrainStorageLimits(
            { ...STORAGE_OPTIONS, maxQueryBatchSize: 30_000 },
            database.capabilities,
          ),
        ).verifyDerivedUsage();
        const active = tx.all(
          "SELECT (SELECT count(*) FROM efs_leases WHERE state IN (0,1)) leases,(SELECT count(*) FROM efs_staging_certificates) staging,(SELECT count(*) FROM efs_operation_results WHERE outcome=-1 AND length(encoded)=0) reservations",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0];
        assert.deepEqual(active, { leases: 0, staging: 0, reservations: 0 });
      });

      const elapsedMs = performance.now() - started;
      assert.ok(elapsedMs < 60_000, `Node smoke took ${elapsedMs.toFixed(1)} ms`);
      const sqliteMetadata = database.transaction("read", (tx) => ({
        ...tx.all("SELECT sqlite_version() sqliteVersion", [], {
          maxRows: 1,
          maxBytes: 128,
        })[0],
        ...tx.all("SELECT page_size FROM pragma_page_size", [], {
          maxRows: 1,
          maxBytes: 128,
        })[0],
      }));
      const result = {
        schema: "efs-correctness-result-v1",
        commit: headCommit(),
        adapter: "node-sqlite-smoke",
        driver: "sqlite-node",
        capabilities: database.capabilities,
        limits: {
          ...DEFAULT_FILESYSTEM_LIMITS,
          ...DEFAULT_STORAGE_LIMITS,
          ...DEFAULT_RUNTIME_LIMITS,
          ...DEFAULT_BRANCH_CONFIGURATION,
          ...STORAGE_OPTIONS,
          cowPageBytes: 8192,
          fastCdcMinimumBytes: DEFAULT_FASTCDC.minimum,
          fastCdcAverageBytes: DEFAULT_FASTCDC.average,
          fastCdcMaximumBytes: DEFAULT_FASTCDC.maximum,
          payloadBytes: PAYLOAD_BYTES,
          cowEdits: COW_EDITS,
          namespaceOperations: NAMESPACE_OPERATIONS,
          readers: ACTORS_PER_KIND,
          writers: ACTORS_PER_KIND,
          operationsPerActor: OPERATIONS_PER_ACTOR,
          restarts,
        },
        schemaVersion: EFS_SCHEMA_VERSION,
        formatVersion: "efs-merkle-manifest-v1",
        seed: SEED,
        fixtureDigest: expectedDigest,
        faultPoint: "bounded-collection-after-first-committed-batch",
        commands: ["pnpm test:smoke:built"],
        environment: runtimeEnvironment({
          sqlite: sqliteMetadata.sqliteVersion,
          sqlitePageSize: sqliteMetadata.page_size,
          journalMode: database.capabilities.journalMode,
          cacheTargetBytes: database.capabilities.cacheTargetBytes,
          mmapLimitBytes: database.capabilities.mmapLimitBytes,
          operatingSystemCacheDropAttempted: false,
          operatingSystemCacheDropSucceeded: false,
        }),
        passed: 1,
        failed: 0,
        elapsedMs: Math.round(elapsedMs),
        metrics: {
          peakManagedResidentBytes,
          objectCount: snapshot.objectCount,
          manifestCount: snapshot.manifestCount,
          completedOperationCount,
          namespaceOperationCount,
          namespaceDigest,
          finalPayloadDigest: digest(expected),
          slowestOperations,
        },
      };
      t.diagnostic(JSON.stringify(result));
      if (process.env.EFS_SMOKE_RESULT_PATH)
        await writeFile(
          path.resolve(process.env.EFS_SMOKE_RESULT_PATH),
          `${JSON.stringify(result, null, 2)}\n`,
        );
    } catch (error) {
      t.diagnostic(
        JSON.stringify({
          failure: true,
          seed: SEED,
          phase,
          completedOperationCount,
          namespaceOperationCount,
          slowestOperations,
          error: String(error),
        }),
      );
      throw error;
    } finally {
      clearTimeout(deadlineDiagnostic);
      try {
        await close();
      } catch {}
      await rm(directory, { recursive: true, force: true });
    }
  },
);

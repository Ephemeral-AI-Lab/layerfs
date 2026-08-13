import { openCloudflareSqlite } from "../../packages/sqlite-cloudflare/dist/index.js";
import { CloudflareSQLiteError } from "../../packages/sqlite-cloudflare/dist/index.js";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import {
  PORTABLE_BRANCH_CASE_IDS,
  PORTABLE_CONFORMANCE_CASE_IDS,
  PORTABLE_DRIVER_CASE_IDS,
  PORTABLE_MAINTENANCE_CASE_IDS,
  PORTABLE_RESTART_CASE_IDS,
  PORTABLE_STORAGE_CONFORMANCE_CASE_IDS,
  type PortableFixtureContext,
  PortableStagingCrashSession,
  PortableSmokeSession,
  preparePortableRestart,
  runBranchConformance,
  runFilesystemConformance,
  runMaintenanceConformance,
  runSQLiteDriverConformance,
  runStorageConformance,
  verifyPortableRestart,
} from "../../packages/testkit/dist/index.js";
import { portableStorageInternals } from "./portable-storage-internals.js";
import { env } from "cloudflare:workers";
import { SELF, evictDurableObject, reset, runInDurableObject } from "cloudflare:test";
import { afterEach, expect, test } from "vitest";

afterEach(async () => {
  await reset();
});

function fixtureContextEvidence(adapter: string) {
  return (context: PortableFixtureContext): void => {
    console.log(`m6-suite-context-evidence ${JSON.stringify({ adapter, ...context })}`);
  };
}

function object(name: string) {
  return env.FILESYSTEM.getByName(name);
}

test("faithful runtime exposes SQLite storage and transactionSync directly", async () => {
  const stub = object("driver-contract");
  await runInDurableObject(stub, async (_instance, state) => {
    const probes = [
      "SELECT application_id AS value FROM pragma_application_id",
      "SELECT application_id AS value FROM pragma_application_id()",
      "PRAGMA application_id",
      "PRAGMA main.application_id",
      "PRAGMA application_id = 1161905747",
      "PRAGMA main.application_id = 1161905747",
      "SELECT user_version AS value FROM pragma_user_version",
      "SELECT user_version AS value FROM pragma_user_version()",
      "PRAGMA user_version",
      "PRAGMA main.user_version",
      "PRAGMA user_version = 1",
      "PRAGMA main.user_version = 1",
      "SELECT sqlite_version() AS value",
      "SELECT count(*) AS value FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
      "SELECT count(*) AS value FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
    ];
    const probeResults = probes.map((sql) => {
      try {
        return { sql, rows: state.storage.sql.exec(sql).toArray() };
      } catch (error) {
        return { sql, error: String(error) };
      }
    });
    expect(probeResults.slice(0, -2)).toEqual(
      probes.slice(0, -2).map((sql) => ({
        sql,
        error: sql.startsWith("SELECT sqlite_version")
          ? "Error: not authorized to use function: sqlite_version at offset 7: SQLITE_ERROR"
          : "Error: not authorized: SQLITE_AUTH",
      })),
    );
    expect(probeResults.slice(-2)).toEqual(
      probes.slice(-2).map((sql) => ({ sql, rows: [{ value: 0 }] })),
    );
    let transactionSyncCalls = 0;
    const storage = {
      sql: state.storage.sql,
      transactionSync<T>(callback: () => T): T {
        transactionSyncCalls += 1;
        return state.storage.transactionSync(callback);
      },
    };
    const driver = await openCloudflareSqlite({ storage });
    expect(driver.capabilities).toMatchObject({
      durability: "acknowledged",
      journalMode: "runtime-managed",
      memoryPolicy: "runtime-managed",
      maxBlobBytes: 2 * 1024 * 1024,
      maxBindings: 100,
      physicalQuotaPolicy: "runtime-enforced",
      schemaIdentityMode: "durable-table",
    });
    driver.transaction("write", (tx) => {
      tx.run("CREATE TABLE faithful_runtime(value BLOB NOT NULL)");
      tx.run("INSERT INTO faithful_runtime(value) VALUES(?)", [Uint8Array.of(1, 2, 3)]);
    });
    expect(
      driver.transaction(
        "read",
        (tx) =>
          tx.all("SELECT value FROM faithful_runtime", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0]?.value,
      ),
    ).toEqual(Uint8Array.of(1, 2, 3));
    expect(transactionSyncCalls).toBe(2);
    expect(state.storage.sql.databaseSize).toBeGreaterThan(0);
    console.log(
      `m6-runtime-evidence ${JSON.stringify({
        driver: "sqlite-cloudflare",
        sqliteBuild: "3.47.0",
        sqliteVersionSource: "workerd-v1.20260810.1-MODULE.bazel",
        sqliteVersionQuery: "forbidden-by-runtime-authorizer-SQLITE_ERROR",
        databaseSize: state.storage.sql.databaseSize,
        capabilities: driver.capabilities,
      })}`,
    );
    driver.close();
  });
});

test("the shared SQLite driver contract passes in the faithful runtime", async () => {
  const stub = object("portable-driver-conformance");
  const results = await runInDurableObject(stub, async (_instance, state) =>
    runSQLiteDriverConformance({
      name: "cloudflare-durable-object-driver",
      recordFixtureContext: fixtureContextEvidence("sqlite-cloudflare"),
      async create() {
        let current = await openCloudflareSqlite({ storage: state.storage });
        return {
          adapter: current,
          capabilities: ["physical-reopen"],
          async reopen() {
            current = await openCloudflareSqlite({ storage: state.storage });
            return current;
          },
          async dispose() {
            current.close();
          },
        };
      },
    }),
  );
  expect(results).toEqual(
    PORTABLE_DRIVER_CASE_IDS.map((id) => ({ id, status: "passed" })),
  );
});

test("the adapter delegates atomic rollback and invalidates callback-scoped transactions", async () => {
  const stub = object("driver-transactions");
  await runInDurableObject(stub, async (_instance, state) => {
    const driver = await openCloudflareSqlite({ storage: state.storage });
    driver.transaction("write", (tx) => {
      tx.run("CREATE TABLE transaction_probe(value INTEGER NOT NULL UNIQUE)");
      tx.run("CREATE TABLE parent_probe(id INTEGER PRIMARY KEY)");
      tx.run(
        "CREATE TABLE child_probe(parent_id INTEGER NOT NULL REFERENCES parent_probe(id))",
      );
    });

    expect(() =>
      driver.transaction("write", (tx) => {
        tx.run("INSERT INTO transaction_probe(value) VALUES(?)", [1]);
        throw new Error("rollback sentinel");
      }),
    ).toThrow("rollback sentinel");
    expect(
      driver.transaction(
        "read",
        (tx) =>
          tx.all("SELECT count(*) AS value FROM transaction_probe", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0]?.value,
      ),
    ).toBe(0);
    expect(() =>
      driver.transaction("write", (tx) => {
        tx.run("INSERT INTO child_probe(parent_id) VALUES(?)", [404]);
      }),
    ).toThrow(/SQLITE_CONSTRAINT.*foreign key|SQLITE_CONSTRAINT.*constraint/i);
    try {
      driver.transaction("write", (tx) => {
        tx.run("INSERT INTO child_probe(parent_id) VALUES(?)", [404]);
      });
      throw new Error("expected a normalized constraint failure");
    } catch (error) {
      expect(error).toBeInstanceOf(CloudflareSQLiteError);
      expect(error).toMatchObject({
        category: "constraint",
        code: "SQLITE_CONSTRAINT",
      });
    }

    let retainedTransaction:
      Parameters<Parameters<typeof driver.transaction>[1]>[0] | null = null;
    driver.transaction("read", (tx) => {
      retainedTransaction = tx;
    });
    expect(() =>
      retainedTransaction!.all("SELECT 1 AS value", [], {
        maxRows: 1,
        maxBytes: 128,
      }),
    ).toThrow(/no longer active/i);
    expect(() =>
      driver.transaction("write", () => driver.transaction("read", () => 0)),
    ).toThrow(/nested/i);
    expect(() =>
      driver.transaction("write", async () => Promise.resolve("not synchronous")),
    ).toThrow(/synchronous/i);
    const callbackSentinel = new Error("SQLITE_FULL callback sentinel");
    try {
      driver.transaction("write", () => {
        throw callbackSentinel;
      });
      throw new Error("expected the callback sentinel to be rethrown");
    } catch (error) {
      expect(error).toBe(callbackSentinel);
    }

    driver.close();
    driver.close();
    expect(() => driver.transaction("read", () => 0)).toThrow(/closed/i);

    const reopened = await openCloudflareSqlite({ storage: state.storage });
    expect(
      reopened.transaction(
        "read",
        (tx) =>
          tx.all("SELECT count(*) AS value FROM transaction_probe", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0]?.value,
      ),
    ).toBe(0);
    reopened.close();
  });
});

test("the adapter enforces bounded statements, bindings, values, and results", async () => {
  const stub = object("driver-bounds");
  await runInDurableObject(stub, async (_instance, state) => {
    const driver = await openCloudflareSqlite({ storage: state.storage });
    driver.transaction("write", (tx) => {
      tx.run("CREATE TABLE bounded_probe(id INTEGER PRIMARY KEY,value BLOB NOT NULL)");
      const backing = Uint8Array.of(99, 1, 2, 3, 99);
      tx.run("INSERT INTO bounded_probe(id,value) VALUES(?,?)", [
        1,
        backing.subarray(1, 4),
      ]);
      backing.fill(0);
      tx.run("INSERT INTO bounded_probe(id,value) VALUES(?,?)", [
        2,
        Uint8Array.of(4, 5, 6),
      ]);
    });

    const first = driver.transaction(
      "read",
      (tx) =>
        tx.all("SELECT value FROM bounded_probe WHERE id=?", [1], {
          maxRows: 1,
          maxBytes: 128,
        })[0]!,
    );
    expect(Object.isFrozen(first)).toBe(true);
    expect(first.value).toEqual(Uint8Array.of(1, 2, 3));
    (first.value as Uint8Array)[0] = 200;
    expect(
      driver.transaction(
        "read",
        (tx) =>
          tx.all("SELECT value FROM bounded_probe WHERE id=?", [1], {
            maxRows: 1,
            maxBytes: 128,
          })[0]?.value,
      ),
    ).toEqual(Uint8Array.of(1, 2, 3));

    expect(() =>
      driver.transaction("read", (tx) =>
        tx.all("SELECT value FROM bounded_probe ORDER BY id", [], {
          maxRows: 1,
          maxBytes: 256,
        }),
      ),
    ).toThrow(/row budget/i);
    expect(() =>
      driver.transaction("read", (tx) =>
        tx.all("SELECT value FROM bounded_probe WHERE id=1", [], {
          maxRows: 1,
          maxBytes: 1,
        }),
      ),
    ).toThrow(/byte budget/i);
    expect(() =>
      driver.transaction("read", (tx) =>
        tx.all("SELECT 1 AS value", [], { maxRows: 0, maxBytes: 128 }),
      ),
    ).toThrow(/invalid query budget/i);
    expect(() =>
      driver.transaction("read", (tx) =>
        tx.all("SELECT ? AS value", ["too long"], {
          maxRows: 1,
          maxBytes: 1,
        }),
      ),
    ).toThrow(/binding value/i);
    expect(() =>
      driver.transaction("write", (tx) =>
        tx.run("INSERT INTO bounded_probe(id,value) VALUES(?,?)", [
          3,
          new Uint8Array(driver.capabilities.maxBlobBytes + 1),
        ]),
      ),
    ).toThrow(/BLOB exceeds/i);
    expect(() =>
      driver.transaction("write", (tx) =>
        tx.run("INSERT INTO bounded_probe(id,value) VALUES(?,?)", [
          Number.MAX_SAFE_INTEGER + 1,
          Uint8Array.of(1),
        ]),
      ),
    ).toThrow(/safe integers/i);
    expect(() =>
      driver.transaction("write", (tx) =>
        tx.run("INSERT INTO bounded_probe(id,value) VALUES(1,X'01')", [
          ...Array.from({ length: driver.capabilities.maxBindings + 1 }, () => null),
        ]),
      ),
    ).toThrow(/binding limit/i);

    const acceptedBindings = Array.from(
      { length: driver.capabilities.maxBindings },
      (_, index) => index,
    );
    const acceptedBindingSql = `SELECT ${acceptedBindings
      .map((_, index) => `? AS value_${index}`)
      .join(",")}`;
    expect(
      driver.transaction(
        "read",
        (tx) =>
          tx.all(acceptedBindingSql, acceptedBindings, {
            maxRows: 1,
            maxBytes: 16 * 1024,
          })[0]?.value_99,
      ),
    ).toBe(99);

    const acceptedBlob = new Uint8Array(driver.capabilities.maxBlobBytes);
    acceptedBlob[0] = 17;
    acceptedBlob[acceptedBlob.byteLength - 1] = 29;
    driver.transaction("write", (tx) => {
      tx.run("INSERT INTO bounded_probe(id,value) VALUES(?,?)", [3, acceptedBlob]);
    });
    const acceptedBlobResult = driver.transaction(
      "read",
      (tx) =>
        tx.all("SELECT value FROM bounded_probe WHERE id=3", [], {
          maxRows: 1,
          maxBytes: driver.capabilities.maxBlobBytes + 128,
        })[0]?.value as Uint8Array,
    );
    expect(acceptedBlobResult.byteLength).toBe(driver.capabilities.maxBlobBytes);
    expect(acceptedBlobResult[0]).toBe(17);
    expect(acceptedBlobResult[acceptedBlobResult.byteLength - 1]).toBe(29);

    driver.close();
  });
});

test("the adapter rejects SQL outside the callback-scoped bounded subset", async () => {
  const stub = object("driver-sql-shape");
  await runInDurableObject(stub, async (_instance, state) => {
    const driver = await openCloudflareSqlite({ storage: state.storage });
    driver.transaction("write", (tx) => {
      tx.run("CREATE TABLE shape_probe(id INTEGER PRIMARY KEY,value INTEGER NOT NULL)");
    });

    expect(() =>
      driver.transaction("read", (tx) => tx.run("UPDATE shape_probe SET value=1")),
    ).toThrow(/read-only/i);
    expect(() =>
      driver.transaction("read", (tx) =>
        tx.all("SELECT 1 AS value; DELETE FROM shape_probe", [], {
          maxRows: 1,
          maxBytes: 128,
        }),
      ),
    ).toThrow(/exactly one statement/i);
    expect(() =>
      driver.transaction("write", (tx) =>
        tx.run("INSERT INTO shape_probe(value) VALUES(1); DELETE FROM shape_probe"),
      ),
    ).toThrow(/exactly one statement/i);
    expect(
      driver.transaction(
        "read",
        (tx) =>
          tx.all("SELECT count(*) AS value FROM shape_probe; -- trailing comment", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0]?.value,
      ),
    ).toBe(0);
    expect(() =>
      driver.transaction("write", (tx) => tx.run("SELECT 1 AS value")),
    ).toThrow(/bounded all/i);
    expect(() =>
      driver.transaction("write", (tx) =>
        tx.all("INSERT INTO shape_probe(value) VALUES(1) RETURNING id", [], {
          maxRows: 1,
          maxBytes: 128,
        }),
      ),
    ).toThrow(/read-only/i);
    expect(() =>
      driver.transaction("write", (tx) =>
        tx.run("INSERT INTO shape_probe(value) VALUES(1) RETURNING id"),
      ),
    ).toThrow(/bounded all/i);
    expect(() =>
      driver.transaction("write", (tx) =>
        tx.run(
          "WITH value(id) AS (VALUES(1)) INSERT INTO shape_probe SELECT id FROM value",
        ),
      ),
    ).toThrow(/common-table expressions/i);
    expect(() =>
      driver.transaction("write", (tx) => tx.run("CREATE TEMP TABLE forbidden(id)")),
    ).toThrow(/temporary/i);
    expect(() =>
      driver.transaction("write", (tx) =>
        tx.run("ATTACH DATABASE ':memory:' AS other"),
      ),
    ).toThrow(/callback-scoped/i);
    expect(() =>
      driver.transaction("write", (tx) => tx.run("PRAGMA foreign_keys=ON")),
    ).toThrow(/PRAGMA/i);

    driver.close();
  });
});

test("the adapter normalizes Cloudflare constraint, busy, corruption, and quota failures", async () => {
  const cursor = {
    rowsRead: 0,
    rowsWritten: 0,
    *[Symbol.iterator]() {},
    toArray: () => [],
  };
  const categories = [
    [
      "UNIQUE constraint failed: probe.id: SQLITE_CONSTRAINT",
      "constraint",
      "SQLITE_CONSTRAINT",
    ],
    ["database is locked: SQLITE_BUSY", "busy", "EBUSY"],
    ["database disk image is malformed: SQLITE_CORRUPT", "corruption", "ECORRUPT"],
    ["database or disk is full: SQLITE_FULL", "resource-limit", "ENOSPC"],
  ] as const;
  for (const [message, category, code] of categories) {
    const storage = {
      sql: {
        databaseSize: 0,
        exec(query: string) {
          if (query === "SELECT 1 AS value") throw new Error(message);
          return cursor;
        },
      },
      transactionSync<T>(callback: () => T): T {
        return callback();
      },
    };
    const driver = await openCloudflareSqlite({ storage });
    try {
      driver.transaction("read", (tx) =>
        tx.all("SELECT 1 AS value", [], { maxRows: 1, maxBytes: 128 }),
      );
      throw new Error(`expected normalized ${category} failure`);
    } catch (error) {
      expect(error).toBeInstanceOf(CloudflareSQLiteError);
      expect(error).toMatchObject({ category, code });
      expect(String(error)).toContain(message);
    }
    driver.close();
  }
  const defaultDriver = await openCloudflareSqlite({
    storage: {
      sql: { ...cursor, databaseSize: 0, exec: () => cursor },
      transactionSync<T>(callback: () => T): T {
        return callback();
      },
    },
  });
  expect(defaultDriver.capabilities.maxPhysicalDatabaseBytes).toBe(1_000_000_000);
  expect(defaultDriver.capabilities.maxJournalBytes).toBe(1_000_000_000);
  expect(defaultDriver.capabilities).toMatchObject({
    physicalQuotaPolicy: "runtime-enforced",
    journalQuotaPolicy: "runtime-enforced",
    journalSizeLimitIsHard: false,
  });
  defaultDriver.close();
  const paidDriver = await openCloudflareSqlite({
    storage: {
      sql: { ...cursor, databaseSize: 0, exec: () => cursor },
      transactionSync<T>(callback: () => T): T {
        return callback();
      },
    },
    maxPhysicalDatabaseBytes: 12_000_000_000,
    maxJournalBytes: 10_000_000_000,
  });
  expect(paidDriver.capabilities.maxPhysicalDatabaseBytes).toBe(10_000_000_000);
  expect(paidDriver.capabilities.maxJournalBytes).toBe(10_000_000_000);
  paidDriver.close();
  await expect(
    openCloudflareSqlite({
      storage: {
        sql: { ...cursor, databaseSize: 0, exec: () => cursor },
        transactionSync<T>(callback: () => T): T {
          return callback();
        },
      },
      maxPhysicalDatabaseBytes: 1_000_000_000,
      maxJournalBytes: 256 * 1024 * 1024,
    }),
  ).rejects.toThrow(/no separately enforceable journal quota/i);
});

test(
  "the shared M2 storage suite passes in the faithful runtime",
  { timeout: 120_000 },
  async () => {
    const stub = object("portable-storage-conformance");
    const results = await runInDurableObject(stub, async (_instance, state) =>
      runStorageConformance(
        {
          name: "cloudflare-durable-object-storage",
          recordFixtureContext: fixtureContextEvidence("sqlite-cloudflare"),
          async create() {
            const adapter = await openCloudflareSqlite({ storage: state.storage });
            return {
              adapter,
              capabilities: [],
              async reopen() {
                return openCloudflareSqlite({ storage: state.storage });
              },
              async dispose() {},
            };
          },
        },
        portableStorageInternals,
      ),
    );
    expect(results).toEqual(
      PORTABLE_STORAGE_CONFORMANCE_CASE_IDS.map((id) => ({ id, status: "passed" })),
    );
  },
);

test("partial staging survives eviction after every committed batch and expires", async () => {
  const stub = object("portable-staging-crash-recovery");
  const session = new PortableStagingCrashSession();
  for (;;) {
    const outcome = await runInDurableObject(stub, async (_instance, state) => {
      const adapter = await openCloudflareSqlite({ storage: state.storage });
      return session.run(adapter, portableStorageInternals);
    });
    if (outcome.status === "complete") {
      expect(outcome.result).toEqual({
        schema: "efs-portable-staging-crash-v1",
        batches: 3,
        physicalRestarts: 3,
        recovered: true,
      });
      break;
    }
    await evictDurableObject(stub);
  }
});

test("the shared portable suite passes against the real Durable Object adapter", async () => {
  const stub = object("portable-conformance");
  const results = await runInDurableObject(stub, async (_instance, state) =>
    runFilesystemConformance({
      name: "cloudflare-durable-object",
      recordFixtureContext: fixtureContextEvidence("sqlite-cloudflare"),
      async create() {
        let current = await openCloudflareSqlite({ storage: state.storage });
        return {
          adapter: current,
          capabilities: ["garbage-collection", "second-connection"],
          collectGarbage(filesystem, options) {
            return filesystem.maintenance.collectGarbage(options);
          },
          async reopen() {
            current = await openCloudflareSqlite({ storage: state.storage });
            return current;
          },
          async openSecondConnection() {
            return openCloudflareSqlite({ storage: state.storage });
          },
          async dispose() {
            current.close();
          },
        };
      },
    }),
  );
  expect(results).toEqual(
    PORTABLE_CONFORMANCE_CASE_IDS.map((id) =>
      id === "read-only-reopen"
        ? {
            id,
            status: "skipped",
            reason: "adapter does not report read-only reopen",
          }
        : { id, status: "passed" },
    ),
  );
});

test("the shared branch suite passes in the faithful runtime", async () => {
  const stub = object("portable-branch-conformance");
  const results = await runInDurableObject(stub, async (_instance, state) =>
    runBranchConformance({
      name: "cloudflare-durable-object-branches",
      recordFixtureContext: fixtureContextEvidence("sqlite-cloudflare"),
      async create() {
        let current = await openCloudflareSqlite({ storage: state.storage });
        return {
          adapter: current,
          capabilities: ["physical-reopen"],
          async reopen() {
            current = await openCloudflareSqlite({ storage: state.storage });
            return current;
          },
          async dispose() {
            current.close();
          },
        };
      },
    }),
  );
  expect(results).toEqual(
    PORTABLE_BRANCH_CASE_IDS.map((id) => ({ id, status: "passed" })),
  );
});

test("the shared maintenance suite passes in the faithful runtime", async () => {
  const stub = object("portable-maintenance-conformance");
  const results = await runInDurableObject(stub, async (_instance, state) =>
    runMaintenanceConformance({
      name: "cloudflare-durable-object-maintenance",
      recordFixtureContext: fixtureContextEvidence("sqlite-cloudflare"),
      async create() {
        let current = await openCloudflareSqlite({ storage: state.storage });
        return {
          adapter: current,
          capabilities: ["garbage-collection", "physical-reopen"],
          collectGarbage(filesystem, options) {
            return filesystem.maintenance.collectGarbage(options);
          },
          async reopen() {
            current = await openCloudflareSqlite({ storage: state.storage });
            return current;
          },
          async dispose() {
            current.close();
          },
        };
      },
    }),
  );
  expect(results).toEqual(
    PORTABLE_MAINTENANCE_CASE_IDS.map((id) => ({ id, status: "passed" })),
  );
});

test(
  "the faithful Durable Object completes the exact finite smoke profile within 60 seconds",
  { timeout: 60_000 },
  async () => {
    const stub = object("portable-smoke");
    const session = new PortableSmokeSession(
      "cloudflare-durable-object-faithful-local",
    );
    let result;
    for (;;) {
      const outcome = await runInDurableObject(stub, async (_instance, state) => {
        const adapter = await openCloudflareSqlite({ storage: state.storage });
        try {
          return await session.run(adapter);
        } finally {
          adapter.close();
        }
      });
      if (outcome.status === "complete") {
        result = outcome.result;
        break;
      }
      const restartStarted = performance.now();
      await evictDurableObject(stub);
      session.recordPhysicalRestart(performance.now() - restartStarted);
    }
    expect(result).toMatchObject({
      schema: "efs-portable-smoke-result-v1",
      adapter: "cloudflare-durable-object-faithful-local",
      seed: 0x5eed5eed,
      completedOperationCount: 9_056,
      namespaceOperationCount: 2_000,
      restarts: 3,
    });
    expect(result.elapsedMs).toBeLessThan(60_000);
    console.log(`m6-smoke-evidence ${JSON.stringify(result)}`);
  },
);

test("the exact preview HTTP surface supports hosted smoke operations", async () => {
  const value = Uint8Array.from({ length: 32 * 1024 }, (_, index) => index & 0xff);
  const rpc = async (command: unknown) => {
    const response = await SELF.fetch("https://preview.invalid/surface/rpc", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(command),
    });
    expect(response.status).toBe(200);
    return response.json();
  };
  await expect(
    rpc({ operation: "mkdir", path: "/work", recursive: true }),
  ).resolves.toEqual({ ok: true });
  const written = await SELF.fetch("https://preview.invalid/surface/file/work/value", {
    method: "PUT",
    headers: { "content-length": String(value.byteLength) },
    body: value,
  });
  expect(written.status).toBe(200);
  expect(await written.json()).toEqual({ ok: true, bytes: value.byteLength });
  const selected = await SELF.fetch("https://preview.invalid/surface/file/work/value");
  expect(selected.status).toBe(200);
  expect(new Uint8Array(await selected.arrayBuffer())).toEqual(value);
  await expect(
    rpc({ operation: "mkdir", path: "/links", recursive: true }),
  ).resolves.toEqual({ ok: true });
  await expect(
    rpc({
      operation: "link",
      source: "/work/value",
      destination: "/links/value",
    }),
  ).resolves.toEqual({ ok: true });
  await expect(rpc({ operation: "stat", path: "/links/value" })).resolves.toMatchObject(
    { type: "file", size: value.byteLength, nlink: 2 },
  );
  await expect(
    rpc({ operation: "branchCreate", branchId: "hosted-surface-branch" }),
  ).resolves.toMatchObject({ id: "hosted-surface-branch", state: "active" });
  await expect(
    rpc({
      operation: "branchWrite",
      branchId: "hosted-surface-branch",
      path: "/branch-value",
      bytes: [1, 2, 3],
    }),
  ).resolves.toEqual({ ok: true });
  await expect(
    rpc({
      operation: "branchPublish",
      branchId: "hosted-surface-branch",
      operationId: "hosted-surface-operation",
    }),
  ).resolves.toMatchObject({ outcome: "merged" });
  await expect(rpc({ operation: "runtimeIdentity" })).resolves.toMatchObject({
    databaseSize: expect.any(Number),
    instanceNonce: expect.any(String),
  });
});

test("the exact preview Durable Object persists filesystem bytes across eviction", async () => {
  const stub = object("eviction");
  expect(await stub.writeText("/durable", "before-eviction")).toBe("ok");
  expect(await stub.readText("/durable")).toBe("before-eviction");
  const before = await stub.runtimeIdentity();
  expect(before.databaseSize).toBeGreaterThan(0);
  expect(
    await runInDurableObject(stub, async (_instance, state) =>
      state.storage.sql
        .exec(
          "SELECT application_id,user_version FROM efs_schema_identity WHERE singleton=1",
        )
        .toArray(),
    ),
  ).toEqual([{ application_id: 0x45414653, user_version: 13 }]);
  await evictDurableObject(stub);
  expect(await stub.readText("/durable")).toBe("before-eviction");
  const after = await stub.runtimeIdentity();
  expect(after.instanceNonce).not.toBe(before.instanceNonce);
  expect(after.databaseSize).toBeGreaterThan(0);
});

test("runtime eviction recovers leases, active branches, and interrupted collection", async () => {
  const stub = object("eviction-recovery");
  const before = await runInDurableObject(stub, async (_instance, state) => {
    const driver = await openCloudflareSqlite({ storage: state.storage });
    let now = 100;
    const filesystem = await EphemeralFS.open({
      database: driver,
      ownsDatabase: false,
      clock: () => now++,
      storage: {
        maxGcBatchSize: 2,
        maxQueryBatchSize: 16,
        readLeaseMs: 10,
        stagingLeaseMs: 20,
      },
    });
    await filesystem.writeFile("/stable", "committed-before-eviction");
    const branch = await filesystem.branches.create("eviction-branch");
    await branch.writeFile("/pending", "branch-value");
    const stream = await filesystem.readStream("/stable");
    const first = await stream.getReader().read();
    expect(first.done).toBe(false);
    await filesystem.writeFile("/orphan", "collect-after-eviction");
    await filesystem.unlink("/orphan");
    const collection = await filesystem.maintenance.collectGarbage({
      runId: "eviction-interrupted-gc",
      maxBatches: 1,
    });
    expect(collection.state).toBe("paused");
    return {
      databaseSize: state.storage.sql.databaseSize,
      activeLeases: state.storage.sql
        .exec("SELECT count(*) value FROM efs_leases WHERE state IN (0,1)")
        .one().value,
    };
  });
  expect(before.databaseSize).toBeGreaterThan(0);
  expect(before.activeLeases).toBeGreaterThan(0);

  await evictDurableObject(stub);

  const after = await runInDurableObject(stub, async (_instance, state) => {
    const driver = await openCloudflareSqlite({ storage: state.storage });
    let now = 10_000;
    const filesystem = await EphemeralFS.open({
      database: driver,
      ownsDatabase: false,
      clock: () => now++,
      storage: {
        maxGcBatchSize: 2,
        maxQueryBatchSize: 16,
        readLeaseMs: 10,
        stagingLeaseMs: 20,
      },
    });
    expect(await filesystem.readFile("/stable", { encoding: "utf8" })).toBe(
      "committed-before-eviction",
    );
    const branch = await filesystem.branches.open("eviction-branch");
    expect(await branch.readFile("/pending", { encoding: "utf8" })).toBe(
      "branch-value",
    );
    await branch.close();
    let collection = await filesystem.maintenance.collectGarbage({
      runId: "eviction-interrupted-gc",
      maxBatches: 1,
    });
    for (let call = 0; call < 10_000 && collection.state !== "complete"; call += 1)
      collection = await filesystem.maintenance.collectGarbage({
        runId: "eviction-interrupted-gc",
        maxBatches: 1,
      });
    expect(collection.state).toBe("complete");
    let cursor: string | undefined;
    for (let batch = 0; batch < 100_000; batch += 1) {
      const verification = await filesystem.maintenance.verify({
        ...(cursor === undefined ? {} : { cursor }),
        maxEntities: 4,
      });
      cursor = verification.nextCursor ?? undefined;
      if (verification.complete) break;
    }
    expect(cursor).toBeUndefined();
    const active = driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT (SELECT count(*) FROM efs_leases WHERE state IN (0,1)) leases,(SELECT count(*) FROM efs_staging_certificates) staging",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    );
    await filesystem.close();
    driver.close();
    return {
      active,
      databaseSize: state.storage.sql.databaseSize,
      collectionState: collection.state,
    };
  });
  expect(after.collectionState).toBe("complete");
  expect(after.active).toEqual({ leases: 0, staging: 0 });
  expect(after.databaseSize).toBeGreaterThan(0);
});

test("the shared restart suite crosses a real Durable Object runtime eviction", async () => {
  const stub = object("portable-shared-runtime-restart");
  const preparation = await runInDurableObject(stub, async (_instance, state) => {
    const adapter = await openCloudflareSqlite({ storage: state.storage });
    return preparePortableRestart(adapter);
  });
  const before = await stub.runtimeIdentity();
  await evictDurableObject(stub);
  const after = await stub.runtimeIdentity();
  expect(after.instanceNonce).not.toBe(before.instanceNonce);
  const result = await runInDurableObject(stub, async (_instance, state) => {
    const adapter = await openCloudflareSqlite({ storage: state.storage });
    try {
      return await verifyPortableRestart(adapter, preparation);
    } finally {
      adapter.close();
    }
  });
  expect(result.cases).toEqual(PORTABLE_RESTART_CASE_IDS);
  expect(result.fixtureDigest).toBe(preparation.fixtureDigest);
  expect(result.activeLeaseRows).toBe(0);
  expect(result.stagingRows).toBe(0);
  expect(result.collectionState).toBe("complete");
  console.log(`m6-restart-evidence ${JSON.stringify(result)}`);
});

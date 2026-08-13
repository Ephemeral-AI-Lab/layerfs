import type {
  FilesystemSQLiteDriver,
  FilesystemSQLiteTransaction,
} from "@ephemeralai/fs/sqlite-driver";
import type { ConformanceAdapterFactory } from "./index.js";
import { recordPortableFixtureContext } from "./fixture-context.js";

export type PortableDriverCaseId =
  | "driver-capabilities"
  | "driver-transactions"
  | "driver-callback-error-identity"
  | "driver-integer-roundtrip"
  | "driver-blob-ownership"
  | "driver-bounds"
  | "driver-sql-shape"
  | "driver-reopen-lifecycle";

export interface PortableDriverCaseResult {
  readonly id: PortableDriverCaseId;
  readonly status: "passed";
}

export const PORTABLE_DRIVER_CASE_IDS = Object.freeze([
  "driver-capabilities",
  "driver-transactions",
  "driver-callback-error-identity",
  "driver-integer-roundtrip",
  "driver-blob-ownership",
  "driver-bounds",
  "driver-sql-shape",
  "driver-reopen-lifecycle",
] as const satisfies readonly PortableDriverCaseId[]);

function invariant(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`portable driver conformance: ${message}`);
}

function expectThrow(callback: () => unknown, pattern: RegExp): void {
  try {
    callback();
  } catch (error) {
    invariant(pattern.test(String(error)), `unexpected rejection ${String(error)}`);
    return;
  }
  throw new Error("portable driver conformance: expected synchronous rejection");
}

function bytesEqual(left: Uint8Array, right: Uint8Array): boolean {
  return (
    left.byteLength === right.byteLength &&
    left.every((byte, index) => byte === right[index])
  );
}

/** Run the identical callback-scoped SQLite contract against a fresh adapter. */
export async function runSQLiteDriverConformance(
  factory: ConformanceAdapterFactory,
): Promise<readonly PortableDriverCaseResult[]> {
  const label = "portable-driver";
  const seed = 0xd21e;
  const fixture = await factory.create({ label, seed });
  await recordPortableFixtureContext(factory, fixture.adapter, label, seed);
  const results: PortableDriverCaseResult[] = [];
  let adapter = fixture.adapter;
  const passed = (id: PortableDriverCaseId): void => {
    results.push(Object.freeze({ id, status: "passed" }));
  };
  try {
    const capabilities = adapter.capabilities;
    invariant(capabilities.maxBindings >= 8, "binding capacity is below the minimum");
    invariant(capabilities.maxBlobBytes > 0, "BLOB capacity is not positive");
    invariant(
      capabilities.maxPhysicalDatabaseBytes > 0 && capabilities.maxJournalBytes > 0,
      "physical ceilings are not finite positive values",
    );
    invariant(
      capabilities.schemaIdentityMode === "sqlite-header" ||
        capabilities.schemaIdentityMode === "durable-table",
      "schema identity mode is not explicit",
    );
    passed("driver-capabilities");

    adapter.transaction("write", (tx) => {
      tx.run(
        "CREATE TABLE portable_driver(id INTEGER PRIMARY KEY,value BLOB NOT NULL)",
      );
      tx.run("CREATE TABLE portable_parent(id INTEGER PRIMARY KEY)");
      tx.run(
        "CREATE TABLE portable_child(parent_id INTEGER NOT NULL REFERENCES portable_parent(id))",
      );
    });
    expectThrow(
      () =>
        adapter.transaction("write", (tx) => {
          tx.run("INSERT INTO portable_driver(id,value) VALUES(?,?)", [
            1,
            Uint8Array.of(1),
          ]);
          throw new Error("portable rollback sentinel");
        }),
      /rollback sentinel/,
    );
    invariant(
      adapter.transaction(
        "read",
        (tx) =>
          tx.all<{ value: number }>("SELECT count(*) value FROM portable_driver", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0]!.value,
      ) === 0,
      "rolled-back row became visible",
    );
    expectThrow(
      () =>
        adapter.transaction("write", (tx) =>
          tx.run("INSERT INTO portable_child(parent_id) VALUES(?)", [404]),
        ),
      /foreign key|constraint/i,
    );
    let escaped: FilesystemSQLiteTransaction | undefined;
    adapter.transaction("read", (tx) => {
      escaped = tx;
    });
    expectThrow(
      () => escaped!.all("SELECT 1 value", [], { maxRows: 1, maxBytes: 128 }),
      /no longer active/i,
    );
    expectThrow(
      () => adapter.transaction("write", () => adapter.transaction("read", () => 0)),
      /nested/i,
    );
    expectThrow(
      () => adapter.transaction("write", async () => Promise.resolve(0)),
      /synchronous/i,
    );
    passed("driver-transactions");

    const callbackSentinel = new Error("SQLITE_FULL callback sentinel");
    let callbackIdentity: unknown;
    try {
      adapter.transaction("write", () => {
        throw callbackSentinel;
      });
    } catch (error) {
      callbackIdentity = error;
    }
    invariant(
      callbackIdentity === callbackSentinel,
      "driver normalized or replaced a callback-thrown error",
    );
    passed("driver-callback-error-identity");

    adapter.transaction("write", (tx) => {
      tx.run("CREATE TABLE portable_integers(id INTEGER PRIMARY KEY,value INTEGER)");
      for (const [id, value] of [
        [1, 0],
        [2, Number.MAX_SAFE_INTEGER - 1],
        [3, Number.MAX_SAFE_INTEGER],
      ] as const)
        tx.run("INSERT INTO portable_integers(id,value) VALUES(?,?)", [id, value]);
    });
    const integers = adapter.transaction("read", (tx) =>
      tx.all<{ value: number }>("SELECT value FROM portable_integers ORDER BY id", [], {
        maxRows: 3,
        maxBytes: 256,
      }),
    );
    invariant(
      JSON.stringify(integers.map((row) => row.value)) ===
        JSON.stringify([0, Number.MAX_SAFE_INTEGER - 1, Number.MAX_SAFE_INTEGER]),
      "safe integers did not round-trip exactly",
    );
    passed("driver-integer-roundtrip");

    const backing = Uint8Array.of(99, 1, 2, 3, 99);
    adapter.transaction("write", (tx) => {
      tx.run("INSERT INTO portable_driver(id,value) VALUES(?,?)", [
        1,
        backing.subarray(1, 4),
      ]);
      tx.run("INSERT INTO portable_driver(id,value) VALUES(?,?)", [
        2,
        Uint8Array.of(4, 5, 6),
      ]);
      tx.run("INSERT INTO portable_driver(id,value) VALUES(?,?)", [
        3,
        new Uint8Array(),
      ]);
    });
    backing.fill(0);
    const selected = adapter.transaction(
      "read",
      (tx) =>
        tx.all<{ value: Uint8Array }>(
          "SELECT value FROM portable_driver WHERE id=1",
          [],
          { maxRows: 1, maxBytes: 128 },
        )[0]!,
    );
    invariant(Object.isFrozen(selected), "result row is mutable");
    invariant(
      bytesEqual(selected.value, Uint8Array.of(1, 2, 3)),
      "input BLOB ownership was not detached",
    );
    selected.value[0] = 200;
    invariant(
      bytesEqual(
        adapter.transaction(
          "read",
          (tx) =>
            tx.all<{ value: Uint8Array }>(
              "SELECT value FROM portable_driver WHERE id=1",
              [],
              { maxRows: 1, maxBytes: 128 },
            )[0]!.value,
        ),
        Uint8Array.of(1, 2, 3),
      ),
      "result BLOB aliases durable storage",
    );
    invariant(
      adapter.transaction(
        "read",
        (tx) =>
          tx.all<{ value: Uint8Array }>(
            "SELECT value FROM portable_driver WHERE id=3",
            [],
            { maxRows: 1, maxBytes: 128 },
          )[0]!.value.byteLength,
      ) === 0,
      "empty BLOB did not round-trip exactly",
    );
    passed("driver-blob-ownership");

    expectThrow(
      () =>
        adapter.transaction("read", (tx) =>
          tx.all("SELECT value FROM portable_driver ORDER BY id", [], {
            maxRows: 1,
            maxBytes: 256,
          }),
        ),
      /row budget/i,
    );
    expectThrow(
      () =>
        adapter.transaction("read", (tx) =>
          tx.all("SELECT value FROM portable_driver WHERE id=1", [], {
            maxRows: 1,
            maxBytes: 1,
          }),
        ),
      /byte budget/i,
    );
    expectThrow(
      () =>
        adapter.transaction("read", (tx) =>
          tx.all("SELECT 1 value", [], { maxRows: 0, maxBytes: 128 }),
        ),
      /invalid query budget/i,
    );
    expectThrow(
      () =>
        adapter.transaction("write", (tx) =>
          tx.run("INSERT INTO portable_driver(id,value) VALUES(?,?)", [
            Number.MAX_SAFE_INTEGER + 1,
            Uint8Array.of(1),
          ]),
        ),
      /safe integer/i,
    );
    passed("driver-bounds");

    expectThrow(
      () => adapter.transaction("read", (tx) => tx.run("DELETE FROM portable_driver")),
      /read-only/i,
    );
    expectThrow(
      () => adapter.transaction("write", (tx) => tx.run("SELECT 1 value")),
      /bounded all/i,
    );
    expectThrow(
      () =>
        adapter.transaction("write", (tx) =>
          tx.run("INSERT INTO portable_driver(value) VALUES(X'01') RETURNING id"),
        ),
      /bounded all/i,
    );
    expectThrow(
      () =>
        adapter.transaction("write", (tx) =>
          tx.run("WITH v(id) AS (VALUES(3)) DELETE FROM portable_driver WHERE id IN v"),
        ),
      /common-table expressions/i,
    );
    expectThrow(
      () => adapter.transaction("write", (tx) => tx.run("CREATE TEMP TABLE bad(id)")),
      /temporary/i,
    );
    expectThrow(
      () =>
        adapter.transaction("write", (tx) =>
          tx.run("ATTACH DATABASE ':memory:' AS other"),
        ),
      /callback-scoped/i,
    );
    passed("driver-sql-shape");

    adapter.close();
    adapter = await fixture.reopen({ physical: true });
    invariant(
      adapter.transaction(
        "read",
        (tx) =>
          tx.all<{ value: number }>("SELECT count(*) value FROM portable_driver", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0]!.value,
      ) === 3,
      "driver reopen lost committed rows",
    );
    adapter.close();
    adapter.close();
    expectThrow(() => adapter.transaction("read", () => 0), /closed/i);
    passed("driver-reopen-lifecycle");
    return Object.freeze(results);
  } finally {
    try {
      adapter.close();
    } catch {}
    await fixture.dispose();
  }
}

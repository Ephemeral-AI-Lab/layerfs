import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { DatabaseSync } from "node:sqlite";
import { test } from "node:test";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { runUnitOfWork } from "../../packages/fs/dist/sqlite/unit-of-work.js";
import { EphemeralFS } from "../../packages/fs/dist/index.js";

function proveReadTransactionsAreReadOnly(driver) {
  driver.transaction("write", (tx) =>
    tx.run("CREATE TABLE readonly_probe(value INTEGER)"),
  );
  const budget = { maxRows: 10, maxBytes: 4096 };
  const attempts = [
    ["DML through run", (tx) => tx.run("INSERT INTO readonly_probe VALUES(1)")],
    [
      "DML through all",
      (tx) => tx.all("INSERT INTO readonly_probe VALUES(2)", [], budget),
    ],
    ["DDL through run", (tx) => tx.run("CREATE TABLE forbidden_run(value INTEGER)")],
    [
      "DDL through all",
      (tx) => tx.all("CREATE TABLE forbidden_all(value INTEGER)", [], budget),
    ],
    ["write PRAGMA through run", (tx) => tx.run("PRAGMA user_version=123")],
    ["write PRAGMA through all", (tx) => tx.all("PRAGMA user_version=124", [], budget)],
    [
      "row-returning write through run",
      (tx) => tx.run("INSERT INTO readonly_probe VALUES(3) RETURNING value"),
    ],
    [
      "row-returning write through all",
      (tx) =>
        tx.all("INSERT INTO readonly_probe VALUES(4) RETURNING value", [], budget),
    ],
  ];
  for (const [label, attempt] of attempts)
    assert.throws(
      () => driver.transaction("read", attempt),
      /EROFS|read-only|readonly|query_only/i,
      label,
    );
  driver.transaction("read", (tx) => {
    assert.equal(
      tx.all("SELECT count(*) count FROM readonly_probe", [], budget)[0].count,
      0,
    );
    assert.equal(
      tx.all(
        "SELECT count(*) count FROM sqlite_master WHERE name IN ('forbidden_run','forbidden_all')",
        [],
        budget,
      )[0].count,
      0,
    );
    assert.equal(
      tx.all("SELECT user_version value FROM pragma_user_version", [], budget)[0].value,
      0,
    );
  });
}

test("Node SQLite driver scopes transactions and enforces result/binding types", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  assert.equal(
    driver.transaction(
      "exclusive",
      (tx) =>
        tx.all("PRAGMA temp_store", [], { maxRows: 1, maxBytes: 128 })[0].temp_store,
    ),
    1,
  );
  let retained;
  driver.transaction("write", (tx) => {
    retained = tx;
    tx.run("CREATE TABLE sample(id INTEGER PRIMARY KEY,value BLOB NOT NULL)");
    const backing = Uint8Array.of(99, 1, 2, 3, 88);
    const view = backing.subarray(1, 4);
    tx.run("INSERT INTO sample(id,value) VALUES(?,?)", [Number.MAX_SAFE_INTEGER, view]);
  });
  assert.throws(
    () => retained.all("SELECT 1", [], { maxRows: 1, maxBytes: 10 }),
    /no longer active/,
  );
  const row = driver.transaction(
    "read",
    (tx) =>
      tx.all("SELECT id,value FROM sample", [], { maxRows: 1, maxBytes: 1024 })[0],
  );
  assert.equal(row.id, Number.MAX_SAFE_INTEGER);
  assert.deepEqual([...row.value], [1, 2, 3]);
  assert.throws(
    () =>
      driver.transaction("write", (tx) =>
        tx.run("INSERT INTO sample VALUES(?,?)", [1.5, new Uint8Array()]),
      ),
    /safe integers/,
  );
  assert.throws(
    () =>
      driver.transaction("read", (tx) =>
        tx.all("SELECT * FROM sample", [], { maxRows: 1, maxBytes: 1 }),
      ),
    /byte budget/,
  );
  for (const mode of ["read", "write"])
    assert.throws(
      () => driver.transaction(mode, (tx) => tx.run("SELECT 1")),
      /require a bounded all\(\) query/,
    );
  driver.close();
  driver.close();
});

test("Node read transactions reject DML, DDL, write PRAGMAs, and RETURNING through run and all", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  try {
    proveReadTransactionsAreReadOnly(driver);
  } finally {
    driver.close();
  }
});

test("close invokes the native handle once and rethrows the first close failure", async () => {
  const original = DatabaseSync.prototype.close;
  const failure = new Error("injected native close failure");
  let calls = 0;
  DatabaseSync.prototype.close = function () {
    calls += 1;
    Reflect.apply(original, this, []);
    throw failure;
  };
  try {
    const driver = await openNodeSqlite({ filename: ":memory:" });
    assert.throws(
      () => driver.close(),
      (error) => error === failure,
    );
    assert.throws(
      () => driver.close(),
      (error) => error === failure,
    );
    assert.equal(calls, 1);
    assert.throws(() => driver.physicalStorage(), /driver is closed/);
  } finally {
    DatabaseSync.prototype.close = original;
  }
});

test("callback scope rejects transaction escapes and result queries cannot mutate", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  driver.transaction("write", (tx) =>
    tx.run("CREATE TABLE scoped(id INTEGER PRIMARY KEY,value INTEGER)"),
  );
  for (const sql of [
    "COMMIT",
    "ROLLBACK",
    "SAVEPOINT escaped",
    "ATTACH ':memory:' AS escaped",
  ])
    assert.throws(
      () => driver.transaction("write", (tx) => tx.run(sql)),
      /callback-scoped transaction contract/,
    );
  assert.throws(
    () =>
      driver.transaction("write", (tx) =>
        tx.all(
          "WITH input(value) AS (VALUES(1)) INSERT INTO scoped(value) SELECT value FROM input RETURNING value",
          [],
          { maxRows: 1, maxBytes: 128 },
        ),
      ),
    /read-only|readonly|bounded contract/,
  );
  assert.equal(
    driver.transaction(
      "read",
      (tx) =>
        tx.all("SELECT count(*) count FROM scoped", [], {
          maxRows: 1,
          maxBytes: 128,
        })[0].count,
    ),
    0,
  );
  driver.close();
});

test("temporary and qualified schema escapes reject before file-backed mutation", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sqlite-schema-contract-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    const driver = await openNodeSqlite({ filename });
    for (const sql of [
      "CREATE TABLE temp.escape_contract(value INTEGER)",
      "CREATE/**/TEMP TABLE escape_comment(value INTEGER)",
      'CREATE TABLE "temp"."escape_quoted"(value INTEGER)',
      "CREATE VIRTUAL/**/TABLE escape_virtual USING fts5(value)",
      "DROP/**/TABLE temp.escape_contract",
      "ALTER/**/TABLE temp.escape_contract RENAME TO escaped",
      "CREATE TRIGGER escape_target AFTER INSERT ON temp . escape_contract BEGIN SELECT 1; END",
    ])
      assert.throws(
        () => driver.transaction("write", (tx) => tx.run(sql)),
        /outside the storage contract/,
      );
    assert.equal(
      driver.transaction(
        "read",
        (tx) =>
          tx.all("SELECT count(*) count FROM sqlite_temp_schema", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0].count,
      ),
      0,
    );
    driver.close();

    const reopened = await openNodeSqlite({ filename, create: false });
    assert.equal(
      reopened.transaction(
        "read",
        (tx) =>
          tx.all("SELECT count(*) count FROM sqlite_schema", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0].count,
      ),
      0,
    );
    reopened.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("file-backed driver reopens read-only and supports a second snapshot connection", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sqlite-"));
  const filename = path.join(directory, "filesystem.db");
  let first;
  let second;
  let readOnly;
  try {
    first = await openNodeSqlite({ filename });
    first.transaction("write", (tx) => {
      tx.run("CREATE TABLE durable(id INTEGER PRIMARY KEY,value TEXT)");
      tx.run("INSERT INTO durable(value) VALUES(?)", ["committed"]);
    });
    second = await openNodeSqlite({ filename });
    assert.equal(
      second.transaction(
        "read",
        (tx) =>
          tx.all("SELECT value FROM durable", [], { maxRows: 1, maxBytes: 1024 })[0]
            .value,
      ),
      "committed",
    );
    second.close();
    second = undefined;
    first.close();
    first = undefined;
    await assert.rejects(
      openNodeSqlite({
        filename,
        create: false,
        maxPhysicalDatabaseBytes: 1,
      }),
      /existing SQLite database exceeds the requested physical profile/,
    );
    await assert.rejects(
      openNodeSqlite({
        filename,
        readOnly: true,
        create: false,
        maxPhysicalDatabaseBytes: 1,
      }),
      /existing SQLite database exceeds the requested physical profile/,
    );
    readOnly = await openNodeSqlite({
      filename,
      readOnly: true,
      create: false,
      cacheTargetBytes: 8192,
      mmapLimitBytes: 0,
    });
    assert.equal(
      readOnly.transaction(
        "read",
        (tx) =>
          tx.all("SELECT value FROM durable", [], { maxRows: 1, maxBytes: 1024 })[0]
            .value,
      ),
      "committed",
    );
    assert.throws(() => readOnly.transaction("write", () => {}), /EROFS/);
    assert.deepEqual(
      {
        cacheTargetBytes: readOnly.capabilities.cacheTargetBytes,
        mmapLimitBytes: readOnly.capabilities.mmapLimitBytes,
      },
      { cacheTargetBytes: 8192, mmapLimitBytes: 0 },
    );
    readOnly.close();
    readOnly = undefined;
  } finally {
    try {
      readOnly?.close();
    } catch {}
    try {
      second?.close();
    } catch {}
    try {
      first?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("bounded units of work roll back row and binding-byte overflow", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  driver.transaction("write", (tx) =>
    tx.run("CREATE TABLE bounded(id INTEGER PRIMARY KEY,value BLOB)"),
  );
  assert.throws(
    () =>
      runUnitOfWork(driver, "write", { maxRows: 1, maxBytes: 100 }, (tx) => {
        tx.run("INSERT INTO bounded(value) VALUES(?)", [Uint8Array.of(1)]);
        tx.run("INSERT INTO bounded(value) VALUES(?)", [Uint8Array.of(2)]);
      }),
    /row limit/,
  );
  assert.equal(
    driver.transaction(
      "read",
      (tx) =>
        tx.all("SELECT count(*) count FROM bounded", [], {
          maxRows: 1,
          maxBytes: 100,
        })[0].count,
    ),
    0,
  );
  assert.throws(
    () =>
      runUnitOfWork(driver, "write", { maxRows: 10, maxBytes: 2 }, (tx) =>
        tx.run("INSERT INTO bounded(value) VALUES(?)", [Uint8Array.of(1, 2, 3)]),
      ),
    /byte limit/,
  );
  assert.equal(
    driver.transaction(
      "read",
      (tx) =>
        tx.all("SELECT count(*) count FROM bounded", [], {
          maxRows: 1,
          maxBytes: 100,
        })[0].count,
    ),
    0,
  );
  driver.close();
});

test("unit-of-work row limits include trigger and foreign-key side effects", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  driver.transaction("exclusive", (tx) => {
    tx.run("CREATE TABLE trigger_source(value INTEGER PRIMARY KEY)");
    tx.run("CREATE TABLE trigger_sink(value INTEGER PRIMARY KEY)");
    tx.run(
      "CREATE TRIGGER bounded_trigger AFTER INSERT ON trigger_source BEGIN INSERT INTO trigger_sink(value) VALUES(NEW.value); END",
    );
    tx.run("CREATE TABLE fk_parent(id INTEGER PRIMARY KEY)");
    tx.run(
      "CREATE TABLE fk_child(id INTEGER PRIMARY KEY,parent_id INTEGER NOT NULL REFERENCES fk_parent(id) ON DELETE CASCADE)",
    );
    tx.run("INSERT INTO fk_parent(id) VALUES(1)");
    tx.run("INSERT INTO fk_child(id,parent_id) VALUES(1,1)");
  });
  assert.throws(
    () =>
      runUnitOfWork(driver, "write", { maxRows: 1, maxBytes: 1024 }, (tx) =>
        tx.run("INSERT INTO trigger_source(value) VALUES(1)"),
      ),
    /row limit/,
  );
  assert.throws(
    () =>
      runUnitOfWork(driver, "write", { maxRows: 1, maxBytes: 1024 }, (tx) =>
        tx.run("DELETE FROM fk_parent WHERE id=1"),
      ),
    /row limit/,
  );
  assert.deepEqual(
    driver.transaction(
      "read",
      (tx) =>
        tx.all(
          "SELECT (SELECT count(*) FROM trigger_source) sources,(SELECT count(*) FROM trigger_sink) sinks,(SELECT count(*) FROM fk_parent) parents,(SELECT count(*) FROM fk_child) children",
          [],
          { maxRows: 1, maxBytes: 256 },
        )[0],
    ),
    { sources: 0, sinks: 0, parents: 1, children: 1 },
  );
  driver.close();
});

test("unit-of-work forwards only remaining intrinsic result and binding budgets", () => {
  class HostileBytes extends Uint8Array {
    get byteLength() {
      return 0;
    }
  }
  let materializedSecond = false;
  let runCalls = 0;
  const observedBudgets = [];
  const fake = {
    readOnly: false,
    kind: "fake",
    capabilities: Object.freeze({}),
    transaction(_mode, callback) {
      return callback({
        scope: Object.freeze({ mode: "read", active: true }),
        run() {
          runCalls += 1;
          return { changes: 0, lastInsertRowid: null };
        },
        all(sql, _bindings, budget) {
          observedBudgets.push(budget);
          if (sql === "first") return [{ payload: new HostileBytes(16) }];
          if (budget.maxRows > 1 || budget.maxBytes > 6) {
            materializedSecond = true;
            return [{ value: 1 }, { value: 2 }];
          }
          throw new RangeError("driver rejected before second query materialization");
        },
      });
    },
    close() {},
  };
  assert.throws(
    () =>
      runUnitOfWork(
        fake,
        "read",
        {
          maxRows: 10,
          maxBytes: 1024,
          maxResultRows: 2,
          maxResultBytes: 68,
        },
        (tx) => {
          tx.all("first", [], { maxRows: 10, maxBytes: 1024 });
          tx.all("second", [], { maxRows: 10, maxBytes: 1024 });
        },
      ),
    /before second query materialization/,
  );
  assert.equal(materializedSecond, false);
  assert.deepEqual(observedBudgets[1], { maxRows: 1, maxBytes: 6 });
  const longAlias = "a".repeat(2048);
  const aliasBudgets = [];
  const aliasFake = {
    ...fake,
    transaction(_mode, callback) {
      let first = true;
      return callback({
        scope: Symbol("alias-budget"),
        run() {
          return { changes: 0 };
        },
        all(_sql, _bindings, budget) {
          aliasBudgets.push(budget);
          if (first) {
            first = false;
            return [{ [longAlias]: 1 }];
          }
          throw new RangeError("second alias query rejected before materialization");
        },
      });
    },
  };
  assert.throws(
    () =>
      runUnitOfWork(
        aliasFake,
        "read",
        { maxRows: 4, maxBytes: 8192, maxResultBytes: 32 + 4096 + 8 + 17 },
        (tx) => {
          tx.all("first-alias", [], { maxRows: 4, maxBytes: 8192 });
          tx.all("second-alias", [], { maxRows: 4, maxBytes: 8192 });
        },
      ),
    /before materialization/,
  );
  assert.deepEqual(aliasBudgets[1], { maxRows: 3, maxBytes: 17 });
  assert.throws(
    () =>
      runUnitOfWork(fake, "write", { maxRows: 1, maxBytes: 8 }, (tx) =>
        tx.run("write", [new HostileBytes(9)]),
      ),
    /byte limit/,
  );
  assert.equal(runCalls, 0);
  assert.throws(
    () =>
      runUnitOfWork(fake, "write", { maxRows: 1, maxBytes: 2 }, (tx) =>
        tx.run("write", ["\ud800"]),
      ),
    /byte limit/,
  );
  assert.equal(runCalls, 0);
});

test("a busy BEGIN leaves the second writer reusable after the first writer commits", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sqlite-busy-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    const first = await openNodeSqlite({ filename, busyTimeoutMs: 0 });
    const second = await openNodeSqlite({ filename, busyTimeoutMs: 0 });
    first.transaction("write", (tx) => {
      tx.run("CREATE TABLE busy_probe(id INTEGER PRIMARY KEY,value INTEGER)");
      assert.throws(() => second.transaction("write", () => {}), /busy|locked/i);
    });
    second.transaction("write", (tx) =>
      tx.run("INSERT INTO busy_probe(value) VALUES(?)", [1]),
    );
    assert.equal(
      second.transaction(
        "read",
        (tx) =>
          tx.all("SELECT count(*) count FROM busy_probe", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0].count,
      ),
      1,
    );
    second.close();
    first.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("BLOB bindings and results are plain owned Uint8Arrays for Buffer and subclasses", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  class HostileBytes extends Uint8Array {
    slice() {
      throw new Error("subclass slice must not be called");
    }
    get byteLength() {
      throw new Error("subclass byteLength must not be read");
    }
  }
  const backing = new HostileBytes([7, 8, 9]);
  const buffer = Buffer.from([10, 11, 12]);
  const empty = new HostileBytes(0);
  driver.transaction("write", (tx) => {
    tx.run("CREATE TABLE owned(id INTEGER PRIMARY KEY,value BLOB NOT NULL)");
    tx.run("INSERT INTO owned VALUES(?,?)", [1, backing]);
    tx.run("INSERT INTO owned VALUES(?,?)", [2, buffer]);
    tx.run("INSERT INTO owned VALUES(?,?)", [3, empty]);
    backing[0] = 99;
    buffer[0] = 99;
  });
  const rows = driver.transaction("read", (tx) =>
    tx.all("SELECT value FROM owned ORDER BY id", [], {
      maxRows: 3,
      maxBytes: 1024,
    }),
  );
  assert.deepEqual([...rows[0].value], [7, 8, 9]);
  assert.deepEqual([...rows[1].value], [10, 11, 12]);
  assert.equal(Object.getPrototypeOf(rows[0].value), Uint8Array.prototype);
  assert.equal(Object.getPrototypeOf(rows[1].value), Uint8Array.prototype);
  assert.equal(Object.getPrototypeOf(rows[2].value), Uint8Array.prototype);
  assert.equal(rows[2].value.length, 0);
  const first = rows[0].value;
  first[0] = 0;
  const reread = driver.transaction(
    "read",
    (tx) =>
      tx.all("SELECT value FROM owned WHERE id=1", [], {
        maxRows: 1,
        maxBytes: 128,
      })[0].value,
  );
  assert.deepEqual([...reread], [7, 8, 9]);
  const callableThenable = Object.assign(() => {}, { then() {} });
  assert.throws(
    () => driver.transaction("read", () => callableThenable),
    /callbacks must be synchronous/,
  );
  driver.close();
});

test("WAL limits use observable checkpoint backpressure plus transaction admission", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sqlite-wal-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    const writer = await openNodeSqlite({
      filename,
      busyTimeoutMs: 0,
      durability: "relaxed-test",
      maxJournalBytes: 64 * 1024,
    });
    writer.transaction("write", (tx) =>
      tx.run("CREATE TABLE wal_probe(id INTEGER PRIMARY KEY,value BLOB NOT NULL)"),
    );
    writer.checkpoint("truncate");
    const reader = await openNodeSqlite({ filename, busyTimeoutMs: 0 });
    let committed = 0;
    let rejected = false;
    reader.transaction("read", (tx) => {
      tx.all("SELECT count(*) count FROM wal_probe", [], {
        maxRows: 1,
        maxBytes: 128,
      });
      for (let index = 0; index < 100; index += 1) {
        try {
          writer.transaction("write", (write) =>
            write.run("INSERT INTO wal_probe(value) VALUES(?)", [
              new Uint8Array(4096).fill(index),
            ]),
          );
          committed += 1;
        } catch (error) {
          assert.match(String(error), /ENOSPC.*WAL.*backpressure/i);
          rejected = true;
          break;
        }
      }
      assert.equal(rejected, true);
      assert.ok(writer.physicalStorage().walBytes > 0);
      assert.ok(
        writer.physicalStorage().walBytes <= writer.capabilities.maxJournalBytes,
      );
    });
    const count = writer.transaction(
      "read",
      (tx) =>
        tx.all("SELECT count(*) count FROM wal_probe", [], {
          maxRows: 1,
          maxBytes: 128,
        })[0].count,
    );
    assert.equal(count, committed);
    const checkpoint = writer.checkpoint("truncate");
    assert.equal(checkpoint.mode, "truncate");
    assert.equal(checkpoint.busy, 0);
    assert.ok((checkpoint.walBytes ?? 0) < writer.capabilities.maxJournalBytes);
    assert.equal(writer.capabilities.journalQuotaPolicy, "checkpoint-backpressure");
    assert.equal(writer.capabilities.journalSizeLimitIsHard, false);
    reader.close();
    writer.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("a pinned reader exposes one soft-target overshoot then backpressures the next writer", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sqlite-wal-tx-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    const setup = await openNodeSqlite({ filename, durability: "relaxed-test" });
    setup.transaction("exclusive", (tx) => {
      tx.run("CREATE TABLE wal_tx(id INTEGER PRIMARY KEY,value BLOB NOT NULL)");
      for (let index = 0; index < 32; index += 1)
        tx.run(`CREATE INDEX wal_tx_${index} ON wal_tx(value,id)`);
    });
    setup.close();
    const writer = await openNodeSqlite({
      filename,
      busyTimeoutMs: 0,
      durability: "relaxed-test",
      maxJournalBytes: 64 * 1024,
    });
    writer.checkpoint("truncate");
    const reader = await openNodeSqlite({ filename, busyTimeoutMs: 0 });
    reader.transaction("read", (tx) => {
      tx.all("SELECT count(*) count FROM wal_tx", [], {
        maxRows: 1,
        maxBytes: 128,
      });
      writer.transaction("write", (write) =>
        write.run("INSERT INTO wal_tx(value) VALUES(?)", [Uint8Array.of(1)]),
      );
      const overshoot = writer.physicalStorage().walBytes;
      assert.ok(overshoot > writer.capabilities.maxJournalBytes);
      assert.throws(
        () =>
          writer.transaction("write", (write) =>
            write.run("INSERT INTO wal_tx(value) VALUES(?)", [Uint8Array.of(2)]),
          ),
        /WAL checkpoint backpressure threshold remains pinned/,
      );
    });
    assert.equal(
      writer.transaction(
        "read",
        (tx) =>
          tx.all("SELECT count(*) count FROM wal_tx", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0].count,
      ),
      1,
    );
    writer.checkpoint("truncate");
    writer.transaction("write", (write) =>
      write.run("INSERT INTO wal_tx(value) VALUES(?)", [Uint8Array.of(2)]),
    );
    reader.close();
    writer.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("SQL-generated payloads use the same truthful soft-WAL backpressure policy", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sqlite-wal-expression-"));
  const filename = path.join(directory, "filesystem.db");
  let writer;
  let reader;
  try {
    const setup = await openNodeSqlite({ filename, durability: "relaxed-test" });
    setup.transaction("write", (tx) =>
      tx.run("CREATE TABLE expression_wal(value TEXT NOT NULL)"),
    );
    setup.close();
    writer = await openNodeSqlite({
      filename,
      busyTimeoutMs: 0,
      durability: "relaxed-test",
      maxJournalBytes: 64 * 1024,
    });
    writer.checkpoint("truncate");
    writer.transaction("write", (write) => {
      write.run("CREATE TABLE expression_source(id INTEGER PRIMARY KEY,value TEXT)");
      write.run("INSERT INTO expression_source(id,value) VALUES(1,'bounded')");
    });
    writer.checkpoint("truncate");
    reader = await openNodeSqlite({ filename, busyTimeoutMs: 0 });
    reader.transaction("read", (tx) => {
      tx.all("SELECT count(*) count FROM expression_wal", [], {
        maxRows: 1,
        maxBytes: 128,
      });
      assert.throws(
        () =>
          writer.transaction("write", (write) =>
            write.run(
              "WITH RECURSIVE c(x,n) AS (VALUES('x',0) UNION ALL SELECT x||x,n+1 FROM c WHERE n<18) INSERT INTO expression_wal(value) SELECT x FROM c ORDER BY n DESC LIMIT 1",
            ),
          ),
        /bounded contract/,
      );
      assert.throws(
        () =>
          writer.transaction("write", (write) =>
            write.run("SELECT 1,1,1,1,1,1,1,1,zeroblob(100000000)"),
          ),
        /expanding expressions/,
      );
      assert.throws(
        () =>
          writer.transaction("write", (write) =>
            write.run(
              "INSERT INTO expression_wal(value) SELECT value FROM expression_source",
            ),
          ),
        /write-from-query statements/,
      );
      assert.throws(
        () =>
          writer.transaction("write", (write) =>
            write.run(
              "CREATE TRIGGER expression_escape AFTER INSERT ON expression_wal BEGIN INSERT INTO expression_wal(value) VALUES(zeroblob(100000000)); END",
            ),
          ),
        /expanding expressions/,
      );
    });
    assert.equal(
      writer.transaction(
        "read",
        (tx) =>
          tx.all("SELECT count(*) count FROM expression_wal", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0].count,
      ),
      0,
    );
    reader.close();
    reader = undefined;
    writer.close();
    writer = undefined;
  } finally {
    try {
      reader?.close();
    } catch {}
    try {
      writer?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("filesystem storage caps cannot silently undercut the configured Node driver", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  await assert.rejects(
    EphemeralFS.open({
      database: driver,
      storage: {
        maxPhysicalDatabaseBytes: driver.capabilities.maxPhysicalDatabaseBytes - 4096,
      },
    }),
    /must be configured on the SQLite adapter/,
  );
  driver.close();
  const higher = await openNodeSqlite({
    filename: ":memory:",
    maxPhysicalDatabaseBytes: 11 * 1024 ** 3,
    maxJournalBytes: 2 * 1024 ** 3,
  });
  await assert.rejects(
    EphemeralFS.open({ database: higher }),
    /must be configured on the SQLite adapter/,
  );
  higher.close();
});

test("matching lower physical caps admit below-cap writes and survive reopen", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sqlite-fs-caps-"));
  const filename = path.join(directory, "filesystem.db");
  const physicalCap = 2 * 1024 * 1024;
  const journalCap = 8 * 1024 * 1024;
  let driver;
  let filesystem;
  try {
    driver = await openNodeSqlite({
      filename,
      maxPhysicalDatabaseBytes: physicalCap,
      maxJournalBytes: journalCap,
    });
    filesystem = await EphemeralFS.open({
      database: driver,
      storage: {
        maxPhysicalDatabaseBytes: driver.capabilities.maxPhysicalDatabaseBytes,
        maxJournalBytes: driver.capabilities.maxJournalBytes,
      },
    });
    const expected = new Uint8Array(64 * 1024).fill(37);
    await filesystem.writeFile("/below-cap", expected);
    assert.deepEqual(await filesystem.readFile("/below-cap"), expected);
    await filesystem.close();
    filesystem = undefined;
    driver.close();
    driver = undefined;

    driver = await openNodeSqlite({
      filename,
      create: false,
      maxPhysicalDatabaseBytes: physicalCap,
      maxJournalBytes: journalCap,
    });
    filesystem = await EphemeralFS.open({
      database: driver,
      storage: {
        maxPhysicalDatabaseBytes: driver.capabilities.maxPhysicalDatabaseBytes,
        maxJournalBytes: driver.capabilities.maxJournalBytes,
      },
    });
    assert.deepEqual(await filesystem.readFile("/below-cap"), expected);
    assert.ok(
      driver.physicalStorage().mainFileBytes <=
        driver.capabilities.maxPhysicalDatabaseBytes,
    );
    await filesystem.close();
    filesystem = undefined;
    driver.close();
    driver = undefined;
  } finally {
    try {
      await filesystem?.close();
    } catch {}
    try {
      driver?.close();
    } catch {}
    await rm(directory, { recursive: true, force: true });
  }
});

test("max_page_count rejects an over-budget transaction without a partial row", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sqlite-pages-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    const driver = await openNodeSqlite({
      filename,
      durability: "relaxed-test",
      maxPhysicalDatabaseBytes: 128 * 1024,
      maxJournalBytes: 1024 * 1024,
    });
    driver.transaction("write", (tx) =>
      tx.run("CREATE TABLE page_probe(id INTEGER PRIMARY KEY,value BLOB NOT NULL)"),
    );
    let committed = 0;
    for (let index = 0; index < 100; index += 1) {
      try {
        driver.transaction("write", (tx) =>
          tx.run("INSERT INTO page_probe(value) VALUES(?)", [
            new Uint8Array(8192).fill(index),
          ]),
        );
        committed += 1;
      } catch (error) {
        assert.match(String(error), /full|ENOSPC/i);
        break;
      }
    }
    assert.ok(committed > 0 && committed < 100);
    assert.equal(
      driver.transaction(
        "read",
        (tx) =>
          tx.all("SELECT count(*) count FROM page_probe", [], {
            maxRows: 1,
            maxBytes: 128,
          })[0].count,
      ),
      committed,
    );
    driver.checkpoint("truncate");
    const physical = driver.physicalStorage();
    assert.ok(physical.mainFileBytes <= driver.capabilities.maxPhysicalDatabaseBytes);
    assert.equal(driver.capabilities.physicalQuotaPolicy, "driver-enforced");
    driver.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

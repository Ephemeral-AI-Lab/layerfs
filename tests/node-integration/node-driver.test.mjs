import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { runUnitOfWork } from "../../packages/fs/dist/sqlite/unit-of-work.js";

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

test("file-backed driver reopens read-only and supports a second snapshot connection", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sqlite-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    const first = await openNodeSqlite({ filename });
    first.transaction("write", (tx) => {
      tx.run("CREATE TABLE durable(value TEXT)");
      tx.run("INSERT INTO durable VALUES(?)", ["committed"]);
    });
    const second = await openNodeSqlite({ filename });
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
    first.close();
    const readOnly = await openNodeSqlite({ filename, readOnly: true, create: false });
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
    readOnly.close();
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test("bounded units of work roll back row and binding-byte overflow", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  driver.transaction("write", (tx) => tx.run("CREATE TABLE bounded(value BLOB)"));
  assert.throws(
    () =>
      runUnitOfWork(driver, "write", { maxRows: 1, maxBytes: 100 }, (tx) => {
        tx.run("INSERT INTO bounded VALUES(?)", [Uint8Array.of(1)]);
        tx.run("INSERT INTO bounded VALUES(?)", [Uint8Array.of(2)]);
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
        tx.run("INSERT INTO bounded VALUES(?)", [Uint8Array.of(1, 2, 3)]),
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

test("a busy BEGIN leaves the second writer reusable after the first writer commits", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sqlite-busy-"));
  const filename = path.join(directory, "filesystem.db");
  try {
    const first = await openNodeSqlite({ filename, busyTimeoutMs: 0 });
    const second = await openNodeSqlite({ filename, busyTimeoutMs: 0 });
    first.transaction("write", (tx) => {
      tx.run("CREATE TABLE busy_probe(value INTEGER)");
      assert.throws(() => second.transaction("write", () => {}), /busy|locked/i);
    });
    second.transaction("write", (tx) =>
      tx.run("INSERT INTO busy_probe VALUES(?)", [1]),
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
  driver.transaction("write", (tx) => {
    tx.run("CREATE TABLE owned(id INTEGER PRIMARY KEY,value BLOB NOT NULL)");
    tx.run("INSERT INTO owned VALUES(?,?)", [1, backing]);
    tx.run("INSERT INTO owned VALUES(?,?)", [2, buffer]);
    backing[0] = 99;
    buffer[0] = 99;
  });
  const rows = driver.transaction("read", (tx) =>
    tx.all("SELECT value FROM owned ORDER BY id", [], {
      maxRows: 2,
      maxBytes: 1024,
    }),
  );
  assert.deepEqual([...rows[0].value], [7, 8, 9]);
  assert.deepEqual([...rows[1].value], [10, 11, 12]);
  assert.equal(Object.getPrototypeOf(rows[0].value), Uint8Array.prototype);
  assert.equal(Object.getPrototypeOf(rows[1].value), Uint8Array.prototype);
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
  driver.close();
});

test("WAL limits are observable checkpoint backpressure, not a claimed hard file ceiling", async () => {
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

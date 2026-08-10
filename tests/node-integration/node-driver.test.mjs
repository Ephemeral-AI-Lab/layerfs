import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "node:test";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";
import { runUnitOfWork } from "../../packages/fs/dist/sqlite/unit-of-work.js";

function proveReadTransactionsAreReadOnly(driver) {
  driver.transaction("write", (tx) => tx.run("CREATE TABLE readonly_probe(value INTEGER)"));
  const budget = { maxRows: 10, maxBytes: 4096 };
  const attempts = [
    ["DML through run", (tx) => tx.run("INSERT INTO readonly_probe VALUES(1)")],
    ["DML through all", (tx) => tx.all("INSERT INTO readonly_probe VALUES(2)", [], budget)],
    ["DDL through run", (tx) => tx.run("CREATE TABLE forbidden_run(value INTEGER)")],
    ["DDL through all", (tx) => tx.all("CREATE TABLE forbidden_all(value INTEGER)", [], budget)],
    ["write PRAGMA through run", (tx) => tx.run("PRAGMA user_version=123")],
    ["write PRAGMA through all", (tx) => tx.all("PRAGMA user_version=124", [], budget)],
    ["row-returning write through run", (tx) => tx.run("INSERT INTO readonly_probe VALUES(3) RETURNING value")],
    ["row-returning write through all", (tx) => tx.all("INSERT INTO readonly_probe VALUES(4) RETURNING value", [], budget)],
  ];
  for (const [label, attempt] of attempts) assert.throws(() => driver.transaction("read", attempt), /EROFS|read-only|readonly|query_only/i, label);
  driver.transaction("read", (tx) => {
    assert.equal(tx.all("SELECT count(*) count FROM readonly_probe", [], budget)[0].count, 0);
    assert.equal(tx.all("SELECT count(*) count FROM sqlite_master WHERE name IN ('forbidden_run','forbidden_all')", [], budget)[0].count, 0);
    assert.equal(tx.all("SELECT user_version value FROM pragma_user_version", [], budget)[0].value, 0);
  });
}

test("Node SQLite driver scopes transactions and enforces result/binding types", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  let retained;
  driver.transaction("write", (tx) => {
    retained = tx;
    tx.run("CREATE TABLE sample(id INTEGER PRIMARY KEY,value BLOB NOT NULL)");
    const backing = Uint8Array.of(99, 1, 2, 3, 88); const view = backing.subarray(1, 4);
    tx.run("INSERT INTO sample(id,value) VALUES(?,?)", [Number.MAX_SAFE_INTEGER, view]);
  });
  assert.throws(() => retained.all("SELECT 1", [], { maxRows: 1, maxBytes: 10 }), /no longer active/);
  const row = driver.transaction("read", (tx) => tx.all("SELECT id,value FROM sample", [], { maxRows: 1, maxBytes: 1024 })[0]);
  assert.equal(row.id, Number.MAX_SAFE_INTEGER); assert.deepEqual([...row.value], [1, 2, 3]);
  assert.throws(() => driver.transaction("write", (tx) => tx.run("INSERT INTO sample VALUES(?,?)", [1.5, new Uint8Array()])), /safe integers/);
  assert.throws(() => driver.transaction("read", (tx) => tx.all("SELECT * FROM sample", [], { maxRows: 1, maxBytes: 1 })), /byte budget/);
  driver.close(); driver.close();
});

test("Node read transactions reject DML, DDL, write PRAGMAs, and RETURNING through run and all", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" }); try { proveReadTransactionsAreReadOnly(driver); } finally { driver.close(); }
});

test("file-backed driver reopens read-only and supports a second snapshot connection", async () => {
  const directory = await mkdtemp(path.join(tmpdir(), "efs-sqlite-")); const filename = path.join(directory, "filesystem.db");
  try {
    const first = await openNodeSqlite({ filename });
    first.transaction("write", (tx) => { tx.run("CREATE TABLE durable(value TEXT)"); tx.run("INSERT INTO durable VALUES(?)", ["committed"]); });
    const second = await openNodeSqlite({ filename });
    assert.equal(second.transaction("read", (tx) => tx.all("SELECT value FROM durable", [], { maxRows: 1, maxBytes: 1024 })[0].value), "committed");
    second.close(); first.close();
    const readOnly = await openNodeSqlite({ filename, readOnly: true, create: false });
    assert.equal(readOnly.transaction("read", (tx) => tx.all("SELECT value FROM durable", [], { maxRows: 1, maxBytes: 1024 })[0].value), "committed");
    assert.throws(() => readOnly.transaction("write", () => {}), /EROFS/); readOnly.close();
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test("bounded units of work roll back row and binding-byte overflow", async () => {
  const driver = await openNodeSqlite({ filename: ":memory:" });
  driver.transaction("write", (tx) => tx.run("CREATE TABLE bounded(value BLOB)"));
  assert.throws(() => runUnitOfWork(driver, "write", { maxRows: 1, maxBytes: 100 }, (tx) => {
    tx.run("INSERT INTO bounded VALUES(?)", [Uint8Array.of(1)]);
    tx.run("INSERT INTO bounded VALUES(?)", [Uint8Array.of(2)]);
  }), /row limit/);
  assert.equal(driver.transaction("read", (tx) => tx.all("SELECT count(*) count FROM bounded", [], { maxRows: 1, maxBytes: 100 })[0].count), 0);
  assert.throws(() => runUnitOfWork(driver, "write", { maxRows: 10, maxBytes: 2 }, (tx) => tx.run("INSERT INTO bounded VALUES(?)", [Uint8Array.of(1, 2, 3)])), /byte limit/);
  assert.equal(driver.transaction("read", (tx) => tx.all("SELECT count(*) count FROM bounded", [], { maxRows: 1, maxBytes: 100 })[0].count), 0);
  driver.close();
});

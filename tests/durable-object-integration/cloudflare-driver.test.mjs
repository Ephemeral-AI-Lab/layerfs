import assert from "node:assert/strict";
import { test } from "node:test";
import { EphemeralFS } from "../../packages/fs/dist/index.js";
import { openCloudflareSqlite } from "../../packages/sqlite-cloudflare/dist/index.js";
import { openNodeSqlite } from "../../packages/sqlite-node/dist/index.js";

class Cursor {
  constructor(rows, rowsWritten = 0) { this.rows = rows; this.rowsRead = 0; this.rowsWritten = rowsWritten; }
  *[Symbol.iterator]() { for (const row of this.rows) { this.rowsRead += 1; yield row; } }
  toArray() { return [...this]; }
}

class RecordingDurableObjectStorage {
  current;
  constructor(node) { this.node = node; this.sql = { exec: (query, ...bindings) => this.exec(query, bindings), get databaseSize() { return 0; } }; }
  transactionSync(callback) { return this.node.transaction("write", (tx) => { this.current = tx; try { return callback(); } finally { this.current = undefined; } }); }
  exec(query, bindings) {
    if (!this.current) throw new Error("SQL used outside transactionSync"); const source = query.trim(); const returnsRows = /^SELECT\b/i.test(source) || (/^PRAGMA\b/i.test(source) && !source.includes("="));
    if (returnsRows) return new Cursor(this.current.all(query, bindings, { maxRows: 200_000, maxBytes: 256 * 1024 * 1024 }));
    const result = this.current.run(query, bindings); return new Cursor([], result.changes);
  }
}

test("Cloudflare adapter normalizes the transaction contract and finite capabilities", async () => {
  const node = await openNodeSqlite({ filename: ":memory:" }); const storage = new RecordingDurableObjectStorage(node); const driver = await openCloudflareSqlite({ storage });
  assert.equal(driver.capabilities.maxBlobBytes, 2 * 1024 * 1024); assert.equal(driver.capabilities.maxBindings, 100); assert.equal(driver.capabilities.journalMode, "runtime-managed"); assert.ok(Number.isFinite(driver.capabilities.maxPhysicalDatabaseBytes));
  let retained; driver.transaction("write", (tx) => { retained = tx; tx.run("CREATE TABLE sample(value BLOB)"); const bytes = Uint8Array.of(9, 1, 2, 3, 8); tx.run("INSERT INTO sample VALUES(?)", [bytes.subarray(1, 4)]); });
  assert.throws(() => retained.all("SELECT 1", [], { maxRows: 1, maxBytes: 10 }), /no longer active/); const value = driver.transaction("read", (tx) => tx.all("SELECT value FROM sample", [], { maxRows: 1, maxBytes: 100 })[0].value); assert.deepEqual([...value], [1, 2, 3]);
  assert.throws(() => driver.transaction("read", (tx) => tx.all(`SELECT ${Array.from({ length: 101 }, () => "?").join(",")}`, Array(101).fill(1), { maxRows: 1, maxBytes: 1000 })), /binding limit/); driver.close(); node.close();
});

test("portable filesystem, branch, maintenance, and restart smoke pass through Durable Object SQLite", { timeout: 60_000 }, async () => {
  const node = await openNodeSqlite({ filename: ":memory:" }); const storage = new RecordingDurableObjectStorage(node); let driver = await openCloudflareSqlite({ storage }); let filesystem = await EphemeralFS.open({ database: driver });
  await filesystem.mkdir("/workspace", { recursive: true }); await filesystem.writeFile("/workspace/main", "main"); const branch = await filesystem.branches.create("do-branch"); await branch.writeFile("/workspace/branch", "branch"); const result = await branch.publish({ operationId: "do-op" }); assert.equal(result.outcome, "merged"); await branch.close();
  assert.equal(await filesystem.readFile("/workspace/branch", { encoding: "utf8" }), "branch"); const verification = await filesystem.maintenance.verify({ maxEntities: 1000 }); assert.equal(verification.complete, true); await filesystem.close(); driver.close();
  driver = await openCloudflareSqlite({ storage }); filesystem = await EphemeralFS.open({ database: driver }); assert.deepEqual(await filesystem.branches.replay("do-op"), result); assert.equal(await filesystem.readFile("/workspace/main", { encoding: "utf8" }), "main"); const collection = await filesystem.maintenance.collectGarbage(); assert.equal(collection.state, "complete"); await filesystem.close(); driver.close(); node.close();
});


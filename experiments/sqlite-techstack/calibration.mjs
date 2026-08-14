import { closeSync, fsyncSync, mkdirSync, openSync, readFileSync, statSync, unlinkSync, writeSync } from "node:fs";
import { join } from "node:path";
import sqlite3 from "sqlite3";
import { DATA, ROOT } from "./common.mjs";

const sqlite = sqlite3.verbose();
const ns = () => process.hrtime.bigint();
const elapsed = (start) => Number(ns() - start);
const source = readFileSync(join(DATA, "fixtures", "cal-sql-10m.bin"));
const RESULTS = join(ROOT, "results");
mkdirSync(RESULTS, { recursive: true });

function openDb(path) { return new Promise((resolve, reject) => { const db = new sqlite.Database(path, (e) => e ? reject(e) : resolve(db)); }); }
function closeDb(db) { return new Promise((resolve, reject) => db.close((e) => e ? reject(e) : resolve())); }
function exec(db, sql) { return new Promise((resolve, reject) => db.exec(sql, (e) => e ? reject(e) : resolve())); }
function run(db, sql, params = []) { return new Promise((resolve, reject) => db.run(sql, params, (e) => e ? reject(e) : resolve())); }

async function apfs(i) {
  const path = join(DATA, `cal-apfs-${i}.bin`); try { unlinkSync(path); } catch {}
  const fd = openSync(path, "w"); const buffer = Buffer.alloc(32768); let offset = 0;
  const start = ns();
  while (offset < source.length) {
    const count = Math.min(buffer.length, source.length - offset);
    source.copy(buffer, 0, offset, offset + count);
    writeSync(fd, buffer, 0, count);
    offset += count;
  }
  const beforeSync = ns(); fsyncSync(fd); const syncNs = elapsed(beforeSync); closeSync(fd);
  return { calibration: "CAL-APFS", bytes: source.length, duration_ns: elapsed(start), sync_ns: syncNs, retained: i > 0 };
}

async function sql(i) {
  const path = join(DATA, `cal-sql-${i}.sqlite`); try { unlinkSync(path); } catch {}
  const db = await openDb(path);
  await exec(db, "PRAGMA page_size=4096; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA mmap_size=0; PRAGMA cache_size=-65536; PRAGMA wal_autocheckpoint=0; CREATE TABLE payloads(payload BLOB NOT NULL) STRICT;");
  const start = ns(); await exec(db, "BEGIN IMMEDIATE;"); await run(db, "INSERT INTO payloads(payload) VALUES(?)", [source]); await exec(db, "COMMIT;");
  const durationNs = elapsed(start); await closeDb(db);
  const checkpointDb = await openDb(path); const checkpointStart = ns(); await exec(checkpointDb, "PRAGMA wal_checkpoint(TRUNCATE)"); const checkpointNs = elapsed(checkpointStart); await closeDb(checkpointDb);
  return { calibration: "CAL-SQL", bytes: source.length, duration_ns: durationNs, checkpoint_ns: checkpointNs, db_bytes: statSync(path).size, retained: i > 0 };
}

for (let i = 0; i < 6; i++) console.log(JSON.stringify(await apfs(i)));
for (let i = 0; i < 6; i++) console.log(JSON.stringify(await sql(i)));

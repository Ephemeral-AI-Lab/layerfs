import { spawnSync } from "node:child_process";
import { closeSync, fsyncSync, mkdirSync, openSync, readFileSync, statSync, unlinkSync, writeSync } from "node:fs";
import { join } from "node:path";
import sqlite3 from "sqlite3";
import {
  DATA, PROFILE, ROOT, SCHEMA, SQL, blake3Hasher, dbSizes, digestDomain, fromHex, hex, loadFixture, streamFileChunks,
} from "./common.mjs";

type Lane = "end_to_end" | "storage_only";
type Operation = "create" | "read" | "edit" | "storage_only" | "materialize";

function editBaseFixtureId(id: string): string { return id.replace(/^edit-small-/, "edit-base-"); }

const sqlite = sqlite3;
const RESULTS = join(ROOT, "results");
mkdirSync(RESULTS, { recursive: true });

const ns = () => process.hrtime.bigint();
const delta = (start: bigint) => Number(ns() - start);

function openDb(path: string): Promise<any> {
  return new Promise((resolve, reject) => {
    const db = new sqlite.Database(path, (error: Error | null) => error ? reject(error) : resolve(db));
  });
}
function closeDb(db: any): Promise<void> { return new Promise((resolve, reject) => db.close((e: Error | null) => e ? reject(e) : resolve())); }
function exec(db: any, sql: string): Promise<void> { return new Promise((resolve, reject) => db.exec(sql, (e: Error | null) => e ? reject(e) : resolve())); }
function run(db: any, sql: string, params: any[] = []): Promise<{ changes: number }> {
  return new Promise((resolve, reject) => db.run(sql, params, function (e: Error | null) { e ? reject(e) : resolve({ changes: this.changes }); }));
}
function get(db: any, sql: string, params: any[] = []): Promise<any> { return new Promise((resolve, reject) => db.get(sql, params, (e: Error | null, row: any) => e ? reject(e) : resolve(row))); }
function all(db: any, sql: string, params: any[] = []): Promise<any[]> { return new Promise((resolve, reject) => db.all(sql, params, (e: Error | null, rows: any[]) => e ? reject(e) : resolve(rows))); }

async function configure(db: any, withSchema: boolean) {
  await exec(db, "PRAGMA page_size=4096; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA mmap_size=0; PRAGMA cache_size=-65536; PRAGMA wal_autocheckpoint=0; PRAGMA foreign_keys=ON;");
  if (withSchema) await exec(db, SCHEMA);
}

async function effectivePragmas(db: any) {
  const journalMode = await get(db, "PRAGMA journal_mode");
  const synchronous = await get(db, "PRAGMA synchronous");
  const pageSize = await get(db, "PRAGMA page_size");
  const mmapSize = await get(db, "PRAGMA mmap_size");
  const cacheSize = await get(db, "PRAGMA cache_size");
  const walAutocheckpoint = await get(db, "PRAGMA wal_autocheckpoint");
  const foreignKeys = await get(db, "PRAGMA foreign_keys");
  return { journal_mode: String(journalMode.journal_mode).toLowerCase(), synchronous: Number(synchronous.synchronous), page_size: Number(pageSize.page_size), mmap_size: Number(mmapSize.mmap_size), cache_size: Number(cacheSize.cache_size), wal_autocheckpoint: Number(walAutocheckpoint.wal_autocheckpoint), foreign_keys: Number(foreignKeys.foreign_keys) };
}

function removeDbFiles(path: string) {
  for (const suffix of ["", "-wal", "-shm"]) try { unlinkSync(path + suffix); } catch {}
}

async function tableCounts(db: any) {
  const row = await get(db, "SELECT (SELECT COUNT(*) FROM objects) AS objects, (SELECT COUNT(*) FROM roots) AS roots, (SELECT COUNT(*) FROM root_members) AS members, (SELECT COUNT(*) FROM current_head) AS heads");
  return { objects: Number(row.objects), roots: Number(row.roots), members: Number(row.members), heads: Number(row.heads) };
}

function assertCounts(actual: any, expected: any, label: string) {
  for (const key of ["objects", "roots", "members", "heads"]) if (actual[key] !== expected[key]) throw new Error(`${label} ${key} count mismatch: ${actual[key]} != ${expected[key]}`);
}

async function admit(db: any, object: any, bytes: Buffer, stats: any) {
  const row = await get(db, SQL.selectObject, [object.kind, fromHex(object.digest)]); stats.statements++; stats.rows_read++;
  if (row) {
    if (row.object_length !== bytes.length || !Buffer.from(row.object_bytes).equals(bytes)) throw new Error(`same typed ID has unequal bytes: ${object.digest}`);
    stats.reused_objects++;
    return;
  }
  await run(db, SQL.insertObject, [object.kind, fromHex(object.digest), object.length, bytes]); stats.statements++; stats.rows_inserted++;
  stats.new_objects++;
}

function fixtureObjects(fixture: any) {
  const byDigest = new Map(fixture.objects.map((object: any) => [object.digest, object]));
  return [...byDigest.values()];
}

async function writeRoot(db: any, fixture: any, stats: any) {
  const root = fromHex(fixture.root_id);
  const manifest = Buffer.from(fixture.manifest_b64, "base64");
  await run(db, SQL.insertRoot, [root, fromHex(fixture.manifest_digest), fromHex(fixture.logical_digest), fixture.bytes, manifest, fixture.object_count]); stats.statements++; stats.rows_inserted++;
  for (let ordinal = 0; ordinal < fixture.objects.length; ordinal++) {
    const object = fixture.objects[ordinal];
    await run(db, SQL.insertMember, [root, ordinal, object.kind, fromHex(object.digest), object.length]); stats.statements++; stats.rows_inserted++;
  }
  const head = await get(db, SQL.selectHead); stats.statements++; stats.rows_read++;
  const generation = head ? Number(head.generation) + 1 : 1;
  await run(db, SQL.upsertHead, [root, generation]); stats.statements++; stats.rows_inserted++;
}

async function verifyRoot(db: any, fixture: any, stats: any) {
  const head = await get(db, SQL.selectHead); stats.statements++; stats.rows_read++;
  if (!head || hex(head.root_id) !== fixture.root_id) throw new Error("current head mismatch");
  const root = await get(db, SQL.selectRoot, [fromHex(fixture.root_id)]); stats.statements++; stats.rows_read++;
  if (!root || hex(root.logical_digest) !== fixture.logical_digest || root.logical_bytes !== fixture.bytes) throw new Error("root metadata mismatch");
  const members = await all(db, SQL.selectMembers, [fromHex(fixture.root_id)]); stats.statements++; stats.rows_read += members.length;
  if (members.length !== fixture.objects.length) throw new Error("member count mismatch");
  const logical = await blake3Hasher(); logical.update(Buffer.from("logical-v1\0"));
  for (const member of members) {
    const object = await get(db, SQL.selectObject, [member.object_kind, member.object_digest]); stats.statements++; stats.rows_read++;
    if (!object) throw new Error("missing member object");
    logical.update(Buffer.from(object.object_bytes));
  }
  if (hex(logical.digest("binary")) !== fixture.logical_digest) throw new Error("reconstructed digest mismatch");
}

async function create(db: any, fixture: any, lane: Lane, stats: any) {
  const logical = await blake3Hasher(); logical.update(Buffer.from("logical-v1\0"));
  const sourcePhase = ns();
  if (lane === "end_to_end") {
    for (const chunk of streamFileChunks(fixture.source_path)) {
      logical.update(chunk);
      const object = { kind: 1, digest: hex(await digestDomain("object-v1", chunk)), length: chunk.length };
      await admit(db, object, chunk, stats);
    }
    stats.phase_source_cdc_ns = delta(sourcePhase);
  } else {
    for (const object of fixtureObjects(fixture)) {
      const chunk = readFileSync(join(DATA, "objects", `${object.kind}-${object.digest}.bin`));
      await admit(db, object, chunk, stats);
    }
    stats.phase_source_cdc_ns = "not_applicable";
    logical.update(Buffer.alloc(0));
  }
  if (lane === "end_to_end" && hex(logical.digest("binary")) !== fixture.logical_digest) throw new Error("fixture logical digest mismatch");
  const rootPhase = ns();
  await writeRoot(db, fixture, stats);
  stats.phase_root_ns = delta(rootPhase);
}

async function doRead(db: any, fixture: any, stats: any) { await verifyRoot(db, fixture, stats); }

async function materialize(db: any, fixture: any, outputPath: string, stats: any) {
  const start = ns();
  const head = await get(db, SQL.selectHead); stats.statements++; stats.rows_read++;
  if (!head || hex(head.root_id) !== fixture.root_id) throw new Error("current head mismatch");
  const root = await get(db, SQL.selectRoot, [fromHex(fixture.root_id)]); stats.statements++; stats.rows_read++;
  if (!root || hex(root.logical_digest) !== fixture.logical_digest || root.logical_bytes !== fixture.bytes) throw new Error("root metadata mismatch");
  const members = await all(db, SQL.selectMembers, [fromHex(fixture.root_id)]); stats.statements++; stats.rows_read += members.length;
  if (members.length !== fixture.objects.length) throw new Error("member count mismatch");

  const logical = await blake3Hasher(); logical.update(Buffer.from("logical-v1\0"));
  const fd = openSync(outputPath, "w");
  let written = 0;
  try {
    for (const member of members) {
      const object = await get(db, SQL.selectObject, [member.object_kind, member.object_digest]); stats.statements++; stats.rows_read++;
      if (!object || Number(object.object_length) !== Number(member.object_length)) throw new Error("missing or malformed member object");
      const data = Buffer.from(object.object_bytes);
      logical.update(data);
      let offset = 0;
      while (offset < data.length) offset += writeSync(fd, data, offset, data.length - offset, null);
      written += data.length;
    }
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
  const digest = hex(logical.digest("binary"));
  if (digest !== fixture.logical_digest) throw new Error("materialized digest mismatch");
  if (written !== fixture.bytes || statSync(outputPath).size !== fixture.bytes) throw new Error("materialized byte count mismatch");
  stats.materialize_ns = delta(start);
  stats.materialized_bytes = written;
  stats.materialized_digest = digest;
}

async function setup(path: string, createSchema: boolean) {
  const db = await openDb(path); await configure(db, createSchema); return db;
}

async function seed(path: string, fixture: any, lane: Lane) {
  const db = await setup(path, true);
  await exec(db, "BEGIN IMMEDIATE;");
  await create(db, fixture, lane, { statements: 0, rows_read: 0, rows_inserted: 0, new_objects: 0, reused_objects: 0 });
  await exec(db, "COMMIT;");
  await closeDb(db);
  await checkpoint(path);
}

async function checkpoint(path: string) {
  const db = await setup(path, false); const start = ns(); await all(db, "PRAGMA wal_checkpoint(TRUNCATE)"); const elapsed = delta(start); await closeDb(db); return elapsed;
}

function crash(path: string, fixtureId: string, mode: string) {
  return (async () => {
    const fixture = loadFixture(fixtureId); const db = await setup(path, false);
    await exec(db, "BEGIN IMMEDIATE;");
    const stats = { statements: 0, rows_read: 0, rows_inserted: 0, new_objects: 0, reused_objects: 0 };
    await create(db, fixture, "end_to_end", stats);
    if (mode === "before-commit") process.exit(37);
    await exec(db, "COMMIT;"); process.exit(38);
  })().catch((error) => { console.error(error); process.exit(1); });
}

async function one(path: string, fixture: any, operation: Operation, lane: Lane, retain = false) {
  const before = dbSizes(path); const stats: any = { statements: 0, rows_read: 0, rows_inserted: 0, new_objects: 0, reused_objects: 0 };
  const setupStart = ns(); const db = await setup(path, true); const pragma = await effectivePragmas(db); const versionRow = await get(db, "SELECT sqlite_version() AS version"); const setupNs = delta(setupStart);
  await exec(db, "BEGIN IMMEDIATE;");
  const start = ns();
  const outputPath = `${path}.materialized`;
  if (operation === "materialize") { try { unlinkSync(outputPath); } catch {} await materialize(db, fixture, outputPath, stats); }
  else if (operation === "create" || operation === "edit" || operation === "storage_only") await create(db, fixture, lane, stats);
  else await doRead(db, fixture, stats);
  await exec(db, "COMMIT;");
  const commitNs = delta(start);
  await closeDb(db);
  const reopenStart = ns(); const reopened = await setup(path, false); await doRead(reopened, fixture, stats); await closeDb(reopened); const reopenNs = delta(reopenStart);
  const checkpointNs = await checkpoint(path);
  if (operation === "materialize") try { unlinkSync(outputPath); } catch {}
  const after = dbSizes(path);
  const usage = process.resourceUsage();
  return {
    schema: "sqlite-techstack-result-v1", candidate: "T-SQL", lane, operation, fixture_id: fixture.fixture_id,
    sqlite_version: versionRow.version,
    pragma,
    duration_ns: commitNs + reopenNs, phase_source_cdc_ns: stats.phase_source_cdc_ns ?? "not_observed", phase_root_ns: stats.phase_root_ns ?? "not_observed",
    phase_sql_commit_ns: commitNs, phase_materialize_ns: stats.materialize_ns ?? "not_observed", materialized_bytes: stats.materialized_bytes ?? "not_observed", materialization_mib_s: stats.materialize_ns ? (stats.materialized_bytes / (1024 * 1024)) / (stats.materialize_ns / 1e9) : "not_observed", materialized_digest: stats.materialized_digest ?? "not_observed", destination_sync: operation === "materialize", phase_close_reopen_verify_ns: reopenNs, checkpoint_ns: checkpointNs, setup_ns: setupNs,
    sqlite_prepare_calls: "unavailable", sqlite_statement_calls: stats.statements, sqlite_rows_read: stats.rows_read, sqlite_rows_inserted: stats.rows_inserted,
    new_objects: stats.new_objects, reused_objects: stats.reused_objects, db_bytes_before: before.db, db_bytes_after: after.db, wal_bytes_before: before.wal, wal_bytes_after: after.wal,
    user_cpu_ns: "unavailable", system_cpu_ns: "unavailable", peak_rss_bytes: usage.maxRSS * 1024, retained: retain,
  };
}

async function selfCheck() {
  const fixture = loadFixture("random-10m"); const path = join(DATA, "self-check-tsql.sqlite");
  removeDbFiles(path);
  const first = await one(path, fixture, "create", "end_to_end");
  if (first.sqlite_version !== "3.51.0") throw new Error(`wrong SQLite: ${first.sqlite_version}`);
  const child = spawnSync(process.execPath, ["--experimental-strip-types", join(ROOT, "tsql.ts"), "crash", path, "duplicate-10m", "before-commit"], { stdio: "inherit" });
  if (child.status !== 37) throw new Error(`crash injection status ${child.status}`);
  const reopened = await setup(path, false); const stats = { statements: 0, rows_read: 0 }; await doRead(reopened, fixture, stats); const baseline = await tableCounts(reopened); if (baseline.roots !== 1 || baseline.members !== fixture.objects.length || baseline.heads !== 1) throw new Error("baseline database count mismatch"); await closeDb(reopened);
  const beforeRecovered = await setup(path, false); await doRead(beforeRecovered, fixture, stats); assertCounts(await tableCounts(beforeRecovered), baseline, "before-commit recovery"); await closeDb(beforeRecovered);
  const reuse = await setup(path, false); await exec(reuse, "BEGIN IMMEDIATE;");
  const reuseStats = { statements: 0, rows_read: 0, rows_inserted: 0, new_objects: 0, reused_objects: 0 };
  const object = fixture.objects[0]; const objectBytes = readFileSync(join(DATA, "objects", `${object.kind}-${object.digest}.bin`));
  await admit(reuse, object, objectBytes, reuseStats);
  if (reuseStats.reused_objects !== 1) throw new Error("equal object was not reused");
  await exec(reuse, "ROLLBACK;"); await closeDb(reuse);
  const committed = spawnSync(process.execPath, ["--experimental-strip-types", join(ROOT, "tsql.ts"), "crash", path, "duplicate-10m", "after-commit"], { stdio: "inherit" });
  if (committed.status !== 38) throw new Error(`after-commit injection status ${committed.status}`);
  const duplicate = loadFixture("duplicate-10m"); const newRoot = await setup(path, false); const newStats = { statements: 0, rows_read: 0 }; await doRead(newRoot, duplicate, newStats); const newObjects = new Set(duplicate.objects.map((item: any) => `${item.kind}:${item.digest}`)); for (const item of fixture.objects) newObjects.delete(`${item.kind}:${item.digest}`); assertCounts(await tableCounts(newRoot), { objects: baseline.objects + newObjects.size, roots: 2, members: baseline.members + duplicate.objects.length, heads: 1 }, "after-commit recovery"); await closeDb(newRoot);
  const corrupt = await setup(path, false); await exec(corrupt, "BEGIN IMMEDIATE;");
  const bytes = Buffer.from("wrong");
  let rejected = false; try { await admit(corrupt, object, bytes, { statements: 0, rows_read: 0, rows_inserted: 0, new_objects: 0, reused_objects: 0 }); } catch (error: any) { if (!String(error.message).includes("unequal")) throw error; rejected = true; } if (!rejected) throw new Error("unequal same-ID accepted");
  await exec(corrupt, "ROLLBACK;"); await closeDb(corrupt);
  console.log(JSON.stringify({ self_check: "pass", fixture: fixture.fixture_id, root_id: fixture.root_id, sqlite_version: first.sqlite_version, bounded_buffer_bytes: PROFILE.max }));
}

async function main() {
  const [, , command, fixtureId = "random-10m", operation = "create"] = process.argv;
  if (command === "crash") return crash(fixtureId, operation, process.argv[5]);
  if (command === "self-check") return selfCheck();
  const fixture = loadFixture(fixtureId); const lane = operation === "storage_only" ? "storage_only" : "end_to_end";
  const samples = [];
  for (let i = 0; i < 6; i++) {
    const path = join(DATA, `tsql-${fixtureId}-${operation}-${i}.sqlite`); removeDbFiles(path);
    if (operation === "read" || operation === "materialize") await seed(path, fixture, "end_to_end");
    if (operation === "edit") await seed(path, loadFixture(editBaseFixtureId(fixtureId)), "end_to_end");
    samples.push(await one(path, fixture, operation, lane, i > 0));
  }
  samples.shift();
  for (const sample of samples) console.log(JSON.stringify(sample));
}

main().catch((error) => { console.error(error); process.exitCode = 1; });

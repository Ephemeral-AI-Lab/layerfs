//! Contended publish (#420): N writer processes publish M captured C12
//! Commits each onto one Branch, after a barrier. One attempt per publish:
//! an error is recorded and the writer moves to its next publish; there is
//! no retry path anywhere in this module.
//!
//!   contend ARM WRITERS M SNAP INFO SQLITE PG OUT
//!     ARM sqlite  shared Store file; each write transaction takes the
//!                 product's exclusive flock on the Store's directory, then
//!                 BEGIN IMMEDIATE (busy_timeout 0); the lane is that flock
//!                 held across head re-read, Commit row, conditional head
//!                 update and lineage.
//!         pg      every statement on Postgres, one round trip each; the lane
//!                 is pg_advisory_xact_lock + head read + Commit row +
//!                 conditional head update + lineage, in one transaction.
//!         cache1rt / cache2rt  Save on the writer's own SQLite cache
//!                 (SQLITE is a directory holding w<i>/store.sqlite); the
//!                 lane and the batched push of the Commit's new rows on
//!                 Postgres in one or two round trips. cache1rt-pre (extra
//!                 variant): one round trip that inserts the content rows
//!                 before taking the lock, so the lane covers only the
//!                 Commit row, head, lineage, totals and policy.
//!         remote-<cache arm>  diagnostic: that arm's remote lane and push
//!                 only, without the local Save (SQLITE unused).
//! Each writer process is single-threaded, so its FIFO turn order is trivial.
//! Synthetic rows never conflict: no merge-in-lane work happens.
use crate::workload::{build_tagged, hex, Kind, Tx, Workload, V};
use postgres::{types::ToSql, Client, SimpleQueryMessage};
use rusqlite::Connection;
use std::{
    collections::{HashMap, HashSet},
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const LOCK: &str = "SELECT pg_advisory_xact_lock(4200)";

/// Bundle arms: BTABLE (bundle table), PLAYOUT (a4|fit), BLEVEL (zstd).
fn bundle_env() -> (String, String, i32) {
    (
        std::env::var("BTABLE").unwrap_or_else(|_| "cb".into()),
        std::env::var("PLAYOUT").unwrap_or_else(|_| "a4".into()),
        std::env::var("BLEVEL").map(|l| l.parse().unwrap()).unwrap_or(3),
    )
}
/// Rows and totals of a bundle database.
fn counts_bundle(c: &mut Client) -> Vec<(String, i64)> {
    let (btable, layout, _) = bundle_env();
    let mut v = vec![
        ("pack".to_string(), c.query_one(&format!("SELECT count(DISTINCT pack_id) FROM pack_{layout}"), &[]).unwrap().get::<_, i64>(0)),
        ("bundle".to_string(), c.query_one(&format!("SELECT count(*) FROM {btable}"), &[]).unwrap().get::<_, i64>(0)),
        ("commit".to_string(), c.query_one("SELECT count(*) FROM \"commit\"", &[]).unwrap().get::<_, i64>(0)),
    ];
    for f in ["logical", "canonical", "stored", "metadata"] {
        v.push((format!("totals.{f}"), c.query_one(&format!("SELECT {f} FROM store_totals"), &[]).unwrap().get(0)));
    }
    v
}

fn now_ns() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
}
fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

#[derive(Default)]
struct Rec {
    start_ns: u128,
    end_ns: u128,
    latency_ms: f64,
    lane_wait_ms: f64,
    lane_hold_ms: f64,
    save_wait_ms: f64,
    statements: usize,
    round_trips: usize,
    commit: String,
    parent: String,
    error: Option<String>,
}
impl Rec {
    fn json(&self, writer: usize, i: usize) -> String {
        format!(
            "{{\"writer\":{writer},\"i\":{i},\"start_ns\":{},\"end_ns\":{},\"latency_ms\":{:.3},\"lane_wait_ms\":{:.3},\"lane_hold_ms\":{:.3},\"save_wait_ms\":{:.3},\"statements\":{},\"round_trips\":{},\"commit\":\"{}\",\"parent\":\"{}\",\"error\":{}}}",
            self.start_ns, self.end_ns, self.latency_ms, self.lane_wait_ms, self.lane_hold_ms, self.save_wait_ms,
            self.statements, self.round_trips, self.commit, self.parent,
            self.error.as_ref().map(|e| format!("{e:?}")).unwrap_or("null".into())
        )
    }
}

/// The product's writer admission: a fresh description of the Store's
/// directory, one blocking exclusive flock, released after COMMIT.
fn flock(dir: &Path) -> nix::fcntl::Flock<std::fs::File> {
    let file = std::fs::File::open(dir).unwrap();
    nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusive).map_err(|(_, e)| e).unwrap()
}

fn open_sqlite(path: &str) -> Connection {
    let c = Connection::open(path).unwrap();
    let mode: String = c.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, "wal");
    c.execute_batch("PRAGMA synchronous=0; PRAGMA foreign_keys=1; PRAGMA checkpoint_fullfsync=1; PRAGMA wal_autocheckpoint=1000; PRAGMA journal_size_limit=4194304; PRAGMA cache_size=-2048; PRAGMA mmap_size=0; PRAGMA temp_store=2; PRAGMA busy_timeout=0;").unwrap();
    c.set_prepared_statement_cache_capacity(512);
    c
}

fn sqlite_rows(c: &Connection, sql: &str, params: &[V]) -> Result<Vec<Vec<rusqlite::types::Value>>, String> {
    let mut p = c.prepare_cached(sql).map_err(|e| e.to_string())?;
    let n = p.column_count();
    let mut rows = p.query(rusqlite::params_from_iter(params.iter())).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    while let Some(r) = rows.next().map_err(|e| e.to_string())? {
        out.push((0..n).map(|i| r.get::<_, rusqlite::types::Value>(i).unwrap()).collect());
    }
    Ok(out)
}

/// Save transactions (everything but the history publish) on SQLite; write
/// transactions under the directory flock when `dir` is given.
fn save_sqlite(w: &Connection, r: &Connection, dir: &Path, txs: &[&Tx], rec: &mut Rec) -> Result<(), String> {
    for t in txs {
        let c = if t.reader { r } else { w };
        let lock = if t.kind == Kind::Write {
            let at = Instant::now();
            let l = flock(dir);
            rec.save_wait_ms += ms(at.elapsed());
            Some(l)
        } else {
            None
        };
        let body = || -> Result<(), String> {
            match t.kind {
                Kind::Write => sqlite_rows(c, "BEGIN IMMEDIATE", &[]).map(drop)?,
                Kind::Read => sqlite_rows(c, "BEGIN", &[]).map(drop)?,
                Kind::Auto => {}
            }
            for s in &t.stmts {
                sqlite_rows(c, &s.sql, &s.params).map_err(|e| format!("{}: {e}", s.label))?;
            }
            if t.kind != Kind::Auto {
                sqlite_rows(c, "COMMIT", &[])?;
            }
            Ok(())
        };
        let result = body();
        rec.statements += t.stmts.len() + if t.kind == Kind::Auto { 0 } else { 2 };
        if result.is_err() && !c.is_autocommit() {
            let _ = c.execute_batch("ROLLBACK");
        }
        if let Some(l) = lock {
            l.unlock().map_err(|(_, e)| e).unwrap();
        }
        result?;
    }
    Ok(())
}

fn commit_params(wl: &Workload, parent: &Option<Vec<u8>>) -> Vec<V> {
    let (cols, row) = &wl.commit_row;
    let mut row = row.clone();
    let i = cols.iter().position(|c| c == "parent_commit_id").unwrap();
    row[i] = parent.clone().map(V::B).unwrap_or(V::NB);
    row
}
/// One-row INSERT for the lane, in the captured column order.
fn insert_sql(table: &str, cols: &[String]) -> String {
    let names = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
    let marks = (1..=cols.len()).map(|i| format!("${i}")).collect::<Vec<_>>().join(",");
    format!("INSERT INTO \"{table}\"({names}) VALUES ({marks}) RETURNING 1")
}
const ADVANCE_SQLITE: &str = "UPDATE branch SET head_commit_id = $1 WHERE branch_id = $2 AND base_layer_id = $3 AND head_commit_id IS $4 RETURNING 1";
const ADVANCE_PG: &str = "UPDATE branch SET head_commit_id = $1 WHERE branch_id = $2 AND base_layer_id = $3 AND head_commit_id IS NOT DISTINCT FROM $4 RETURNING 1";
const HEAD: &str = "SELECT head_commit_id FROM branch WHERE branch_id = $1";

fn lane_sqlite(w: &Connection, dir: &Path, wl: &Workload, ins: &(String, String), rec: &mut Rec) -> Result<(), String> {
    let at = Instant::now();
    let l = flock(dir);
    rec.lane_wait_ms = ms(at.elapsed());
    let held = Instant::now();
    let body = || -> Result<Option<Vec<u8>>, String> {
        sqlite_rows(w, "BEGIN IMMEDIATE", &[])?;
        let head = match sqlite_rows(w, HEAD, &[V::B(wl.branch.clone())])?.first().map(|r| r[0].clone()) {
            Some(rusqlite::types::Value::Blob(b)) => Some(b),
            _ => None,
        };
        sqlite_rows(w, &ins.0, &commit_params(wl, &head))?;
        let advanced = sqlite_rows(
            w,
            ADVANCE_SQLITE,
            &[V::B(wl.new_head.clone()), V::B(wl.branch.clone()), V::B(wl.base_layer.clone()), head.clone().map(V::B).unwrap_or(V::NB)],
        )?;
        if advanced.len() != 1 {
            return Err("conditional head update matched no row".into());
        }
        sqlite_rows(w, &ins.1, &wl.lineage_row.1)?;
        sqlite_rows(w, "COMMIT", &[])?;
        Ok(head)
    };
    let result = body();
    rec.statements += 6;
    if result.is_err() && !w.is_autocommit() {
        let _ = w.execute_batch("ROLLBACK");
    }
    l.unlock().map_err(|(_, e)| e).unwrap();
    rec.lane_hold_ms = ms(held.elapsed());
    rec.parent = result?.map(|h| hex(&h)).unwrap_or_default();
    Ok(())
}

type Prepared = HashMap<String, postgres::Statement>;
fn pg_text(s: &crate::workload::St) -> &str {
    s.pg_sql.as_deref().unwrap_or(&s.sql)
}
fn prepare(c: &mut Client, txs: &[&Tx], extra: &[&str]) -> Prepared {
    let mut p = Prepared::new();
    for sql in txs.iter().flat_map(|t| t.stmts.iter().map(pg_text)).chain(extra.iter().copied()) {
        if !p.contains_key(sql) {
            p.insert(sql.to_owned(), c.prepare(sql).unwrap_or_else(|e| panic!("prepare {sql}: {e}")));
        }
    }
    p
}
fn pg_query(c: &mut Client, p: &Prepared, sql: &str, params: &[V]) -> Result<usize, String> {
    let boxed: Vec<Box<dyn ToSql + Sync>> = params.iter().map(V::pg).collect();
    let refs: Vec<&(dyn ToSql + Sync)> = boxed.iter().map(|b| b.as_ref()).collect();
    c.query(&p[sql], &refs).map(|r| r.len()).map_err(|e| e.to_string())
}

fn save_pg(w: &mut Client, r: &mut Client, wp: &Prepared, rp: &Prepared, txs: &[&Tx], rec: &mut Rec) -> Result<(), String> {
    for t in txs {
        let (c, p) = if t.reader { (&mut *r, rp) } else { (&mut *w, wp) };
        let body = |c: &mut Client| -> Result<(), String> {
            if t.kind != Kind::Auto {
                c.batch_execute("BEGIN").map_err(|e| e.to_string())?;
            }
            for s in &t.stmts {
                pg_query(c, p, pg_text(s), &s.params).map_err(|e| format!("{}: {e}", s.label))?;
            }
            if t.kind != Kind::Auto {
                c.batch_execute("COMMIT").map_err(|e| e.to_string())?;
            }
            Ok(())
        };
        let result = body(c);
        let n = t.stmts.len() + if t.kind == Kind::Auto { 0 } else { 2 };
        rec.statements += n;
        rec.round_trips += n;
        if result.is_err() {
            let _ = c.batch_execute("ROLLBACK");
        }
        result?;
    }
    Ok(())
}

fn lane_pg(c: &mut Client, p: &Prepared, wl: &Workload, ins: &(String, String), rec: &mut Rec) -> Result<(), String> {
    let body = |c: &mut Client, rec: &mut Rec| -> Result<Option<Vec<u8>>, String> {
        c.batch_execute("BEGIN").map_err(|e| e.to_string())?;
        let at = Instant::now();
        c.query(&p[LOCK], &[]).map_err(|e| e.to_string())?;
        rec.lane_wait_ms = ms(at.elapsed());
        let held = Instant::now();
        let head: Option<Vec<u8>> = c.query_one(&p[HEAD], &[&wl.branch]).map_err(|e| e.to_string())?.get(0);
        pg_query(c, p, &ins.0, &commit_params(wl, &head))?;
        let advanced = pg_query(
            c,
            p,
            ADVANCE_PG,
            &[V::B(wl.new_head.clone()), V::B(wl.branch.clone()), V::B(wl.base_layer.clone()), head.clone().map(V::B).unwrap_or(V::NB)],
        )?;
        if advanced != 1 {
            return Err("conditional head update matched no row".into());
        }
        pg_query(c, p, &ins.1, &wl.lineage_row.1)?;
        c.batch_execute("COMMIT").map_err(|e| e.to_string())?;
        rec.lane_hold_ms = ms(held.elapsed());
        Ok(head)
    };
    let result = body(c, rec);
    rec.statements += 7;
    rec.round_trips += 7;
    if result.is_err() {
        let _ = c.batch_execute("ROLLBACK");
    }
    rec.parent = result?.map(|h| hex(&h)).unwrap_or_default();
    Ok(())
}

fn literal_insert(table: &str, cols: &[String], rows: &[Vec<V>], suffix: &str) -> String {
    let names = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
    let rows = rows.iter().map(|r| format!("({})", r.iter().map(V::literal).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(",");
    format!("INSERT INTO \"{table}\"({names}) VALUES {rows}{suffix};")
}
const CLOCK: &str = "SELECT (extract(epoch from clock_timestamp())*1e6)::bigint::text;";

/// The Commit's new rows and history rows as one literal batch; `parent` is
/// a SQL expression (a literal, or the head read under the lock).
fn push_batch(wl: &Workload, parent: &str) -> String {
    rows_batch(wl) + &lane_batch(wl, parent)
}
/// The Save's new content rows (pack, objects, inode values, signatures,
/// digests, hints): no row another writer touches.
fn rows_batch(wl: &Workload) -> String {
    let mut b = String::new();
    for p in &wl.push {
        if !p.rows.is_empty() {
            b += &literal_insert(p.table, &p.columns, &p.rows, p.conflict);
        }
    }
    b
}
/// The part that must follow the lock: Commit row, conditional head update,
/// lineage, the shared totals and policy rows, and the observed parent.
fn lane_batch(wl: &Workload, parent: &str) -> String {
    let mut b = String::new();
    let (cols, row) = &wl.commit_row;
    let values = cols
        .iter()
        .zip(row)
        .map(|(c, v)| if c == "parent_commit_id" { parent.to_owned() } else { v.literal() })
        .collect::<Vec<_>>()
        .join(",");
    let new = V::B(wl.new_head.clone()).literal();
    let branch = V::B(wl.branch.clone()).literal();
    b += &format!("INSERT INTO \"commit\"({}) SELECT {values};", cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(","));
    b += &format!(
        "UPDATE branch SET head_commit_id = {new} WHERE branch_id = {branch} AND base_layer_id = {} AND head_commit_id IS NOT DISTINCT FROM (SELECT parent_commit_id FROM \"commit\" WHERE commit_id = {new});",
        V::B(wl.base_layer.clone()).literal()
    );
    b += &format!("DO $$BEGIN IF (SELECT head_commit_id FROM branch WHERE branch_id = {branch}) IS DISTINCT FROM {new} THEN RAISE EXCEPTION 'conditional head update matched no row'; END IF; END$$;");
    b += &literal_insert("commit_lineage", &wl.lineage_row.0, std::slice::from_ref(&wl.lineage_row.1), "");
    b += &format!("UPDATE store_totals SET canonical=canonical+{},stored=stored+{},metadata=metadata+{} WHERE id=1;", wl.totals[0], wl.totals[1], wl.totals[2]);
    b += &format!("UPDATE store_policy SET next_pack_id=GREATEST(next_pack_id,{}),next_ordinal=GREATEST(next_ordinal,{}) WHERE id=1;", wl.policy[0], wl.policy[1]);
    b += &format!("SELECT coalesce(encode(parent_commit_id,'hex'),'') FROM \"commit\" WHERE commit_id = {new};");
    b
}
fn texts(messages: &[SimpleQueryMessage]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|m| match m {
            SimpleQueryMessage::Row(r) => Some(r.get(0).unwrap_or("").to_owned()),
            _ => None,
        })
        .collect()
}
const PUSH_STATEMENTS: usize = 4 + 6 + 7;

fn lane_remote(c: &mut Client, wl: &Workload, variant: &str, rec: &mut Rec) -> Result<(), String> {
    let branch = V::B(wl.branch.clone()).literal();
    let head_sql = format!("(SELECT head_commit_id FROM branch WHERE branch_id = {branch})");
    let result: Result<(), String> = (|| {
        match variant {
            "cache1rt" => {
                let sql = format!("BEGIN; {CLOCK} {LOCK}; {CLOCK} {} {CLOCK} COMMIT;", push_batch(wl, &head_sql));
                let rows = texts(&c.simple_query(&sql).map_err(|e| e.to_string())?);
                // rows: clock, lock(void), clock, parent, clock
                let t: Vec<f64> = [0, 2, 4].iter().map(|&i| rows[i].parse::<f64>().unwrap()).collect();
                rec.lane_wait_ms = (t[1] - t[0]) / 1e3;
                rec.lane_hold_ms = (t[2] - t[1]) / 1e3;
                rec.parent = rows[3].clone();
                rec.round_trips += 1;
            }
            "cache1rt-pre" => {
                // Rows first (no lock needed), then the lane, in one round trip.
                let sql = format!("BEGIN; {} {CLOCK} {LOCK}; {CLOCK} {} {CLOCK} COMMIT;", rows_batch(wl), lane_batch(wl, &head_sql));
                let rows = texts(&c.simple_query(&sql).map_err(|e| e.to_string())?);
                let t: Vec<f64> = [0, 2, 4].iter().map(|&i| rows[i].parse::<f64>().unwrap()).collect();
                rec.lane_wait_ms = (t[1] - t[0]) / 1e3;
                rec.lane_hold_ms = (t[2] - t[1]) / 1e3;
                rec.parent = rows[3].clone();
                rec.round_trips += 1;
            }
            "cache2rt" => {
                let first = format!("BEGIN; {CLOCK} {LOCK}; {CLOCK} SELECT coalesce(encode(head_commit_id,'hex'),'') FROM branch WHERE branch_id = {branch};");
                let rows = texts(&c.simple_query(&first).map_err(|e| e.to_string())?);
                let (t0, t1) = (rows[0].parse::<f64>().unwrap(), rows[2].parse::<f64>().unwrap());
                let head = rows[3].clone();
                let parent = if head.is_empty() { "NULL".to_owned() } else { format!("'\\x{head}'::bytea") };
                let second = format!("{} {CLOCK} COMMIT;", push_batch(wl, &parent));
                let rows = texts(&c.simple_query(&second).map_err(|e| e.to_string())?);
                let t2 = rows[1].parse::<f64>().unwrap();
                rec.lane_wait_ms = (t1 - t0) / 1e3;
                rec.lane_hold_ms = (t2 - t1) / 1e3;
                if rows[0] != head {
                    return Err("parent differs from the head read under the lock".into());
                }
                rec.parent = head;
                rec.round_trips += 2;
            }
            other => panic!("variant {other}"),
        }
        Ok(())
    })();
    rec.statements += PUSH_STATEMENTS;
    if result.is_err() {
        let _ = c.batch_execute("ROLLBACK");
    }
    result
}

pub fn child(arm: &str, idx: usize, m: usize, snap: &str, info: &str, sqlite: &str, url: &str) {
    let workloads: Vec<Workload> = (0..m).map(|i| build_tagged(snap, info, Some((idx * m + i) as u64))).collect();
    let cache = arm.starts_with("cache") || arm == "bundle";
    let bundle_arm = arm.ends_with("bundle");
    let save_txs = |wl: &Workload| -> Vec<Tx> {
        wl.commit
            .iter()
            .filter(|t| t.label != "history.publish")
            .map(|t| Tx {
                kind: t.kind,
                reader: t.reader,
                label: t.label,
                stmts: t
                    .stmts
                    .iter()
                    .filter(|s| !(cache && s.label == "branch.by_id"))
                    .map(|s| crate::workload::St { sql: s.sql.clone(), pg_sql: s.pg_sql.clone(), params: s.params.clone(), label: s.label })
                    .collect(),
            })
            .collect()
    };
    let saves: Vec<Vec<Tx>> = workloads.iter().map(save_txs).collect();
    let ins = (insert_sql("commit", &workloads[0].commit_row.0), insert_sql("commit_lineage", &workloads[0].lineage_row.0));
    let store = if cache { format!("{sqlite}/w{idx}/store.sqlite") } else { sqlite.to_owned() };
    let dir = Path::new(&store).parent().unwrap().to_owned();
    let remote_only = arm.starts_with("remote-");
    let sq = (arm != "pg" && !remote_only).then(|| (open_sqlite(&store), open_sqlite(&store)));
    let mut pgc = (arm != "sqlite").then(|| (crate::pg(url), crate::pg(url)));
    // Untimed: statement caches warm, as on long-lived connections.
    let refs0: Vec<&Tx> = saves[0].iter().collect();
    let mut prepared = None;
    if let Some((w, r)) = &sq {
        for c in [w, r] {
            for sql in refs0.iter().flat_map(|t| t.stmts.iter().map(|s| s.sql.as_str())).chain(["BEGIN IMMEDIATE", "BEGIN", "COMMIT", HEAD, ins.0.as_str(), ins.1.as_str(), ADVANCE_SQLITE]) {
                drop(c.prepare_cached(sql).unwrap());
            }
        }
    }
    if let Some((w, r)) = pgc.as_mut().filter(|_| !bundle_arm) {
        let lane = [LOCK, HEAD, ins.0.as_str(), ins.1.as_str(), ADVANCE_PG];
        let wp = prepare(w, &refs0.iter().filter(|t| !t.reader).copied().collect::<Vec<_>>(), &lane);
        let rp = prepare(r, &refs0.iter().filter(|t| t.reader).copied().collect::<Vec<_>>(), &[]);
        prepared = Some((wp, rp));
    }
    println!("ready");
    std::io::stdout().flush().unwrap();
    let mut go = String::new();
    std::io::stdin().read_line(&mut go).unwrap();
    let mut out = Vec::new();
    for (i, wl) in workloads.iter().enumerate() {
        let mut rec = Rec { start_ns: now_ns(), commit: hex(&wl.new_head), ..Rec::default() };
        let t0 = Instant::now();
        let txs: Vec<&Tx> = saves[i].iter().collect();
        let result = match arm {
            "sqlite" => {
                let (w, r) = sq.as_ref().unwrap();
                save_sqlite(w, r, &dir, &txs, &mut rec).and_then(|_| lane_sqlite(w, &dir, wl, &ins, &mut rec))
            }
            "pg" => {
                let (w, r) = pgc.as_mut().unwrap();
                let (wp, rp) = prepared.as_ref().unwrap();
                save_pg(w, r, wp, rp, &txs, &mut rec).and_then(|_| lane_pg(w, wp, wl, &ins, &mut rec))
            }
            "cache1rt" | "cache1rt-pre" | "cache2rt" => {
                let (w, r) = sq.as_ref().unwrap();
                let (pw, _) = pgc.as_mut().unwrap();
                save_sqlite(w, r, &dir, &txs, &mut rec).and_then(|_| lane_remote(pw, wl, arm, &mut rec))
            }
            // Diagnostic: the remote lane and push alone, no local Save.
            "bundle" | "remote-bundle" => {
                let save = if arm == "bundle" {
                    let (w, r) = sq.as_ref().unwrap();
                    save_sqlite(w, r, &dir, &txs, &mut rec)
                } else {
                    Ok(())
                };
                save.and_then(|_| {
                    let (pw, _) = pgc.as_mut().unwrap();
                    let (btable, layout, level) = bundle_env();
                    let mut refs = crate::codec::MemRefs::default();
                    let rows = crate::publish::rows_of(wl);
                    let (raw, _) = crate::codec::encode(&rows, 0, &wl.new_head, true, true, &mut refs);
                    let body = crate::codec::compress(&raw, level, None);
                    let pre = crate::publish::pre_lock_sql(wl, &btable, &layout, &body);
                    rec.statements += 4 + 1 + wl.push[0].rows.len() + 7;
                    rec.round_trips += 1;
                    crate::publish::publish(pw, wl, &pre).map(|(wait, hold, parent)| {
                        rec.lane_wait_ms = wait;
                        rec.lane_hold_ms = hold;
                        rec.parent = parent;
                    })
                })
            }
            "remote-cache1rt" | "remote-cache1rt-pre" | "remote-cache2rt" => {
                let (pw, _) = pgc.as_mut().unwrap();
                lane_remote(pw, wl, &arm["remote-".len()..], &mut rec)
            }
            other => panic!("arm {other}"),
        };
        rec.latency_ms = ms(t0.elapsed());
        rec.end_ns = now_ns();
        rec.error = result.err();
        out.push(rec.json(idx, i));
    }
    println!("[{}]", out.join(","));
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * p).round() as usize]
}

/// Row counts and totals of the database a run publishes into.
fn counts_sqlite(c: &Connection) -> Vec<(String, i64)> {
    let mut v: Vec<(String, i64)> = ["pack", "object_location", "inode_value", "binding_hint", "file_digest", "content_signature", "commit", "commit_lineage"]
        .iter()
        .map(|t| (t.to_string(), c.query_row(&format!("SELECT count(*) FROM \"{t}\""), [], |r| r.get(0)).unwrap()))
        .collect();
    for f in ["logical", "canonical", "stored", "metadata"] {
        v.push((format!("totals.{f}"), c.query_row(&format!("SELECT {f} FROM store_totals"), [], |r| r.get(0)).unwrap()));
    }
    v
}
fn counts_pg(c: &mut Client) -> Vec<(String, i64)> {
    let mut v: Vec<(String, i64)> = ["pack", "object_location", "inode_value", "binding_hint", "file_digest", "content_signature", "commit", "commit_lineage"]
        .iter()
        .map(|t| (t.to_string(), c.query_one(&format!("SELECT count(*) FROM \"{t}\""), &[]).unwrap().get::<_, i64>(0)))
        .collect();
    for f in ["logical", "canonical", "stored", "metadata"] {
        v.push((format!("totals.{f}"), c.query_one(&format!("SELECT {f} FROM store_totals"), &[]).unwrap().get(0)));
    }
    v
}

pub fn parent(arm: &str, writers: usize, m: usize, snap: &str, info: &str, sqlite: &str, url: &str, out: &str) {
    let probe = crate::workload::build_tagged(snap, info, Some(0));
    let branch = probe.branch.clone();
    let shared_pg = arm != "sqlite";
    // Start state of the database the lane publishes into.
    let (start_head, before): (Option<Vec<u8>>, Vec<(String, i64)>) = if shared_pg {
        let mut c = crate::pg(url);
        let counts = if arm.ends_with("bundle") { counts_bundle(&mut c) } else { counts_pg(&mut c) };
        (c.query_one(HEAD, &[&branch]).unwrap().get(0), counts)
    } else {
        let c = open_sqlite(sqlite);
        (c.query_row("SELECT head_commit_id FROM branch WHERE branch_id=?1", [&branch], |r| r.get(0)).unwrap(), counts_sqlite(&c))
    };
    let local_before: Vec<Vec<(String, i64)>> = if arm.starts_with("cache") || arm == "bundle" {
        (0..writers).map(|i| counts_sqlite(&open_sqlite(&format!("{sqlite}/w{i}/store.sqlite")))).collect()
    } else {
        Vec::new()
    };
    let exe = std::env::current_exe().unwrap();
    let mut kids: Vec<_> = (0..writers)
        .map(|i| {
            Command::new(&exe)
                .args(["contend-child", arm, &i.to_string(), &m.to_string(), snap, info, sqlite, url])
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    let mut readers: Vec<_> = kids.iter_mut().map(|k| BufReader::new(k.stdout.take().unwrap())).collect();
    for r in readers.iter_mut() {
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        assert_eq!(line.trim(), "ready", "writer did not get ready");
    }
    // Barrier: every writer is prepared; release them together.
    for k in kids.iter_mut() {
        k.stdin.as_mut().unwrap().write_all(b"go\n").unwrap();
        k.stdin.as_mut().unwrap().flush().unwrap();
    }
    let mut records: Vec<String> = Vec::new();
    for (k, r) in kids.iter_mut().zip(readers.iter_mut()) {
        let mut line = String::new();
        r.read_line(&mut line).unwrap();
        records.push(line.trim().to_owned());
        assert!(k.wait().unwrap().success(), "writer process failed");
    }
    // Parse records (flat JSON; the fields are fixed).
    struct R {
        start: u128,
        end: u128,
        lat: f64,
        wait: f64,
        hold: f64,
        save_wait: f64,
        st: usize,
        rt: usize,
        commit: String,
        parent: String,
        error: Option<String>,
    }
    let mut rs: Vec<R> = Vec::new();
    for list in &records {
        for obj in list.trim_start_matches('[').trim_end_matches(']').split("},{") {
            let field = |name: &str| -> String {
                let key = format!("\"{name}\":");
                let at = obj.find(&key).unwrap() + key.len();
                let rest = &obj[at..];
                if let Some(s) = rest.strip_prefix('"') {
                    s[..s.find('"').unwrap()].to_owned()
                } else {
                    rest.split([',', '}']).next().unwrap().to_owned()
                }
            };
            rs.push(R {
                start: field("start_ns").parse().unwrap(),
                end: field("end_ns").parse().unwrap(),
                lat: field("latency_ms").parse().unwrap(),
                wait: field("lane_wait_ms").parse().unwrap(),
                hold: field("lane_hold_ms").parse().unwrap(),
                save_wait: field("save_wait_ms").parse().unwrap(),
                st: field("statements").parse().unwrap(),
                rt: field("round_trips").parse().unwrap(),
                commit: field("commit"),
                parent: field("parent"),
                error: (field("error") != "null").then(|| field("error")),
            });
        }
    }
    let n = rs.len();
    let errors: Vec<&String> = rs.iter().filter_map(|r| r.error.as_ref()).collect();
    let wall_ms = (rs.iter().map(|r| r.end).max().unwrap() - rs.iter().map(|r| r.start).min().unwrap()) as f64 / 1e6;
    let mut lat: Vec<f64> = rs.iter().map(|r| r.lat).collect();
    let mut wait: Vec<f64> = rs.iter().map(|r| r.wait).collect();
    let mut hold: Vec<f64> = rs.iter().map(|r| r.hold).collect();
    let mut save_wait: Vec<f64> = rs.iter().map(|r| r.save_wait).collect();
    let st = rs.iter().map(|r| r.st).sum::<usize>() as f64 / n as f64;
    let rt = rs.iter().map(|r| r.rt).sum::<usize>() as f64 / n as f64;

    // Correctness: walk the chain from the final head to the start head.
    let acked: HashMap<String, String> = rs.iter().filter(|r| r.error.is_none()).map(|r| (r.commit.clone(), r.parent.clone())).collect();
    let dup = rs.iter().filter(|r| r.error.is_none()).count() - acked.len();
    let mut chain: Vec<(String, String)> = Vec::new();
    let (final_head, after, fk) = if shared_pg {
        let mut c = crate::pg(url);
        let final_head: Option<Vec<u8>> = c.query_one(HEAD, &[&branch]).unwrap().get(0);
        let mut at = final_head.clone();
        while at != start_head && chain.len() <= n + 1 {
            let h = at.clone().unwrap();
            let p: Option<Vec<u8>> = c.query_one("SELECT parent_commit_id FROM \"commit\" WHERE commit_id=$1", &[&h]).unwrap().get(0);
            chain.push((hex(&h), p.as_deref().map(hex).unwrap_or_default()));
            at = p;
        }
        if arm.ends_with("bundle") {
            // Orphans among the Commit/Branch rows, and every published
            // bundle decoded and counted against the Commit's new rows.
            let fk: i64 = c
                .query_one("SELECT (SELECT count(*) FROM \"commit\" c WHERE c.parent_commit_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM \"commit\" p WHERE p.commit_id=c.parent_commit_id)) + (SELECT count(*) FROM branch b WHERE b.head_commit_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM \"commit\" p WHERE p.commit_id=b.head_commit_id))", &[])
                .unwrap()
                .get(0);
            let (btable, _, _) = bundle_env();
            let want = crate::publish::rows_of(&probe);
            let dd = crate::codec::DDict::new(None);
            let (mut decoded, mut exact) = (0, 0);
            let mut seqs: Vec<i64> = Vec::new();
            for r in c.query(&format!("SELECT b.body, c.commit_id, c.seq FROM {btable} b JOIN \"commit\" c USING (commit_id) WHERE c.seq > (SELECT max(seq) FROM \"commit\") - $1 ORDER BY c.seq"), &[&(n as i64)]).unwrap() {
                let body: Vec<u8> = r.get(0);
                let id: Vec<u8> = r.get(1);
                seqs.push(r.get(2));
                let mut refs = crate::codec::MemRefs::default();
                let (rows, _) = crate::codec::decode(&dd.decompress(&body), 0, &id, &mut refs);
                decoded += 1;
                let same = [rows.ol.len() == want.ol.len(), rows.iv.len() == want.iv.len(), rows.bh.len() == want.bh.len(), rows.fd.len() == want.fd.len(), rows.cs.len() == want.cs.len(), rows.cl.len() == want.cl.len()];
                exact += usize::from(same.iter().all(|x| *x));
            }
            let gapless = seqs.windows(2).all(|w| w[1] == w[0] + 1);
            (final_head, counts_bundle(&mut c), format!("{{\"orphans\":{fk},\"bundles_decoded\":{decoded},\"bundles_exact_rows\":{exact},\"seq_gapless\":{gapless}}}"))
        } else {
        let fk: i64 = c
            .query_one(
                "SELECT (SELECT count(*) FROM object_location o WHERE NOT EXISTS (SELECT 1 FROM pack p WHERE p.pack_id=o.pack_id)) + (SELECT count(*) FROM content_signature s WHERE NOT EXISTS (SELECT 1 FROM object_location o WHERE o.object_id=s.object_id)) + (SELECT count(*) FROM \"commit\" c WHERE c.parent_commit_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM \"commit\" p WHERE p.commit_id=c.parent_commit_id)) + (SELECT count(*) FROM branch b WHERE b.head_commit_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM \"commit\" p WHERE p.commit_id=b.head_commit_id))",
                &[],
            )
            .unwrap()
            .get(0);
        (final_head, counts_pg(&mut c), format!("{{\"orphans\":{fk}}}"))
        }
    } else {
        let c = open_sqlite(sqlite);
        let final_head: Option<Vec<u8>> = c.query_row("SELECT head_commit_id FROM branch WHERE branch_id=?1", [&branch], |r| r.get(0)).unwrap();
        let mut at = final_head.clone();
        while at != start_head && chain.len() <= n + 1 {
            let h = at.clone().unwrap();
            let p: Option<Vec<u8>> = c.query_row("SELECT parent_commit_id FROM \"commit\" WHERE commit_id=?1", [&h], |r| r.get(0)).unwrap();
            chain.push((hex(&h), p.as_deref().map(hex).unwrap_or_default()));
            at = p;
        }
        let integrity: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
        let fkc: i64 = c.prepare("PRAGMA foreign_key_check").unwrap().query_map([], |_| Ok(())).unwrap().count() as i64;
        (final_head, counts_sqlite(&c), format!("{{\"integrity_check\":{integrity:?},\"foreign_key_violations\":{fkc}}}"))
    };
    let chain_set: HashSet<&String> = chain.iter().map(|(c, _)| c).collect();
    let acked_set: HashSet<&String> = acked.keys().collect();
    let parents_match = chain.iter().all(|(c, p)| acked.get(c) == Some(p));
    let linear = chain.len() == n && chain_set.len() == n;
    // Expected growth per publish.
    let rows_of = |t: &str| probe.push.iter().find(|p| p.table == t).map(|p| p.rows.len() as i64).unwrap_or(0);
    let logical = probe.commit_row.1[probe.commit_row.0.iter().position(|c| c == "logical").unwrap()].clone();
    let logical = if let V::I(v) = logical { v } else { 0 };
    let k = (n - errors.len()) as i64;
    let mut expected: Vec<(String, i64)> = Vec::new();
    if arm.ends_with("bundle") {
        expected.push(("pack".into(), k * rows_of("pack")));
        expected.push(("bundle".into(), k));
        expected.push(("commit".into(), k));
    } else {
        for t in ["pack", "object_location", "inode_value", "binding_hint", "file_digest"] {
            expected.push((t.into(), k * rows_of(t)));
        }
        expected.push(("commit".into(), k));
        expected.push(("commit_lineage".into(), k));
    }
    expected.push(("totals.logical".into(), k * logical));
    expected.push(("totals.canonical".into(), k * probe.totals[0]));
    expected.push(("totals.stored".into(), k * probe.totals[1]));
    expected.push(("totals.metadata".into(), k * probe.totals[2]));
    let delta: HashMap<String, i64> = after.iter().zip(&before).map(|((n, a), (_, b))| (n.clone(), a - b)).collect();
    let growth = expected
        .iter()
        .map(|(t, e)| format!("\"{t}\":{{\"expected\":{e},\"actual\":{},\"ok\":{}}}", delta[t], delta[t] == *e))
        .collect::<Vec<_>>()
        .join(",");
    let growth_ok = expected.iter().all(|(t, e)| delta[t] == *e);
    let local = if arm.starts_with("cache") || arm == "bundle" {
        let parts: Vec<String> = (0..writers)
            .map(|i| {
                let c = open_sqlite(&format!("{sqlite}/w{i}/store.sqlite"));
                let integrity: String = c.query_row("PRAGMA integrity_check", [], |r| r.get(0)).unwrap();
                let a = counts_sqlite(&c);
                let ol = a[1].1 - local_before[i][1].1;
                format!("{{\"writer\":{i},\"integrity_check\":{integrity:?},\"object_location_growth\":{ol},\"expected\":{}}}", m as i64 * rows_of("object_location"))
            })
            .collect();
        format!(",\"local_caches\":[{}]", parts.join(","))
    } else {
        String::new()
    };
    let text = format!(
        "{{\"arm\":\"{arm}\",\"writers\":{writers},\"per_writer\":{m},\"publishes\":{n},\"wall_ms\":{wall_ms:.1},\"throughput_per_s\":{:.2},\
\"latency_ms\":{{\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},\"lane_wait_ms\":{{\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},\"lane_hold_ms\":{{\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}},\
\"save_writer_wait_ms\":{{\"p50\":{:.3},\"p95\":{:.3}}},\"statements_per_publish\":{st:.1},\"round_trips_per_publish\":{rt:.1},\
\"correctness\":{{\"errors\":{},\"error_samples\":{:?},\"retries\":0,\"duplicates\":{dup},\"chain_length\":{},\"linear\":{linear},\"chain_equals_acked\":{},\"parents_match_lane_head\":{parents_match},\"start_head\":\"{}\",\"final_head\":\"{}\",\"growth_ok\":{growth_ok},\"growth\":{{{growth}}},\"store_check\":{fk}}}{local}}}",
        n as f64 / (wall_ms / 1e3),
        pct(&mut lat, 0.5), pct(&mut lat, 0.95), pct(&mut lat, 1.0),
        pct(&mut wait, 0.5), pct(&mut wait, 0.95), pct(&mut wait, 1.0),
        pct(&mut hold, 0.5), pct(&mut hold, 0.95), pct(&mut hold, 1.0),
        pct(&mut save_wait, 0.5), pct(&mut save_wait, 0.95),
        errors.len(), errors.iter().take(3).collect::<Vec<_>>(),
        chain.len(), chain_set == acked_set,
        start_head.as_deref().map(hex).unwrap_or_default(),
        final_head.as_deref().map(hex).unwrap_or_default(),
    );
    std::fs::write(out, format!("{text}\n")).unwrap();
    std::fs::write(format!("{out}.records.jsonl"), records.join("\n") + "\n").unwrap();
    println!("{text}");
}

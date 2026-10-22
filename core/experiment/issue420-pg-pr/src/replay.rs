//! One arm of the captured C12 Store work, timed once.
//!
//! sqlite: every transaction on a writable clone with the product profile.
//! pg:     every transaction on a Postgres clone database, one round trip
//!         per statement (BEGIN and COMMIT included), statements prepared
//!         before timing, writer and reader on separate connections.
//! cache:  the local part on a SQLite clone (the host cache) and only the
//!         lane and publish on Postgres: advisory lock, head read, Commit
//!         row, head update, lineage and every new row of the Commit,
//!         as `stmt` (one round trip per statement), `2rt` (lock+head, then
//!         one batch) or `1rt` (one batch with a server-side head guard).
//! All statement caches are warm (prepared before timing) in every arm.
use crate::workload::{build, hex, summary, Kind, Push, Tx, Workload, V};
use postgres::{types::ToSql, Client, Statement};
use rusqlite::Connection;
use std::{collections::HashMap, time::Instant};

#[derive(Default, Clone, Copy)]
struct Stats {
    ms: f64,
    statements: usize,
    round_trips: usize,
    transactions: usize,
    writes: usize,
    rows: usize,
}
impl Stats {
    fn json(&self) -> String {
        format!(
            "{{\"ms\":{:.3},\"statements\":{},\"round_trips\":{},\"transactions\":{},\"write_transactions\":{},\"rows_returned\":{}}}",
            self.ms, self.statements, self.round_trips, self.transactions, self.writes, self.rows
        )
    }
    fn add(self, o: Stats) -> Stats {
        Stats {
            ms: self.ms + o.ms,
            statements: self.statements + o.statements,
            round_trips: self.round_trips + o.round_trips,
            transactions: self.transactions + o.transactions,
            writes: self.writes + o.writes,
            rows: self.rows + o.rows,
        }
    }
}

fn open_sqlite(path: &str) -> Connection {
    let c = Connection::open(path).unwrap();
    let mode: String = c.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, "wal", "clone is not WAL");
    for (name, value) in [
        ("synchronous", 0),
        ("foreign_keys", 1),
        ("checkpoint_fullfsync", 1),
        ("wal_autocheckpoint", 1000),
        ("journal_size_limit", 4194304),
        ("cache_size", -2048),
        ("mmap_size", 0),
        ("temp_store", 2),
        ("busy_timeout", 0),
    ] {
        c.query_row(&format!("PRAGMA {name}={value}"), [], |_| Ok(())).or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(()),
            e => Err(e),
        }).unwrap();
    }
    c.set_prepared_statement_cache_capacity(512);
    c
}

fn warm_sqlite(c: &Connection, txs: &[&Tx]) {
    for sql in ["BEGIN IMMEDIATE", "BEGIN", "COMMIT"] {
        drop(c.prepare_cached(sql).unwrap());
    }
    for t in txs {
        for s in &t.stmts {
            drop(c.prepare_cached(&s.sql).unwrap());
        }
    }
}

fn run_sqlite(w: &Connection, r: &Connection, txs: &[&Tx]) -> Stats {
    let mut st = Stats::default();
    let started = Instant::now();
    for t in txs {
        let c = if t.reader { r } else { w };
        let exec = |sql: &str, st: &mut Stats| {
            c.prepare_cached(sql).unwrap().execute([]).unwrap();
            st.statements += 1;
        };
        match t.kind {
            Kind::Write => exec("BEGIN IMMEDIATE", &mut st),
            Kind::Read => exec("BEGIN", &mut st),
            Kind::Auto => {}
        }
        for s in &t.stmts {
            let mut p = c.prepare_cached(&s.sql).unwrap();
            let mut rows = p.query(rusqlite::params_from_iter(s.params.iter())).unwrap_or_else(|e| panic!("{}: {e}", s.label));
            while rows.next().unwrap_or_else(|e| panic!("{}: {e}", s.label)).is_some() {
                st.rows += 1;
            }
            st.statements += 1;
        }
        if t.kind != Kind::Auto {
            exec("COMMIT", &mut st);
            st.transactions += 1;
            st.writes += usize::from(t.kind == Kind::Write);
        }
    }
    st.ms = started.elapsed().as_secs_f64() * 1e3;
    st.round_trips = 0;
    st
}

type Params = Vec<Vec<Box<dyn ToSql + Sync>>>;
struct Prepared {
    statements: HashMap<String, Statement>,
    params: Vec<Params>,
}
fn pg_text(s: &crate::workload::St) -> &str {
    s.pg_sql.as_deref().unwrap_or(&s.sql)
}
fn prepare_pg(w: &mut Client, r: &mut Client, txs: &[&Tx]) -> (Prepared, Prepared) {
    let mut wp = Prepared { statements: HashMap::new(), params: Vec::new() };
    let mut rp = Prepared { statements: HashMap::new(), params: Vec::new() };
    for t in txs {
        let (c, p) = if t.reader { (&mut *r, &mut rp) } else { (&mut *w, &mut wp) };
        for s in &t.stmts {
            let text = pg_text(s);
            if !p.statements.contains_key(text) {
                let stmt = c.prepare(text).unwrap_or_else(|e| panic!("prepare {}: {e}", s.label));
                p.statements.insert(text.to_owned(), stmt);
            }
        }
    }
    for t in txs {
        let params = t.stmts.iter().map(|s| s.params.iter().map(V::pg).collect()).collect();
        if t.reader {
            rp.params.push(params);
            wp.params.push(Vec::new());
        } else {
            wp.params.push(params);
            rp.params.push(Vec::new());
        }
    }
    (wp, rp)
}

fn run_pg(w: &mut Client, r: &mut Client, txs: &[&Tx], wp: &Prepared, rp: &Prepared) -> Stats {
    let mut st = Stats::default();
    let started = Instant::now();
    for (i, t) in txs.iter().enumerate() {
        let (c, p) = if t.reader { (&mut *r, rp) } else { (&mut *w, wp) };
        if t.kind != Kind::Auto {
            c.batch_execute("BEGIN").unwrap();
            st.statements += 1;
        }
        for (k, s) in t.stmts.iter().enumerate() {
            let refs: Vec<&(dyn ToSql + Sync)> = p.params[i][k].iter().map(|b| b.as_ref()).collect();
            let rows = c.query(&p.statements[pg_text(s)], &refs).unwrap_or_else(|e| panic!("{}: {e}", s.label));
            st.rows += rows.len();
            st.statements += 1;
        }
        if t.kind != Kind::Auto {
            c.batch_execute("COMMIT").unwrap();
            st.statements += 1;
            st.transactions += 1;
            st.writes += usize::from(t.kind == Kind::Write);
        }
    }
    st.ms = started.elapsed().as_secs_f64() * 1e3;
    st.round_trips = st.statements;
    st
}

fn literal_insert(p: &Push) -> String {
    let cols = p.columns.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
    let rows = p.rows.iter().map(|r| format!("({})", r.iter().map(V::literal).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(",");
    format!("INSERT INTO \"{}\"({cols}) VALUES {rows}{};", p.table, p.conflict)
}

/// The remote publish's writes, in FK order, as (sql, params) for `stmt`
/// and as one literal batch for `2rt`/`1rt`.
fn publish_writes(w: &Workload) -> (Vec<(String, Vec<V>)>, String) {
    let mut stmts = Vec::new();
    let mut batch = String::new();
    let (cc, cr) = &w.commit_row;
    let insert = |table: &str, cols: &[String], rows: usize, suffix: &str| {
        let width = cols.len();
        let tuples = (0..rows).map(|r| format!("({})", (1..=width).map(|c| format!("${}", r * width + c)).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(",");
        format!("INSERT INTO \"{table}\"({}) VALUES {tuples}{suffix}", cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(","))
    };
    for p in &w.push {
        if p.rows.is_empty() {
            continue;
        }
        stmts.push((insert(p.table, &p.columns, p.rows.len(), p.conflict), p.rows.iter().flatten().cloned().collect()));
        batch += &literal_insert(p);
    }
    let commit = Push { table: "commit", columns: cc.clone(), rows: vec![cr.clone()], conflict: "" };
    stmts.push((insert("commit", cc, 1, ""), cr.clone()));
    batch += &literal_insert(&commit);
    let (lc, lr) = &w.lineage_row;
    stmts.push((insert("commit_lineage", lc, 1, ""), lr.clone()));
    batch += &literal_insert(&Push { table: "commit_lineage", columns: lc.clone(), rows: vec![lr.clone()], conflict: "" });
    let advance = vec![V::B(w.new_head.clone()), V::B(w.branch.clone()), V::B(w.base_layer.clone())];
    stmts.push(("UPDATE branch SET head_commit_id = $1 WHERE branch_id = $2 AND base_layer_id = $3 RETURNING 1".into(), advance.clone()));
    batch += &format!("UPDATE branch SET head_commit_id = {} WHERE branch_id = {} AND base_layer_id = {};", advance[0].literal(), advance[1].literal(), advance[2].literal());
    let totals = w.totals.iter().map(|v| V::I(*v)).collect::<Vec<_>>();
    stmts.push(("UPDATE store_totals SET canonical=canonical+$1,stored=stored+$2,metadata=metadata+$3 WHERE id=1".into(), totals.clone()));
    batch += &format!("UPDATE store_totals SET canonical=canonical+{},stored=stored+{},metadata=metadata+{} WHERE id=1;", w.totals[0], w.totals[1], w.totals[2]);
    let policy = vec![V::I(w.policy[0]), V::I(w.policy[1])];
    stmts.push(("UPDATE store_policy SET next_pack_id=$1,next_ordinal=$2 WHERE id=1".into(), policy));
    batch += &format!("UPDATE store_policy SET next_pack_id={},next_ordinal={} WHERE id=1;", w.policy[0], w.policy[1]);
    (stmts, batch)
}

const LOCK: &str = "SELECT pg_advisory_xact_lock(4200)";
const HEAD: &str = "SELECT head_commit_id FROM branch WHERE branch_id = $1";

fn head_literal(w: &Workload) -> String {
    w.expected_head.as_ref().map(|h| format!("'\\x{}'::bytea", hex(h))).unwrap_or_else(|| "NULL".into())
}

/// pr copy: the remote lane and publish on the pr layout, one variant:
/// `1rt` = BEGIN; lock; head guard; content rows (pr::rows_batch); Commit,
/// lineage, head, totals, policy; COMMIT. `1rt-pre` = the same round trip
/// with the content rows inserted before the lock.
fn remote_commit(c: &mut Client, w: &Workload, variant: &str) -> Stats {
    let mut st = Stats { transactions: 1, writes: 1, ..Stats::default() };
    let branch = V::B(w.branch.clone());
    // Untimed: this backend's catalog caches warm for the tables touched.
    for sql in [LOCK, HEAD] {
        drop(c.prepare(sql).unwrap());
    }
    crate::pr::warm(c);
    let rows = crate::pr::rows_batch(w);
    let pushed = w.push.iter().filter(|p| !p.rows.is_empty()).count();
    let (cc, cr) = &w.commit_row;
    let mut lane = literal_insert(&Push { table: "commit", columns: cc.clone(), rows: vec![cr.clone()], conflict: "" });
    let (lc, lr) = &w.lineage_row;
    lane += &literal_insert(&Push { table: "commit_lineage", columns: lc.clone(), rows: vec![lr.clone()], conflict: "" });
    lane += &format!("UPDATE branch SET head_commit_id = {} WHERE branch_id = {} AND base_layer_id = {};", V::B(w.new_head.clone()).literal(), branch.literal(), V::B(w.base_layer.clone()).literal());
    lane += &format!("UPDATE store_totals SET canonical=canonical+{},stored=stored+{},metadata=metadata+{} WHERE id=1;", w.totals[0], w.totals[1], w.totals[2]);
    lane += &format!("UPDATE store_policy SET next_pack_id={},next_ordinal={} WHERE id=1;", w.policy[0], w.policy[1]);
    let guard = format!(
        "DO $$BEGIN IF NOT EXISTS (SELECT 1 FROM branch WHERE branch_id = {} AND head_commit_id IS NOT DISTINCT FROM {}) THEN RAISE EXCEPTION 'head moved'; END IF; END$$;",
        branch.literal(),
        head_literal(w)
    );
    let all = match variant {
        "1rt" => format!("BEGIN; {LOCK}; {guard} {rows} {lane} COMMIT;"),
        "1rt-pre" => format!("BEGIN; {rows} {LOCK}; {guard} {lane} COMMIT;"),
        other => panic!("pr variant {other}: only 1rt and 1rt-pre are ported"),
    };
    let started = Instant::now();
    c.batch_execute(&all).unwrap();
    st.ms = started.elapsed().as_secs_f64() * 1e3;
    // Same accounting as the plain port: BEGIN, lock, guard, COMMIT + one per write.
    st.statements = 4 + pushed + 5;
    st.round_trips = 1;
    st
}

/// The remote part of a host-cache mount: the head read and the global
/// serial reservation.
fn remote_mount(c: &mut Client, w: &Workload, variant: &str) -> Stats {
    let (scope, hw) = (&w.scope.0, w.scope.1);
    let mut st = Stats::default();
    let branch = V::B(w.branch.clone()).pg();
    let scope_p = V::B(scope.clone()).pg();
    for sql in [HEAD, "SELECT highwater, authority_id FROM scope_allocator WHERE scope_id = $1", "UPDATE scope_allocator SET highwater = $1 WHERE scope_id = $2 AND highwater = $3 RETURNING 1", "UPDATE scope_allocator SET highwater = highwater + 1024 WHERE scope_id = $1 RETURNING highwater"] {
        drop(c.prepare(sql).unwrap());
    }
    match variant {
        "stmt" => {
            let head = c.prepare(HEAD).unwrap();
            let read = c.prepare("SELECT highwater, authority_id FROM scope_allocator WHERE scope_id = $1").unwrap();
            let adv = c.prepare("UPDATE scope_allocator SET highwater = $1 WHERE scope_id = $2 AND highwater = $3 RETURNING 1").unwrap();
            let (next, old) = (hw + 1024, hw);
            let started = Instant::now();
            st.rows += c.query(&head, &[branch.as_ref()]).unwrap().len();
            c.batch_execute("BEGIN").unwrap();
            st.rows += c.query(&read, &[scope_p.as_ref()]).unwrap().len();
            st.rows += c.query(&adv, &[&next, scope_p.as_ref(), &old]).unwrap().len();
            c.batch_execute("COMMIT").unwrap();
            st.ms = started.elapsed().as_secs_f64() * 1e3;
            st.statements = 5;
            st.round_trips = 5;
            st.transactions = 1;
            st.writes = 1;
        }
        "2rt" => {
            let head = c.prepare(HEAD).unwrap();
            let adv = c.prepare("UPDATE scope_allocator SET highwater = highwater + 1024 WHERE scope_id = $1 RETURNING highwater").unwrap();
            let started = Instant::now();
            st.rows += c.query(&head, &[branch.as_ref()]).unwrap().len();
            st.rows += c.query(&adv, &[scope_p.as_ref()]).unwrap().len();
            st.ms = started.elapsed().as_secs_f64() * 1e3;
            st.statements = 2;
            st.round_trips = 2;
        }
        "1rt" => {
            let sql = format!(
                "SELECT head_commit_id FROM branch WHERE branch_id = {}; UPDATE scope_allocator SET highwater = highwater + 1024 WHERE scope_id = {} RETURNING highwater;",
                V::B(w.branch.clone()).literal(),
                V::B(scope.clone()).literal()
            );
            let started = Instant::now();
            st.rows += c.simple_query(&sql).unwrap().len();
            st.ms = started.elapsed().as_secs_f64() * 1e3;
            st.statements = 2;
            st.round_trips = 1;
        }
        other => panic!("variant {other}"),
    }
    st
}

fn head_sqlite(c: &Connection, w: &Workload) -> Option<Vec<u8>> {
    c.query_row("SELECT head_commit_id FROM branch WHERE branch_id=?1", [&w.branch], |r| r.get(0)).unwrap()
}
fn head_pg(c: &mut Client, w: &Workload) -> Option<Vec<u8>> {
    c.query_one("SELECT head_commit_id FROM branch WHERE branch_id=$1", &[&w.branch]).unwrap().get(0)
}

pub fn main(arm: &str, variant: &str, snap: &str, info: &str, sqlite: &str, url: &str, out: &str) {
    let w = build(snap, info);
    let mount: Vec<&Tx> = w.mount.iter().collect();
    let commit: Vec<&Tx> = w.commit.iter().collect();
    let mut parts: Vec<String> = Vec::new();
    let verified: bool;
    match arm {
        "sqlite" => {
            let (wc, rc) = (open_sqlite(sqlite), open_sqlite(sqlite));
            let all: Vec<&Tx> = mount.iter().chain(commit.iter()).copied().collect();
            warm_sqlite(&wc, &all);
            warm_sqlite(&rc, &all);
            let m = run_sqlite(&wc, &rc, &mount);
            let c = run_sqlite(&wc, &rc, &commit);
            parts.push(format!("\"mount\":{}", m.json()));
            parts.push(format!("\"commit\":{}", c.json()));
            verified = head_sqlite(&wc, &w).as_deref() == Some(&w.new_head[..]);
        }
        "pg" => {
            let (mut wc, mut rc) = (crate::pg(url), crate::pg(url));
            let all: Vec<&Tx> = mount.iter().chain(commit.iter()).copied().collect();
            let (mw, mr) = prepare_pg(&mut wc, &mut rc, &mount);
            let (cw, cr) = prepare_pg(&mut wc, &mut rc, &commit);
            drop(all);
            let m = run_pg(&mut wc, &mut rc, &mount, &mw, &mr);
            let c = run_pg(&mut wc, &mut rc, &commit, &cw, &cr);
            parts.push(format!("\"mount\":{}", m.json()));
            parts.push(format!("\"commit\":{}", c.json()));
            verified = head_pg(&mut wc, &w).as_deref() == Some(&w.new_head[..]);
        }
        "cache" | "cache-local" | "cache-remote" => {
            // Local: everything but the head reads and the history publish.
            let local_mount: Vec<Tx> = w
                .mount
                .iter()
                .filter(|t| !t.reader || t.kind != Kind::Write)
                .filter(|t| t.label != "attach.serials")
                .map(|t| Tx {
                    kind: t.kind,
                    reader: t.reader,
                    label: t.label,
                    stmts: t.stmts.iter().filter(|s| s.label != "branch.by_id").map(clone_st).collect(),
                })
                .collect();
            let local_commit: Vec<Tx> = w
                .commit
                .iter()
                .filter(|t| t.label != "history.publish")
                .map(|t| Tx {
                    kind: t.kind,
                    reader: t.reader,
                    label: t.label,
                    stmts: t.stmts.iter().filter(|s| s.label != "branch.by_id").map(clone_st).collect(),
                })
                .collect();
            let lm: Vec<&Tx> = local_mount.iter().collect();
            let lc: Vec<&Tx> = local_commit.iter().collect();
            // `cache-local` / `cache-remote` run one side alone, each on a
            // clone written last before it (equal cache state with the
            // sqlite and pg arms); `cache` runs both in one process.
            let local = arm != "cache-remote";
            let remote_side = arm != "cache-local";
            let (mut m_local, mut c_local) = (Stats::default(), Stats::default());
            if local {
                let (wc, rc) = (open_sqlite(sqlite), open_sqlite(sqlite));
                let all: Vec<&Tx> = lm.iter().chain(lc.iter()).copied().collect();
                warm_sqlite(&wc, &all);
                warm_sqlite(&rc, &all);
                m_local = run_sqlite(&wc, &rc, &lm);
                c_local = run_sqlite(&wc, &rc, &lc);
            }
            let (mut m_remote, mut c_remote) = (Stats::default(), Stats::default());
            verified = if remote_side {
                let mut remote = crate::pg(url);
                remote.batch_execute("SELECT 1").unwrap();
                m_remote = remote_mount(&mut remote, &w, if variant == "1rt-pre" { "1rt" } else { variant });
                c_remote = remote_commit(&mut remote, &w, variant);
                head_pg(&mut remote, &w).as_deref() == Some(&w.new_head[..])
            } else {
                true
            };
            for (name, local, rem) in [("mount", m_local, m_remote), ("commit", c_local, c_remote)] {
                parts.push(format!("\"{name}\":{{\"local\":{},\"remote\":{},\"total\":{}}}", local.json(), rem.json(), local.add(rem).json()));
            }
        }
        other => panic!("arm {other}"),
    }
    let text = format!(
        "{{\"arm\":\"{arm}\",\"variant\":\"{variant}\",\"base\":\"{}\",\"verified_head\":{verified},{},\"workload\":{}}}",
        if w.same_base { "a" } else { "b" },
        parts.join(","),
        summary(&w)
    );
    std::fs::write(out, format!("{text}\n")).unwrap();
    println!("{text}");
}

fn clone_st(s: &crate::workload::St) -> crate::workload::St {
    crate::workload::St { sql: s.sql.clone(), pg_sql: s.pg_sql.clone(), params: s.params.clone(), label: s.label }
}

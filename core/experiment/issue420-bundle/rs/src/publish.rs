//! Bundle publish (#420): the captured C12 Commit as one bundle row, pushed
//! with its packs in one round trip before the lane's lock is taken.
//!
//!   bpub SNAP INFO CACHE URL BTABLE PLAYOUT LEVEL DICT OUT
//!     local:  encode the Commit's new rows as a colref bundle against the
//!             host cache's numbering (CACHE, its side tables), zstd LEVEL
//!             (DICT file or '-'), then number its objects/names in the
//!             side tables (in a transaction rolled back afterwards so the
//!             cache stays at its applied seq for the catch-up case)
//!     remote: mount = head, scope reservation and newest seq in one round
//!             trip; publish = packs + bundle, then lock, head check,
//!             Commit row (seq = newest + 1), head, totals, policy; one
//!             round trip
//! The Save's local SQLite work is unchanged from the cache arm and is not
//! re-measured here (reused from the pg/20261022T190000Z cache-local receipt).
use crate::apply::SqlRefs;
use crate::codec::{compress, encode, Bh, Cl, Cs, Fd, Iv, Ol, Refs, Rows};
use crate::workload::{build, hex, Push, Workload, V};
use postgres::{Client, SimpleQueryMessage};
use std::time::Instant;

fn col(p: &Push, name: &str) -> usize {
    p.columns.iter().position(|c| c == name).unwrap_or_else(|| panic!("{}.{name}", p.table))
}
fn int(v: &V) -> i64 {
    match v {
        V::I(i) => *i,
        other => panic!("int {other:?}"),
    }
}
fn blob(v: &V) -> Vec<u8> {
    match v {
        V::B(b) => b.clone(),
        other => panic!("blob {other:?}"),
    }
}

/// The Commit's new small rows as a bundle's rows.
pub fn rows_of(w: &Workload) -> Rows {
    let mut rows = Rows::default();
    let t = |name: &str| w.push.iter().find(|p| p.table == name).unwrap();
    let p = t("object_location");
    for r in &p.rows {
        rows.ol.push(Ol {
            id: blob(&r[col(p, "object_id")]),
            role: int(&r[col(p, "role")]),
            clen: int(&r[col(p, "canonical_length")]),
            pack: int(&r[col(p, "pack_id")]),
            group: int(&r[col(p, "group_number")]),
            rec: int(&r[col(p, "record_number")]),
        });
    }
    let p = t("inode_value");
    for r in &p.rows {
        rows.iv.push(Iv { ordinal: int(&r[col(p, "ordinal")]), value: blob(&r[col(p, "value")]) });
    }
    let p = t("binding_hint");
    for r in &p.rows {
        rows.bh.push(Bh { child: int(&r[col(p, "child")]), parent: int(&r[col(p, "parent")]), name: blob(&r[col(p, "name")]) });
    }
    let p = t("file_digest");
    for r in &p.rows {
        rows.fd.push(Fd { digest: blob(&r[col(p, "digest")]), length: int(&r[col(p, "length")]), root: blob(&r[col(p, "root")]) });
    }
    let p = t("content_signature");
    for r in &p.rows {
        rows.cs.push(Cs { slot: int(&r[col(p, "slot")]), stamp: int(&r[col(p, "stamp")]), object: blob(&r[col(p, "object_id")]), signature: blob(&r[col(p, "signature")]) });
    }
    let (lc, lr) = &w.lineage_row;
    let li = |n: &str| lc.iter().position(|c| c == n).unwrap();
    rows.cl.push(Cl {
        run: blob(&lr[li("run")]),
        depth: int(&lr[li("depth")]),
        newest: int(&lr[li("newest")]),
        merged: match &lr[li("merged")] {
            V::B(b) => Some(b.clone()),
            _ => None,
        },
    });
    rows
}

fn lit(b: &[u8]) -> String {
    format!("'\\x{}'::bytea", hex(b))
}

/// The packs and the bundle row: no row another writer touches, so they go
/// before the lock.
pub fn pre_lock_sql(w: &Workload, btable: &str, layout: &str, body: &[u8]) -> String {
    let p = w.push.iter().find(|p| p.table == "pack").unwrap();
    let mut s = String::new();
    for r in &p.rows {
        let (id, dom, dig, b) = (int(&r[col(p, "pack_id")]), int(&r[col(p, "domain")]), blob(&r[col(p, "digest")]), blob(&r[col(p, "body")]));
        match layout {
            "a4" => s += &format!("INSERT INTO pack_a4 VALUES ({id},{},{dom},{},{});", b.len(), lit(&dig), lit(&b)),
            "fit" => {
                let mut at = 0;
                let mut seq = 0;
                while at < b.len() {
                    let cap = if seq == 0 { crate::store::SLICE0 } else { crate::store::SLICE };
                    let end = (at + cap).min(b.len());
                    let (d, g) = if seq == 0 { (dom.to_string(), lit(&dig)) } else { ("NULL".into(), "NULL".into()) };
                    s += &format!("INSERT INTO pack_fit VALUES ({id},{seq},{d},{g},{});", lit(&b[at..end]));
                    at = end;
                    seq += 1;
                }
            }
            other => panic!("layout {other}"),
        }
    }
    s += &format!("INSERT INTO {btable} (commit_id, body) VALUES ({}, {});", lit(&w.new_head), lit(body));
    s
}

/// After the lock: Commit row (parent = `parent` SQL, seq = newest + 1),
/// conditional head update and its check, totals, policy, the parent.
pub fn lane_sql(w: &Workload, parent: &str) -> String {
    let (cols, row) = &w.commit_row;
    let values = cols.iter().zip(row).map(|(c, v)| if c == "parent_commit_id" { parent.to_owned() } else { v.literal() }).collect::<Vec<_>>().join(",");
    let names = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
    let new = lit(&w.new_head);
    let branch = lit(&w.branch);
    let mut b = format!("INSERT INTO \"commit\"({names}, seq) SELECT {values}, (SELECT coalesce(max(seq),0)+1 FROM \"commit\");");
    b += &format!(
        "UPDATE branch SET head_commit_id = {new} WHERE branch_id = {branch} AND base_layer_id = {} AND head_commit_id IS NOT DISTINCT FROM (SELECT parent_commit_id FROM \"commit\" WHERE commit_id = {new});",
        lit(&w.base_layer)
    );
    b += &format!("DO $$BEGIN IF (SELECT head_commit_id FROM branch WHERE branch_id = {branch}) IS DISTINCT FROM {new} THEN RAISE EXCEPTION 'conditional head update matched no row'; END IF; END$$;");
    b += &format!("UPDATE store_totals SET canonical=canonical+{},stored=stored+{},metadata=metadata+{} WHERE id=1;", w.totals[0], w.totals[1], w.totals[2]);
    b += &format!("UPDATE store_policy SET next_pack_id=GREATEST(next_pack_id,{}),next_ordinal=GREATEST(next_ordinal,{}) WHERE id=1;", w.policy[0], w.policy[1]);
    b += &format!("SELECT coalesce(encode(parent_commit_id,'hex'),'') FROM \"commit\" WHERE commit_id = {new};");
    b
}

pub const LOCK: &str = "SELECT pg_advisory_xact_lock(4200)";
pub const CLOCK: &str = "SELECT (extract(epoch from clock_timestamp())*1e6)::bigint::text;";

pub fn texts(messages: &[SimpleQueryMessage]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|m| match m {
            SimpleQueryMessage::Row(r) => Some(r.get(0).unwrap_or("").to_owned()),
            _ => None,
        })
        .collect()
}

/// One round trip: rows first, then the lane. Returns (wait, hold, parent).
pub fn publish(c: &mut Client, w: &Workload, pre: &str) -> Result<(f64, f64, String), String> {
    let head = format!("(SELECT head_commit_id FROM branch WHERE branch_id = {})", lit(&w.branch));
    let sql = format!("BEGIN; {pre} {CLOCK} {LOCK}; {CLOCK} {} {CLOCK} COMMIT;", lane_sql(w, &head));
    match c.simple_query(&sql) {
        Ok(m) => {
            let rows = texts(&m);
            let t: Vec<f64> = [0, 2, 4].iter().map(|&i| rows[i].parse::<f64>().unwrap()).collect();
            Ok(((t[1] - t[0]) / 1e3, (t[2] - t[1]) / 1e3, rows[3].clone()))
        }
        Err(e) => {
            let _ = c.batch_execute("ROLLBACK");
            Err(e.to_string())
        }
    }
}

pub fn main(snap: &str, info: &str, cache: &str, url: &str, btable: &str, layout: &str, level: i32, dict: &str, out: &str) {
    let w = build(snap, info);
    let rows = rows_of(&w);
    let dict = (dict != "-").then(|| std::fs::read(dict).unwrap());
    let c = crate::apply::open(cache, true);
    let applied: i64 = c.query_row("SELECT applied FROM bx_meta", [], |r| r.get(0)).unwrap();
    // Warm the cache connection's statements (untimed), as a long-lived host.
    {
        let mut refs = SqlRefs::new(&c);
        let _ = refs.object_num(&[0u8; 32]);
        let _ = refs.name_num(b"");
        let _ = refs.commit_bundle(&[0u8; 33]);
    }
    // Local: encode + compress + number own objects/names.
    let t = Instant::now();
    let mut refs = SqlRefs::new(&c);
    // A table named cb_col_* holds self-contained bundles (no numbering).
    let colref = !btable.starts_with("cb_col_");
    // cb_colrefobj_*: objects numbered, binding names literal.
    let names = !btable.starts_with("cb_colrefobj_");
    let (raw, new_names) = encode(&rows, (applied + 1) as u64, &w.new_head, colref, names, &mut refs);
    let encode_ms = t.elapsed().as_secs_f64() * 1e3;
    let lookups = refs.lookups;
    let t = Instant::now();
    let body = compress(&raw, level, dict.as_deref());
    let compress_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    c.execute_batch("SAVEPOINT own").unwrap();
    refs.absorb(&rows, &new_names, (applied + 1) as u64, &w.new_head, colref);
    refs.flush(applied + 1);
    c.execute_batch("ROLLBACK TO own; RELEASE own").unwrap();
    let number_ms = t.elapsed().as_secs_f64() * 1e3;
    drop(refs);
    // Remote.
    let pre = pre_lock_sql(&w, btable, layout, &body);
    let mut pg = crate::pg(url);
    pg.batch_execute("SELECT 1").unwrap();
    // Untimed: this backend's catalog caches warm for the tables touched.
    pg.batch_execute(&format!("SELECT 1 FROM {btable} LIMIT 0; SELECT 1 FROM pack_{layout} LIMIT 0; SELECT 1 FROM \"commit\" LIMIT 0; SELECT 1 FROM branch LIMIT 0; SELECT 1 FROM scope_allocator LIMIT 0")).unwrap();
    let mount_sql = format!(
        "SELECT head_commit_id FROM branch WHERE branch_id = {}; UPDATE scope_allocator SET highwater = highwater + 1024 WHERE scope_id = {} RETURNING highwater; SELECT max(seq) FROM \"commit\";",
        lit(&w.branch),
        lit(&w.scope.0)
    );
    let t = Instant::now();
    let m = pg.simple_query(&mount_sql).unwrap();
    let mount_ms = t.elapsed().as_secs_f64() * 1e3;
    let mount_rows = texts(&m).len();
    let t = Instant::now();
    let (wait, hold, parent) = publish(&mut pg, &w, &pre).unwrap();
    let publish_ms = t.elapsed().as_secs_f64() * 1e3;
    let head: Option<Vec<u8>> = pg.query_one("SELECT head_commit_id FROM branch WHERE branch_id=$1", &[&w.branch]).unwrap().get(0);
    let seq: i64 = pg.query_one("SELECT seq FROM \"commit\" WHERE commit_id=$1", &[&w.new_head]).unwrap().get(0);
    let verified = head.as_deref() == Some(&w.new_head[..]) && parent == w.expected_head.as_deref().map(hex).unwrap_or_default();
    let text = format!(
        "{{\"btable\":\"{btable}\",\"layout\":\"{layout}\",\"level\":{level},\"dict\":{},\"rows\":{},\"raw_bytes\":{},\"body_bytes\":{},\"pre_lock_sql_bytes\":{},\
\"local\":{{\"encode_ms\":{encode_ms:.3},\"lookups\":{lookups},\"compress_ms\":{compress_ms:.3},\"number_own_ms\":{number_ms:.3}}},\
\"remote\":{{\"mount_ms\":{mount_ms:.3},\"mount_round_trips\":1,\"mount_rows\":{mount_rows},\"publish_ms\":{publish_ms:.3},\"publish_round_trips\":1,\"lane_wait_ms\":{wait:.3},\"lane_hold_ms\":{hold:.3}}},\
\"seq\":{seq},\"applied_before\":{applied},\"verified_head\":{verified}}}",
        dict.is_some(),
        rows.count(),
        raw.len(),
        body.len(),
        pre.len()
    );
    std::fs::write(out, format!("{text}\n")).unwrap();
    println!("{text}");
}

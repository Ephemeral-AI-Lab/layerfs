//! The storage study's recommended Postgres layout "pr" (#420): the host-cache
//! push of one Commit's new rows, written for
//!   pack            -> pk_a4    (same columns, reordered table)
//!   object_location -> ol_a2    (hash exclusion: ON CONFLICT DO NOTHING)
//!   inode_value     -> iv_b32z  (bucket = ordinal/32, present bitmap, packed values)
//!   binding_hint    -> bh_b3    (bucket = child/128, packed entries)
//!   file_digest     -> fd_a1    (same columns, reordered table)
//! content_signature, commit, lineage, branch, totals and policy are as ported.
//! Each bucket table takes one server-side INSERT ... ON CONFLICT (bucket)
//! DO UPDATE appending only when the entries are not already present
//! (study: issue420-pg-size/pub/publish_pr.sql); no client read-modify-write.
//! Buckets are written in ascending order so concurrent writers that share a
//! bucket row take its lock in one order.
use crate::workload::{col, Push, Workload, V};
use postgres::Client;
use std::collections::BTreeMap;

fn literal_rows(table: &str, cols: &[String], rows: &[Vec<V>], suffix: &str) -> String {
    let names = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
    let rows = rows.iter().map(|r| format!("({})", r.iter().map(V::literal).collect::<Vec<_>>().join(","))).collect::<Vec<_>>().join(",");
    format!("INSERT INTO {table}({names}) VALUES {rows}{suffix};")
}

/// inode values grouped by 32-ordinal bucket: (bucket, present bits, packed values).
pub fn iv_buckets(p: &Push) -> BTreeMap<i64, (i64, Vec<u8>)> {
    let (oi, vi) = (col(&p.columns, "ordinal"), col(&p.columns, "value"));
    let mut rows: Vec<(i64, Vec<u8>)> = p.rows.iter().map(|r| (r[oi].int(), r[vi].bytes())).collect();
    rows.sort();
    let mut out: BTreeMap<i64, (i64, Vec<u8>)> = BTreeMap::new();
    for (o, v) in rows {
        assert_eq!(v.len(), 73, "inode value width");
        let e = out.entry(o / 32).or_default();
        e.0 |= 1i64 << (o % 32);
        e.1.extend_from_slice(&v);
    }
    out
}

/// binding hints grouped by 128-child bucket, entries (child%128, parent BE, len, name)
/// in (child, parent, name) order.
pub fn bh_buckets(p: &Push) -> BTreeMap<i64, Vec<u8>> {
    let (ci, pi, ni) = (col(&p.columns, "child"), col(&p.columns, "parent"), col(&p.columns, "name"));
    let mut rows: Vec<(i64, i64, Vec<u8>)> = p.rows.iter().map(|r| (r[ci].int(), r[pi].int(), r[ni].bytes())).collect();
    rows.sort();
    let mut out: BTreeMap<i64, Vec<u8>> = BTreeMap::new();
    for (c, parent, name) in rows {
        assert!((1..=255).contains(&name.len()));
        let e = out.entry(c / 128).or_default();
        e.push((c % 128) as u8);
        e.extend_from_slice(&parent.to_be_bytes());
        e.push(name.len() as u8);
        e.extend_from_slice(&name);
    }
    out
}

/// Bytes the hint entries of one Commit add (10 + name length each).
pub fn bh_bytes(p: &Push) -> i64 {
    let ni = col(&p.columns, "name");
    p.rows.iter().map(|r| 10 + r[ni].bytes().len() as i64).sum()
}

/// One statement per non-empty pushed table, in the plain port's order.
pub fn rows_batch(w: &Workload) -> String {
    let mut b = String::new();
    for p in &w.push {
        if p.rows.is_empty() {
            continue;
        }
        b += &match p.table {
            "pack" => literal_rows("pk_a4", &p.columns, &p.rows, ""),
            "object_location" => literal_rows("ol_a2", &p.columns, &p.rows, " ON CONFLICT DO NOTHING"),
            "file_digest" => literal_rows("fd_a1", &p.columns, &p.rows, " ON CONFLICT(digest, length) DO UPDATE SET root = excluded.root"),
            "content_signature" => literal_rows("content_signature", &p.columns, &p.rows, p.conflict),
            "inode_value" => {
                let rows = iv_buckets(p)
                    .into_iter()
                    .map(|(k, (bits, vals))| format!("({k},{bits},{})", V::B(vals).literal()))
                    .collect::<Vec<_>>()
                    .join(",");
                format!("INSERT INTO iv_b32z (bucket, present, vals) VALUES {rows} ON CONFLICT (bucket) DO UPDATE SET present = iv_b32z.present | excluded.present, vals = iv_b32z.vals || excluded.vals WHERE iv_b32z.present & excluded.present = 0;")
            }
            "binding_hint" => {
                let rows = bh_buckets(p)
                    .into_iter()
                    .map(|(k, e)| format!("({k},{})", V::B(e).literal()))
                    .collect::<Vec<_>>()
                    .join(",");
                format!("INSERT INTO bh_b3 (bucket, entries) VALUES {rows} ON CONFLICT (bucket) DO UPDATE SET entries = bh_b3.entries || excluded.entries WHERE position(excluded.entries IN bh_b3.entries) = 0;")
            }
            other => panic!("pr: unported push table {other}"),
        };
    }
    b
}

/// Untimed catalog warm-up for the tables a pr publish touches (plans that
/// open each table and its indexes), as on a long-lived connection.
pub const WARM: [&str; 9] = [
    "SELECT 1 FROM pk_a4 WHERE pack_id = $1",
    "SELECT 1 FROM ol_a2 WHERE object_id = $1",
    "SELECT 1 FROM iv_b32z WHERE bucket = $1",
    "SELECT 1 FROM bh_b3 WHERE bucket = $1",
    "SELECT 1 FROM fd_a1 WHERE digest = $1 AND length = $2",
    "SELECT 1 FROM content_signature WHERE slot = $1",
    "SELECT 1 FROM commit_lineage WHERE commit_id = $1",
    "SELECT 1 FROM store_totals WHERE id = $1",
    "SELECT 1 FROM store_policy WHERE id = $1",
];
pub fn warm(c: &mut Client) {
    for sql in WARM {
        drop(c.prepare(sql).unwrap());
    }
}

/// Server-side count of packed hint entries (walks 10 + name length).
pub const BH_COUNT_FN: &str = "CREATE OR REPLACE FUNCTION bh_count(e bytea) RETURNS int LANGUAGE plpgsql IMMUTABLE STRICT AS $$
DECLARE i int := 0; n int := 0; l int := octet_length(e);
BEGIN WHILE i < l LOOP n := n + 1; i := i + 10 + get_byte(e, i + 9); END LOOP; IF i <> l THEN RAISE EXCEPTION 'torn hint bucket'; END IF; RETURN n; END$$";

/// Logical entry counts of the pr tables, keyed like the plain port's tables.
pub fn counts(c: &mut Client) -> Vec<(String, i64)> {
    c.batch_execute(BH_COUNT_FN).unwrap();
    let mut v = Vec::new();
    for (name, sql) in [
        ("pack", "SELECT count(*) FROM pk_a4"),
        ("object_location", "SELECT count(*) FROM ol_a2"),
        ("inode_value", "SELECT coalesce(sum(bit_count(present::bit(64))), 0)::int8 FROM iv_b32z"),
        ("inode_value.bytes", "SELECT coalesce(sum(octet_length(vals)), 0)::int8 FROM iv_b32z"),
        ("binding_hint", "SELECT coalesce(sum(bh_count(entries)), 0)::int8 FROM bh_b3"),
        ("binding_hint.bytes", "SELECT coalesce(sum(octet_length(entries)), 0)::int8 FROM bh_b3"),
        ("file_digest", "SELECT count(*) FROM fd_a1"),
        ("content_signature", "SELECT count(*) FROM content_signature"),
        ("commit", "SELECT count(*) FROM \"commit\""),
        ("commit_lineage", "SELECT count(*) FROM commit_lineage"),
        ("totals.logical", "SELECT logical FROM store_totals"),
        ("totals.canonical", "SELECT canonical FROM store_totals"),
        ("totals.stored", "SELECT stored FROM store_totals"),
        ("totals.metadata", "SELECT metadata FROM store_totals"),
    ] {
        v.push((name.to_owned(), c.query_one(sql, &[]).unwrap().get::<_, i64>(0)));
    }
    v
}

/// Orphans the pr layout cannot declare as FKs, plus the history FKs' targets.
pub const ORPHANS: &str = "SELECT (SELECT count(*) FROM ol_a2 o WHERE NOT EXISTS (SELECT 1 FROM pk_a4 p WHERE p.pack_id=o.pack_id)) + (SELECT count(*) FROM content_signature s WHERE NOT EXISTS (SELECT 1 FROM ol_a2 o WHERE o.object_id=s.object_id)) + (SELECT count(*) FROM \"commit\" c WHERE c.parent_commit_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM \"commit\" p WHERE p.commit_id=c.parent_commit_id)) + (SELECT count(*) FROM branch b WHERE b.head_commit_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM \"commit\" p WHERE p.commit_id=b.head_commit_id))";

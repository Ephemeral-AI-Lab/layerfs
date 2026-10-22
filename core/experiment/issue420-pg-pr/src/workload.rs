//! The C12 cycle's global-Store work as statements, from a capture.
//!
//! Exact: every row the Commit adds or changes (row diff s2 -> s3) and the
//! mount's one change (s0 -> s1), written with the product's statement texts
//! and paging (persistence/sql and backend/sqlite/*). Documented equivalents:
//! reads the diff cannot show, sized from the Commit's storage telemetry
//! (207 locates over 758 missed ids, 6 pack selections, 1 signature load),
//! one digest lookup per recorded file digest (windows of 64), the head,
//! lane and base-layer reads, and the reader-open checks at mount. Remaining
//! statements up to the measured counts are padded with one-id locates of
//! existing objects and reported as pads.
use rusqlite::{types::Value, Connection};
use std::collections::BTreeMap;

/// Measured in the capture's trace (writer statements include BEGIN/COMMIT).
pub const COMMIT_WRITER_STATEMENTS: usize = 864;
pub const COMMIT_TRANSACTIONS: usize = 18;
pub const COMMIT_READER_STATEMENTS: usize = 9;
pub const MOUNT_READER_STATEMENTS: usize = 40;
pub const MOUNT_READER_TRANSACTIONS: usize = 3;
pub const LOCATES: usize = 207;
pub const LOCATED_IDS: usize = 758;
pub const PACK_SELECTIONS: usize = 6;
const BATCH_OBJECT_LIMIT: usize = 512;
const DIGEST_WINDOW: usize = 64;

#[derive(Clone, Debug)]
pub enum V {
    I(i64),
    B(Vec<u8>),
    T(String),
    NI,
    NB,
    NT,
}
impl rusqlite::ToSql for V {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput::Borrowed, ValueRef};
        Ok(Borrowed(match self {
            V::I(v) => ValueRef::Integer(*v),
            V::B(b) => ValueRef::Blob(b),
            V::T(t) => ValueRef::Text(t.as_bytes()),
            _ => ValueRef::Null,
        }))
    }
}
impl V {
    pub fn pg(&self) -> Box<dyn postgres::types::ToSql + Sync> {
        match self {
            V::I(v) => Box::new(*v),
            V::B(b) => Box::new(b.clone()),
            V::T(t) => Box::new(t.clone()),
            V::NI => Box::new(None::<i64>),
            V::NB => Box::new(None::<Vec<u8>>),
            V::NT => Box::new(None::<String>),
        }
    }
    /// A literal for a simple-query batch.
    pub fn literal(&self) -> String {
        match self {
            V::I(v) => v.to_string(),
            V::B(b) => format!("'\\x{}'::bytea", hex(b)),
            V::T(t) => format!("'{}'", t.replace('\'', "''")),
            _ => "NULL".into(),
        }
    }
    pub fn bytes(&self) -> Vec<u8> {
        match self {
            V::B(b) => b.clone(),
            other => panic!("not a blob: {other:?}"),
        }
    }
    pub fn int(&self) -> i64 {
        match self {
            V::I(v) => *v,
            other => panic!("not an integer: {other:?}"),
        }
    }
}
pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

pub struct St {
    pub sql: String,
    /// The PostgreSQL text where it differs (catalog checks only).
    pub pg_sql: Option<String>,
    pub params: Vec<V>,
    pub label: &'static str,
}
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Write,
    Read,
    Auto,
}
pub struct Tx {
    pub kind: Kind,
    pub reader: bool,
    pub label: &'static str,
    pub stmts: Vec<St>,
}
impl Tx {
    fn new(kind: Kind, reader: bool, label: &'static str) -> Self {
        Self { kind, reader, label, stmts: Vec::new() }
    }
    fn push(&mut self, label: &'static str, sql: impl Into<String>, params: Vec<V>) {
        self.stmts.push(St { sql: sql.into(), pg_sql: None, params, label });
    }
}

/// Rows a host-cache publish pushes, per table, with its conflict clause.
pub struct Push {
    pub table: &'static str,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<V>>,
    pub conflict: &'static str,
}

pub struct Workload {
    pub mount: Vec<Tx>,
    pub commit: Vec<Tx>,
    pub push: Vec<Push>,
    pub commit_row: (Vec<String>, Vec<V>),
    pub lineage_row: (Vec<String>, Vec<V>),
    pub branch: Vec<u8>,
    pub base_layer: Vec<u8>,
    pub expected_head: Option<Vec<u8>>,
    pub new_head: Vec<u8>,
    pub totals: [i64; 3],
    pub policy: [i64; 2],
    pub scope: (Vec<u8>, i64),
    pub same_base: bool,
    pub pads: (usize, usize),
    pub notes: Vec<String>,
}

fn open_pair(snap: &str, after: &str, before: &str) -> Connection {
    let c = crate::ro(&format!("{snap}/{after}.sqlite"));
    c.execute_batch(&format!("ATTACH DATABASE 'file:{snap}/{before}.sqlite?mode=ro&immutable=1' AS old")).unwrap();
    c
}

fn decl(db: &Connection, table: &str) -> Vec<(String, String)> {
    let mut s = db.prepare(&format!("PRAGMA main.table_info(\"{table}\")")).unwrap();
    s.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn value(v: Value, ty: &str) -> V {
    match (v, ty.to_uppercase().as_str()) {
        (Value::Null, "INTEGER") => V::NI,
        (Value::Null, "BLOB") => V::NB,
        (Value::Null, _) => V::NT,
        (Value::Integer(i), _) => V::I(i),
        (Value::Blob(b), _) => V::B(b),
        (Value::Text(t), _) => V::T(t),
        (Value::Real(_), _) => panic!("REAL"),
    }
}

/// Rows of `table` in the newer snapshot that the older one lacks.
fn added(db: &Connection, table: &str) -> (Vec<String>, Vec<Vec<V>>) {
    let cols = decl(db, table);
    let names = cols.iter().map(|(n, _)| format!("\"{n}\"")).collect::<Vec<_>>().join(",");
    let sql = format!("SELECT {names} FROM main.\"{table}\" EXCEPT SELECT {names} FROM old.\"{table}\" ORDER BY 1");
    let mut s = db.prepare(&sql).unwrap();
    let rows = s
        .query_map([], |r| {
            Ok((0..cols.len()).map(|i| value(r.get::<_, Value>(i).unwrap(), &cols[i].1)).collect::<Vec<V>>())
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    (cols.into_iter().map(|(n, _)| n).collect(), rows)
}

fn one<T: rusqlite::types::FromSql>(db: &Connection, sql: &str) -> T {
    db.query_row(sql, [], |r| r.get(0)).unwrap()
}

pub fn col(cols: &[String], name: &str) -> usize {
    cols.iter().position(|c| c == name).unwrap_or_else(|| panic!("column {name}"))
}

const LOCATE: &str = "SELECT o.object_id,o.role,o.canonical_length,o.group_number,o.record_number,p.pack_id,p.domain,p.digest,p.length FROM object_location o JOIN pack p ON p.pack_id=o.pack_id WHERE o.object_id IN";
fn locate(ids: &[Vec<u8>]) -> (String, Vec<V>) {
    let marks = (1..=ids.len()).map(|i| format!("${i}")).collect::<Vec<_>>().join(",");
    (format!("{LOCATE}({marks})"), ids.iter().cloned().map(V::B).collect())
}
fn values_sql(table: &str, cols: &[String], rows: usize, suffix: &str) -> String {
    let width = cols.len();
    let tuples = (0..rows)
        .map(|r| format!("({})", (1..=width).map(|c| format!("${}", r * width + c)).collect::<Vec<_>>().join(",")))
        .collect::<Vec<_>>()
        .join(",");
    let names = cols.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
    format!("INSERT INTO \"{table}\"({names}) VALUES {tuples}{suffix}")
}
/// The product's power-of-two pages (<= 512 rows) as statements.
fn pages(tx: &mut Tx, label: &'static str, table: &str, cols: &[String], rows: &[Vec<V>], suffix: &str) {
    let mut rest = rows;
    while !rest.is_empty() {
        let n = 1usize << rest.len().min(BATCH_OBJECT_LIMIT).ilog2();
        let (page, tail) = rest.split_at(n);
        rest = tail;
        tx.push(label, values_sql(table, cols, n, suffix), page.iter().flatten().cloned().collect());
    }
}

pub fn build(snap: &str, info: &str) -> Workload {
    build_tagged(snap, info, None)
}

/// Strides that keep tagged publishes disjoint: one reservation of pack ids,
/// ordinals and serials per tag.
pub const PACK_STRIDE: i64 = 4096;
pub const ORDINAL_STRIDE: i64 = 1024;
/// pr copy: a tagged Commit's ordinals start on a 1024-aligned block, so no
/// two tags (and no tag and the base) share a 32-ordinal inode bucket.
fn ordinal_pad(base_no: i64, tag: Option<u64>) -> i64 {
    if tag.is_some() { (base_no + ORDINAL_STRIDE - 1) / ORDINAL_STRIDE * ORDINAL_STRIDE - base_no } else { 0 }
}
pub const SERIAL_STRIDE: i64 = 1024;

/// `tag` k makes the k-th disjoint copy of the Commit's rows for a contended
/// run: pack ids, ordinals and fresh serials shift by one stride per tag;
/// object ids, file digests, the Commit id, its root and lineage run have
/// their last four bytes XORed with k+1 (the same map everywhere they are
/// referenced); the lineage is a new run at depth 1; the policy reservation
/// statements become monotonic (max/GREATEST) so concurrent writers stay
/// valid. The Commit's parent is set at publish time from the lane's head.
pub fn build_tagged(snap: &str, info: &str, tag: Option<u64>) -> Workload {
    let commit = open_pair(snap, "s3-after-commit", "s2-after-exec");
    let base = crate::ro(info);
    let mut notes = Vec::new();
    // Origins in the captured Store and in the replay base.
    let old_branch: Vec<u8> = one(&commit, "SELECT branch_id FROM main.branch WHERE name='c12'");
    let same_base = base
        .query_row("SELECT 1 FROM branch WHERE branch_id=?1", [&old_branch], |_| Ok(()))
        .is_ok();
    let (old_np, old_no): (i64, i64) = commit
        .query_row("SELECT next_pack_id,next_ordinal FROM old.store_policy", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    let (new_np, new_no): (i64, i64) = commit
        .query_row("SELECT next_pack_id,next_ordinal FROM main.store_policy", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    let (base_np, base_no): (i64, i64) = base
        .query_row("SELECT next_pack_id,next_ordinal FROM store_policy", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    let mount = open_pair(snap, "s1-after-mount", "s0-before-mount");
    let old_lo: i64 = one(&mount, "SELECT highwater FROM old.scope_allocator");
    let (scope_id, base_hw): (Vec<u8>, i64) = base
        .query_row("SELECT scope_id, highwater FROM scope_allocator", [], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    let step: i64 = one::<i64>(&mount, "SELECT highwater FROM main.scope_allocator") - old_lo;
    // The Branch the replay publishes to.
    let (branch, stack, base_layer, head): (Vec<u8>, Vec<u8>, Vec<u8>, Option<Vec<u8>>) = if same_base {
        commit
            .query_row(
                "SELECT branch_id, layer_stack_id, base_layer_id, head_commit_id FROM old.branch WHERE branch_id=?1",
                [&old_branch],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap()
    } else {
        base.query_row(
            "SELECT branch_id, layer_stack_id, base_layer_id, head_commit_id FROM branch WHERE head_commit_id IS NOT NULL ORDER BY branch_id LIMIT 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
    };
    let pack_map = |p: i64| p - old_np + base_np;
    let ordinal_map = |o: i64| o - old_no + base_no;
    let serial_map = |s: i64| if same_base || s < old_lo { s } else { s - old_lo + base_hw };
    if !same_base {
        notes.push(format!(
            "base b remap: pack ids +{}, ordinals +{}, fresh serials >= {old_lo} moved to >= {base_hw}; publish to Branch {} (head {})",
            base_np - old_np, base_no - old_no, hex(&branch), head.as_deref().map(hex).unwrap_or_default()
        ));
    }

    // Exact rows.
    let mut tables: BTreeMap<&str, (Vec<String>, Vec<Vec<V>>)> = BTreeMap::new();
    for t in ["pack", "object_location", "inode_value", "binding_hint", "file_digest", "content_signature", "commit", "commit_lineage", "reach", "absorbed"] {
        tables.insert(t, added(&commit, t));
    }
    for t in ["reach", "absorbed"] {
        assert!(tables[t].1.is_empty(), "{t} rows added: port them");
    }
    {
        let (c, rows) = tables.get_mut("pack").unwrap();
        let i = col(c, "pack_id");
        for r in rows.iter_mut() {
            r[i] = V::I(pack_map(r[i].int()));
        }
    }
    {
        let (c, rows) = tables.get_mut("object_location").unwrap();
        let i = col(c, "pack_id");
        for r in rows.iter_mut() {
            r[i] = V::I(pack_map(r[i].int()));
        }
    }
    {
        let (c, rows) = tables.get_mut("inode_value").unwrap();
        let i = col(c, "ordinal");
        for r in rows.iter_mut() {
            r[i] = V::I(ordinal_map(r[i].int()));
        }
    }
    {
        let (c, rows) = tables.get_mut("binding_hint").unwrap();
        let (ci, pi) = (col(c, "child"), col(c, "parent"));
        for r in rows.iter_mut() {
            r[ci] = V::I(serial_map(r[ci].int()));
            r[pi] = V::I(serial_map(r[pi].int()));
        }
    }
    let (commit_cols, mut commit_rows) = tables.remove("commit").unwrap();
    assert_eq!(commit_rows.len(), 1);
    let mut commit_row = commit_rows.remove(0);
    let new_head = commit_row[col(&commit_cols, "commit_id")].bytes();
    if !same_base {
        commit_row[col(&commit_cols, "layer_stack_id")] = V::B(stack.clone());
        commit_row[col(&commit_cols, "base_layer_id")] = V::B(base_layer.clone());
        commit_row[col(&commit_cols, "parent_commit_id")] = head.clone().map(V::B).unwrap_or(V::NB);
        if !matches!(commit_row[col(&commit_cols, "branch_id")], V::NB) {
            commit_row[col(&commit_cols, "branch_id")] = V::B(branch.clone());
        }
    }
    let (lineage_cols, mut lineage_rows) = tables.remove("commit_lineage").unwrap();
    assert_eq!(lineage_rows.len(), 1);
    let mut lineage_row = lineage_rows.remove(0);
    if !same_base {
        // Extend the parent's run when its next depth is free, else a new run.
        let parent: Option<(Vec<u8>, i64)> = head.as_ref().and_then(|h| {
            base.query_row("SELECT run, depth FROM commit_lineage WHERE commit_id=?1", [h], |r| Ok((r.get(0)?, r.get(1)?))).ok()
        });
        let (run, depth) = match parent {
            Some((run, d))
                if base
                    .query_row("SELECT 1 FROM commit_lineage WHERE run=?1 AND depth=?2", rusqlite::params![run, d + 1], |_| Ok(()))
                    .is_err() =>
            {
                (run, d + 1)
            }
            _ => (new_head.clone(), 1),
        };
        lineage_row[col(&lineage_cols, "run")] = V::B(run);
        lineage_row[col(&lineage_cols, "depth")] = V::I(depth);
    }
    if let Some(k) = tag {
        let x = (k as u32 + 1).to_be_bytes();
        let flip = |v: &mut V| {
            if let V::B(b) = v {
                let n = b.len();
                for (i, t) in x.iter().enumerate() {
                    b[n - 4 + i] ^= t;
                }
            }
        };
        let fresh_lo = if same_base { old_lo } else { base_hw };
        let k = k as i64;
        for (t, column) in [("object_location", "object_id"), ("content_signature", "object_id"), ("file_digest", "digest"), ("file_digest", "root")] {
            let (c, rows) = tables.get_mut(t).unwrap();
            let i = col(c, column);
            rows.iter_mut().for_each(|r| flip(&mut r[i]));
        }
        let pad = ordinal_pad(base_no, tag);
        for (t, column, stride, extra) in [("pack", "pack_id", PACK_STRIDE, 0), ("object_location", "pack_id", PACK_STRIDE, 0), ("inode_value", "ordinal", ORDINAL_STRIDE, pad)] {
            let (c, rows) = tables.get_mut(t).unwrap();
            let i = col(c, column);
            rows.iter_mut().for_each(|r| r[i] = V::I(r[i].int() + extra + k * stride));
        }
        {
            let (c, rows) = tables.get_mut("binding_hint").unwrap();
            for column in ["child", "parent"] {
                let i = col(c, column);
                for r in rows.iter_mut() {
                    let v = r[i].int();
                    if v >= fresh_lo {
                        r[i] = V::I(v + k * SERIAL_STRIDE);
                    }
                }
            }
        }
        flip(&mut commit_row[col(&commit_cols, "commit_id")]);
        flip(&mut commit_row[col(&commit_cols, "root_id")]);
        let id = commit_row[col(&commit_cols, "commit_id")].clone();
        lineage_row[col(&lineage_cols, "commit_id")] = id.clone();
        lineage_row[col(&lineage_cols, "run")] = id;
        lineage_row[col(&lineage_cols, "depth")] = V::I(1);
    }
    let new_head = commit_row[col(&commit_cols, "commit_id")].bytes();
    let objects = &tables["object_location"];
    let object_ids: Vec<Vec<u8>> = objects.1.iter().map(|r| r[col(&objects.0, "object_id")].bytes()).collect();
    let digests = &tables["file_digest"];
    let existing_objects: Vec<Vec<u8>> = {
        let mut s = base.prepare("SELECT object_id FROM object_location ORDER BY object_id LIMIT 200").unwrap();
        s.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect()
    };
    let existing_packs: Vec<i64> = {
        let mut s = base.prepare("SELECT pack_id FROM pack ORDER BY pack_id LIMIT ?1").unwrap();
        let have: Vec<i64> = s.query_map([PACK_SELECTIONS as i64], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
        // A small base has fewer packs than selections: read them again.
        (0..PACK_SELECTIONS).map(|i| have[i % have.len()]).collect()
    };
    let collisions: i64 = object_ids
        .iter()
        .filter(|id| base.query_row("SELECT 1 FROM object_location WHERE object_id=?1", [id], |_| Ok(())).is_ok())
        .count() as i64;
    if collisions > 0 {
        notes.push(format!("{collisions} new object ids already exist in the base (ON CONFLICT DO NOTHING keeps them)"));
    }

    // Commit transactions, writer connection unless marked reader.
    let mut txs: Vec<Tx> = Vec::new();
    let mut w1 = Tx::new(Kind::Write, false, "reserve.packs");
    w1.push("policy.read", "SELECT next_pack_id,next_ordinal FROM store_policy WHERE id=1", vec![]);
    let shift = tag.unwrap_or(0) as i64;
    if tag.is_some() {
        w1.stmts.push(St { sql: "UPDATE store_policy SET next_pack_id=max(next_pack_id,$1),next_ordinal=max(next_ordinal,$2) WHERE id=1".into(), pg_sql: Some("UPDATE store_policy SET next_pack_id=GREATEST(next_pack_id,$1),next_ordinal=GREATEST(next_ordinal,$2) WHERE id=1".into()), params: vec![V::I(base_np + (new_np - old_np) + shift * PACK_STRIDE), V::I(base_no + shift * ORDINAL_STRIDE + ordinal_pad(base_no, tag))], label: "policy.reserve" });
    } else {
        w1.push("policy.reserve", "UPDATE store_policy SET next_pack_id=$1,next_ordinal=$2 WHERE id=1", vec![V::I(base_np + (new_np - old_np)), V::I(base_no)]);
    }
    txs.push(w1);
    let mut w2 = Tx::new(Kind::Write, false, "reserve.ordinals");
    if tag.is_some() {
        w2.stmts.push(St { sql: "UPDATE store_policy SET next_ordinal=max(next_ordinal,$1) WHERE id=1".into(), pg_sql: Some("UPDATE store_policy SET next_ordinal=GREATEST(next_ordinal,$1) WHERE id=1".into()), params: vec![V::I(base_no + (new_no - old_no) + shift * ORDINAL_STRIDE + ordinal_pad(base_no, tag))], label: "policy.ordinals" });
    } else {
        w2.push("policy.ordinals", "UPDATE store_policy SET next_ordinal=$1 WHERE id=1", vec![V::I(base_no + (new_no - old_no))]);
    }
    txs.push(w2);
    // Save locates: 758 missed ids over 207 statements, from the new objects.
    let mut auto = Tx::new(Kind::Auto, false, "save.locate");
    for i in 0..LOCATES {
        let n = LOCATED_IDS / LOCATES + usize::from(i < LOCATED_IDS % LOCATES);
        let ids: Vec<Vec<u8>> = (0..n).map(|k| object_ids[(i * 4 + k) % object_ids.len()].clone()).collect();
        let (sql, params) = locate(&ids);
        auto.push("locate.batch", sql, params);
    }
    txs.push(auto);
    for window in digests.1.chunks(DIGEST_WINDOW) {
        let mut r = Tx::new(Kind::Read, false, "digest.roots");
        for row in window {
            r.push("digest_of", "SELECT f.root FROM file_digest f JOIN object_location o ON o.object_id = f.root WHERE f.digest = $1 AND f.length = $2", vec![row[0].clone(), row[1].clone()]);
        }
        txs.push(r);
    }
    for p in &existing_packs {
        let mut r = Tx::new(Kind::Read, false, "pack.selection");
        r.push("pack.header", "SELECT pack_id,domain,digest,length FROM pack WHERE pack_id=$1", vec![V::I(*p)]);
        txs.push(r);
    }
    // Save publications: one per new pack, its objects, totals; the metadata
    // pack's publication also carries inode values and signatures.
    let (pack_cols, pack_rows) = &tables["pack"];
    let last_metadata = pack_rows.iter().rposition(|r| r[col(pack_cols, "domain")].int() == 0);
    let (sig_cols, sig_rows) = &tables["content_signature"];
    let (iv_cols, iv_rows) = &tables["inode_value"];
    for (k, p) in pack_rows.iter().enumerate() {
        let mut w = Tx::new(Kind::Write, false, "save.publish");
        let id = p[col(pack_cols, "pack_id")].int();
        w.push("pack.insert", values_sql("pack", pack_cols, 1, ""), p.clone());
        let mine: Vec<Vec<V>> = objects.1.iter().filter(|r| r[col(&objects.0, "pack_id")].int() == id).cloned().collect();
        pages(&mut w, "objects.insert", "object_location", &objects.0, &mine, " ON CONFLICT(object_id) DO NOTHING RETURNING object_id");
        let canonical: i64 = mine.iter().map(|r| r[col(&objects.0, "canonical_length")].int()).sum();
        let length = p[col(pack_cols, "length")].int();
        let metadata = if p[col(pack_cols, "domain")].int() == 0 { length } else { 0 };
        w.push("totals.update", "UPDATE store_totals SET canonical=canonical+$1,stored=stored+$2,metadata=metadata+$3 WHERE id=1", vec![V::I(canonical), V::I(length), V::I(metadata)]);
        if Some(k) == last_metadata {
            pages(&mut w, "inode_values.insert", "inode_value", iv_cols, iv_rows, "");
        }
        // Signatures name objects of any pack: last publication (FK order).
        if k + 1 == pack_rows.len() {
            if !sig_rows.is_empty() {
                w.push("signatures.upsert", values_sql("content_signature", sig_cols, sig_rows.len(), " ON CONFLICT(slot) DO UPDATE SET stamp=excluded.stamp,object_id=excluded.object_id,signature=excluded.signature WHERE excluded.stamp>=content_signature.stamp"), sig_rows.iter().flatten().cloned().collect());
            }
        }
        txs.push(w);
    }
    let unplaced = objects.1.iter().filter(|r| !pack_rows.iter().any(|p| p[col(pack_cols, "pack_id")].int() == r[col(&objects.0, "pack_id")].int())).count();
    if unplaced > 0 {
        notes.push(format!("{unplaced} new objects point at packs that existed before the Commit; not replayed"));
    }
    let mut head_read = Tx::new(Kind::Read, false, "head");
    head_read.push("branch.by_id", "SELECT branch_id, layer_stack_id, name, base_layer_id, head_commit_id, forked_from_id FROM branch WHERE branch_id = $1", vec![V::B(branch.clone())]);
    head_read.push("signatures.load", "SELECT slot,stamp,object_id,signature FROM content_signature ORDER BY slot LIMIT 8193", vec![]);
    txs.push(head_read);
    let mut known = Tx::new(Kind::Read, false, "logical.known");
    known.push("layer.by_id", "SELECT y.layer_id, y.layer_stack_id, y.parent_layer_id, y.root_id, y.source_branch_id, y.source_commit_id, y.logical, l.overwrote_count FROM layer y JOIN layer_lineage l ON l.layer_id = y.layer_id WHERE y.layer_id = $1", vec![V::B(base_layer.clone())]);
    txs.push(known);
    let mut wd = Tx::new(Kind::Write, false, "digests.record");
    for row in &digests.1 {
        wd.push("digest_insert", "INSERT INTO file_digest(digest, length, root) VALUES ($1, $2, $3) ON CONFLICT(digest, length) DO UPDATE SET root = excluded.root", row.clone());
    }
    txs.push(wd);
    let mut wh = Tx::new(Kind::Write, false, "hints.record");
    for row in &tables["binding_hint"].1 {
        wh.push("hint_insert", "INSERT INTO binding_hint(child, parent, name) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING", row.clone());
    }
    txs.push(wh);
    let mut wp = Tx::new(Kind::Write, false, "history.publish");
    wp.push("branch.by_id", "SELECT branch_id, layer_stack_id, name, base_layer_id, head_commit_id, forked_from_id FROM branch WHERE branch_id = $1", vec![V::B(branch.clone())]);
    wp.push("commit.insert", "INSERT INTO \"commit\" (commit_id, layer_stack_id, root_id, parent_commit_id, base_layer_id, second_parent_id, branch_id, via, message, merged_from_id, created_ms, logical, new_canonical, new_stored, new_metadata, lww, displaced_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) RETURNING 1", commit_row.clone());
    wp.push("branch.advance", "UPDATE branch SET head_commit_id = $1 WHERE branch_id = $2 AND base_layer_id = $3 RETURNING 1", vec![V::B(new_head.clone()), V::B(branch.clone()), V::B(base_layer.clone())]);
    wp.push("lineage.insert", "INSERT INTO commit_lineage (commit_id, run, depth, newest, merged) VALUES ($1, $2, $3, $4, $5) RETURNING 1", lineage_row.clone());
    txs.push(wp);
    let mut reader = Tx::new(Kind::Auto, true, "reader.locate");
    for k in 0..COMMIT_READER_STATEMENTS {
        let (sql, params) = locate(&existing_objects[k % existing_objects.len()..][..1]);
        reader.push("locate.one", sql, params);
    }
    txs.push(reader);
    let in_tx = txs.iter().filter(|t| t.kind != Kind::Auto).count();
    assert_eq!(in_tx, COMMIT_TRANSACTIONS, "modeled Commit transactions");
    let writer_now: usize = txs.iter().filter(|t| !t.reader).map(|t| t.stmts.len() + if t.kind == Kind::Auto { 0 } else { 2 }).sum();
    let commit_pad = COMMIT_WRITER_STATEMENTS.saturating_sub(writer_now);
    if writer_now > COMMIT_WRITER_STATEMENTS {
        notes.push(format!("modeled writer statements {writer_now} exceed measured {COMMIT_WRITER_STATEMENTS}"));
    }
    let mut pad = Tx::new(Kind::Auto, false, "pad.locate");
    for k in 0..commit_pad {
        let (sql, params) = locate(&existing_objects[k % existing_objects.len()..][..1]);
        pad.push("pad.locate_one", sql, params);
    }
    txs.insert(3, pad);

    // Mount: reader open checks, bind snapshot reads, Attach's serial reservation.
    let mut mtx: Vec<Tx> = Vec::new();
    let mut r1 = Tx::new(Kind::Read, true, "reader.open");
    r1.push("history_meta", "SELECT catalog_id,catalog_incarnation,binding_key,identity_format,schema_version,schema_source,schema_definition FROM history_meta WHERE id=1", vec![]);
    r1.push("store_policy", "SELECT format_profile,small_file_threshold_bytes,whole_file_delta_max_depth,chunk_delta_max_depth,metadata_delta_max_depth,schema_version FROM store_policy WHERE id=1", vec![]);
    for t in ["merge_record", "file_digest", "commit_lineage", "binding_hint"] {
        r1.stmts.push(St {
            sql: "SELECT 1 FROM sqlite_schema WHERE type='table' AND name=$1".into(),
            pg_sql: Some("SELECT 1 FROM pg_class WHERE relkind='r' AND relname=$1".into()),
            params: vec![V::T(t.into())],
            label: "table.exists",
        });
    }
    mtx.push(r1);
    let mut r2 = Tx::new(Kind::Read, true, "bind.snapshot");
    r2.push("branch.by_id", "SELECT branch_id, layer_stack_id, name, base_layer_id, head_commit_id, forked_from_id FROM branch WHERE branch_id = $1", vec![V::B(branch.clone())]);
    r2.push("layer.by_id", "SELECT y.layer_id, y.layer_stack_id, y.parent_layer_id, y.root_id, y.source_branch_id, y.source_commit_id, y.logical, l.overwrote_count FROM layer y JOIN layer_lineage l ON l.layer_id = y.layer_id WHERE y.layer_id = $1", vec![V::B(base_layer.clone())]);
    r2.push("stack.by_id", "SELECT layer_stack_id, name, scope_id, profile_id, head_layer_id FROM layer_stack WHERE layer_stack_id = $1", vec![V::B(stack.clone())]);
    mtx.push(r2);
    let mut r3 = Tx::new(Kind::Read, true, "bind.root");
    match &head {
        Some(h) => r3.push("commit.by_id", "SELECT commit_id, layer_stack_id, root_id, parent_commit_id, base_layer_id, second_parent_id, branch_id, via, message, merged_from_id, created_ms, logical, new_canonical, new_stored, new_metadata, lww, displaced_id FROM \"commit\" WHERE commit_id = $1", vec![V::B(h.clone())]),
        None => r3.push("layer.root", "SELECT root_id FROM layer WHERE layer_id = $1", vec![V::B(base_layer.clone())]),
    }
    let mount_body: usize = MOUNT_READER_STATEMENTS - 2 * MOUNT_READER_TRANSACTIONS;
    let mount_now = r3.stmts.len() + mtx.iter().map(|t| t.stmts.len()).sum::<usize>();
    let mount_pad = mount_body.saturating_sub(mount_now);
    for k in 0..mount_pad {
        let (sql, params) = locate(&existing_objects[k % existing_objects.len()..][..1]);
        r3.push("pad.locate_one", sql, params);
    }
    mtx.push(r3);
    let mut wa = Tx::new(Kind::Write, false, "attach.serials");
    wa.push("scope.by_id", "SELECT highwater, authority_id FROM scope_allocator WHERE scope_id = $1", vec![V::B(scope_id.clone())]);
    wa.push("scope.advance", "UPDATE scope_allocator SET highwater = $1 WHERE scope_id = $2 AND highwater = $3 RETURNING 1", vec![V::I(base_hw + step), V::B(scope_id.clone()), V::I(base_hw)]);
    mtx.push(wa);

    // What a host-cache publish pushes: every new row of the Commit.
    let mut push = Vec::new();
    for (t, conflict) in [
        ("pack", ""),
        ("object_location", " ON CONFLICT(object_id) DO NOTHING"),
        ("inode_value", ""),
        ("binding_hint", " ON CONFLICT DO NOTHING"),
        ("file_digest", " ON CONFLICT(digest, length) DO UPDATE SET root = excluded.root"),
        ("content_signature", " ON CONFLICT(slot) DO UPDATE SET stamp=excluded.stamp,object_id=excluded.object_id,signature=excluded.signature WHERE excluded.stamp>=content_signature.stamp"),
    ] {
        let (c, rows) = tables.remove(t).unwrap();
        push.push(Push { table: t, columns: c, rows, conflict });
    }
    let totals = pack_rows_totals(&push);
    Workload {
        mount: mtx,
        commit: txs,
        push,
        commit_row: (commit_cols, commit_row),
        lineage_row: (lineage_cols, lineage_row),
        branch,
        base_layer,
        expected_head: head,
        new_head,
        totals,
        policy: [base_np + (new_np - old_np) + tag.unwrap_or(0) as i64 * PACK_STRIDE, base_no + (new_no - old_no) + tag.unwrap_or(0) as i64 * ORDINAL_STRIDE + ordinal_pad(base_no, tag)],
        scope: (scope_id, base_hw),
        same_base,
        pads: (commit_pad, mount_pad),
        notes,
    }
}

fn pack_rows_totals(push: &[Push]) -> [i64; 3] {
    let objects = &push[1];
    let canonical = objects.rows.iter().map(|r| r[col(&objects.columns, "canonical_length")].int()).sum();
    let packs = &push[0];
    let stored = packs.rows.iter().map(|r| r[col(&packs.columns, "length")].int()).sum();
    let metadata = packs
        .rows
        .iter()
        .filter(|r| r[col(&packs.columns, "domain")].int() == 0)
        .map(|r| r[col(&packs.columns, "length")].int())
        .sum();
    [canonical, stored, metadata]
}

pub fn summary(w: &Workload) -> String {
    let count = |txs: &[Tx]| {
        let mut by: BTreeMap<&str, usize> = BTreeMap::new();
        for t in txs {
            for s in &t.stmts {
                *by.entry(s.label).or_default() += 1;
            }
        }
        let body: usize = txs.iter().map(|t| t.stmts.len()).sum();
        let txn = txs.iter().filter(|t| t.kind != Kind::Auto).count();
        let writes = txs.iter().filter(|t| t.kind == Kind::Write).count();
        let reader: usize = txs.iter().filter(|t| t.reader).map(|t| t.stmts.len() + if t.kind == Kind::Auto { 0 } else { 2 }).sum();
        format!(
            "{{\"statements_with_begin_commit\":{},\"body\":{body},\"transactions\":{txn},\"write_transactions\":{writes},\"reader_statements\":{reader},\"by_label\":{}}}",
            body + 2 * txn,
            serde_like(&by)
        )
    };
    let rows = w.push.iter().map(|p| format!("\"{}\":{}", p.table, p.rows.len())).collect::<Vec<_>>().join(",");
    format!(
        "{{\"same_base\":{},\"commit\":{},\"mount\":{},\"pads\":{{\"commit_writer\":{},\"mount_reader\":{}}},\"pushed_rows\":{{{rows}}},\"notes\":[{}]}}",
        w.same_base,
        count(&w.commit),
        count(&w.mount),
        w.pads.0,
        w.pads.1,
        w.notes.iter().map(|n| format!("{n:?}")).collect::<Vec<_>>().join(",")
    )
}
fn serde_like(m: &BTreeMap<&str, usize>) -> String {
    format!("{{{}}}", m.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect::<Vec<_>>().join(","))
}

pub fn describe(snap: &str, info: &str) {
    println!("{}", summary(&build(snap, info)));
}

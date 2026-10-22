//! Host side of the bundle model (#420): a local SQLite Store cache that
//! applies published Commit bundles fetched from Postgres.
//!
//!   bcache-new FIXTURE CACHE      an empty Store (the fixture's schema) plus
//!       the static history rows (stacks, layers, branches, meta, policy,
//!       scope) and the bundle side tables; no Commit, object or pack yet
//!   bapply CACHE URL BTABLE PLAYOUT CODEC DICT COUNT BATCH OUT
//!       apply the next COUNT bundles after the cache's applied seq, BATCH
//!       bundles per fetch (2 round trips: bundles+Commit rows, then packs)
//!       and per SQLite transaction; CODEC zstd|raw (raw: Postgres-side
//!       lz4/plain body); DICT a file or '-'. Writes per-batch timings.
//!   bverify CACHE FIXTURE         content tables equal both ways (EXCEPT)
//!
//! Side tables (colref numbering, kept by the host): bx_object(num, id),
//! bx_name(num, name), bx_commit(seq, commit id), bx_meta(applied seq).
use crate::codec::{decode, DDict, Refs, Rows};
use postgres::Client;
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::time::Instant;

pub const SIDE: &str = "
CREATE TABLE IF NOT EXISTS bx_object (num INTEGER PRIMARY KEY, object_id BLOB NOT NULL UNIQUE) STRICT;
CREATE TABLE IF NOT EXISTS bx_name (num INTEGER PRIMARY KEY, name BLOB NOT NULL UNIQUE) STRICT;
CREATE TABLE IF NOT EXISTS bx_commit (seq INTEGER PRIMARY KEY, commit_id BLOB NOT NULL UNIQUE) STRICT;
CREATE TABLE IF NOT EXISTS bx_meta (id INTEGER PRIMARY KEY CHECK(id=1), applied INTEGER NOT NULL, objects INTEGER NOT NULL, names INTEGER NOT NULL) STRICT;
INSERT OR IGNORE INTO bx_meta VALUES (1, -1, 0, 0);
";

/// The product profile (backend/sqlite/profile.rs), foreign keys as given.
pub fn open(path: &str, fk: bool) -> Connection {
    let c = Connection::open(path).unwrap();
    let mode: String = c.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, "wal");
    c.execute_batch(&format!(
        "PRAGMA synchronous=0; PRAGMA foreign_keys={}; PRAGMA wal_autocheckpoint=1000; PRAGMA journal_size_limit=4194304; PRAGMA cache_size=-2048; PRAGMA mmap_size=0; PRAGMA temp_store=2; PRAGMA busy_timeout=0;",
        u8::from(fk)
    ))
    .unwrap();
    c.set_prepared_statement_cache_capacity(64);
    c
}

const STATIC: [&str; 9] = ["layer_stack", "layer", "layer_lineage", "branch", "history_meta", "store_policy", "scope_allocator", "reach", "absorbed"];

pub fn new_cache(fixture: &str, cache: &str) {
    assert!(!std::path::Path::new(cache).exists(), "cache exists");
    let t = Instant::now();
    let c = Connection::open(cache).unwrap();
    c.execute_batch("PRAGMA page_size=4096; PRAGMA auto_vacuum=2;").unwrap();
    let f = crate::ro(fixture);
    let mut s = f.prepare("SELECT sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY rowid").unwrap();
    let ddl: Vec<String> = s.query_map([], |r| r.get(0)).unwrap().map(Result::unwrap).collect();
    c.execute_batch("BEGIN").unwrap();
    for d in &ddl {
        c.execute_batch(d).unwrap();
    }
    c.execute_batch("COMMIT").unwrap();
    drop(c);
    let c = open(cache, false);
    c.execute_batch(&format!("ATTACH 'file:{fixture}?mode=ro&immutable=1' AS fx")).unwrap();
    c.execute_batch("BEGIN").unwrap();
    for t in STATIC {
        c.execute_batch(&format!("INSERT INTO main.\"{t}\" SELECT * FROM fx.\"{t}\"")).unwrap();
    }
    c.execute_batch("INSERT INTO main.store_totals(id) VALUES (1); COMMIT; DETACH fx;").unwrap();
    c.execute_batch(SIDE).unwrap();
    println!("{{\"cache\":\"{cache}\",\"statements\":{},\"ms\":{:.0}}}", ddl.len(), t.elapsed().as_secs_f64() * 1e3);
}

/// Numbering over the side tables plus not-yet-inserted decoded bundles.
pub struct SqlRefs<'a> {
    pub c: &'a Connection,
    pub objects: u64,
    pub names: u64,
    pending_objects: Vec<Vec<u8>>,
    pending_names: Vec<Vec<u8>>,
    pending_index: HashMap<Vec<u8>, u64>,
    pending_name_index: HashMap<Vec<u8>, u64>,
    pending_commits: Vec<(u64, Vec<u8>)>,
    pub lookups: usize,
}
impl<'a> SqlRefs<'a> {
    pub fn new(c: &'a Connection) -> Self {
        let (objects, names): (i64, i64) = c.query_row("SELECT objects, names FROM bx_meta", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
        SqlRefs {
            c,
            objects: objects as u64,
            names: names as u64,
            pending_objects: Vec::new(),
            pending_names: Vec::new(),
            pending_index: HashMap::new(),
            pending_name_index: HashMap::new(),
            pending_commits: Vec::new(),
            lookups: 0,
        }
    }
    fn stored_objects(&self) -> u64 {
        self.objects - self.pending_objects.len() as u64
    }
    fn stored_names(&self) -> u64 {
        self.names - self.pending_names.len() as u64
    }
    /// A decoded bundle's new objects/names/commit become referable.
    pub fn absorb(&mut self, rows: &Rows, new_names: &[Vec<u8>], seq: u64, commit: &[u8], numbered: bool) {
        if numbered {
            // An id or name a concurrent bundle already numbered keeps its
            // number: every host applies in seq order, so all agree.
            for o in &rows.ol {
                if self.object_num(&o.id).is_none() {
                    self.pending_index.insert(o.id.clone(), self.objects);
                    self.pending_objects.push(o.id.clone());
                    self.objects += 1;
                }
            }
            for n in new_names {
                if self.name_num(n).is_none() {
                    self.pending_name_index.insert(n.clone(), self.names);
                    self.pending_names.push(n.clone());
                    self.names += 1;
                }
            }
        }
        self.pending_commits.push((seq, commit.to_vec()));
    }
    /// Write the pending side rows (inside the caller's transaction).
    pub fn flush(&mut self, applied: i64) {
        let (o0, n0) = (self.stored_objects(), self.stored_names());
        {
            let mut s = self.c.prepare_cached("INSERT INTO bx_object(num, object_id) VALUES (?1, ?2)").unwrap();
            for (i, id) in self.pending_objects.iter().enumerate() {
                s.execute(params![(o0 + i as u64) as i64, id]).unwrap();
            }
            let mut s = self.c.prepare_cached("INSERT INTO bx_name(num, name) VALUES (?1, ?2)").unwrap();
            for (i, n) in self.pending_names.iter().enumerate() {
                s.execute(params![(n0 + i as u64) as i64, n]).unwrap();
            }
            let mut s = self.c.prepare_cached("INSERT INTO bx_commit(seq, commit_id) VALUES (?1, ?2)").unwrap();
            for (seq, id) in &self.pending_commits {
                s.execute(params![*seq as i64, id]).unwrap();
            }
        }
        self.c
            .prepare_cached("UPDATE bx_meta SET applied=?1, objects=?2, names=?3 WHERE id=1")
            .unwrap()
            .execute(params![applied, self.objects as i64, self.names as i64])
            .unwrap();
        self.pending_objects.clear();
        self.pending_names.clear();
        self.pending_index.clear();
        self.pending_name_index.clear();
        self.pending_commits.clear();
    }
}
impl Refs for SqlRefs<'_> {
    fn objects(&mut self) -> u64 {
        self.objects
    }
    fn object(&mut self, num: u64) -> Vec<u8> {
        let stored = self.stored_objects();
        if num >= stored {
            return self.pending_objects[(num - stored) as usize].clone();
        }
        self.lookups += 1;
        self.c.prepare_cached("SELECT object_id FROM bx_object WHERE num=?1").unwrap().query_row([num as i64], |r| r.get(0)).unwrap()
    }
    fn object_num(&mut self, id: &[u8]) -> Option<u64> {
        if let Some(n) = self.pending_index.get(id) {
            return Some(*n);
        }
        self.lookups += 1;
        self.c
            .prepare_cached("SELECT num FROM bx_object WHERE object_id=?1")
            .unwrap()
            .query_row([id], |r| r.get::<_, i64>(0))
            .optional()
            .unwrap()
            .map(|n| n as u64)
    }
    fn names(&mut self) -> u64 {
        self.names
    }
    fn name(&mut self, num: u64) -> Vec<u8> {
        let stored = self.stored_names();
        if num >= stored {
            return self.pending_names[(num - stored) as usize].clone();
        }
        self.lookups += 1;
        self.c.prepare_cached("SELECT name FROM bx_name WHERE num=?1").unwrap().query_row([num as i64], |r| r.get(0)).unwrap()
    }
    fn name_num(&mut self, name: &[u8]) -> Option<u64> {
        if let Some(n) = self.pending_name_index.get(name) {
            return Some(*n);
        }
        self.lookups += 1;
        self.c
            .prepare_cached("SELECT num FROM bx_name WHERE name=?1")
            .unwrap()
            .query_row([name], |r| r.get::<_, i64>(0))
            .optional()
            .unwrap()
            .map(|n| n as u64)
    }
    fn commit(&mut self, bundle: u64) -> Vec<u8> {
        if let Some((_, id)) = self.pending_commits.iter().find(|(s, _)| *s == bundle) {
            return id.clone();
        }
        self.lookups += 1;
        self.c.prepare_cached("SELECT commit_id FROM bx_commit WHERE seq=?1").unwrap().query_row([bundle as i64], |r| r.get(0)).unwrap()
    }
    fn commit_bundle(&mut self, id: &[u8]) -> Option<u64> {
        if let Some((s, _)) = self.pending_commits.iter().find(|(_, c)| c == id) {
            return Some(*s);
        }
        self.lookups += 1;
        self.c
            .prepare_cached("SELECT seq FROM bx_commit WHERE commit_id=?1")
            .unwrap()
            .query_row([id], |r| r.get::<_, i64>(0))
            .optional()
            .unwrap()
            .map(|n| n as u64)
    }
}

/// A Commit row fetched from Postgres in the SQLite column order.
pub type CommitRow = Vec<rusqlite::types::Value>;

pub fn commit_columns(c: &Connection) -> Vec<(String, String)> {
    let mut s = c.prepare("PRAGMA table_info(\"commit\")").unwrap();
    s.query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?))).unwrap().map(Result::unwrap).collect()
}

pub fn pg_commit_row(row: &postgres::Row, at: usize, cols: &[(String, String)]) -> CommitRow {
    use rusqlite::types::Value;
    cols.iter()
        .enumerate()
        .map(|(i, (_, ty))| match ty.to_uppercase().as_str() {
            "INTEGER" => row.get::<_, Option<i64>>(at + i).map(Value::Integer).unwrap_or(Value::Null),
            "BLOB" => row.get::<_, Option<Vec<u8>>>(at + i).map(Value::Blob).unwrap_or(Value::Null),
            _ => row.get::<_, Option<String>>(at + i).map(Value::Text).unwrap_or(Value::Null),
        })
        .collect()
}

pub struct Pack {
    pub id: i64,
    pub domain: i64,
    pub digest: Vec<u8>,
    pub body: Vec<u8>,
}

/// Fetch whole packs by id in one round trip.
pub fn fetch_packs(pg: &mut Client, layout: &str, ids: &[i64]) -> Vec<Pack> {
    match layout {
        "a4" => pg
            .query("SELECT pack_id, domain, digest, body FROM pack_a4 WHERE pack_id = ANY($1)", &[&ids])
            .unwrap()
            .iter()
            .map(|r| Pack { id: r.get(0), domain: i64::from(r.get::<_, i16>(1)), digest: r.get(2), body: r.get(3) })
            .collect(),
        "fit" => {
            let mut out: Vec<Pack> = Vec::new();
            for r in pg.query("SELECT pack_id, seq, domain, digest, body FROM pack_fit WHERE pack_id = ANY($1) ORDER BY pack_id, seq", &[&ids]).unwrap() {
                let (id, seq): (i64, i32) = (r.get(0), r.get(1));
                let body: Vec<u8> = r.get(4);
                if seq == 0 {
                    out.push(Pack { id, domain: i64::from(r.get::<_, i16>(2)), digest: r.get(3), body });
                } else {
                    let p = out.last_mut().unwrap();
                    assert_eq!(p.id, id);
                    p.body.extend_from_slice(&body);
                }
            }
            out
        }
        other => panic!("layout {other}"),
    }
}

/// Insert one decoded bundle's rows (inside the caller's transaction).
pub fn insert_rows(c: &Connection, rows: &Rows, commit_id: &[u8], commit: Option<&CommitRow>, ncols: usize, packs: &[&Pack], move_head: bool) {
    let mut s = c.prepare_cached("INSERT INTO pack(pack_id, domain, digest, length, body) VALUES (?1, ?2, ?3, ?4, ?5)").unwrap();
    let mut stored = 0i64;
    let mut metadata = 0i64;
    for p in packs {
        s.execute(params![p.id, p.domain, p.digest, p.body.len() as i64, p.body]).unwrap();
        stored += p.body.len() as i64;
        if p.domain == 0 {
            metadata += p.body.len() as i64;
        }
    }
    let mut s = c.prepare_cached("INSERT INTO object_location(object_id, role, canonical_length, pack_id, group_number, record_number) VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT(object_id) DO NOTHING").unwrap();
    let mut canonical = 0i64;
    for o in &rows.ol {
        s.execute(params![o.id, o.role, o.clen, o.pack, o.group, o.rec]).unwrap();
        canonical += o.clen;
    }
    let mut s = c.prepare_cached("INSERT INTO inode_value(ordinal, value) VALUES (?1, ?2)").unwrap();
    for v in &rows.iv {
        s.execute(params![v.ordinal, v.value]).unwrap();
    }
    let mut s = c.prepare_cached("INSERT INTO binding_hint(child, parent, name) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING").unwrap();
    for h in &rows.bh {
        s.execute(params![h.child, h.parent, h.name]).unwrap();
    }
    let mut s = c.prepare_cached("INSERT INTO file_digest(digest, length, root) VALUES (?1, ?2, ?3) ON CONFLICT(digest, length) DO UPDATE SET root = excluded.root").unwrap();
    for d in &rows.fd {
        s.execute(params![d.digest, d.length, d.root]).unwrap();
    }
    let mut s = c.prepare_cached("INSERT INTO content_signature(slot, stamp, object_id, signature) VALUES (?1, ?2, ?3, ?4) ON CONFLICT(slot) DO UPDATE SET stamp=excluded.stamp, object_id=excluded.object_id, signature=excluded.signature WHERE excluded.stamp>=content_signature.stamp").unwrap();
    for x in &rows.cs {
        s.execute(params![x.slot, x.stamp, x.object, x.signature]).unwrap();
    }
    if let Some(row) = commit {
        let marks = (1..=ncols).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",");
        c.prepare_cached(&format!("INSERT INTO \"commit\" VALUES ({marks})")).unwrap().execute(rusqlite::params_from_iter(row.iter())).unwrap();
    }
    let mut s = c.prepare_cached("INSERT INTO commit_lineage(commit_id, run, depth, newest, merged) VALUES (?1, ?2, ?3, ?4, ?5)").unwrap();
    for l in &rows.cl {
        s.execute(params![commit_id, l.run, l.depth, l.newest, l.merged]).unwrap();
    }
    c.prepare_cached("UPDATE store_totals SET canonical=canonical+?1, stored=stored+?2, metadata=metadata+?3 WHERE id=1").unwrap().execute(params![canonical, stored, metadata]).unwrap();
    if move_head {
        if let Some(row) = commit {
            // branch_id is column 6 (commit_id, layer_stack_id, root_id, parent, base, second, branch_id, ...)
            if let rusqlite::types::Value::Blob(b) = &row[6] {
                c.prepare_cached("UPDATE branch SET head_commit_id=?1 WHERE branch_id=?2").unwrap().execute(params![commit_id, b]).unwrap();
            }
        }
    }
}

#[derive(Default, Clone, Copy)]
pub struct Phase {
    pub fetch_ms: f64,
    pub decode_ms: f64,
    pub pack_ms: f64,
    pub apply_ms: f64,
    pub round_trips: usize,
    pub bundles: usize,
    pub rows: usize,
    pub body_bytes: usize,
    pub pack_bytes: usize,
    pub lookups: usize,
}

/// Fetch, decode and apply up to `count` bundles after the applied seq,
/// `batch` per round-trip pair and transaction. Returns per-batch phases.
pub fn apply_range(c: &Connection, pg: &mut Client, btable: &str, layout: &str, codec: &str, dict: &DDict, count: usize, batch: usize, move_head: bool) -> Vec<Phase> {
    let cols = commit_columns(c);
    let names = cols.iter().map(|(n, _)| format!("c.\"{n}\"")).collect::<Vec<_>>().join(",");
    let q = pg.prepare(&format!("SELECT c.seq, b.body, {names} FROM \"commit\" c JOIN {btable} b ON b.commit_id = c.commit_id WHERE c.seq > $1 ORDER BY c.seq LIMIT $2")).unwrap();
    let init = pg.prepare(&format!("SELECT body FROM {btable} WHERE commit_id = $1")).unwrap();
    let mut out = Vec::new();
    let mut done = 0;
    while done < count {
        let mut ph = Phase::default();
        let applied: i64 = c.query_row("SELECT applied FROM bx_meta", [], |r| r.get(0)).unwrap();
        let n = batch.min(count - done);
        let t = Instant::now();
        // (seq, commit id, body, commit row)
        let mut fetched: Vec<(u64, Vec<u8>, Vec<u8>, Option<CommitRow>)> = Vec::new();
        if applied < 0 {
            let zero = vec![0u8; 33];
            let r = pg.query_one(&init, &[&zero]).unwrap();
            fetched.push((0, zero, r.get(0), None));
            ph.round_trips += 1;
        }
        let rest = n - fetched.len();
        if rest > 0 {
            for r in pg.query(&q, &[&applied.max(0), &(rest as i64)]).unwrap() {
                let row = pg_commit_row(&r, 2, &cols);
                let id = match &row[0] {
                    rusqlite::types::Value::Blob(b) => b.clone(),
                    _ => unreachable!(),
                };
                fetched.push((r.get::<_, i64>(0) as u64, id, r.get(1), Some(row)));
            }
            ph.round_trips += 1;
        }
        ph.fetch_ms = t.elapsed().as_secs_f64() * 1e3;
        if fetched.is_empty() {
            break;
        }
        let t = Instant::now();
        c.execute_batch("BEGIN IMMEDIATE").unwrap();
        let mut refs = SqlRefs::new(c);
        let mut decoded = Vec::with_capacity(fetched.len());
        for (seq, id, body, _) in &fetched {
            ph.body_bytes += body.len();
            let raw = if codec == "zstd" { dict.decompress(body) } else { body.clone() };
            let (rows, new_names) = decode(&raw, *seq, id, &mut refs);
            let numbered = raw[1] & 1 != 0;
            refs.absorb(&rows, &new_names, *seq, id, numbered);
            ph.rows += rows.count();
            decoded.push(rows);
        }
        ph.decode_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let mut ids: Vec<i64> = decoded.iter().flat_map(|r| r.ol.iter().map(|o| o.pack)).collect();
        ids.sort_unstable();
        ids.dedup();
        let packs = fetch_packs(pg, layout, &ids);
        assert_eq!(packs.len(), ids.len(), "packs fetched");
        ph.round_trips += 1;
        ph.pack_bytes = packs.iter().map(|p| p.body.len()).sum();
        let by_id: HashMap<i64, &Pack> = packs.iter().map(|p| (p.id, p)).collect();
        ph.pack_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        for ((seq, id, _, row), rows) in fetched.iter().zip(&decoded) {
            let mut mine: Vec<i64> = rows.ol.iter().map(|o| o.pack).collect();
            mine.sort_unstable();
            mine.dedup();
            let ps: Vec<&Pack> = mine.iter().filter_map(|p| by_id.get(p).copied()).collect();
            let _ = seq;
            insert_rows(c, rows, id, row.as_ref(), cols.len(), &ps, move_head);
        }
        refs.flush(fetched.last().unwrap().0 as i64);
        ph.lookups = refs.lookups;
        c.execute_batch("COMMIT").unwrap();
        ph.apply_ms = t.elapsed().as_secs_f64() * 1e3;
        ph.bundles = fetched.len();
        done += fetched.len();
        out.push(ph);
    }
    out
}

pub fn main_apply(cache: &str, url: &str, btable: &str, layout: &str, codec: &str, dict: &str, count: usize, batch: usize, out: &str) {
    let dict_bytes = (dict != "-").then(|| std::fs::read(dict).unwrap());
    let dd = DDict::new(dict_bytes.as_deref());
    // Bulk bootstrap batches (batch > 1) run with foreign keys off, as a
    // loader; one-bundle catch-up runs the product profile (keys on).
    let c = open(cache, batch == 1);
    let mut pg = crate::pg(url);
    let t = Instant::now();
    let phases = apply_range(&c, &mut pg, btable, layout, codec, &dd, count, batch, batch == 1);
    let wall = t.elapsed().as_secs_f64() * 1e3;
    let sum = |f: fn(&Phase) -> f64| phases.iter().map(f).sum::<f64>();
    let bundles: usize = phases.iter().map(|p| p.bundles).sum();
    let mut per: Vec<f64> = phases.iter().map(|p| p.fetch_ms + p.decode_ms + p.pack_ms + p.apply_ms).collect();
    per.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| if per.is_empty() { 0.0 } else { per[((per.len() - 1) as f64 * p).round() as usize] };
    let applied: i64 = c.query_row("SELECT applied FROM bx_meta", [], |r| r.get(0)).unwrap();
    let lines: Vec<String> = phases
        .iter()
        .map(|p| {
            format!(
                "{{\"bundles\":{},\"rows\":{},\"body_bytes\":{},\"pack_bytes\":{},\"round_trips\":{},\"lookups\":{},\"fetch_ms\":{:.3},\"decode_ms\":{:.3},\"pack_ms\":{:.3},\"apply_ms\":{:.3}}}",
                p.bundles, p.rows, p.body_bytes, p.pack_bytes, p.round_trips, p.lookups, p.fetch_ms, p.decode_ms, p.pack_ms, p.apply_ms
            )
        })
        .collect();
    std::fs::write(format!("{out}.batches.jsonl"), lines.join("\n") + "\n").unwrap();
    let text = format!(
        "{{\"cache\":\"{cache}\",\"table\":\"{btable}\",\"layout\":\"{layout}\",\"codec\":\"{codec}\",\"batch\":{batch},\"bundles\":{bundles},\"applied_seq\":{applied},\"wall_ms\":{wall:.1},\"bundles_per_s\":{:.1},\
\"fetch_ms\":{:.1},\"decode_ms\":{:.1},\"pack_ms\":{:.1},\"apply_ms\":{:.1},\"rows\":{},\"body_bytes\":{},\"pack_bytes\":{},\"round_trips\":{},\"lookups\":{},\"per_batch_ms\":{{\"p50\":{:.3},\"p95\":{:.3},\"max\":{:.3}}}}}",
        bundles as f64 / (wall / 1e3),
        sum(|p| p.fetch_ms), sum(|p| p.decode_ms), sum(|p| p.pack_ms), sum(|p| p.apply_ms),
        phases.iter().map(|p| p.rows).sum::<usize>(), phases.iter().map(|p| p.body_bytes).sum::<usize>(), phases.iter().map(|p| p.pack_bytes).sum::<usize>(),
        phases.iter().map(|p| p.round_trips).sum::<usize>(), phases.iter().map(|p| p.lookups).sum::<usize>(),
        q(0.5), q(0.95), q(1.0)
    );
    std::fs::write(out, format!("{text}\n")).unwrap();
    println!("{text}");
}

/// Content tables equal both ways, plus foreign keys and integrity.
pub fn verify(cache: &str, fixture: &str) {
    let t = Instant::now();
    let c = open(cache, false);
    c.execute_batch(&format!("ATTACH 'file:{fixture}?mode=ro&immutable=1' AS fx")).unwrap();
    let mut parts = Vec::new();
    // (table, join key, equality of the other columns); equal iff both
    // counts equal the matched count.
    for (table, key, same) in [
        ("pack", "m.pack_id=f.pack_id", "m.domain=f.domain AND m.digest=f.digest AND m.length=f.length AND m.body=f.body"),
        ("object_location", "m.object_id=f.object_id", "m.role=f.role AND m.canonical_length=f.canonical_length AND m.pack_id=f.pack_id AND m.group_number=f.group_number AND m.record_number=f.record_number"),
        ("inode_value", "m.ordinal=f.ordinal", "m.value=f.value"),
        ("binding_hint", "m.child=f.child AND m.parent=f.parent AND m.name=f.name", "1"),
        ("file_digest", "m.digest=f.digest AND m.length=f.length", "m.root=f.root"),
        ("content_signature", "m.slot=f.slot", "m.stamp=f.stamp AND m.object_id=f.object_id AND m.signature=f.signature"),
        ("\"commit\"", "m.commit_id=f.commit_id", "m.root_id=f.root_id AND m.parent_commit_id IS f.parent_commit_id AND m.branch_id IS f.branch_id AND m.created_ms=f.created_ms AND m.logical=f.logical AND m.lww=f.lww AND m.message IS f.message"),
        ("commit_lineage", "m.commit_id=f.commit_id", "m.run=f.run AND m.depth=f.depth AND m.newest=f.newest AND m.merged IS f.merged"),
    ] {
        let n: i64 = c.query_row(&format!("SELECT count(*) FROM main.{table}"), [], |r| r.get(0)).unwrap();
        let nf: i64 = c.query_row(&format!("SELECT count(*) FROM fx.{table}"), [], |r| r.get(0)).unwrap();
        let eq: i64 = c.query_row(&format!("SELECT count(*) FROM main.{table} m JOIN fx.{table} f ON {key} WHERE {same}"), [], |r| r.get(0)).unwrap();
        parts.push(format!("\"{}\":{{\"cache\":{n},\"fixture\":{nf},\"equal\":{eq},\"ok\":{}}}", table.trim_matches('"'), n == nf && nf == eq));
    }
    let fk = c.prepare("PRAGMA main.foreign_key_check").unwrap().query_map([], |_| Ok(())).unwrap().count();
    let totals: (i64, i64, i64) = c.query_row("SELECT canonical, stored, metadata FROM main.store_totals", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    let fx: (i64, i64, i64) = c.query_row("SELECT canonical, stored, metadata FROM fx.store_totals", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
    println!(
        "{{\"tables\":{{{}}},\"foreign_key_violations\":{fk},\"totals_cache\":{totals:?},\"totals_fixture\":{fx:?},\"ms\":{:.0}}}",
        parts.join(","),
        t.elapsed().as_secs_f64() * 1e3
    );
}

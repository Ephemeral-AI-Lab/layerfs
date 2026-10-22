//! Postgres side of the bundle model (#420): ported history tables, bundle
//! tables per variant, pack layouts, sizes.
//!
//!   bport FIXTURE COMMITS URL      ported history tables + commit.seq, keys
//!   bload URL TABLE BODIES COMMITS MODE [DICT]
//!       one bundle table: (commit_id bytea PK, body bytea); MODE z<level>
//!       (zstd, STORAGE EXTERNAL), d<level> (zstd with DICT), lz4 (raw body,
//!       COMPRESSION lz4), raw (raw body, STORAGE EXTERNAL)
//!   bpacks FIXTURE URL LAYOUT FROM TO  pack ids [FROM, TO): a4 = the study's
//!       pk_a4 (one row per pack, lz4 TOAST, toast_tuple_target 8160);
//!       fit = page-filling slices (8,084 / 8,120 bytes, STORAGE PLAIN,
//!       full slices first, then remainders largest first)
//!   bpkeys URL LAYOUT              keys after the pack load, VACUUM ANALYZE
//!   bsizes URL                     every relation, JSON
use crate::codec::{compress, read_bodies, read_commits};
use crate::{pg, ro};
use postgres::binary_copy::BinaryCopyInWriter;
use postgres::types::{ToSql, Type};
use std::time::Instant;

/// History tables kept as rows in Postgres (everything the bundle does not carry).
pub const PORTED: [&str; 11] = [
    "commit", "branch", "layer", "layer_lineage", "layer_stack", "scope_allocator", "store_policy", "store_totals", "history_meta", "reach", "absorbed",
];

pub fn port(fixture: &str, commits: &str, url: &str) {
    let mut c = pg(url);
    let t = Instant::now();
    let db = ro(fixture);
    for table in PORTED {
        let cols = crate::columns(&db, table);
        let ddl = cols.iter().map(|(n, ty, nn)| format!("\"{n}\" {ty}{}", if *nn { " NOT NULL" } else { "" })).collect::<Vec<_>>().join(", ");
        c.batch_execute(&format!("CREATE TABLE \"{table}\" ({ddl})")).unwrap();
        crate::load(fixture, url, table);
    }
    // Publication order: commit.seq = the bundle index (commits.bin order).
    let order = read_commits(commits);
    c.batch_execute("CREATE TEMP TABLE seqmap (commit_id bytea, seq bigint)").unwrap();
    {
        let sink = c.copy_in("COPY seqmap FROM STDIN BINARY").unwrap();
        let mut w = BinaryCopyInWriter::new(sink, &[Type::BYTEA, Type::INT8]);
        for (k, id) in order.iter().enumerate().skip(1) {
            w.write(&[id as &(dyn ToSql + Sync), &(k as i64)]).unwrap();
        }
        w.finish().unwrap();
    }
    let pk = |t: &str| crate::primary_key(&db, t).iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
    let mut sql = String::from(
        "CREATE TABLE commit_seq AS SELECT c.*, s.seq FROM \"commit\" c JOIN seqmap s USING (commit_id) ORDER BY s.seq;
         DROP TABLE \"commit\"; ALTER TABLE commit_seq RENAME TO \"commit\";
         ALTER TABLE \"commit\" ALTER COLUMN seq SET NOT NULL;",
    );
    for t in PORTED {
        sql += &format!("ALTER TABLE \"{t}\" ADD PRIMARY KEY ({});", pk(t));
    }
    let mut s = db.prepare("SELECT tbl_name, sql FROM sqlite_master WHERE type='index' AND sql IS NOT NULL ORDER BY name").unwrap();
    for (table, idx) in s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).unwrap().map(Result::unwrap) {
        if PORTED.contains(&table.as_str()) {
            sql += &idx;
            sql += ";";
        }
    }
    sql += "CREATE UNIQUE INDEX commit_seq_order ON \"commit\"(seq);
        ALTER TABLE branch ADD FOREIGN KEY (layer_stack_id, head_commit_id) REFERENCES \"commit\"(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED;
        ALTER TABLE \"commit\" ADD FOREIGN KEY (layer_stack_id, parent_commit_id) REFERENCES \"commit\"(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED;
        CREATE FUNCTION commit_logical() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN UPDATE store_totals SET logical=logical+NEW.logical WHERE id=1; RETURN NULL; END$$;
        CREATE TRIGGER commit_logical AFTER INSERT ON \"commit\" FOR EACH ROW EXECUTE FUNCTION commit_logical();
        CREATE TRIGGER layer_logical AFTER INSERT ON layer FOR EACH ROW EXECUTE FUNCTION commit_logical();";
    c.batch_execute(&sql).unwrap();
    c.batch_execute("VACUUM ANALYZE").unwrap();
    let n: i64 = c.query_one("SELECT count(*) FROM \"commit\"", &[]).unwrap().get(0);
    println!("{{\"ported\":{},\"commits\":{n},\"ms\":{:.0}}}", PORTED.len(), t.elapsed().as_secs_f64() * 1e3);
}

pub fn load_bundles(url: &str, table: &str, bodies: &str, commits: &str, mode: &str, dict: Option<&str>) {
    let t = Instant::now();
    let bodies = read_bodies(bodies);
    let ids = read_commits(commits);
    assert_eq!(bodies.len(), ids.len());
    let dict = dict.map(|p| std::fs::read(p).unwrap());
    let (storage, compressed) = match mode.as_bytes()[0] {
        b'z' | b'd' => ("bytea STORAGE EXTERNAL", true),
        _ if mode == "lz4" => ("bytea COMPRESSION lz4", false),
        _ if mode == "raw" => ("bytea STORAGE EXTERNAL", false),
        _ => panic!("mode {mode}"),
    };
    let level: i32 = if compressed { mode[1..].parse().unwrap() } else { 0 };
    let mut c = pg(url);
    c.batch_execute(&format!("CREATE TABLE {table} (commit_id bytea NOT NULL, body {storage} NOT NULL) WITH (toast_tuple_target = 8160)")).unwrap();
    let (mut raw, mut stored) = (0usize, 0usize);
    {
        let sink = c.copy_in(&format!("COPY {table} FROM STDIN BINARY")).unwrap();
        let mut w = BinaryCopyInWriter::new(sink, &[Type::BYTEA, Type::BYTEA]);
        for (body, id) in bodies.iter().zip(&ids) {
            raw += body.len();
            let b = if compressed { compress(body, level, if mode.starts_with('d') { dict.as_deref() } else { None }) } else { body.clone() };
            stored += b.len();
            w.write(&[id as &(dyn ToSql + Sync), &b]).unwrap();
        }
        w.finish().unwrap();
    }
    c.batch_execute(&format!("ALTER TABLE {table} ADD PRIMARY KEY (commit_id)")).unwrap();
    c.batch_execute(&format!("VACUUM ANALYZE {table}")).unwrap();
    if let Some(d) = &dict {
        if mode.starts_with('d') {
            c.batch_execute("CREATE TABLE IF NOT EXISTS bundle_dict (name text PRIMARY KEY, dict bytea NOT NULL)").unwrap();
            c.execute("INSERT INTO bundle_dict VALUES ($1, $2)", &[&table, d]).unwrap();
        }
    }
    println!(
        "{{\"table\":\"{table}\",\"mode\":\"{mode}\",\"bundles\":{},\"raw_bytes\":{raw},\"client_bytes\":{stored},\"dict_bytes\":{},\"ms\":{:.0}}}",
        bodies.len(),
        dict.as_ref().map(|d| d.len()).unwrap_or(0),
        t.elapsed().as_secs_f64() * 1e3
    );
}

pub const SLICE0: usize = 8084;
pub const SLICE: usize = 8120;

pub fn load_packs(fixture: &str, url: &str, layout: &str, from: i64, to: i64) {
    let t = Instant::now();
    let db = ro(fixture);
    let mut c = pg(url);
    let mut s = db.prepare("SELECT pack_id, length, domain, digest, body FROM pack WHERE pack_id >= ?1 AND pack_id < ?2 ORDER BY pack_id").unwrap();
    let rows = s
        .query_map([from, to], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, Vec<u8>>(3)?, r.get::<_, Vec<u8>>(4)?)))
        .unwrap()
        .map(Result::unwrap);
    let (mut n, mut bytes) = (0usize, 0usize);
    match layout {
        "a4" => {
            c.batch_execute("CREATE TABLE IF NOT EXISTS pack_a4 (pack_id bigint NOT NULL, length int4 NOT NULL, domain int2 NOT NULL, digest bytea NOT NULL, body bytea COMPRESSION lz4 NOT NULL) WITH (toast_tuple_target = 8160)").unwrap();
            let sink = c.copy_in("COPY pack_a4 FROM STDIN BINARY").unwrap();
            let mut w = BinaryCopyInWriter::new(sink, &[Type::INT8, Type::INT4, Type::INT2, Type::BYTEA, Type::BYTEA]);
            for (id, len, dom, dig, body) in rows {
                w.write(&[&id as &(dyn ToSql + Sync), &(len as i32), &(dom as i16), &dig, &body]).unwrap();
                n += 1;
                bytes += body.len();
            }
            w.finish().unwrap();
        }
        "fit" => {
            c.batch_execute("CREATE TABLE IF NOT EXISTS pack_fit (pack_id bigint NOT NULL, seq int4 NOT NULL, domain int2, digest bytea, body bytea STORAGE PLAIN NOT NULL)").unwrap();
            // (pack, seq, domain/digest on seq 0, slice)
            let mut full: Vec<(i64, i32, Option<(i16, Vec<u8>)>, Vec<u8>)> = Vec::new();
            let mut tails = Vec::new();
            for (id, _len, dom, dig, body) in rows {
                n += 1;
                bytes += body.len();
                let mut at = 0;
                let mut seq = 0;
                while at < body.len() {
                    let cap = if seq == 0 { SLICE0 } else { SLICE };
                    let end = (at + cap).min(body.len());
                    let head = (seq == 0).then(|| (dom as i16, dig.clone()));
                    let piece = (id, seq, head, body[at..end].to_vec());
                    if end - at == cap { full.push(piece) } else { tails.push(piece) }
                    at = end;
                    seq += 1;
                }
            }
            tails.sort_by(|a, b| b.3.len().cmp(&a.3.len()));
            let sink = c.copy_in("COPY pack_fit FROM STDIN BINARY").unwrap();
            let mut w = BinaryCopyInWriter::new(sink, &[Type::INT8, Type::INT4, Type::INT2, Type::BYTEA, Type::BYTEA]);
            for (id, seq, head, piece) in full.iter().chain(tails.iter()) {
                let (dom, dig) = match head {
                    Some((d, g)) => (Some(*d), Some(g.clone())),
                    None => (None, None),
                };
                w.write(&[id as &(dyn ToSql + Sync), seq, &dom, &dig, piece]).unwrap();
            }
            w.finish().unwrap();
        }
        other => panic!("layout {other}"),
    }
    println!("{{\"layout\":\"{layout}\",\"from\":{from},\"to\":{to},\"packs\":{n},\"bytes\":{bytes},\"ms\":{:.0}}}", t.elapsed().as_secs_f64() * 1e3);
}

pub fn pack_keys(url: &str, layout: &str) {
    let t = Instant::now();
    let mut c = pg(url);
    let sql = match layout {
        "a4" => "ALTER TABLE pack_a4 ADD PRIMARY KEY (pack_id)",
        "fit" => "ALTER TABLE pack_fit ADD PRIMARY KEY (pack_id, seq)",
        other => panic!("layout {other}"),
    };
    c.batch_execute(sql).unwrap();
    c.batch_execute(&format!("VACUUM ANALYZE pack_{layout}")).unwrap();
    println!("{{\"keys\":\"{layout}\",\"ms\":{:.0}}}", t.elapsed().as_secs_f64() * 1e3);
}

pub fn sizes(url: &str) {
    let mut c = pg(url);
    let rows = c
        .query(
            "SELECT c.relname, pg_total_relation_size(c.oid), pg_relation_size(c.oid), pg_relation_size(c.oid,'fsm')+pg_relation_size(c.oid,'vm'),
                    coalesce(pg_total_relation_size(nullif(c.reltoastrelid,0)),0), pg_indexes_size(c.oid), c.reltuples::bigint
             FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE c.relkind='r' AND n.nspname='public' ORDER BY 1",
            &[],
        )
        .unwrap();
    let parts: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "\"{}\":{{\"total\":{},\"heap\":{},\"fsm_vm\":{},\"toast\":{},\"indexes\":{},\"rows\":{}}}",
                r.get::<_, String>(0), r.get::<_, i64>(1), r.get::<_, i64>(2), r.get::<_, i64>(3), r.get::<_, i64>(4), r.get::<_, i64>(5), r.get::<_, i64>(6)
            )
        })
        .collect();
    let db: i64 = c.query_one("SELECT pg_database_size(current_database())", &[]).unwrap().get(0);
    println!("{{\"database\":{db},\"tables\":{{{}}}}}", parts.join(","));
}

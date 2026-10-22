//! Issue #420 experiment: the C12 cycle's global-Store work replayed on the
//! product SQLite profile and on PostgreSQL. Not product code.
//!
//! Subcommands (one bounded step each; see ../README in the receipts):
//!   schema PG                         ported tables, no keys yet
//!   load SQLITE PG TABLE              COPY BINARY one table from a SQLite file
//!   finish PG                         keys, indexes, FKs, triggers, ANALYZE
//!   sizes SQLITE PG                   per-table bytes, both engines (JSON)
//!   rtt PG N                          SELECT 1 round trips (JSON)
//!   replay ARM VARIANT SNAP INFO SQLITE PG OUT
//!       ARM sqlite | pg | cache; VARIANT stmt | 2rt | 1rt (cache only)
//!       SNAP: capture directory (s0..s3 .sqlite); INFO: pristine base file
//!       SQLITE: writable SQLite clone (sqlite, cache); PG: clone database URL
mod contend;
mod pr;
mod replay;
mod workload;

use postgres::{Client, NoTls};
use rusqlite::{types::ValueRef, Connection, OpenFlags};
use std::time::Instant;

/// Tables the captured work touches or reads, ported with their rows.
pub const TABLES: [&str; 18] = [
    "pack", "object_location", "inode_value", "binding_hint", "file_digest", "commit",
    "commit_lineage", "branch", "layer", "layer_lineage", "layer_stack", "content_signature",
    "scope_allocator", "store_policy", "store_totals", "history_meta", "reach", "absorbed",
];

pub fn ro(path: &str) -> Connection {
    Connection::open_with_flags(
        format!("file:{path}?mode=ro&immutable=1"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .expect("open sqlite read-only")
}

pub fn pg(url: &str) -> Client {
    Client::connect(url, NoTls).expect("connect postgres")
}

/// Column name, PG type and NOT NULL, from the STRICT SQLite declaration.
fn columns(db: &Connection, table: &str) -> Vec<(String, &'static str, bool)> {
    let mut s = db.prepare(&format!("PRAGMA table_info(\"{table}\")")).unwrap();
    let rows = s
        .query_map([], |r| Ok((r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)? != 0)))
        .unwrap();
    rows.map(|r| {
        let (name, ty, notnull) = r.unwrap();
        let pg = match ty.to_uppercase().as_str() {
            "INTEGER" => "bigint",
            "BLOB" => "bytea",
            "TEXT" => "text",
            other => panic!("{table}.{name}: unported type {other}"),
        };
        (name, pg, notnull)
    })
    .collect()
}

fn primary_key(db: &Connection, table: &str) -> Vec<String> {
    let mut s = db.prepare(&format!("PRAGMA table_info(\"{table}\")")).unwrap();
    let mut pk: Vec<(i64, String)> = s
        .query_map([], |r| Ok((r.get::<_, i64>(5)?, r.get::<_, String>(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .filter(|(n, _)| *n > 0)
        .collect();
    pk.sort();
    pk.into_iter().map(|(_, c)| c).collect()
}

fn schema(url: &str, sqlite: &str) {
    let db = ro(sqlite);
    let mut c = pg(url);
    let mut ddl = String::new();
    for t in TABLES {
        let cols = columns(&db, t)
            .into_iter()
            .map(|(n, ty, nn)| format!("\"{n}\" {ty}{}", if nn { " NOT NULL" } else { "" }))
            .collect::<Vec<_>>()
            .join(", ");
        ddl += &format!("CREATE TABLE \"{t}\" ({cols});\n");
    }
    // Pack bodies are already compressed/encoded: no TOAST compression.
    ddl += "ALTER TABLE pack ALTER COLUMN body SET STORAGE EXTERNAL;\n";
    c.batch_execute(&ddl).unwrap();
    println!("{{\"schema\":\"created\",\"tables\":{}}}", TABLES.len());
}

fn load(sqlite: &str, url: &str, table: &str) {
    use postgres::binary_copy::BinaryCopyInWriter;
    use postgres::types::{ToSql, Type};
    let db = ro(sqlite);
    let cols = columns(&db, table);
    let types: Vec<Type> = cols
        .iter()
        .map(|(_, ty, _)| match *ty {
            "bigint" => Type::INT8,
            "bytea" => Type::BYTEA,
            _ => Type::TEXT,
        })
        .collect();
    let names = cols.iter().map(|(n, _, _)| format!("\"{n}\"")).collect::<Vec<_>>().join(",");
    let mut c = pg(url);
    let started = Instant::now();
    let sink = c.copy_in(&format!("COPY \"{table}\" ({names}) FROM STDIN BINARY")).unwrap();
    let mut writer = BinaryCopyInWriter::new(sink, &types);
    let mut s = db.prepare(&format!("SELECT {names} FROM \"{table}\"")).unwrap();
    let mut rows = s.query([]).unwrap();
    let (mut count, mut bytes) = (0u64, 0u64);
    while let Some(row) = rows.next().unwrap() {
        let mut values: Vec<Box<dyn ToSql + Sync>> = Vec::with_capacity(cols.len());
        for (i, (_, ty, _)) in cols.iter().enumerate() {
            values.push(match (row.get_ref(i).unwrap(), *ty) {
                (ValueRef::Null, "bigint") => Box::new(None::<i64>),
                (ValueRef::Null, "bytea") => Box::new(None::<Vec<u8>>),
                (ValueRef::Null, _) => Box::new(None::<String>),
                (ValueRef::Integer(v), _) => Box::new(v),
                (ValueRef::Blob(b), _) => {
                    bytes += b.len() as u64;
                    Box::new(b.to_vec())
                }
                (ValueRef::Text(t), _) => {
                    bytes += t.len() as u64;
                    Box::new(String::from_utf8(t.to_vec()).unwrap())
                }
                (ValueRef::Real(_), _) => panic!("{table}: REAL value"),
            });
        }
        let refs: Vec<&(dyn ToSql + Sync)> = values.iter().map(|b| b.as_ref()).collect();
        writer.write(&refs).unwrap();
        count += 1;
    }
    writer.finish().unwrap();
    println!(
        "{{\"table\":\"{table}\",\"rows\":{count},\"value_bytes\":{bytes},\"ms\":{:.1}}}",
        started.elapsed().as_secs_f64() * 1e3
    );
}

/// Keys, the product's secondary indexes (same text), its FKs on the tables
/// the publish touches, the totals triggers, then ANALYZE.
fn finish(url: &str, sqlite: &str) {
    let db = ro(sqlite);
    let mut c = pg(url);
    let mut steps: Vec<(String, String)> = Vec::new();
    for t in TABLES {
        let pk = primary_key(&db, t).iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(",");
        steps.push((format!("pk {t}"), format!("ALTER TABLE \"{t}\" ADD PRIMARY KEY ({pk})")));
    }
    let mut s = db
        .prepare("SELECT name, tbl_name, sql FROM sqlite_master WHERE type='index' AND sql IS NOT NULL ORDER BY name")
        .unwrap();
    for (name, table, sql) in s
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
        .unwrap()
        .map(Result::unwrap)
    {
        if TABLES.contains(&table.as_str()) {
            steps.push((format!("index {name}"), sql));
        }
    }
    for (name, sql) in [
        ("fk object_location.pack", "ALTER TABLE object_location ADD FOREIGN KEY (pack_id) REFERENCES pack(pack_id)"),
        ("fk content_signature.object", "ALTER TABLE content_signature ADD FOREIGN KEY (object_id) REFERENCES object_location(object_id)"),
        ("fk branch.stack", "ALTER TABLE branch ADD FOREIGN KEY (layer_stack_id) REFERENCES layer_stack(layer_stack_id) DEFERRABLE INITIALLY DEFERRED"),
        ("fk branch.base", "ALTER TABLE branch ADD FOREIGN KEY (layer_stack_id, base_layer_id) REFERENCES layer(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED"),
        ("fk branch.head", "ALTER TABLE branch ADD FOREIGN KEY (layer_stack_id, head_commit_id) REFERENCES \"commit\"(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED"),
        ("fk commit.base", "ALTER TABLE \"commit\" ADD FOREIGN KEY (layer_stack_id, base_layer_id) REFERENCES layer(layer_stack_id, layer_id) DEFERRABLE INITIALLY DEFERRED"),
        ("fk commit.parent", "ALTER TABLE \"commit\" ADD FOREIGN KEY (layer_stack_id, parent_commit_id) REFERENCES \"commit\"(layer_stack_id, commit_id) DEFERRABLE INITIALLY DEFERRED"),
        ("triggers", "CREATE FUNCTION commit_logical() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN UPDATE store_totals SET logical=logical+NEW.logical WHERE id=1; RETURN NULL; END$$;
CREATE TRIGGER commit_logical AFTER INSERT ON \"commit\" FOR EACH ROW EXECUTE FUNCTION commit_logical();
CREATE TRIGGER layer_logical AFTER INSERT ON layer FOR EACH ROW EXECUTE FUNCTION commit_logical();
CREATE FUNCTION pack_immutable() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'published pack immutable'; END$$;
CREATE TRIGGER pack_immutable BEFORE UPDATE OR DELETE ON pack FOR EACH ROW EXECUTE FUNCTION pack_immutable();"),
        ("analyze", "ANALYZE"),
    ] {
        steps.push((name.to_owned(), sql.to_owned()));
    }
    let mut out = Vec::new();
    for (name, sql) in steps {
        let t = Instant::now();
        c.batch_execute(&sql).unwrap_or_else(|e| panic!("{name}: {e}"));
        out.push(format!("\"{name}\":{:.1}", t.elapsed().as_secs_f64() * 1e3));
    }
    println!("{{{}}}", out.join(","));
}

fn sizes(sqlite: &str, url: &str) {
    let db = ro(sqlite);
    let mut by_table = std::collections::BTreeMap::<String, i64>::new();
    let stat = db.prepare(
        "SELECT coalesce(m.tbl_name, d.name), sum(d.pgsize) FROM dbstat d LEFT JOIN sqlite_master m ON m.name=d.name GROUP BY 1",
    );
    match stat {
        Ok(mut s) => {
            for (t, b) in s
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
                .unwrap()
                .map(Result::unwrap)
            {
                by_table.insert(t, b);
            }
        }
        Err(e) => eprintln!("dbstat unavailable: {e}"),
    }
    let mut c = pg(url);
    let mut parts = Vec::new();
    for t in TABLES {
        let r = c
            .query_one(
                "SELECT pg_total_relation_size(c.oid), pg_relation_size(c.oid), coalesce(pg_total_relation_size(nullif(c.reltoastrelid,0)),0), pg_indexes_size(c.oid) FROM pg_class c WHERE c.relname=$1 AND c.relkind='r'",
                &[&t],
            )
            .unwrap();
        let (total, heap, toast, idx): (i64, i64, i64, i64) = (r.get(0), r.get(1), r.get(2), r.get(3));
        parts.push(format!(
            "\"{t}\":{{\"sqlite\":{},\"pg_total\":{total},\"pg_heap\":{heap},\"pg_toast\":{toast},\"pg_indexes\":{idx}}}",
            by_table.get(t).copied().unwrap_or(-1)
        ));
    }
    let db_size: i64 = c.query_one("SELECT pg_database_size(current_database())", &[]).unwrap().get(0);
    let file = std::fs::metadata(sqlite).map(|m| m.len()).unwrap_or(0);
    println!("{{\"sqlite_file\":{file},\"pg_database\":{db_size},\"tables\":{{{}}}}}", parts.join(","));
}

fn rtt(url: &str, n: usize) {
    let mut c = pg(url);
    let s = c.prepare("SELECT 1").unwrap();
    for _ in 0..20 {
        c.query(&s, &[]).unwrap();
    }
    let mut us: Vec<f64> = (0..n)
        .map(|_| {
            let t = Instant::now();
            c.query(&s, &[]).unwrap();
            t.elapsed().as_secs_f64() * 1e6
        })
        .collect();
    us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| us[((us.len() - 1) as f64 * p).round() as usize];
    println!(
        "{{\"calls\":{n},\"warmup\":20,\"p50_us\":{:.1},\"p90_us\":{:.1},\"p99_us\":{:.1},\"min_us\":{:.1},\"max_us\":{:.1}}}",
        q(0.5), q(0.9), q(0.99), us[0], us[us.len() - 1]
    );
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| a.get(i).map(String::as_str).unwrap_or_else(|| panic!("missing argument {i}"));
    match arg(0) {
        "schema" => schema(arg(1), arg(2)),
        "load" => load(arg(1), arg(2), arg(3)),
        "finish" => finish(arg(1), arg(2)),
        "sizes" => sizes(arg(1), arg(2)),
        "rtt" => rtt(arg(1), arg(2).parse().unwrap()),
        "replay" => replay::main(arg(1), arg(2), arg(3), arg(4), arg(5), arg(6), arg(7)),
        "workload" => workload::describe(arg(1), arg(2)),
        "contend" => contend::parent(arg(1), arg(2).parse().unwrap(), arg(3).parse().unwrap(), arg(4), arg(5), arg(6), arg(7), arg(8)),
        "contend-child" => contend::child(arg(1), arg(2).parse().unwrap(), arg(3).parse().unwrap(), arg(4), arg(5), arg(6), arg(7)),
        other => panic!("unknown subcommand {other}"),
    }
}

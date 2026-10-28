//! Read-only plans for unchanged Store reservation/publication statements.
use rusqlite::{Connection, OpenFlags};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("closed database path required")?;
    let database = Connection::open_with_flags(
        format!("file:{path}?immutable=1"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;
    database.pragma_update(None, "foreign_keys", 1)?;
    println!(
        "ENGINE version={} source_id={}",
        rusqlite::version(),
        database.query_row("SELECT sqlite_source_id()", [], |r| r.get::<_, String>(0))?
    );
    for option in database
        .prepare("PRAGMA compile_options")?
        .query_map([], |r| r.get::<_, String>(0))?
    {
        println!("OPTION {}", option?);
    }
    for (name, sql, values) in [
        (
            "cursor",
            "SELECT next_pack_id,next_ordinal FROM store_policy WHERE id=1",
            vec![],
        ),
        (
            "advance",
            "UPDATE store_policy SET next_pack_id=?1,next_ordinal=?2 WHERE id=1",
            vec![64_i64, 102_i64],
        ),
    ] {
        let mut query = database.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?;
        for row in query.query_map(rusqlite::params_from_iter(values.iter()), |r| {
            r.get::<_, String>(3)
        })? {
            println!("PLAN {name} {}", row?);
        }
        let mut query = database.prepare(&format!("EXPLAIN {sql}"))?;
        for row in query.query_map(rusqlite::params_from_iter(values.iter()), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })? {
            println!("OPCODE {name} {:?}", row?);
        }
    }
    for rows in [1_usize, 2, 4, 8, 16, 32] {
        for (name, sql) in [
            (
                "pack",
                format!(
                    "INSERT INTO pack(pack_id,domain,digest,length,body) VALUES {}",
                    std::iter::repeat_n("(?,?,?,?,?)", rows)
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            ),
            (
                "locations",
                format!(
                    "INSERT INTO object_location(object_id,role,canonical_length,pack_id,group_number,record_number) VALUES {} ON CONFLICT(object_id) DO NOTHING RETURNING object_id",
                    std::iter::repeat_n("(?,?,?,?,?,?)", rows)
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            ),
        ] {
            println!("SOURCE {name} rows={rows} sql={sql}");
            // Only the EXPLAIN programs run. No INSERT is stepped. Bound NULLs
            // are representative bind slots, not a product publication fixture.
            for prefix in ["EXPLAIN QUERY PLAN", "EXPLAIN"] {
                let mut statement = database.prepare(&format!("{prefix} {sql}"))?;
                let values = vec![rusqlite::types::Value::Null; statement.parameter_count()];
                let count = statement.column_count();
                for row in statement.query_map(rusqlite::params_from_iter(values), |r| {
                    (0..count)
                        .map(|i| r.get::<_, rusqlite::types::Value>(i))
                        .collect::<Result<Vec<_>, _>>()
                })? {
                    println!("PUBLICATION {prefix} {name} rows={rows} {:?}", row?);
                }
            }
        }
    }
    let version: i64 = database.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if matches!(version, 7 | 10) {
        for (name,sql) in [
            ("segment_extent","SELECT p.body IS NULL,p.segment_id,p.segment_offset,s.device,s.inode,s.length FROM pack p LEFT JOIN body_segment s ON s.segment_id=p.segment_id WHERE p.pack_id=?1"),
            ("segment_insert","INSERT INTO body_segment(segment_id,device,inode,length) VALUES(?1,?2,?3,?4)"),
            ("extent_insert","INSERT INTO pack(pack_id,domain,digest,length,body,segment_id,segment_offset) VALUES(?1,1,?2,?3,NULL,?4,?5)"),
        ] {
            println!("SOURCE {name} sql={sql}");
            for prefix in ["EXPLAIN QUERY PLAN","EXPLAIN"] {
                let mut statement=database.prepare(&format!("{prefix} {sql}"))?;
                let values=vec![rusqlite::types::Value::Null;statement.parameter_count()];let count=statement.column_count();
                for row in statement.query_map(rusqlite::params_from_iter(values),|r|(0..count).map(|i|r.get::<_,rusqlite::types::Value>(i)).collect::<Result<Vec<_>,_>>())? {
                    println!("SEGMENT {prefix} {name} {:?}",row?);
                }
            }
        }
    }
    Ok(())
}

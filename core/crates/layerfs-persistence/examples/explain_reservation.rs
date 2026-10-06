//! Read-only plans for the unchanged Store reservation statements.
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
    Ok(())
}

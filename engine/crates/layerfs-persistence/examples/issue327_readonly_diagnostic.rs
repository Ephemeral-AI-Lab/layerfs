//! One labeled diagnostic of sealed WAL read-only setup, outside product source.
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let c = rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .unwrap();
    for sql in [
        "PRAGMA synchronous=OFF",
        "PRAGMA fullfsync=0",
        "PRAGMA journal_mode",
        "PRAGMA user_version",
        "BEGIN",
        "SELECT count(*) FROM history_meta",
        "COMMIT",
    ] {
        let result = (|| -> rusqlite::Result<()> {
            let mut stmt = c.prepare(sql)?;
            let mut rows = stmt.query([])?;
            while rows.next()?.is_some() {}
            Ok(())
        })();
        println!("{sql}: {result:?}");
    }
}

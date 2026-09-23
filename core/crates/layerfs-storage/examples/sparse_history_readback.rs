//! Reopen a closed Store and authenticate every distinct stored canonical object.
//!
//! Usage: sparse_history_readback STORE.sqlite

use std::error::Error;
use std::path::PathBuf;

use layerfs_content::object::{AuthenticatedObjects, ObjectId};
use layerfs_storage::{Store, StoreProvider};
use layerfs_telemetry::timer::Timing;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = PathBuf::from(args.next().ok_or("STORE.sqlite is required")?);
    if args.next().is_some() {
        return Err("expected exactly one Store path".into());
    }

    let connection =
        rusqlite::Connection::open_with_flags(&path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let page_size: i64 = connection.query_row("PRAGMA page_size", [], |row| row.get(0))?;
    if page_size != 4096 {
        return Err(format!("SQLite page size {page_size}, expected 4096").into());
    }
    let rows: i64 = connection.query_row("SELECT count(*) FROM objects", [], |row| row.get(0))?;
    let mut ids = Vec::new();
    let mut statement =
        connection.prepare("SELECT DISTINCT object_id FROM objects ORDER BY object_id")?;
    let mut query = statement.query([])?;
    while let Some(row) = query.next()? {
        let bytes: Vec<u8> = row.get(0)?;
        ids.push(ObjectId::from_bytes(&bytes)?);
    }
    drop(query);
    drop(statement);
    drop(connection);

    let store = Timing::disabled("sparse.readback.open", |scope| {
        Store::open(&path, scope.child("store"))
    })
    .0?;
    let provider = StoreProvider::new(&store);
    let mut digest = blake3::Hasher::new();
    let mut canonical_bytes = 0_u64;
    for wave in ids.chunks(32) {
        let values = provider.read_canonical_batch(wave)?;
        if values.len() != wave.len() {
            return Err("Store read returned a short wave".into());
        }
        for (id, canonical) in wave.iter().zip(values) {
            if ObjectId::for_bytes(&canonical) != *id {
                return Err(format!("canonical identity mismatch: {id}").into());
            }
            canonical_bytes += canonical.len() as u64;
            digest.update(id.as_bytes());
            digest.update(&(canonical.len() as u64).to_le_bytes());
            digest.update(&canonical);
        }
    }
    println!(
        "{{\"page_size\":{page_size},\"object_rows\":{rows},\"distinct_objects\":{},\"canonical_bytes\":{canonical_bytes},\"canonical_digest\":\"{}\",\"read_connections\":{}}}",
        ids.len(),
        digest.finalize(),
        provider.connection_opens(),
    );
    Ok(())
}

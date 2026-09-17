//! Reviewer probe: what page size (and storage profile) does the product's own
//! SQLite connection actually use?
//!
//! Opens files through the PRODUCT's public connection helper
//! (layerfs_storage::sqlite::connection::open), which applies the declared
//! pragma profile, and reads the effective values back from the same library the
//! product links. Read-only w.r.t. product source.

use std::path::Path;

use layerfs_storage::sqlite::connection;

fn scalar(connection: &rusqlite::Connection, pragma: &str) -> String {
    match connection.query_row(&format!("PRAGMA {pragma}"), [], |row| {
        row.get::<_, rusqlite::types::Value>(0)
    }) {
        Ok(value) => format!("{value:?}"),
        Err(error) => format!("<{error}>"),
    }
}

fn report(label: &str, path: &Path, create: bool) {
    let connection = match connection::open(path, create) {
        Ok(connection) => connection,
        Err(error) => {
            println!("RESULT {label} open=Err({error:?})");
            return;
        }
    };
    let version: String = connection
        .query_row("select sqlite_version()", [], |row| row.get(0))
        .unwrap_or_else(|error| format!("<{error}>"));
    let source: String = connection
        .query_row("select sqlite_source_id()", [], |row| row.get(0))
        .unwrap_or_else(|error| format!("<{error}>"));
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    println!("RESULT {label} path={} bytes={bytes}", path.display());
    println!("  sqlite_version = {version}");
    println!("  sqlite_source_id = {source}");
    for pragma in [
        "page_size",
        "page_count",
        "freelist_count",
        "max_page_count",
        "auto_vacuum",
        "cache_size",
        "mmap_size",
        "journal_mode",
        "synchronous",
        "temp_store",
        "foreign_keys",
        "busy_timeout",
        "application_id",
        "user_version",
        "encoding",
    ] {
        println!("  {pragma} = {}", scalar(&connection, pragma));
    }
    // What a storage engine would report as the atomic I/O unit.
    println!(
        "  file_bytes / page_size = {:.3}",
        bytes as f64 / 4096.0
    );
}

fn main() {
    let output = std::env::args().nth(1).expect("output directory");
    std::fs::create_dir_all(&output).expect("output directory");

    // 1. A file created by the product's own connection helper (no schema).
    let plain = Path::new(&output).join("plain.sqlite");
    let _ = std::fs::remove_file(&plain);
    report("connection.open(create=true)", &plain, true);

    // 2. A real Store created by the product, then reopened.
    let store_path = Path::new(&output).join("store.sqlite");
    let _ = std::fs::remove_file(&store_path);
    let created = layerfs_telemetry::timer::Timing::disabled("probe.create", |timing| {
        layerfs_storage::Store::create(&store_path, layerfs_storage::StoragePolicy::frozen_default(), timing.child("create"))
    })
    .0;
    match created {
        Ok(_) => report("Store::create", &store_path, false),
        Err(error) => println!("RESULT Store::create=Err({error:?})"),
    }
}

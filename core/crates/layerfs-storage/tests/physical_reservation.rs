//! Public save keeps SQLite bytes intact while reserving bounded host blocks.

#[cfg(target_os = "macos")]
mod support;

#[cfg(target_os = "macos")]
mod macos {

    use std::os::unix::fs::MetadataExt;

    use super::support::{construct_file, create_store, disabled, patterned, TempDir};
    use layerfs_storage::StorageError;

    #[test]
    fn save_reserves_physical_headroom_without_growing_sqlite_pages() {
        let dir = TempDir::new("physical_reservation");
        let path = dir.store_path("physical_reservation");
        let store = create_store(&path);
        let (collected, _, _) = construct_file(&patterned(300_000));
        disabled(|scope| {
            let mut save = store.begin_save(scope.child("begin"))?;
            for object in collected.finalized() {
                save.accept(object)?;
            }
            save.finish(scope.child("finish"))?;
            Ok::<_, StorageError>(())
        })
        .expect("public save");

        let metadata = path.metadata().expect("Store metadata");
        let connection = rusqlite::Connection::open(&path).expect("reopen SQLite");
        let pages: i64 = connection
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .expect("page count");
        let page_size: i64 = connection
            .query_row("PRAGMA page_size", [], |row| row.get(0))
            .expect("page size");
        assert_eq!(metadata.len(), (pages * page_size) as u64);
        assert!(metadata.blocks() * 512 >= metadata.len() + 1_048_576);
        assert_eq!(
            connection
                .query_row::<String, _, _>("PRAGMA quick_check", [], |row| row.get(0))
                .expect("database integrity"),
            "ok"
        );
    }
}

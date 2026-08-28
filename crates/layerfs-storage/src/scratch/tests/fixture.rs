use super::super::*;
use crate::scratch::schema::DISK_TABLE_CACHE_KIB;
use std::path::Path;

pub(super) fn assert_default_cache_budget(table: &DiskTable) -> i64 {
    let page_size = table
        .connection()
        .query_row("PRAGMA page_size", [], |row| row.get::<_, i64>(0))
        .unwrap();
    let expected_pages = (i64::from(DISK_TABLE_CACHE_KIB) * 1024 + page_size - 1) / page_size;
    assert_eq!(
        table
            .connection()
            .query_row("PRAGMA cache_size", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        expected_pages
    );
    assert_eq!(
        table
            .connection()
            .query_row("PRAGMA cache_spill", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        expected_pages
    );
    expected_pages
}

pub(super) struct ScratchDriver;
impl crate::generation::StoreGenerationDriver for ScratchDriver {
    fn available_bytes(&self, _directory: &Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }
    fn install_selector(&self, prepared: &Path, current: &Path) -> std::io::Result<()> {
        std::fs::rename(prepared, current)
    }
    fn sync_directory(&self, directory: &Path) -> std::io::Result<()> {
        std::fs::File::open(directory)?.sync_all()
    }
    fn file_identity(&self, path: &Path) -> std::io::Result<Vec<u8>> {
        Ok(path.as_os_str().to_string_lossy().into_owned().into_bytes())
    }
    fn remove_file_if_identity(&self, path: &Path, _expected: &[u8]) -> std::io::Result<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

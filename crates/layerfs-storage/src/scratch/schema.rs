pub(super) const DISK_TABLE_CACHE_KIB: u32 = 256;
pub(super) const SCRATCH_SCHEMA: &str = "PRAGMA journal_mode=DELETE;
                 PRAGMA synchronous=FULL;
                 PRAGMA temp_store=FILE;
                 PRAGMA mmap_size=0;
                 PRAGMA busy_timeout=0;
                 CREATE TABLE entries (
                    key BLOB PRIMARY KEY,
                    value BLOB NOT NULL,
                    pending INTEGER NOT NULL CHECK (pending IN (0, 1))
                 );
                 CREATE INDEX entries_pending_key ON entries (pending, key);
                 CREATE TABLE scratch_owner (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    format_marker TEXT NOT NULL,
                    store_id BLOB NOT NULL CHECK (length(store_id) = 32)
                 );";
pub(super) const SCRATCH_MARKER: &str = "layerfs-owned-scratch-v1";

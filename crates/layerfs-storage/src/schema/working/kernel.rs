//! Working metadata, local objects, deltas, and retained roots.

pub(super) const SCHEMAS: [(&str, &str); 4] = [
    (
        "layerfs_store_meta",
        "CREATE TABLE layerfs_store_meta (
            store_id INTEGER PRIMARY KEY CHECK (store_id = 1),
            format_marker TEXT NOT NULL CHECK (format_marker = 'layerfs-working-sqlite'),
            schema_version INTEGER NOT NULL CHECK (schema_version = 1),
            store_role TEXT NOT NULL CHECK (store_role = 'working'),
            storage_id BLOB NOT NULL UNIQUE CHECK (length(storage_id) = 32),
            next_inode_serial INTEGER NOT NULL CHECK (next_inode_serial >= 0),
            trusted_history INTEGER NOT NULL CHECK (trusted_history IN (0, 1)),
            journal_mode TEXT NOT NULL CHECK (lower(journal_mode) = 'delete'),
            synchronous INTEGER NOT NULL CHECK (synchronous = 2),
            temp_store INTEGER NOT NULL CHECK (temp_store = 1),
            mmap_size INTEGER NOT NULL CHECK (mmap_size = 0)
        )",
    ),
    (
        "layerfs_objects",
        "CREATE TABLE layerfs_objects (
            rowid INTEGER PRIMARY KEY,
            object_id BLOB NOT NULL UNIQUE CHECK (length(object_id) = 32),
            kind INTEGER NOT NULL CHECK (kind >= 0),
            canonical_length INTEGER NOT NULL CHECK (canonical_length >= 0),
            canonical_bytes BLOB NOT NULL,
            CHECK (canonical_length = length(canonical_bytes))
        )",
    ),
    (
        "layerfs_deltas",
        "CREATE TABLE layerfs_deltas (
            delta_id BLOB PRIMARY KEY CHECK (length(delta_id) = 32),
            format_version INTEGER NOT NULL CHECK (format_version >= 0),
            parent_root_id BLOB CHECK (parent_root_id IS NULL OR length(parent_root_id) = 32),
            result_root_id BLOB NOT NULL CHECK (length(result_root_id) = 32),
            payload BLOB NOT NULL
        )",
    ),
    (
        "layerfs_retained_roots",
        "CREATE TABLE layerfs_retained_roots (
            root_id BLOB PRIMARY KEY CHECK (length(root_id) = 32)
        )",
    ),
];

//! Full metadata, authenticated objects, deltas, and retained roots.

pub(super) const SCHEMAS: [(&str, &str); 6] = [
    (
        "layerfs_store_meta",
        "CREATE TABLE layerfs_store_meta (
            store_id INTEGER PRIMARY KEY CHECK (store_id = 1),
            format_marker TEXT NOT NULL CHECK (format_marker = 'layerfs-full-sqlite'),
            schema_version INTEGER NOT NULL CHECK (schema_version = 1),
            store_role TEXT NOT NULL CHECK (store_role IN ('durable', 'durable_cache')),
            storage_id BLOB NOT NULL UNIQUE CHECK (length(storage_id) = 32),
            durable_storage_id BLOB NOT NULL UNIQUE CHECK (length(durable_storage_id) = 32),
            next_inode_serial INTEGER NOT NULL CHECK (next_inode_serial >= 0),
            trusted_history INTEGER NOT NULL CHECK (trusted_history IN (0, 1)),
            journal_mode TEXT NOT NULL CHECK (lower(journal_mode) = 'delete'),
            synchronous INTEGER NOT NULL CHECK (synchronous = 2),
            temp_store INTEGER NOT NULL CHECK (temp_store = 1),
            mmap_size INTEGER NOT NULL CHECK (mmap_size = 0),
            CHECK ((store_role = 'durable' AND durable_storage_id = storage_id)
                OR (store_role = 'durable_cache' AND durable_storage_id != storage_id))
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
            payload BLOB NOT NULL,
            FOREIGN KEY(parent_root_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(result_root_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_retained_roots",
        "CREATE TABLE layerfs_retained_roots (
            root_id BLOB PRIMARY KEY CHECK (length(root_id) = 32),
            FOREIGN KEY(root_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_version_leases",
        "CREATE TABLE layerfs_version_leases (
            lease_id BLOB PRIMARY KEY CHECK (length(lease_id) = 32),
            target_kind TEXT NOT NULL CHECK (target_kind IN ('layer', 'operation_version')),
            layer_stack_id BLOB CHECK (
                layer_stack_id IS NULL OR length(layer_stack_id) = 32),
            layer_id BLOB CHECK (layer_id IS NULL OR length(layer_id) = 32),
            branch_id BLOB CHECK (branch_id IS NULL OR length(branch_id) = 32),
            operation_version_id BLOB CHECK (
                operation_version_id IS NULL OR length(operation_version_id) = 32),
            owner_kind TEXT NOT NULL CHECK (owner_kind IN
                ('branch', 'operation_workspace', 'mount', 'materialization',
                 'layer_candidate', 'child_branch_merge', 'layer_stack_merge',
                 'sync', 'tracking_ref', 'explicit')),
            owner_id BLOB NOT NULL CHECK (length(owner_id) = 32),
            created_at INTEGER NOT NULL,
            expires_at INTEGER CHECK (expires_at IS NULL OR expires_at >= created_at),
            UNIQUE(layer_stack_id, layer_id, owner_kind, owner_id),
            UNIQUE(branch_id, operation_version_id, owner_kind, owner_id),
            CHECK ((target_kind = 'layer' AND layer_stack_id IS NOT NULL
                       AND layer_id IS NOT NULL AND branch_id IS NULL
                       AND operation_version_id IS NULL)
                   OR (target_kind = 'operation_version' AND layer_stack_id IS NULL
                       AND layer_id IS NULL AND branch_id IS NOT NULL
                       AND operation_version_id IS NOT NULL)),
            FOREIGN KEY(layer_stack_id, layer_id)
                REFERENCES layerfs_layers(layer_stack_id, layer_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
    (
        "layerfs_released_versions",
        "CREATE TABLE layerfs_released_versions (
            release_id BLOB PRIMARY KEY CHECK (length(release_id) = 32),
            target_kind TEXT NOT NULL CHECK (target_kind IN ('layer', 'operation_version')),
            layer_stack_id BLOB CHECK (
                layer_stack_id IS NULL OR length(layer_stack_id) = 32),
            layer_id BLOB CHECK (layer_id IS NULL OR length(layer_id) = 32),
            branch_id BLOB CHECK (branch_id IS NULL OR length(branch_id) = 32),
            operation_version_id BLOB CHECK (
                operation_version_id IS NULL OR length(operation_version_id) = 32),
            root_id BLOB NOT NULL CHECK (length(root_id) = 32),
            release_generation INTEGER NOT NULL CHECK (release_generation > 0),
            request_id BLOB NOT NULL CHECK (length(request_id) = 32),
            UNIQUE(layer_stack_id, layer_id),
            UNIQUE(branch_id, operation_version_id),
            CHECK ((target_kind = 'layer' AND layer_stack_id IS NOT NULL
                       AND layer_id IS NOT NULL AND branch_id IS NULL
                       AND operation_version_id IS NULL)
                   OR (target_kind = 'operation_version' AND layer_stack_id IS NULL
                       AND layer_id IS NULL AND branch_id IS NOT NULL
                       AND operation_version_id IS NOT NULL)),
            FOREIGN KEY(layer_stack_id, layer_id)
                REFERENCES layerfs_layers(layer_stack_id, layer_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(branch_id, operation_version_id)
                REFERENCES layerfs_operation_versions(branch_id, operation_version_id)
                DEFERRABLE INITIALLY DEFERRED,
            FOREIGN KEY(root_id) REFERENCES layerfs_objects(object_id)
                DEFERRABLE INITIALLY DEFERRED
        )",
    ),
];

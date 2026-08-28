//! Authenticated external Durable base-binding DDL.

pub(super) const SCHEMAS: [(&str, &str); 1] = [(
    "layerfs_working_base_bindings",
    "CREATE TABLE layerfs_working_base_bindings (
        binding_id BLOB PRIMARY KEY CHECK (length(binding_id) = 32),
        durable_storage_id BLOB NOT NULL CHECK (length(durable_storage_id) = 32),
        target_kind TEXT NOT NULL CHECK (target_kind IN ('branch', 'layer_stack')),
        target_id BLOB NOT NULL CHECK (length(target_id) = 32),
        target_version_id BLOB CHECK (
            target_version_id IS NULL OR length(target_version_id) = 32),
        generation INTEGER NOT NULL CHECK (generation >= 0),
        root_id BLOB NOT NULL CHECK (length(root_id) = 32),
        verification_receipt_id BLOB NOT NULL CHECK (
            length(verification_receipt_id) = 32),
        authority_pin_id BLOB NOT NULL CHECK (length(authority_pin_id) = 32),
        pin_expires_at INTEGER,
        status TEXT NOT NULL CHECK (status IN ('external_pinned', 'invalid')),
        CHECK ((target_kind = 'branch'
                   AND ((generation = 0 AND target_version_id IS NULL)
                     OR (generation > 0 AND target_version_id IS NOT NULL)))
               OR (target_kind = 'layer_stack' AND target_version_id IS NOT NULL)),
        UNIQUE(durable_storage_id, target_kind, target_id, generation)
    )",
)];

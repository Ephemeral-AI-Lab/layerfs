//! Full named owner-cleanup index manifest.

pub(super) const INDEX_SCHEMAS: [(&str, &str); 4] = [
    (
        "layerfs_full_sync_batch_receipts_owner_idx",
        "CREATE INDEX layerfs_full_sync_batch_receipts_owner_idx
         ON layerfs_sync_batch_receipts (owner_request_id, direction)",
    ),
    (
        "layerfs_full_sync_object_pins_owner_idx",
        "CREATE INDEX layerfs_full_sync_object_pins_owner_idx
         ON layerfs_sync_object_pins
            (owner_request_id, direction, request_id, object_id)",
    ),
    (
        "layerfs_full_transfer_state_owner_idx",
        "CREATE INDEX layerfs_full_transfer_state_owner_idx
         ON layerfs_transfer_state
            (owner_request_id, direction, request_id, batch_sequence)",
    ),
    (
        "layerfs_full_version_leases_owner_idx",
        "CREATE INDEX layerfs_full_version_leases_owner_idx
         ON layerfs_version_leases (owner_kind, owner_id)",
    ),
];

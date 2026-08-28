//! Working named owner-cleanup index manifest.

pub(super) const INDEX_SCHEMAS: [(&str, &str); 2] = [
    (
        "layerfs_working_transfer_state_owner_idx",
        "CREATE INDEX layerfs_working_transfer_state_owner_idx
         ON layerfs_transfer_state
            (owner_request_id, direction, request_id, batch_sequence)",
    ),
    (
        "layerfs_working_version_leases_owner_idx",
        "CREATE INDEX layerfs_working_version_leases_owner_idx
         ON layerfs_version_leases (owner_kind, owner_id)",
    ),
];

UPDATE init_native_file SET file_root=?3 WHERE operation_id=?1 AND canonical_position=?2 AND file_root IS NULL RETURNING canonical_position

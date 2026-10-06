UPDATE init_entry SET content_root=?3 WHERE operation_id=?1 AND kind=2 AND position=?2 AND content_root IS NULL RETURNING position

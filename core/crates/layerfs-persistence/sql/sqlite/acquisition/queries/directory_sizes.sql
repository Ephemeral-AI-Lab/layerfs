SELECT length(native_path)+8 FROM init_entry WHERE operation_id=?1 AND kind=2 AND position>?2 ORDER BY position LIMIT ?3+0

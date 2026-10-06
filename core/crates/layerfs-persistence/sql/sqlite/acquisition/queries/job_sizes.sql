SELECT length(native_path)+76 FROM init_native_file WHERE operation_id=?1 AND canonical_position>?2 ORDER BY canonical_position LIMIT ?3+0

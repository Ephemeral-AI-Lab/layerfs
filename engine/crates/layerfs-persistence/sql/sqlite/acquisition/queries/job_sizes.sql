SELECT length(host_path)+68 FROM init_host_file WHERE operation_id=?1 AND canonical_position>?2 ORDER BY canonical_position LIMIT ?3+0

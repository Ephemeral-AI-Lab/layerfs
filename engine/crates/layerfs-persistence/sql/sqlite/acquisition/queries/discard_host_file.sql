DELETE FROM init_host_file WHERE operation_id=?1 AND canonical_position<=?2 RETURNING length(host_path)+60+coalesce(length(file_root),0)

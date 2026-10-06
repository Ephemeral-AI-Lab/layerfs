DELETE FROM init_native_file WHERE operation_id=?1 AND canonical_position<=?2 RETURNING length(native_path)+60+coalesce(length(file_root),0)

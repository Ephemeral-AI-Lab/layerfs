DELETE FROM init_entry WHERE operation_id=?1 AND (parent_position,name)<=(?2,?3) RETURNING length(name)+coalesce(length(native_path),0)+coalesce(length(native),0)+32+coalesce(length(content_root),0)

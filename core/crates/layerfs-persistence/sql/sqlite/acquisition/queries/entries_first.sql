SELECT parent_position,name,position,kind,canonical_position,metadata_root,content_root FROM init_entry WHERE operation_id=?1 ORDER BY parent_position,name LIMIT ?2

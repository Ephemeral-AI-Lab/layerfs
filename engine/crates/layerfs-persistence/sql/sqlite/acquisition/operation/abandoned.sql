SELECT operation_id,owner_epoch,phase,held_rows,held_bytes FROM init_operation WHERE operation_id>?1 AND owner_epoch<>?2 ORDER BY operation_id LIMIT ?3+0

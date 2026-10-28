SELECT phase,held_rows,held_bytes,peak_rows,peak_bytes,removed_rows,removed_bytes FROM init_operation WHERE operation_id=?1 AND owner_epoch=?2

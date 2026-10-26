DELETE FROM init_operation WHERE operation_id=?1 AND owner_epoch=?2 AND held_rows=0 RETURNING operation_id

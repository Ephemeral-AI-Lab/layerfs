UPDATE init_operation SET phase=?3 WHERE operation_id=?1 AND owner_epoch=?2 RETURNING phase

UPDATE init_operation SET held_rows=held_rows-?3,held_bytes=held_bytes-?4,removed_rows=removed_rows+?3,removed_bytes=removed_bytes+?4 WHERE operation_id=?1 AND owner_epoch=?2 RETURNING held_rows

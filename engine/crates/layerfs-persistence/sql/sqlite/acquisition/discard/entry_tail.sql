SELECT parent_position,name FROM init_entry WHERE operation_id=?1 ORDER BY parent_position DESC,name DESC LIMIT 1

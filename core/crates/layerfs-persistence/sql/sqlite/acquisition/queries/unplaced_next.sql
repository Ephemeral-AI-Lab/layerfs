SELECT name,kind,native FROM init_entry WHERE operation_id=?1 AND parent_position=?2 AND name>?3 AND position IS NULL ORDER BY name LIMIT ?4+0

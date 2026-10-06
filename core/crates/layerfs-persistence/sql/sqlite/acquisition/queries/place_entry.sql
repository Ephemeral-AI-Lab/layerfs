UPDATE init_entry SET position=?4,canonical_position=?5,native=NULL WHERE operation_id=?1 AND parent_position=?2 AND name=?3 AND position IS NULL RETURNING kind

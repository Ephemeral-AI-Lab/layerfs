SELECT canonical_position FROM init_host_file WHERE operation_id=?1 ORDER BY canonical_position LIMIT 1 OFFSET ?2+0

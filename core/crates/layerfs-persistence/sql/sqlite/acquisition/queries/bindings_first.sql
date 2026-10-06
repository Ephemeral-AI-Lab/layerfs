SELECT owner.phase,entry.parent_position,entry.name,entry.position,entry.canonical_position
FROM init_operation AS owner LEFT JOIN init_entry AS entry ON entry.operation_id=owner.operation_id
WHERE owner.operation_id=?1 AND owner.owner_epoch=?2
ORDER BY entry.parent_position,entry.name LIMIT ?3+0

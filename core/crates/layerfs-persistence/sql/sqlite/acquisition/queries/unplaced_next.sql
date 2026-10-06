SELECT owner.phase,entry.name,entry.kind,entry.native
FROM init_operation AS owner LEFT JOIN init_entry AS entry
ON entry.operation_id=owner.operation_id AND entry.parent_position=?3 AND entry.name>?4 AND entry.position IS NULL
WHERE owner.operation_id=?1 AND owner.owner_epoch=?2
ORDER BY entry.name LIMIT ?5+0

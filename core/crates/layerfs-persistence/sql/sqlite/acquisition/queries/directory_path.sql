SELECT owner.phase,entry.native_path
FROM init_operation AS owner LEFT JOIN init_entry AS entry
ON entry.operation_id=owner.operation_id AND entry.kind=2 AND entry.position=?3
WHERE owner.operation_id=?1 AND owner.owner_epoch=?2

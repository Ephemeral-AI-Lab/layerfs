SELECT owner.phase,native.canonical_position,native.device,native.inode,native.evidence,native.native_path
FROM init_operation AS owner LEFT JOIN init_native_file AS native
ON native.operation_id=owner.operation_id AND native.canonical_position=?3
WHERE owner.operation_id=?1 AND owner.owner_epoch=?2

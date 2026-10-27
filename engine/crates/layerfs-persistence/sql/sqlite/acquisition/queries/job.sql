SELECT owner.phase,host_file.canonical_position,host_file.device,host_file.inode,host_file.evidence,host_file.host_path
FROM init_operation AS owner LEFT JOIN init_host_file AS host_file
ON host_file.operation_id=owner.operation_id AND host_file.canonical_position=?3
WHERE owner.operation_id=?1 AND owner.owner_epoch=?2

INSERT INTO init_operation(owner_epoch,phase,source_device,source_inode,stack_id,scope) VALUES(coalesce(?1,1),1,?2,?3,?4,?5) RETURNING operation_id

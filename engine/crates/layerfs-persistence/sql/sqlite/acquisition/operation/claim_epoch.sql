UPDATE init_operation SET owner_epoch=operation_id WHERE operation_id=?1 RETURNING owner_epoch

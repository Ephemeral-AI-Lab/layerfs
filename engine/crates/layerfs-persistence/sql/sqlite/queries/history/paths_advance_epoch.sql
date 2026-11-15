UPDATE project_epoch SET epoch=epoch+1 WHERE layer_stack_id=$1 RETURNING 1;

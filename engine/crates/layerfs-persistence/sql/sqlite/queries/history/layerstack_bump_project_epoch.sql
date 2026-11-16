INSERT INTO project_epoch (layer_stack_id, epoch) VALUES ($1, 1) ON CONFLICT (layer_stack_id) DO UPDATE SET epoch = epoch + 1 RETURNING 1;

UPDATE layer_stack SET head_layer_id = $1 WHERE layer_stack_id = $2 AND head_layer_id = $3 RETURNING 1;

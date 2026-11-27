SELECT layer_id, count(*) OVER () FROM layer WHERE layer_stack_id = $1 AND layer_id >= $2 AND layer_id <= $3 ORDER BY layer_id LIMIT 1;

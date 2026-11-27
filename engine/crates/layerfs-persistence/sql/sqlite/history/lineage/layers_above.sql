SELECT layer_id FROM layer_lineage WHERE layer_stack_id = $1 AND height > $2 AND height <= $3 ORDER BY height DESC LIMIT $4;

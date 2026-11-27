SELECT s.name, CASE WHEN r.layer_stack_id IS NULL THEN 0 ELSE 1 END FROM layer_stack s LEFT JOIN layer_stack_retired r ON r.layer_stack_id = s.layer_stack_id WHERE s.name = $1;

DELETE FROM reach
 WHERE NOT EXISTS (SELECT 1 FROM layer_stack s WHERE s.layer_stack_id = reach.key)
   AND NOT EXISTS (SELECT 1 FROM commit_lineage cl WHERE cl.run = reach.key);

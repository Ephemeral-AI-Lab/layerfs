DELETE FROM layer_lineage
 WHERE layer_stack_id = $1
   AND NOT EXISTS (SELECT 1 FROM layer l WHERE l.layer_id = layer_lineage.layer_id);

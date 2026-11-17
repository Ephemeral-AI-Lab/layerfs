SELECT COALESCE(MAX(history_serial),0) FROM (SELECT history_serial FROM "commit" WHERE layer_stack_id=$1 UNION ALL SELECT history_serial FROM layer WHERE layer_stack_id=$1) AS serials;

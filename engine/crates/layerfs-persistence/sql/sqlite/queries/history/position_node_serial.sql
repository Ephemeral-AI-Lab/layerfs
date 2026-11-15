SELECT history_serial FROM "commit" WHERE commit_id=$1 UNION ALL SELECT history_serial FROM layer WHERE layer_id=$1 LIMIT 1;

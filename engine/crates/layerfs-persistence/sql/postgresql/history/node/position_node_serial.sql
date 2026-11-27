SELECT history_serial FROM {s}."commit" WHERE commit_id=$1 UNION ALL SELECT history_serial FROM {s}.layer WHERE layer_id=$1 LIMIT 1;

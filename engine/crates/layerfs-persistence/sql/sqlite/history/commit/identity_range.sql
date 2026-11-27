SELECT commit_id, count(*) OVER () FROM "commit" WHERE layer_stack_id = $1 AND commit_id >= $2 AND commit_id <= $3 ORDER BY commit_id LIMIT 1;

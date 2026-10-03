UPDATE history_meta SET next_stage_token = $1 WHERE id = 1 AND next_stage_token = $2 RETURNING 1;

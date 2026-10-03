DELETE FROM ${schema}.workspace_stage WHERE workspace_id = $1 AND stage_token = $2 RETURNING 1;

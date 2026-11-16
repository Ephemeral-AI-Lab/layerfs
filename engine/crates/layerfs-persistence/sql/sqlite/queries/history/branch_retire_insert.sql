INSERT INTO branch_retired (branch_id, removed_ms) VALUES ($1, $2) ON CONFLICT(branch_id) DO NOTHING RETURNING branch_id;

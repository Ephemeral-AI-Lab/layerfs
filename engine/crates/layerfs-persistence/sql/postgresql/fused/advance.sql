WITH a AS (UPDATE {s}.branch SET head_commit_id=$1::bytea WHERE branch_id=$2::bytea AND base_layer_id=$3::bytea 
AND head_commit_id IS NOT DISTINCT FROM $4::bytea 
AND 1/(current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM $5::text)::int=1 RETURNING 1) 
SELECT 1/(count(*)=1)::int FROM a

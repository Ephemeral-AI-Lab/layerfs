WITH ins AS (INSERT INTO {s}.commit_lineage (commit_id,run,depth,newest,merged) 
SELECT $1::bytea,$1::bytea,1,l.height,NULL::bytea FROM {s}.layer_lineage l WHERE l.layer_id=$2::bytea 
AND 1/(current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM $3::text)::int=1 RETURNING 1) 
SELECT 1/(count(*)=1)::int FROM ins

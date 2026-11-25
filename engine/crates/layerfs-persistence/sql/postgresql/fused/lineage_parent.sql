WITH ins AS (INSERT INTO {s}.commit_lineage (commit_id,run,depth,newest,merged) 
SELECT $1::bytea,CASE WHEN EXISTS (SELECT 1 FROM {s}.commit_lineage x WHERE x.run=l.run AND x.depth=l.depth+1) THEN $1::bytea ELSE l.run END,l.depth+1,l.newest,NULL::bytea 
FROM {s}.commit_lineage l WHERE l.commit_id=$2::bytea 
AND 1/(current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM $3::text)::int=1 RETURNING 1) 
SELECT 1/(count(*)=1)::int FROM ins

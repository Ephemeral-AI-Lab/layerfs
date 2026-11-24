WITH e AS (SELECT p.epoch FROM {s}.project_epoch p WHERE p.layer_stack_id=$1::bytea FOR SHARE) 
SELECT 1/((SELECT count(*) FROM e)=1 AND (SELECT min(e.epoch) FROM e)::text IS NOT DISTINCT FROM current_setting('layerfs.epoch',true) 
AND current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM $2::text)::int

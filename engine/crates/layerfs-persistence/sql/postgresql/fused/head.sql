SELECT CASE WHEN NOT x.unmoved THEN (1/(x.n-x.n))::text 
WHEN x.epoch IS NULL THEN (32768+x.n)::smallint::text 
ELSE set_config('layerfs.epoch',x.epoch::text,true) END 
FROM (SELECT current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM $6::text 
AND EXISTS (SELECT 1 FROM {s}.branch b WHERE b.branch_id=$1::bytea AND b.layer_stack_id=$2::bytea 
AND b.base_layer_id=$3::bytea AND b.head_commit_id IS NOT DISTINCT FROM $4::bytea 
AND NOT EXISTS (SELECT 1 FROM {s}.branch_retired r WHERE r.branch_id=b.branch_id) 
AND NOT EXISTS (SELECT 1 FROM {s}.layer_stack_retired d WHERE d.layer_stack_id=b.layer_stack_id)) 
AND CASE WHEN $4::bytea IS NULL THEN EXISTS (SELECT 1 FROM {s}.layer l WHERE l.layer_id=$3::bytea AND l.root_id=$5::bytea) 
ELSE EXISTS (SELECT 1 FROM {s}."commit" c WHERE c.commit_id=$4::bytea AND c.root_id=$5::bytea 
AND c.layer_stack_id=$2::bytea AND c.base_layer_id=$3::bytea) END AS unmoved, 
(SELECT p.epoch FROM {s}.project_epoch p WHERE p.layer_stack_id=$2::bytea) AS epoch, 
(SELECT count(*) FROM {s}.project_epoch p WHERE p.layer_stack_id=$2::bytea) AS n) x

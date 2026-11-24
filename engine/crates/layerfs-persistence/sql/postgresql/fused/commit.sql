WITH serial AS (UPDATE {s}.history_meta SET next_history_serial=next_history_serial+1 
WHERE id=1 AND 1/(current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM $18::text)::int=1 
RETURNING next_history_serial-1 AS n), 
ins AS (INSERT INTO {s}."commit" (commit_id,layer_stack_id,root_id,parent_commit_id,base_layer_id,second_parent_id,branch_id,via,message,merged_from_id,created_ms,logical,new_canonical,new_stored,new_metadata,lww,displaced_id,history_serial) 
SELECT $1::bytea,$2::bytea,$3::bytea,$4::bytea,$5::bytea,$6::bytea,$7::bytea,$8::bigint,$9::text,$10::bytea,$11::bigint,$12::bigint,$13::bigint,$14::bigint,$15::bigint,$16::bytea,$17::bytea,serial.n FROM serial 
ON CONFLICT (commit_id) DO NOTHING RETURNING history_serial) 
SELECT CASE WHEN i.k=1 THEN i.s ELSE 1/(i.k-i.k) END FROM (SELECT count(*) AS k,max(history_serial) AS s FROM ins) i

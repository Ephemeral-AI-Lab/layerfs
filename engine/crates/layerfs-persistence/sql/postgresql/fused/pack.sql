INSERT INTO {s}.pack (pack_id,domain,digest,length,body,directory) 
SELECT * FROM unnest($1::bigint[],$2::bigint[],$3::bytea[],$4::bigint[],$5::bytea[],$6::bytea[]) 
WHERE 1/(current_setting('layerfs.attempt',true) IS NOT DISTINCT FROM $7::text)::int=1

SELECT p.pack_id,p.directory,p.domain,CASE WHEN octet_length(p.digest)=32 THEN p.digest END,p.length FROM {schema}.pack p WHERE p.pack_id=ANY($1::bigint[])

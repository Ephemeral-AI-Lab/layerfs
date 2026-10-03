SELECT o.object_id,o.role,o.canonical_length,o.pack_id,o.group_number,o.record_number,
    p.domain,p.digest,p.length
FROM ${schema}.object o JOIN ${schema}.pack p USING(pack_id)
WHERE o.object_id=ANY($1::BYTEA[]) ORDER BY o.object_id;

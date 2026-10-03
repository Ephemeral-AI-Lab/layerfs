SELECT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest,p.metadata_window_start
FROM ${schema}.store_policy p LEFT JOIN LATERAL (
    SELECT DISTINCT v.first_ordinal,v.count,v.pack_id,v.group_number,v.digest
    FROM unnest($1::BIGINT[]) ordinal
    JOIN ${schema}.metadata_value_group v ON v.first_ordinal<=ordinal AND v.first_ordinal+v.count>ordinal
    ORDER BY v.first_ordinal
) g ON true WHERE p.id=1 ORDER BY g.first_ordinal;

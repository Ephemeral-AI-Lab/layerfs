SELECT g.first_ordinal,g.count,g.pack_id,g.group_number,g.digest,p.metadata_window_start
FROM ${schema}.store_policy p LEFT JOIN LATERAL (
    SELECT first_ordinal,count,pack_id,group_number,digest FROM ${schema}.metadata_value_group
    WHERE first_ordinal>=$1::BIGINT ORDER BY first_ordinal LIMIT $2::BIGINT+1
) g ON true WHERE p.id=1 ORDER BY g.first_ordinal;

UNION ALL
SELECT 2, NULL, NULL, NULL, NULL, NULL, p.pack_id,
       CASE WHEN octet_length(p.body) = selected.length THEN p.body END,
       NULL, NULL, NULL
FROM (
    SELECT p.pack_id, p.length, sum(p.length) OVER (ORDER BY p.pack_id) AS bytes
    FROM {schema}.pack p
    JOIN (
        SELECT pack_id
        FROM o
        GROUP BY pack_id
        HAVING bool_and(COALESCE(ancestors = ''::bytea, false) AND role IN (6, 7, 8, 9))
    ) candidates USING (pack_id)
    WHERE p.domain = 0
      AND p.length BETWEEN 32 AND $4::bigint
      AND p.pack_id <> ALL($3::bigint[])
      AND substring(p.directory FROM 1 FOR 12) IN (
          decode('4c465041434b000018000000', 'hex'),
          decode('4c465041434b00001a000000', 'hex')
      )
) selected
JOIN {schema}.pack p USING (pack_id)
WHERE selected.bytes <= $4

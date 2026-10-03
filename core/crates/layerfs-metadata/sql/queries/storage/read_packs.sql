WITH wanted AS MATERIALIZED (
    SELECT pack_id,domain,digest,length,body FROM ${schema}.pack WHERE pack_id=ANY($1::BIGINT[])
), bounds AS (
    SELECT COALESCE(sum(length),0)<=4194304 AND bool_and(domain=0) AS valid FROM wanted
)
SELECT CASE WHEN b.valid THEN 0 ELSE 1 END::SMALLINT AS status,
    p.pack_id,p.domain,p.digest,p.length,CASE WHEN b.valid THEN p.body ELSE NULL END AS body
FROM wanted p CROSS JOIN bounds b ORDER BY p.pack_id;

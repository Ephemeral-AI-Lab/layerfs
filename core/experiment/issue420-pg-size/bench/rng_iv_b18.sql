\set i random(1, 10000)
SELECT bucket, present, vals FROM iv_b18 WHERE bucket BETWEEN (SELECT ordinal / 18 FROM iv_keys WHERE i = :i) AND (SELECT (ordinal + 127) / 18 FROM iv_keys WHERE i = :i);

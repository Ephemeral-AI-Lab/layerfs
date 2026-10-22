\set i random(1, 10000)
SELECT bucket, present, vals FROM iv_b32z WHERE bucket BETWEEN (SELECT ordinal / 32 FROM iv_keys WHERE i = :i) AND (SELECT (ordinal + 127) / 32 FROM iv_keys WHERE i = :i);

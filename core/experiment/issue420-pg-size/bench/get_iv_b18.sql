\set i random(1, 10000)
SELECT present, vals FROM iv_b18 WHERE bucket = (SELECT ordinal / 18 FROM iv_keys WHERE i = :i);

\set i random(1, 10000)
SELECT present, vals FROM iv_b32z WHERE bucket = (SELECT ordinal / 32 FROM iv_keys WHERE i = :i);

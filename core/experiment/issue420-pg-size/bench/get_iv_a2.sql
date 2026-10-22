\set i random(1, 10000)
SELECT value FROM iv_a2 WHERE ordinal = (SELECT ordinal FROM iv_keys WHERE i = :i);

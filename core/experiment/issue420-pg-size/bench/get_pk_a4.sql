\set i random(1, 2000)
SELECT body FROM pk_a4 WHERE pack_id = (SELECT pack_id FROM pk_keys WHERE i = :i);

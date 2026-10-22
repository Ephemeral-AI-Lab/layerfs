\set i random(1, 2000)
SELECT body FROM pack WHERE pack_id = (SELECT pack_id FROM pk_keys WHERE i = :i);

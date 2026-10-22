\set i random(1, 2000)
SELECT data FROM pk_b_slice WHERE pack_id = (SELECT pack_id FROM pk_keys WHERE i = :i) ORDER BY seq;

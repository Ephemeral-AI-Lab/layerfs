\set i random(1, 10000)
SELECT entries FROM bh_b3 WHERE bucket = (SELECT child / 128 FROM bh_keys WHERE i = :i);

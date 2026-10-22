\set i random(1, 10000)
SELECT entries FROM bh_b2 WHERE bucket = (SELECT child / 16 FROM bh_keys WHERE i = :i);

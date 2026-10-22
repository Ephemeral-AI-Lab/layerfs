\set i random(1, 10000)
SELECT child, parent, name FROM binding_hint WHERE child = (SELECT child FROM bh_keys WHERE i = :i);
